use std::any::TypeId;

use bevy_app::{App, First, MainScheduleOrder, Plugin};
use bevy_ecs::{
    component::ComponentId,
    query::QueryBuilder,
    reflect::AppTypeRegistry,
    schedule::ScheduleLabel,
    system::Resource,
    world::{FilteredEntityRef, World},
};
use bevy_log::debug;
use bevy_reflect::{serde::ReflectSerializer, ReflectFromPtr};
use bevy_utils::hashbrown::{HashMap, HashSet};
use brp::*;
use crossbeam_channel::{Receiver, Sender};

pub mod brp;

#[cfg(feature = "http")]
pub mod http;

pub struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(ProcessBrp);

        let mut order = app.world.resource_mut::<MainScheduleOrder>();
        order.insert_after(First, ProcessBrp);

        app.add_systems(ProcessBrp, process_brp_sessions);

        app.insert_resource(BrpSessions::default());

        #[cfg(feature = "http")]
        app.add_plugins(http::HttpRemotePlugin);
    }
}

#[derive(Resource, Default, Clone)]
pub struct BrpSessions(Vec<BrpSession>);

#[derive(Debug, Clone)]
pub struct BrpSession {
    pub label: String,
    pub component_format: BrpComponentFormat,
    pub request_sender: Sender<BrpRequest>,
    pub request_receiver: Receiver<BrpRequest>,
    pub response_sender: Sender<BrpResponse>,
    pub response_receiver: Receiver<BrpResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrpComponentFormat {
    Json,
    Ron,
}

impl BrpSessions {
    pub fn open(
        &mut self,
        label: impl Into<String>,
        component_format: BrpComponentFormat,
    ) -> BrpSession {
        let (request_sender, request_receiver) = crossbeam_channel::unbounded();
        let (response_sender, response_receiver) = crossbeam_channel::unbounded();

        let session = BrpSession {
            label: label.into(),
            component_format,
            request_sender,
            request_receiver,
            response_sender,
            response_receiver,
        };

        for existing_session in self.0.iter() {
            assert_ne!(existing_session.label, session.label);
        }

        self.0.push(session.clone());

        session
    }

