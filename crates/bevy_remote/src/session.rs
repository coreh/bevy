use std::sync::{Arc, RwLock};

use bevy_asset::{ReflectAsset, ReflectHandle};
use bevy_ecs::{
    entity::Entity,
    query::QueryBuilder,
    reflect::AppTypeRegistry,
    system::Resource,
    world::{FilteredEntityRef, World},
};
use bevy_log::debug;
use bevy_reflect::std_traits::ReflectDefault;
use bevy_utils::HashMap;
use crossbeam_channel::{Receiver, Sender};

use crate::{
    component_id_for_name, AnyEntityRef, BrpAssetName, BrpComponentName, BrpError, BrpId,
    BrpPredicate, BrpQueryData, BrpQueryFilter, BrpQueryResult, BrpQueryResults, BrpRequest,
    BrpRequestContent, BrpResponse, BrpResponseContent, BrpSerializedData,
    RemoteSerializationFormat,
};

#[derive(Resource, Default, Clone)]
pub struct RemoteSessions(Arc<RwLock<Vec<RemoteSession>>>);

#[derive(Debug, Clone)]
pub struct RemoteSession {
    pub label: String,
    pub serialization_format: RemoteSerializationFormat,
    pub request_sender: Sender<BrpRequest>,
    pub request_receiver: Receiver<BrpRequest>,
    pub response_sender: Sender<BrpResponse>,
    pub response_receiver: Receiver<BrpResponse>,
}

