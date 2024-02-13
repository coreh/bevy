//! Allows Bevy to communicate with remote clients over a network or other transports.
//!
//! Currently, the following transports are supported:
//!
//! - HTTP (via the `http` feature)
//! - WASM (enabled by default on the `wasm32-unknown-unknown` target)

use std::any::TypeId;

use bevy_app::{App, First, MainScheduleOrder, Plugin};
use bevy_ecs::{
    component::ComponentId, reflect::AppTypeRegistry, schedule::ScheduleLabel, world::World,
};
use brp::*;
pub use session::{RemoteSession, RemoteSessions};

pub mod brp;
pub mod session;

mod data;

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
    world.resource::<RemoteSessions>().clone().process(world);
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
