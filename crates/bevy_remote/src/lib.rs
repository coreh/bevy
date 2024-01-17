use std::{
    ptr::NonNull,
    sync::{Arc, RwLock},
};

use bevy_app::{App, First, MainScheduleOrder, Plugin};
use bevy_ecs::{
    component::ComponentInfo,
    entity::Entity,
    ptr::OwningPtr,
    query::QueryBuilder,
    reflect::AppTypeRegistry,
    schedule::ScheduleLabel,
    system::Resource,
    world::{EntityWorldMut, FilteredEntityRef, World},
};
use bevy_log::{debug, warn};
use bevy_reflect::{
    serde::{ReflectSerializer, TypedReflectDeserializer},
    ReflectFromPtr, TypeRegistry,
};
use bevy_utils::hashbrown::{HashMap, HashSet};
use brp::*;
use crossbeam_channel::{Receiver, Sender};
use serde::de::DeserializeSeed;

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

    pub fn component_by_name(&self, name: &str) -> Result<ComponentInfo, BrpError> {
        let internal = self.0.read().unwrap();
        if let Some(component) = (*internal).components_by_name.get(name) {
            return Ok(component.clone());
        }

        if (*internal).ambiguous_short_names.contains(name) {
            return Err(BrpError::ComponentAmbiguous(name.to_string()));
        }

        if let Some(component) = (*internal).components_by_short_name.get(name) {
            return Ok(component.clone());
        }

        Err(BrpError::ComponentNotFound(name.to_string()))
    }
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

        let response = match process_brp_request(world, &session, &request) {
            Ok(response) => response,
            Err(err) => BrpResponse::from_error(request.id, err),
        };

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
) -> Result<BrpResponse, BrpError> {
    match request.request {
        BrpRequestContent::Ping => Ok(BrpResponse::new(request.id, BrpResponseContent::Ok)),
        BrpRequestContent::Query {
            ref data,
            ref filter,
        } => process_brp_query_request(world, session, request.id, data, filter),
        BrpRequestContent::Insert {
            ref entity,
            ref components,
        } => process_brp_insert_request(world, session, request.id, entity, components),
        _ => Err(BrpError::Unimplemented),
    }
}

fn process_brp_query_request(
    world: &mut World,
    session: &RemoteSession,
    id: BrpId,
    data: &BrpQueryData,
    filter: &BrpQueryFilter,
) -> Result<BrpResponse, BrpError> {
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
            .chain(filter.when.iter())
            .map(|component_name| component_name.0.as_str()),
    );

    let mut builder = QueryBuilder::<FilteredEntityRef>::new(world);

    for component_name in &data.components {
        builder.ref_id(remote_cache.component_by_name(&component_name.0)?.id());
    }

    for component_name in &data.optional {
        let component = remote_cache.component_by_name(&component_name.0)?;
        builder.optional(|query| {
            query.ref_id(component.id());
        });
    }

    for component_name in &data.has {
        let component = remote_cache.component_by_name(&component_name.0)?;
        builder.optional(|query| {
            query.ref_id(component.id());
        });
    }

    for component_name in &filter.with {
        builder.with_id(remote_cache.component_by_name(&component_name.0)?.id());
    }

    for component_name in &filter.without {
        builder.without_id(remote_cache.component_by_name(&component_name.0)?.id());
    }

    let mut query = builder.build();

    let mut results = BrpQueryResults::default();

    let type_registry = &*type_registry_arc.read();

    for entity in query.iter(world) {
        if !process_brp_predicate(
            world,
            session,
            id,
            &entity,
            &type_registry,
            &remote_cache,
            &filter.when,
        )? {
            continue;
        }

        let mut result = BrpQueryResult {
            entity: entity.id(),
            components: HashMap::new(),
            optional: HashMap::new(),
            has: HashMap::new(),
        };

        for component_name in &data.components {
            let component = remote_cache.component_by_name(&component_name.0)?;

            result.components.insert(
                component_name.clone(),
                serialize_component(&entity, &type_registry, &component, session)?,
            );
        }

        for component_name in &data.optional {
            let component = remote_cache.component_by_name(&component_name.0)?;

            result.optional.insert(
                component_name.clone(),
                if entity.contains_id(component.id()) {
                    Some(serialize_component(
                        &entity,
                        &type_registry,
                        &component,
                        session,
                    )?)
                } else {
                    None
                },
            );
        }

        for component_name in &data.has {
            let component = remote_cache.component_by_name(&component_name.0)?;

            result
                .has
                .insert(component_name.clone(), entity.contains_id(component.id()));
        }

        results.push(result);
    }

    Ok(BrpResponse::new(
        id,
        BrpResponseContent::Query { entities: results },
    ))
}

