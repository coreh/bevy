//! Allows Bevy to communicate with remote clients over a network or other transports.
//!
//! Currently, the following transports are supported:
//!
//! - HTTP (via the `http` feature)
//! - WASM (enabled by default on the `wasm32-unknown-unknown` target)

use std::any::TypeId;

use bevy_app::{App, First, MainScheduleOrder, Plugin};
use bevy_asset::{ReflectAsset, ReflectHandle};
use bevy_ecs::{
    component::ComponentId,
    ptr::Ptr,
    reflect::{AppTypeRegistry, ReflectComponent},
    schedule::ScheduleLabel,
    world::{EntityRef, EntityWorldMut, FilteredEntityRef, World},
};
use bevy_log::warn;
use bevy_reflect::{
    serde::{ReflectSerializer, TypedReflectDeserializer},
    std_traits::ReflectDefault,
    Reflect, ReflectFromPtr, TypeRegistry,
};
use brp::*;
use serde::de::DeserializeSeed;
use session::{RemoteSession, RemoteSessions};

pub mod brp;
pub mod session;

#[cfg(feature = "http")]
pub mod http;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

pub struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        app.init_schedule(Remote);

        let mut order = app.world.resource_mut::<MainScheduleOrder>();
        order.insert_after(First, Remote);

        app.add_systems(Remote, process_brp_sessions);

        app.insert_resource(RemoteSessions::default());

        #[cfg(feature = "http")]
        app.add_plugins(http::HttpRemotePlugin);

        #[cfg(target_arch = "wasm32")]
        app.add_plugins(wasm::WasmRemotePlugin);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteSerializationFormat {
    Json,
    Json5,
    Ron,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, ScheduleLabel)]
pub struct Remote;

fn process_brp_sessions(world: &mut World) {
    let sessions = (*world.resource::<RemoteSessions>()).clone();
    for session in sessions.0.read().unwrap().iter() {
        session.process(world);
    }
}

fn type_and_component_id_for_name(
    world: &World,
    component_name: &String,
) -> Result<(TypeId, ComponentId), BrpError> {
    let registry = world.resource::<AppTypeRegistry>().read();

    let type_id = registry
        .get_with_type_path(component_name)
        .or_else(|| registry.get_with_short_type_path(component_name))
        .ok_or_else(|| BrpError::MissingTypeRegistration(component_name.clone()))?
        .type_id();

    let component_id = world
        .components()
        .get_id(type_id)
        .ok_or_else(|| BrpError::MissingComponentId(component_name.clone()))?;

    Ok((type_id, component_id))
}

fn type_id_for_name(world: &World, component_name: &String) -> Result<TypeId, BrpError> {
    Ok(type_and_component_id_for_name(world, component_name)?.0)
}

fn component_id_for_name(world: &World, component_name: &String) -> Result<ComponentId, BrpError> {
    Ok(type_and_component_id_for_name(world, component_name)?.1)
}