    pub fn close(&mut self, label: &str) {
        let index = self
            .0
            .iter()
            .position(|session| session.label == label)
            .unwrap();

        self.0.remove(index);
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, ScheduleLabel)]
pub struct ProcessBrp;

fn process_brp_sessions(world: &mut World) {
    let sessions = (*world.resource::<BrpSessions>()).clone();
    for session in sessions.0.iter() {
        process_brp_session(world, session);
    }
}

fn process_brp_session(world: &mut World, session: &BrpSession) {
    loop {
        let request = match session.request_receiver.try_recv() {
            Ok(request) => request,
            Err(err) => match err {
                crossbeam_channel::TryRecvError::Empty => break, // no more requests for now
                crossbeam_channel::TryRecvError::Disconnected => {
                    panic!("BRP request channel disconnected")
                }
            },
        };

        let response = process_brp_request(world, &session, &request);

        match session.response_sender.send(response) {
            Ok(_) => {}
            Err(err) => {
                panic!("BRP response channel disconnected: {:?}", err)
            }
        }

        debug!("Received {:?} from session {:?}", request, session.label);
    }
}

fn process_brp_request(
    world: &mut World,
    session: &BrpSession,
    request: &BrpRequest,
) -> BrpResponse {
    match request.request {
        BrpRequestContent::Ping => BrpResponse::new(request.id, BrpResponseContent::Ok),
        BrpRequestContent::Query {
            ref data,
            ref filter,
        } => process_brp_query_request(world, session, request.id, data, filter),
        _ => BrpResponse::from_error(request.id, BrpError::Unimplemented),
    }
}

fn process_brp_query_request(
    world: &mut World,
    session: &BrpSession,
    id: BrpId,
    data: &BrpQueryData,
    filter: &BrpQueryFilter,
) -> BrpResponse {
    let type_registry_arc = (**world.resource::<AppTypeRegistry>()).clone();

    let mut mentioned_components = HashSet::<String>::new();
    let mut component_id_map = HashMap::<String, ComponentId>::new();
    let mut type_id_map = HashMap::<String, TypeId>::new();

    for component_name in data
        .components
        .iter()
        .chain(data.optional.iter())
        .chain(data.has.iter())
        .chain(filter.with.iter())
        .chain(filter.without.iter())
    {
        mentioned_components.insert(component_name.0.clone());
    }

    for component in world.components().iter() {
        let name = component.name();
        if mentioned_components.contains(name) {
            component_id_map.insert(name.to_string(), component.id());
            if let Some(type_id) = component.type_id() {
                type_id_map.insert(name.to_string(), type_id);
            }
        }
    }

    let mut builder = QueryBuilder::<FilteredEntityRef>::new(world);

    for component_name in &data.components {
        let Some(component_id) = component_id_map.get(&component_name.0) else {
            return BrpResponse::from_error(
                id,
                BrpError::ComponentNotFound(component_name.0.clone()),
            );
        };

        builder.ref_id(*component_id);
    }

    for component_name in &data.optional {
        let Some(component_id) = component_id_map.get(&component_name.0) else {
            return BrpResponse::from_error(
                id,
                BrpError::ComponentNotFound(component_name.0.clone()),
            );
        };
        builder.optional(|query| {
            query.ref_id(*component_id);
        });
    }

    for component_name in &data.has {
        let Some(component_id) = component_id_map.get(&component_name.0) else {
            return BrpResponse::from_error(
                id,
                BrpError::ComponentNotFound(component_name.0.clone()),
            );
        };

        builder.optional(|query| {
            query.ref_id(*component_id);
        });
    }

    for component_name in &filter.with {
        let Some(component_id) = component_id_map.get(&component_name.0) else {
            return BrpResponse::from_error(
                id,
                BrpError::ComponentNotFound(component_name.0.clone()),
            );
        };

        builder.with_id(*component_id);
    }

    for component_name in &filter.without {
        let Some(component_id) = component_id_map.get(&component_name.0) else {
            return BrpResponse::from_error(
                id,
                BrpError::ComponentNotFound(component_name.0.clone()),
            );
        };

        builder.without_id(*component_id);
    }

    let mut query = builder.build();

    let mut results = BrpQueryResults::default();

    for entity in query.iter(world) {
        let mut result = BrpQueryResult {
            entity: BrpEntity(entity.id()),
            components: HashMap::new(),
            optional: HashMap::new(),
            has: HashMap::new(),
        };

        for component_name in &data.components {
            let component_id = component_id_map[&component_name.0];
            let Some(type_id) = type_id_map.get(&component_name.0) else {
                return BrpResponse::from_error(
                    id,
                    BrpError::ComponentNotReflectable(component_name.0.clone()),
                );
            };

            let type_registry = type_registry_arc.read();
            let type_registration = type_registry.get(*type_id).unwrap();
            let reflect_from_ptr = type_registration.data::<ReflectFromPtr>().unwrap();
            let component_ptr = entity.get_by_id(component_id).unwrap();
            let reflect = unsafe { reflect_from_ptr.as_reflect(component_ptr) };

            let serializer = ReflectSerializer::new(reflect, &type_registry);

            let output = match session.component_format {
                BrpComponentFormat::Ron => {
                    BrpComponent::Ron(ron::ser::to_string(&serializer).unwrap())
                }
                BrpComponentFormat::Json => {
                    BrpComponent::Json(serde_json::ser::to_string(&serializer).unwrap())
                }
            };

            result.components.insert(component_name.clone(), output);
        }

        results.push(result);
    }

    BrpResponse::new(id, BrpResponseContent::Query { entities: results })
}