fn process_brp_predicate(
    world: &World,
    session: &RemoteSession,
    id: BrpId,
    entity: &FilteredEntityRef<'_>,
    type_registry: &TypeRegistry,
    remote_cache: &RemoteCache,
    predicate: &BrpPredicate,
) -> Result<bool, BrpError> {
    match predicate {
        BrpPredicate::Always => Ok(true),
        BrpPredicate::And(predicates) => {
            for predicate in predicates.iter() {
                if !process_brp_predicate(
                    world,
                    session,
                    id,
                    entity,
                    type_registry,
                    remote_cache,
                    predicate,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        BrpPredicate::Or(predicates) => {
            for predicate in predicates.iter() {
                if process_brp_predicate(
                    world,
                    session,
                    id,
                    entity,
                    type_registry,
                    remote_cache,
                    predicate,
                )? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        BrpPredicate::Not(predicate) => Ok(!process_brp_predicate(
            world,
            session,
            id,
            entity,
            type_registry,
            remote_cache,
            predicate,
        )?),
        BrpPredicate::Eq(components) => {
            for (component_name, component_value) in components.iter() {
                let component = remote_cache.component_by_name(&component_name.0)?;

                if !partial_eq_component(
                    entity,
                    &type_registry,
                    &component,
                    component_value,
                    session,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

fn process_brp_insert_request(
    world: &mut World,
    session: &RemoteSession,
    id: BrpId,
    entity: &Entity,
    components: &HashMap<BrpComponentName, BrpComponent>,
) -> Result<BrpResponse, BrpError> {
    let remote_cache = world.resource::<RemoteCache>().clone();
    let type_registry_arc = (**world.resource::<AppTypeRegistry>()).clone();

    remote_cache.update_components(
        world,
        components
            .keys()
            .map(|component_name| component_name.0.as_str()),
    );

    let Some(mut entity) = world.get_entity_mut(*entity) else {
        return Err(BrpError::EntityNotFound);
    };

    let type_registry = &*type_registry_arc.read();

    for (component_name, component) in components.iter() {
        debug!("Trying to find component {:?}", component_name);
        let component_info = remote_cache.component_by_name(&component_name.0)?;
        debug!("Found component {:?}", component_info);

        deserialize_component(
            &mut entity,
            &type_registry,
            &component_info,
            component,
            session,
        )?
    }

    Ok(BrpResponse::new(id, BrpResponseContent::Ok))
}

fn serialize_component(
    entity: &FilteredEntityRef<'_>,
    type_registry: &TypeRegistry,
    component: &ComponentInfo,
    session: &RemoteSession,
) -> Result<BrpComponent, BrpError> {
    let component_id = component.id();
    let Some(type_id) = component.type_id() else {
        return Err(BrpError::ComponentMissingTypeId(
            component.name().to_string(),
        ));
    };
    let type_registration = type_registry.get(type_id);
    let Some(type_registration) = type_registration else {
        return Err(BrpError::ComponentMissingTypeRegistration(
            component.name().to_string(),
        ));
    };
    let Some(reflect_from_ptr) = type_registration.data::<ReflectFromPtr>() else {
        return Err(BrpError::ComponentMissingReflect(
            component.name().to_string(),
        ));
    };
    let Some(component_ptr) = entity.get_by_id(component_id) else {
        return Err(BrpError::ComponentInvalidAccess(
            component.name().to_string(),
        ));
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

fn deserialize_component(
    entity: &mut EntityWorldMut<'_>,
    type_registry: &TypeRegistry,
    component: &ComponentInfo,
    input: &BrpComponent,
    session: &RemoteSession,
) -> Result<(), BrpError> {
    let component_id = component.id();
    let Some(type_id) = component.type_id() else {
        return Err(BrpError::ComponentMissingTypeId(
            component.name().to_string(),
        ));
    };
    let type_registration = type_registry.get(type_id);
    let Some(type_registration) = type_registration else {
        return Err(BrpError::ComponentMissingTypeRegistration(
            component.name().to_string(),
        ));
    };
    let Some(reflect_from_ptr) = type_registration.data::<ReflectFromPtr>() else {
        return Err(BrpError::ComponentMissingReflect(
            component.name().to_string(),
        ));
    };

    let reflect_deserializer = TypedReflectDeserializer::new(&type_registration, &type_registry);
    let reflected = match input {
        BrpComponent::Json(string) => {
            if session.component_format != RemoteComponentFormat::Json {
                warn!("Received component in JSON format, but session is not set to JSON. Accepting anyway.");
            }
            let mut deserializer = serde_json::de::Deserializer::from_str(&string);
            match reflect_deserializer.deserialize(&mut deserializer) {
                Ok(r) => r,
                Err(_) => {
                    return Err(BrpError::ComponentDeserialization(
                        component.name().to_string(),
                    ));
                }
            }
        }
        BrpComponent::Ron(string) => {
            if session.component_format != RemoteComponentFormat::Ron {
                warn!("Received component in RON format, but session is not set to RON. Accepting anyway.");
            }
            let mut deserializer = ron::de::Deserializer::from_str(&string).unwrap();
            match reflect_deserializer.deserialize(&mut deserializer) {
                Ok(r) => r,
                Err(_) => {
                    return Err(BrpError::ComponentDeserialization(
                        component.name().to_string(),
                    ));
                }
            }
        }
    };

    // SAFETY: We got the `ComponentId`, `TypeId` and `Layout` from the same `ComponentInfo` so the
    // representations are compatible. We hand over the owning pointer to the world entity
    // after applying the reflected data to it, and its now the world's responsibility to
    // free the memory.
    unsafe {
        let mut owning_ptr =
            OwningPtr::new(NonNull::new(std::alloc::alloc(component.layout())).unwrap());
        let reflect = reflect_from_ptr.as_reflect_mut(owning_ptr.as_mut());
        reflect.apply(&*reflected);
        entity.insert_by_id(component_id, owning_ptr);
    };

    Ok(())
}

fn partial_eq_component(
    entity: &FilteredEntityRef<'_>,
    type_registry: &TypeRegistry,
    component: &ComponentInfo,
    input: &BrpComponent,
    session: &RemoteSession,
) -> Result<bool, BrpError> {
    let component_id = component.id();
    let Some(type_id) = component.type_id() else {
        return Err(BrpError::ComponentMissingTypeId(
            component.name().to_string(),
        ));
    };
    let type_registration = type_registry.get(type_id);
    let Some(type_registration) = type_registration else {
        return Err(BrpError::ComponentMissingTypeRegistration(
            component.name().to_string(),
        ));
    };
    let Some(reflect_from_ptr) = type_registration.data::<ReflectFromPtr>() else {
        return Err(BrpError::ComponentMissingReflect(
            component.name().to_string(),
        ));
    };

    let reflect_deserializer = TypedReflectDeserializer::new(&type_registration, &type_registry);
    let reflected = match input {
        BrpComponent::Json(string) => {
            if session.component_format != RemoteComponentFormat::Json {
                warn!("Received component in JSON format, but session is not set to JSON. Accepting anyway.");
            }
            let mut deserializer = serde_json::de::Deserializer::from_str(&string);
            match reflect_deserializer.deserialize(&mut deserializer) {
                Ok(r) => r,
                Err(_) => {
                    return Err(BrpError::ComponentDeserialization(
                        component.name().to_string(),
                    ));
                }
            }
        }
        BrpComponent::Ron(string) => {
            if session.component_format != RemoteComponentFormat::Ron {
                warn!("Received component in RON format, but session is not set to RON. Accepting anyway.");
            }
            let mut deserializer = ron::de::Deserializer::from_str(&string).unwrap();
            match reflect_deserializer.deserialize(&mut deserializer) {
                Ok(r) => r,
                Err(_) => {
                    return Err(BrpError::ComponentDeserialization(
                        component.name().to_string(),
                    ));
                }
            }
        }
    };

    // SAFETY: We got the `ComponentId`, `TypeId` and `Layout` from the same `ComponentInfo` so the
    // representations are compatible. We hand over the owning pointer to the world entity
    // after applying the reflected data to it, and its now the world's responsibility to
    // free the memory.
    unsafe {
        let reflect = match entity.get_by_id(component_id) {
            Some(ptr) => reflect_from_ptr.as_reflect(ptr),
            None => return Ok(false), // If the component is missing, it can't be equal
        };
        // Order is important here, since `reflected` is dynamic but `reflect` is potentially static
        // We want the dynamic comparison implementation to be used (So it compares “structurally”)
        // TODO: Figure out if there's a way to make both orders give matching results
        match reflected.reflect_partial_eq(reflect) {
            Some(r) => Ok(r),
            None => Err(BrpError::ComponentMissingPartialEq(
                component.name().to_string(),
            )),
        }
    }
}