fn process_brp_predicate(
    world: &World,
    session: &RemoteSession,
    id: BrpId,
    entity: &FilteredEntityRef<'_>,
    predicate: &BrpPredicate,
) -> Result<bool, BrpError> {
    match predicate {
        BrpPredicate::Always => Ok(true),
        BrpPredicate::All(predicates) => {
            for predicate in predicates.iter() {
                if !process_brp_predicate(world, session, id, entity, predicate)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        BrpPredicate::Any(predicates) => {
            for predicate in predicates.iter() {
                if process_brp_predicate(world, session, id, entity, predicate)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        BrpPredicate::Not(predicate) => Ok(!process_brp_predicate(
            world, session, id, entity, predicate,
        )?),
        BrpPredicate::PartialEq(components) => {
            for (component_name, component_value) in components.iter() {
                if !component_value.try_partial_eq_entity_component(
                    world,
                    entity,
                    component_name,
                    session,
                )? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
    }
}

enum AnyEntityRef<'a> {
    EntityRef(&'a EntityRef<'a>),
    FilteredEntityRef(&'a FilteredEntityRef<'a>),
}

impl<'w> AnyEntityRef<'w> {
    fn get_by_id(&self, id: ComponentId) -> Option<Ptr<'w>> {
        match self {
            AnyEntityRef::EntityRef(entity) => entity.get_by_id(id),
            AnyEntityRef::FilteredEntityRef(entity) => entity.get_by_id(id),
        }
    }
}

impl BrpSerializedData {
    fn try_from_entity_component(
        world: &World,
        entity: &AnyEntityRef<'_>,
        component_name: &BrpComponentName,
        serialization_format: RemoteSerializationFormat,
    ) -> Result<BrpSerializedData, BrpError> {
        let type_registry = world.resource::<AppTypeRegistry>().read();
        let (type_id, component_id) = type_and_component_id_for_name(world, component_name)?;
        let type_registration = type_registry.get(type_id);
        let Some(type_registration) = type_registration else {
            return Err(BrpError::MissingTypeRegistration(component_name.clone()));
        };
        let Some(reflect_from_ptr) = type_registration.data::<ReflectFromPtr>() else {
            return Err(BrpError::MissingReflect(component_name.clone()));
        };
        let Some(component_ptr) = entity.get_by_id(component_id) else {
            return Err(BrpError::ComponentInvalidAccess(component_name.clone()));
        };

        // SAFETY: We got the `ComponentId` and `TypeId` from the same `ComponentInfo` so the
        // `TypeRegistration`, `ReflectFromPtr` and `&dyn Reflect` are all for the same type,
        // with the same memory layout.
        // We don't keep the `&dyn Reflect` we obtain around, we immediately serialize it and
        // discard it.
        // The `FilteredEntityRef` guarantees that we hold the proper access to the
        // data.
        unsafe {
            let reflect = reflect_from_ptr.as_reflect(component_ptr);

            Self::try_from_reflect(reflect, &*type_registry, serialization_format)
        }
    }

    fn try_from_asset(
        world: &World,
        name: &BrpAssetName,
        handle: &BrpSerializedData,
        serialization_format: RemoteSerializationFormat,
    ) -> Result<BrpSerializedData, BrpError> {
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

        let Some(reflect_asset) = asset_type_registration.data::<ReflectAsset>() else {
            return Err(BrpError::MissingTypeRegistration(name.clone()));
        };

        let reflected =
            handle.try_deserialize(world, type_registration, name, serialization_format)?;

        let Some(reflect_default) = type_registration.data::<ReflectDefault>() else {
            return Err(BrpError::MissingDefault(name.clone()));
        };

        let mut reflect = reflect_default.default();

        reflect.apply(&*reflected);

        let untyped_handle = reflect_handle
            .downcast_handle_untyped(reflect.as_any())
            .unwrap();

        let Some(asset_reflect) = reflect_asset.get(world, untyped_handle) else {
            return Err(BrpError::AssetNotFound(name.clone()));
        };

        Self::try_from_reflect(asset_reflect, type_registry, serialization_format)
    }

    fn try_from_reflect(
        reflect: &dyn Reflect,
        type_registry: &TypeRegistry,
        serialization_format: RemoteSerializationFormat,
    ) -> Result<BrpSerializedData, BrpError> {
        let serializer = ReflectSerializer::new(reflect, type_registry);
        Ok(match serialization_format {
            RemoteSerializationFormat::Ron => BrpSerializedData::Ron(
                ron::ser::to_string(&serializer)
                    .map_err(|e| BrpError::Serialization(e.to_string()))?,
            ),
            RemoteSerializationFormat::Json5 => BrpSerializedData::Json5(
                json5::to_string(&serializer)
                    .map_err(|e| BrpError::Serialization(e.to_string()))?,
            ),
            RemoteSerializationFormat::Json => BrpSerializedData::Json(
                serde_json::ser::to_string(&serializer)
                    .map_err(|e| BrpError::Serialization(e.to_string()))?,
            ),
        })
    }

    fn try_partial_eq_entity_component(
        &self,
        world: &World,
        entity: &FilteredEntityRef<'_>,
        component_name: &BrpComponentName,
        session: &RemoteSession,
    ) -> Result<bool, BrpError> {
        let type_registry = world.resource::<AppTypeRegistry>().read();
        let (type_id, component_id) = type_and_component_id_for_name(world, component_name)?;
        let type_registration = type_registry.get(type_id);
        let Some(type_registration) = type_registration else {
            return Err(BrpError::MissingTypeRegistration(component_name.clone()));
        };
        let Some(reflect_from_ptr) = type_registration.data::<ReflectFromPtr>() else {
            return Err(BrpError::MissingReflect(component_name.clone()));
        };

        let reflected = self.try_deserialize(
            world,
            type_registration,
            component_name,
            session.serialization_format,
        )?;

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
                None => Err(BrpError::MissingPartialEq(component_name.clone())),
            }
        }
    }

    fn try_deserialize(
        &self,
        world: &World,
        type_registration: &bevy_reflect::TypeRegistration,
        component_name: &String,
        serialization_format: RemoteSerializationFormat,
    ) -> Result<Box<dyn Reflect>, BrpError> {
        let type_registry = world.resource::<AppTypeRegistry>().read();
        let reflect_deserializer =
            TypedReflectDeserializer::new(&type_registration, &type_registry);
        let reflected = match self {
            BrpSerializedData::Json(string) => {
                if serialization_format != RemoteSerializationFormat::Json {
                    warn!("Received component in JSON format, but session is not set to JSON. Accepting anyway.");
                }
                let mut deserializer = serde_json::de::Deserializer::from_str(&string);
                match reflect_deserializer.deserialize(&mut deserializer) {
                    Ok(r) => r,
                    Err(e) => {
                        return Err(BrpError::Deserialization {
                            type_name: component_name.clone(),
                            error: e.to_string(),
                        });
                    }
                }
            }
            BrpSerializedData::Json5(string) => {
                if serialization_format != RemoteSerializationFormat::Json5 {
                    warn!("Received component in JSON5 format, but session is not set to JSON5. Accepting anyway.");
                }
                let mut deserializer = json5::Deserializer::from_str(&string).unwrap();
                match reflect_deserializer.deserialize(&mut deserializer) {
                    Ok(r) => r,
                    Err(e) => {
                        return Err(BrpError::Deserialization {
                            type_name: component_name.clone(),
                            error: e.to_string(),
                        });
                    }
                }
            }
            BrpSerializedData::Ron(string) => {
                if serialization_format != RemoteSerializationFormat::Ron {
                    warn!("Received component in RON format, but session is not set to RON. Accepting anyway.");
                }
                let mut deserializer = ron::de::Deserializer::from_str(&string).unwrap();
                match reflect_deserializer.deserialize(&mut deserializer) {
                    Ok(r) => r,
                    Err(e) => {
                        return Err(BrpError::Deserialization {
                            type_name: component_name.clone(),
                            error: e.to_string(),
                        });
                    }
                }
            }
            BrpSerializedData::Default => {
                let Some(reflect_default) = type_registration.data::<ReflectDefault>() else {
                    return Err(BrpError::MissingDefault(component_name.clone()));
                };
                reflect_default.default()
            }
            BrpSerializedData::Unserializable => {
                return Err(BrpError::Deserialization {
                    type_name: component_name.clone(),
                    error: "Data is unserializable".to_string(),
                })
            }
        };
        Ok(reflected)
    }

    fn try_insert_component(
        &self,
        entity: &mut EntityWorldMut<'_>,
        component_name: &BrpComponentName,
        serialization_format: RemoteSerializationFormat,
    ) -> Result<(), BrpError> {
        let (reflect_component, reflect, type_registry_arc) = {
            let world = entity.world();
            let type_id = type_id_for_name(world, component_name)?;
            let type_registry_arc = world.resource::<AppTypeRegistry>();
            let type_registry = type_registry_arc.read();
            let type_registration = type_registry.get(type_id);
            let Some(type_registration) = type_registration else {
                return Err(BrpError::MissingTypeRegistration(component_name.clone()));
            };

            let reflected = self.try_deserialize(
                world,
                type_registration,
                component_name,
                serialization_format,
            )?;

            let Some(reflect_default) = type_registration.data::<ReflectDefault>() else {
                return Err(BrpError::MissingDefault(component_name.clone()));
            };

            let Some(reflect_component) = type_registration.data::<ReflectComponent>() else {
                return Err(BrpError::MissingReflect(component_name.clone()));
            };

            let mut reflect = reflect_default.default();

            reflect.apply(&*reflected);

            (
                reflect_component.clone(),
                reflect,
                type_registry_arc.clone(),
            )
        };

        reflect_component.insert(entity, &*reflect, &*type_registry_arc.read());

        Ok(())
    }
}