impl RemoteSessions {
    pub fn open(
        &self,
        label: impl Into<String>,
        serialization_format: RemoteSerializationFormat,
    ) -> RemoteSession {
        let (request_sender, request_receiver) = crossbeam_channel::unbounded();
        let (response_sender, response_receiver) = crossbeam_channel::unbounded();

        let session = RemoteSession {
            label: label.into(),
            serialization_format,
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

    pub(crate) fn process(&self, world: &mut World) {
        let sessions = self.0.read().unwrap();

        for session in sessions.iter() {
            session.process(world);
        }
    }
}

impl RemoteSession {
    pub(crate) fn process(&self, world: &mut World) {
        loop {
            let request = match self.request_receiver.try_recv() {
                Ok(request) => request,
                Err(err) => match err {
                    crossbeam_channel::TryRecvError::Empty => break, // no more requests for now
                    crossbeam_channel::TryRecvError::Disconnected => {
                        panic!("BRP request channel disconnected")
                    }
                },
            };

            let response = match self.process_request(world, &request) {
                Ok(response) => response,
                Err(err) => BrpResponse::from_error(request.id, err),
            };

            match self.response_sender.send(response) {
                Ok(_) => {}
                Err(err) => {
                    panic!("BRP response channel disconnected: {:?}", err)
                }
            }

            debug!("Received {:?} from session {:?}", request, self.label);
        }
    }

    fn process_request(
        &self,
        world: &mut World,
        request: &BrpRequest,
    ) -> Result<BrpResponse, BrpError> {
        match request.request {
            BrpRequestContent::Ping => Ok(BrpResponse::new(request.id, BrpResponseContent::Ok)),
            BrpRequestContent::GetEntity {
                entity,
                ref data,
                ref filter,
            } => self.process_get_entity_request(world, request.id, data, filter, entity),
            BrpRequestContent::QueryEntities {
                ref data,
                ref filter,
            } => self.process_query_request(world, request.id, data, filter),
            BrpRequestContent::InsertComponent {
                ref entity,
                ref components,
            } => self.process_insert_request(world, request.id, entity, components),
            BrpRequestContent::GetAsset {
                ref name,
                ref handle,
            } => self.process_get_asset_request(world, request.id, name, handle),
            BrpRequestContent::InsertAsset {
                ref name,
                ref handle,
                ref asset,
            } => self.process_insert_asset_request(world, request.id, name, handle, asset),
            _ => Err(BrpError::Unimplemented),
        }
    }

    fn process_get_entity_request(
        &self,
        world: &mut World,
        id: BrpId,
        data: &BrpQueryData,
        filter: &BrpQueryFilter,
        entity: Entity,
    ) -> Result<BrpResponse, BrpError> {
        let query_response =
            self.process_get_or_query_request(world, id, data, filter, Some(entity));

        match query_response {
            Ok(BrpResponse {
                response: BrpResponseContent::QueryEntities { mut entities },
                ..
            }) => {
                if entities.len() != 1 {
                    return Err(BrpError::EntityNotFound);
                }

                Ok(BrpResponse::new(
                    id,
                    BrpResponseContent::GetEntity {
                        entity: entities.pop().unwrap(),
                    },
                ))
            }
            other => other,
        }
    }

    fn process_query_request(
        &self,
        world: &mut World,
        id: BrpId,
        data: &BrpQueryData,
        filter: &BrpQueryFilter,
    ) -> Result<BrpResponse, BrpError> {
        self.process_get_or_query_request(world, id, data, filter, None)
    }

    fn process_get_or_query_request(
        &self,
        world: &mut World,
        id: BrpId,
        data: &BrpQueryData,
        filter: &BrpQueryFilter,
        entity: Option<Entity>,
    ) -> Result<BrpResponse, BrpError> {
        let mut builder = QueryBuilder::<FilteredEntityRef>::new(world);

        let fetch_all_components = data.components.len() == 1 && data.components[0] == "*";

        if !fetch_all_components {
            for component_name in &data.components {
                builder.ref_id(component_id_for_name(builder.world(), component_name)?);
            }
        }

        for component_name in &data.optional {
            let component_id = component_id_for_name(builder.world(), component_name)?;
            builder.optional(|query| {
                query.ref_id(component_id);
            });
        }

        for component_name in &data.has {
            let component_id = component_id_for_name(builder.world(), component_name)?;
            builder.optional(|query| {
                query.ref_id(component_id);
            });
        }

        for component_name in &filter.with {
            let component_id = component_id_for_name(builder.world(), component_name)?;
            builder.with_id(component_id);
        }

        for component_name in &filter.without {
            let component_id = component_id_for_name(builder.world(), component_name)?;
            builder.without_id(component_id);
        }

        for component_name in filter.when.iter() {
            let component_id = component_id_for_name(builder.world(), component_name)?;
            builder.optional(|query| {
                query.ref_id(component_id);
            });
        }

        let mut query = builder.build();

        let mut results = BrpQueryResults::default();

        let (mut _1, mut _2);
        let entities: &mut dyn Iterator<Item = FilteredEntityRef> = if let Some(entity) = entity {
            _1 = query.get(world, entity).into_iter();
            &mut _1
        } else {
            _2 = query.iter(world).into_iter();
            &mut _2
        };

        for entity in entities {
            if !self.try_process_predicate(world, &entity, &filter.when)? {
                continue;
            }

            let mut result = BrpQueryResult {
                entity: entity.id(),
                components: HashMap::new(),
                optional: HashMap::new(),
                has: HashMap::new(),
            };

            if !fetch_all_components {
                for component_name in &data.components {
                    result.components.insert(
                        component_name.clone(),
                        BrpSerializedData::try_from_entity_component(
                            world,
                            &AnyEntityRef::FilteredEntityRef(&entity),
                            component_name,
                            self.serialization_format,
                        )?,
                    );
                }
            }

            for component_name in &data.optional {
                let component_id = component_id_for_name(world, component_name)?;
                result.optional.insert(
                    component_name.clone(),
                    if entity.contains_id(component_id) {
                        Some(BrpSerializedData::try_from_entity_component(
                            world,
                            &AnyEntityRef::FilteredEntityRef(&entity),
                            component_name,
                            self.serialization_format,
                        )?)
                    } else {
                        None
                    },
                );
            }

            for component_name in &data.has {
                let component_id = component_id_for_name(world, component_name)?;

                result
                    .has
                    .insert(component_name.clone(), entity.contains_id(component_id));
            }

            results.push(result);
        }

        if fetch_all_components {
            for result in &mut results {
                let entity = world.entity(result.entity);
                for component in world.components().iter() {
                    let component_id = component.id();
                    let component_name = component.name().to_string();
                    if entity.contains_id(component_id) {
                        match BrpSerializedData::try_from_entity_component(
                            world,
                            &AnyEntityRef::EntityRef(&entity),
                            &component_name,
                            self.serialization_format,
                        ) {
                            Ok(serialized) => {
                                result.components.insert(component_name, serialized);
                            }
                            Err(
                                BrpError::MissingTypeRegistration(_)
                                | BrpError::MissingReflect(_)
                                | BrpError::MissingTypeId(_)
                                | BrpError::Serialization(_),
                            ) => {
                                result
                                    .components
                                    .insert(component_name, BrpSerializedData::Unserializable);
                            }
                            Err(err) => return Err(err),
                        }
                    }
                }
            }
        }

        Ok(BrpResponse::new(
            id,
            BrpResponseContent::QueryEntities { entities: results },
        ))
    }

    fn process_insert_request(
        &self,
        world: &mut World,
        id: BrpId,
        entity: &Entity,
        components: &HashMap<BrpComponentName, BrpSerializedData>,
    ) -> Result<BrpResponse, BrpError> {
        let Some(mut entity) = world.get_entity_mut(*entity) else {
            return Err(BrpError::EntityNotFound);
        };

        for (component_name, component) in components.iter() {
            component.try_insert_component(
                &mut entity,
                component_name,
                self.serialization_format,
            )?
        }

        Ok(BrpResponse::new(id, BrpResponseContent::Ok))
    }

    fn process_get_asset_request(
        &self,
        world: &mut World,
        id: BrpId,
        name: &BrpAssetName,
        handle: &BrpSerializedData,
    ) -> Result<BrpResponse, BrpError> {
        let output =
            BrpSerializedData::try_from_asset(world, name, handle, self.serialization_format)?;

        Ok(BrpResponse::new(
            id,
            BrpResponseContent::GetAsset {
                name: name.clone(),
                handle: handle.clone(),
                asset: output,
            },
        ))
    }

    fn process_insert_asset_request(
        &self,
        world: &mut World,
        id: BrpId,
        name: &BrpAssetName,
        handle: &BrpSerializedData,
        data: &BrpSerializedData,
    ) -> Result<BrpResponse, BrpError> {
        let type_registry_arc = (**world.resource::<AppTypeRegistry>()).clone();

        let type_registry = &*type_registry_arc.read();

        let Some(type_registration) = type_registry.get_with_type_path(name) else {
            return Err(BrpError::MissingTypeRegistration(name.clone()));
        };

        let Some(reflect_handle) = type_registration.data::<ReflectHandle>() else {
            return Err(BrpError::AssetNotFound(name.clone()));
        };

        let Some(asset_type_registration) = type_registry.get(reflect_handle.asset_type_id())
        else {
            return Err(BrpError::MissingTypeRegistration(name.clone()));
        };

        let asset_name = asset_type_registration.type_info().type_path().to_string();

        let Some(reflect_asset) = asset_type_registration.data::<ReflectAsset>() else {
            return Err(BrpError::MissingTypeRegistration(name.clone()));
        };

        let reflected =
            handle.try_deserialize(world, type_registration, name, self.serialization_format)?;

        let Some(reflect_default) = type_registration.data::<ReflectDefault>() else {
            return Err(BrpError::MissingDefault(name.clone()));
        };

        let mut reflect = reflect_default.default();
        reflect.apply(&*reflected);

        let untyped_handle = reflect_handle
            .downcast_handle_untyped(reflect.as_any())
            .unwrap();

        let asset_reflected = data.try_deserialize(
            world,
            asset_type_registration,
            &asset_name,
            self.serialization_format,
        )?;

        reflect_asset.insert(world, untyped_handle, &*asset_reflected);

        Ok(BrpResponse::new(id, BrpResponseContent::Ok))
    }

    fn try_process_predicate(
        &self,
        world: &World,
        entity: &FilteredEntityRef<'_>,
        predicate: &BrpPredicate,
    ) -> Result<bool, BrpError> {
        match predicate {
            BrpPredicate::Always => Ok(true),
            BrpPredicate::All(predicates) => {
                for predicate in predicates.iter() {
                    if !self.try_process_predicate(world, entity, predicate)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            BrpPredicate::Any(predicates) => {
                for predicate in predicates.iter() {
                    if self.try_process_predicate(world, entity, predicate)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            BrpPredicate::Not(predicate) => {
                Ok(!self.try_process_predicate(world, entity, predicate)?)
            }
            BrpPredicate::PartialEq(components) => {
                for (component_name, component_value) in components.iter() {
                    if !component_value.try_partial_eq_entity_component(
                        world,
                        entity,
                        component_name,
                        self.serialization_format,
                    )? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
        }
    }
}
