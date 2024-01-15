use std::sync::{Arc, RwLock};

use bevy_app::{App, First, MainScheduleOrder, Plugin};
use bevy_ecs::{
    component::ComponentInfo,
    query::QueryBuilder,
    reflect::AppTypeRegistry,
    schedule::ScheduleLabel,
    system::Resource,
    world::{FilteredEntityRef, World},
};
use bevy_log::debug;
use bevy_reflect::{serde::ReflectSerializer, ReflectFromPtr, TypeRegistry};
use bevy_utils::hashbrown::{HashMap, HashSet};
use brp::*;
use crossbeam_channel::{Receiver, Sender};

pub mod brp;

#[cfg(feature = "http")]
pub mod http;

pub struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(Remote);

        let mut order = app.world.resource_mut::<MainScheduleOrder>();
        order.insert_after(First, Remote);

        app.add_systems(Remote, process_brp_sessions);

        app.insert_resource(RemoteSessions::default());
        app.insert_resource(RemoteCache::default());

        #[cfg(feature = "http")]
        app.add_plugins(http::HttpRemotePlugin);
    }
}

#[derive(Resource, Default, Clone)]
pub struct RemoteSessions(Arc<RwLock<Vec<RemoteSession>>>);

#[derive(Debug, Clone)]
pub struct RemoteSession {
    pub label: String,
    pub component_format: RemoteComponentFormat,
    pub request_sender: Sender<BrpRequest>,
    pub request_receiver: Receiver<BrpRequest>,
    pub response_sender: Sender<BrpResponse>,
    pub response_receiver: Receiver<BrpResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteComponentFormat {
    Json,
    Ron,
}

#[derive(Debug, Clone, Resource, Default)]
pub struct RemoteCache(Arc<RwLock<RemoteCacheInternal>>);

impl RemoteCache {
    pub fn update_components<'a>(
        &self,
        world: &mut World,
        component_names: impl IntoIterator<Item = &'a str>,
    ) {
        let mut internal = self.0.write().unwrap();
        let mut missing_components = HashSet::<String>::new();

        for component_name in component_names {
            if !(*internal).components_by_name.contains_key(component_name)
                && !(*internal)
                    .components_by_short_name
                    .contains_key(component_name)
            {
                missing_components.insert(component_name.to_string());
            }
        }

        if missing_components.is_empty() {
            // Bail early if we don't need to update anything
            return;
        }

        for component in world.components().iter() {
            let name = component.name();
            let short_name = bevy_utils::get_short_name(name);

            if missing_components.contains(name) || missing_components.contains(&short_name) {
                (*internal)
                    .components_by_name
                    .insert(name.to_string(), component.clone());

                if (*internal)
                    .components_by_short_name
                    .contains_key(&short_name)
                {
                    (*internal).ambiguous_short_names.insert(short_name.clone());
                }

                (*internal)
                    .components_by_short_name
                    .insert(short_name, component.clone());
            }
        }
    }

