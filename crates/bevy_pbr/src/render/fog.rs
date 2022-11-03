use bevy_app::{App, Plugin};
use bevy_asset::{load_internal_asset, HandleUntyped};
use bevy_ecs::{prelude::*, schedule::SystemLabel};
use bevy_math::Vec4;
use bevy_reflect::TypeUuid;
use bevy_render::{
    extract_component::ExtractComponentPlugin,
    render_resource::{DynamicUniformBuffer, Shader, ShaderType},
    renderer::{RenderDevice, RenderQueue},
    view::ExtractedView,
    RenderApp, RenderStage,
};

use crate::{FogFalloff, FogSettings};

/// The GPU-side representation of the fog configuration that's sent as a uniform to the shader
#[derive(Copy, Clone, ShaderType, Default, Debug)]
pub struct GpuFog {
    /// unsigned int representation of the active fog falloff mode
    mode: u32,
    /// fog color
    color: Vec4,
    /// for linear fog, `start`; for other modes of fog, `density`.
    density_or_start: f32,
    /// for linear fog, `end`; for other modes of fog, unused
    end: f32,
}

const GPU_FOG_MODE_OFF: u32 = 0;
const GPU_FOG_MODE_LINEAR: u32 = 1;
const GPU_FOG_MODE_EXPONENTIAL: u32 = 2;
const GPU_FOG_MODE_EXPONENTIAL_SQUARED: u32 = 3;

/// Metadata for fog
#[derive(Default, Resource)]
pub struct FogMeta {
    pub gpu_fogs: DynamicUniformBuffer<GpuFog>,
}

/// Prepares fog metadata and writes the fog-related uniform buffers to the GPU
pub fn prepare_fog(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut fog_meta: ResMut<FogMeta>,
    views: Query<(Entity, Option<&FogSettings>), With<ExtractedView>>,
) {
    for (entity, fog) in &views {
        let gpu_fog = if let Some(fog) = fog {
            match &fog.falloff {
                FogFalloff::Linear { start, end } => GpuFog {
                    mode: GPU_FOG_MODE_LINEAR,
                    color: fog.color.into(),
                    density_or_start: *start,
                    end: *end,
                },
                FogFalloff::Exponential { density } => GpuFog {
                    mode: GPU_FOG_MODE_EXPONENTIAL,
                    color: fog.color.into(),
                    density_or_start: *density,
                    ..Default::default()
                },
                FogFalloff::ExponentialSquared { density } => GpuFog {
                    mode: GPU_FOG_MODE_EXPONENTIAL_SQUARED,
                    color: fog.color.into(),
                    density_or_start: *density,
                    ..Default::default()
                },
            }
        } else {
            // If no fog is added to a camera, by default it's off
            GpuFog {
                mode: GPU_FOG_MODE_OFF,
                ..Default::default()
            }
        };

        // This is later read by `SetMeshViewBindGroup<I>`
        commands.entity(entity).insert(ViewFogUniformOffset {
            offset: fog_meta.gpu_fogs.push(gpu_fog),
        });
    }

    fog_meta
        .gpu_fogs
        .write_buffer(&render_device, &render_queue);
}

/// Labels for fog-related systems
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemLabel)]
pub enum RenderFogSystems {
    PrepareFog,
}

/// Inserted on each `Entity` with an `ExtractedView` to keep track of its offset
/// in the `gpu_fogs` `DynamicUniformBuffer` within `FogMeta`
#[derive(Component)]
pub struct ViewFogUniformOffset {
    pub offset: u32,
}

/// Handle for the fog WGSL Shader internal asset
pub const FOG_SHADER_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Shader::TYPE_UUID, 4913569193382610166);

/// A plugin that consolidates fog extraction, preparation and related resources/assets
pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, FOG_SHADER_HANDLE, "fog.wgsl", Shader::from_wgsl);

        app.add_plugin(ExtractComponentPlugin::<FogSettings>::default());

        if let Ok(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app.init_resource::<FogMeta>().add_system_to_stage(
                RenderStage::Prepare,
                prepare_fog.label(RenderFogSystems::PrepareFog),
            );
        }
    }
}