    pub fn component_by_name(&self, name: &str) -> Result<ComponentInfo, RemoteComponentError> {
        let internal = self.0.read().unwrap();
        if let Some(component) = (*internal).components_by_name.get(name) {
            return Ok(component.clone());
        }

        if (*internal).ambiguous_short_names.contains(name) {
            return Err(RemoteComponentError::Ambiguous);
        }

        if let Some(component) = (*internal).components_by_short_name.get(name) {
            return Ok(component.clone());
        }

        Err(RemoteComponentError::NotFound)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteComponentError {
    NotFound,
    Ambiguous,
    MissingTypeId,
    MissingTypeRegistration,
    MissingReflect,
    InvalidAccess,
}

macro_rules! try_for_component {
    ($id:expr, $name:expr, $op:expr) => {
        match $op {
            Ok(r) => r,
            Err(err) => match err {
                RemoteComponentError::NotFound => {
                    return BrpResponse::from_error(
                        $id,
                        BrpError::ComponentNotFound($name.clone()),
                    );
                }
                RemoteComponentError::Ambiguous => {
                    return BrpResponse::from_error(
                        $id,
                        BrpError::ComponentAmbiguous($name.clone()),
                    );
                }
                RemoteComponentError::MissingTypeId => {
                    return BrpResponse::from_error(
                        $id,
                        BrpError::ComponentMissingTypeId($name.clone()),
                    );
                }
                RemoteComponentError::MissingTypeRegistration => {
                    return BrpResponse::from_error(
                        $id,
                        BrpError::ComponentMissingTypeRegistration($name.clone()),
                    );
                }
                RemoteComponentError::MissingReflect => {
                    return BrpResponse::from_error(
                        $id,
                        BrpError::ComponentMissingReflect($name.clone()),
                    );
                }
                RemoteComponentError::InvalidAccess => {
                    return BrpResponse::from_error(
                        $id,
                        BrpError::ComponentInvalidAccess($name.clone()),
                    );
                }
            },
        }
    };
}

#[derive(Debug, Default)]
pub struct RemoteCacheInternal {
    pub components_by_name: HashMap<String, ComponentInfo>,
    pub components_by_short_name: HashMap<String, ComponentInfo>,
    pub ambiguous_short_names: HashSet<String>,
}

impl RemoteSessions {
    pub fn open(
        &self,
        label: impl Into<String>,
        component_format: RemoteComponentFormat,
    ) -> RemoteSession {
        let (request_sender, request_receiver) = crossbeam_channel::unbounded();
        let (response_sender, response_receiver) = crossbeam_channel::unbounded();

        let session = RemoteSession {
            label: label.into(),
            component_format,
            request_sender,
            request_receiver,
            response_sender,
            response_receiver,
        };

        let mut sessions = self.0.write().unwrap();

        for existing_session in sessions.iter() {
            assert_ne!(existing_session.label, session.label);
        }

        (*sessions).push(session.clone());

        session
    }

    pub fn close(&self, label: &str) {
        let mut sessions = self.0.write().unwrap();

        let index = (*sessions)
            .iter()
            .position(|session| session.label == label)
            .unwrap();

        sessions.remove(index);
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, ScheduleLabel)]
pub struct Remote;

fn process_brp_sessions(world: &mut World) {
    let sessions = (*world.resource::<RemoteSessions>()).clone();
    for session in sessions.0.read().unwrap().iter() {
        process_brp_session(world, session);
    }
}

fn process_brp_session(world: &mut World, session: &RemoteSession) {
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
    session: &RemoteSession,
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
    session: &RemoteSession,
    id: BrpId,
    data: &BrpQueryData,
    filter: &BrpQueryFilter,
) -> BrpResponse {
    let type_registry_arc = (**world.resource::<AppTypeRegistry>()).clone();

    let remote_cache = world.resource::<RemoteCache>().clone();

    remote_cache.update_components(
        world,
        data.components
            .iter()
            .chain(data.optional.iter())
            .chain(data.has.iter())
            .chain(filter.with.iter())
            .chain(filter.without.iter())
            .map(|component_name| component_name.0.as_str()),
    );

    let mut builder = QueryBuilder::<FilteredEntityRef>::new(world);

    for component_name in &data.components {
        builder.ref_id(
            try_for_component!(
                id,
                &component_name.0,
                remote_cache.component_by_name(&component_name.0)
            )
            .id(),
        );
    }

    for component_name in &data.optional {
        let component = try_for_component!(
            id,
            &component_name.0,
            remote_cache.component_by_name(&component_name.0)
        );
        builder.optional(|query| {
            query.ref_id(component.id());
        });
    }

    for component_name in &data.has {
        let component = try_for_component!(
            id,
            &component_name.0,
            remote_cache.component_by_name(&component_name.0)
        );
        builder.optional(|query| {
            query.ref_id(component.id());
        });
    }

    for component_name in &filter.with {
        builder.with_id(
            try_for_component!(
                id,
                &component_name.0,
                remote_cache.component_by_name(&component_name.0)
            )
            .id(),
        );
    }

    for component_name in &filter.without {
        builder.without_id(
            try_for_component!(
                id,
                &component_name.0,
                remote_cache.component_by_name(&component_name.0)
            )
            .id(),
        );
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
            let component = try_for_component!(
                id,
                &component_name.0,
                remote_cache.component_by_name(&component_name.0)
            );

            let output = try_for_component!(
                id,
                &component_name.0,
                serialize_component(&entity, &*type_registry_arc.read(), &component, session)
            );

            if component.type_id().is_none() {
                return BrpResponse::from_error(
                    id,
                    BrpError::ComponentMissingTypeId(component_name.0.clone()),
                );
            };

            result.components.insert(component_name.clone(), output);
        }

        results.push(result);
    }

    BrpResponse::new(id, BrpResponseContent::Query { entities: results })
}

fn serialize_component(
    entity: &FilteredEntityRef<'_>,
    type_registry: &TypeRegistry,
    component: &ComponentInfo,
    session: &RemoteSession,
) -> Result<BrpComponent, RemoteComponentError> {
    let component_id = component.id();
    let Some(type_id) = component.type_id() else {
        return Err(RemoteComponentError::MissingTypeId);
    };
    let type_registration = type_registry.get(type_id);
    let Some(type_registration) = type_registration else {
        return Err(RemoteComponentError::MissingTypeRegistration);
    };
    let Some(reflect_from_ptr) = type_registration.data::<ReflectFromPtr>() else {
        return Err(RemoteComponentError::MissingReflect);
    };
    let Some(component_ptr) = entity.get_by_id(component_id) else {
        return Err(RemoteComponentError::InvalidAccess);
    };

    // SAFETY: We got the `ComponentId` and `TypeId` from the same `ComponentInfo` so the
    // `TypeRegistration`, `ReflectFromPtr` and `&dyn Reflect` are all for the same type,
    // with the same memory layout.
    // We don't keep the `&dyn Reflect` we obtain around, we immediately serialize it and
    // discard it.
    // The `FilteredEntityRef` guarantees that we hold the proper access to the
    // data.
    let output = unsafe {
        let reflect = reflect_from_ptr.as_reflect(component_ptr);
        let serializer = ReflectSerializer::new(reflect, &type_registry);
        match session.component_format {
            RemoteComponentFormat::Ron => {
                BrpComponent::Ron(ron::ser::to_string(&serializer).unwrap())
            }
            RemoteComponentFormat::Json => {
                BrpComponent::Json(serde_json::ser::to_string(&serializer).unwrap())
            }
        }
    };

    Ok(output)
}
