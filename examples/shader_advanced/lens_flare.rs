//! Exploratory comparison of two lens flare techniques.
//!
//! Both modes share a prefilter pass (threshold + half-res box downsample of the
//! main texture) so that only the feature-generation math differs between them:
//!
//! * **Screen-space** — John Chapman's "pseudo" lens flare. Ghosts are produced by
//!   mirroring the frame about its centre and taking scaled samples along the
//!   vector towards the centre. Costs nothing per light and needs no scene
//!   knowledge, but cannot represent sources that are off-screen.
//! * **Analytic** — the classic light-driven ghost chain, placed along the axis
//!   from the light's projected position through the centre of the frame. Handles
//!   off-screen sources and is far more art-directable, at the cost of per-light
//!   plumbing and a real occlusion test.
//!
//! Press `Space` to switch modes, `Left`/`Right` to orbit.

use bevy::{
    camera::Hdr,
    core_pipeline::{schedule::Core3d, tonemapping::tonemapping, Core3dSystems, FullscreenShader},
    image::ToExtents,
    post_process::bloom::{bloom, Bloom},
    prelude::*,
    render::{
        extract_component::{
            ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
            UniformComponentPlugin,
        },
        render_resource::{
            binding_types::{sampler, texture_2d, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, ViewQuery},
        texture::{CachedTexture, TextureCache},
        view::ViewTarget,
        Render, RenderApp, RenderStartup, RenderSystems,
    },
};

const SHADER_ASSET_PATH: &str = "shaders/lens_flare.wgsl";

/// Format of the view target this example renders into. Matches what [`Hdr`]
/// gives us; a general implementation would specialize on the view's format.
const HDR_FORMAT: TextureFormat = TextureFormat::Rgba16Float;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, LensFlarePlugin))
        .add_systems(Startup, setup)
        .add_systems(Update, (orbit_camera, switch_mode, project_flare_source))
        .run();
}

/// The user-facing settings, which double as the shader uniform.
#[derive(Component, Clone, Copy, ExtractComponent, ShaderType)]
#[extract_app(RenderApp)]
struct LensFlare {
    /// Screen-space position of the flare source. Written by
    /// [`project_flare_source`]; only read in [`FlareMode::Analytic`].
    light_uv: Vec2,
    intensity: f32,
    threshold: f32,
    ghost_dispersal: f32,
    halo_width: f32,
    chromatic_offset: f32,
    aspect: f32,
    ghost_count: u32,
    mode: u32,
    _padding: Vec2,
}

impl Default for LensFlare {
    fn default() -> Self {
        Self {
            light_uv: Vec2::splat(0.5),
            intensity: 0.8,
            threshold: 3.0,
            ghost_dispersal: 0.35,
            halo_width: 0.45,
            chromatic_offset: 0.006,
            aspect: 1.0,
            ghost_count: 6,
            mode: FlareMode::ScreenSpace as u32,
            _padding: Vec2::ZERO,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FlareMode {
    ScreenSpace = 0,
    Analytic = 1,
}

struct LensFlarePlugin;

impl Plugin for LensFlarePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<LensFlare>::default(),
            UniformComponentPlugin::<LensFlare>::default(),
        ));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .add_systems(RenderStartup, init_lens_flare_pipelines)
            .add_systems(
                Render,
                prepare_lens_flare_textures.in_set(RenderSystems::PrepareResources),
            )
            .add_systems(
                Core3d,
                (prefilter_pass, composite_pass)
                    .chain()
                    .after(bloom)
                    .before(tonemapping)
                    .in_set(Core3dSystems::PostProcess),
            );
    }
}

/// Half-resolution HDR buffer holding the thresholded scene.
#[derive(Component)]
struct LensFlareTexture(CachedTexture);

fn prepare_lens_flare_textures(
    mut commands: Commands,
    mut texture_cache: ResMut<TextureCache>,
    render_device: Res<RenderDevice>,
    views: Query<(Entity, &bevy::render::camera::ExtractedCamera), With<LensFlare>>,
) {
    for (entity, camera) in &views {
        let Some(viewport) = camera.physical_viewport_size else {
            continue;
        };

        let texture = texture_cache.get(
            &render_device,
            TextureDescriptor {
                label: Some("lens_flare_texture"),
                size: (viewport / 2).max(UVec2::ONE).to_extents(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: HDR_FORMAT,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
        );

        commands.entity(entity).insert(LensFlareTexture(texture));
    }
}

#[derive(Resource)]
struct LensFlarePipelines {
    prefilter_layout: BindGroupLayoutDescriptor,
    composite_layout: BindGroupLayoutDescriptor,
    sampler: Sampler,
    prefilter: CachedRenderPipelineId,
    composite: CachedRenderPipelineId,
}

fn init_lens_flare_pipelines(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
    pipeline_cache: Res<PipelineCache>,
) {
    let prefilter_layout = BindGroupLayoutDescriptor::new(
        "lens_flare_prefilter_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<LensFlare>(true),
            ),
        ),
    );

    // Same first three bindings, plus the prefiltered buffer the ghosts read from.
    let composite_layout = BindGroupLayoutDescriptor::new(
        "lens_flare_composite_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                texture_2d(TextureSampleType::Float { filterable: true }),
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<LensFlare>(true),
                texture_2d(TextureSampleType::Float { filterable: true }),
            ),
        ),
    );

    // Clamped so that ghosts sampled past the frame edge fade out against the
    // border rather than wrapping bright content back into view.
    let sampler = render_device.create_sampler(&SamplerDescriptor {
        label: Some("lens_flare_sampler"),
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });

    let shader = asset_server.load(SHADER_ASSET_PATH);
    let vertex = fullscreen_shader.to_vertex_state();

    let target = |format| {
        vec![Some(ColorTargetState {
            format,
            blend: None,
            write_mask: ColorWrites::ALL,
        })]
    };

    let prefilter = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("lens_flare_prefilter_pipeline".into()),
        layout: vec![prefilter_layout.clone()],
        vertex: vertex.clone(),
        fragment: Some(FragmentState {
            shader: shader.clone(),
            entry_point: Some("prefilter".into()),
            targets: target(HDR_FORMAT),
            ..default()
        }),
        ..default()
    });

    let composite = pipeline_cache.queue_render_pipeline(RenderPipelineDescriptor {
        label: Some("lens_flare_composite_pipeline".into()),
        layout: vec![composite_layout.clone()],
        vertex,
        fragment: Some(FragmentState {
            shader,
            entry_point: Some("composite".into()),
            targets: target(HDR_FORMAT),
            ..default()
        }),
        ..default()
    });

    commands.insert_resource(LensFlarePipelines {
        prefilter_layout,
        composite_layout,
        sampler,
        prefilter,
        composite,
    });
}

fn prefilter_pass(
    view: ViewQuery<(
        &ViewTarget,
        &LensFlareTexture,
        &DynamicUniformIndex<LensFlare>,
    )>,
    pipelines: Option<Res<LensFlarePipelines>>,
    pipeline_cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<LensFlare>>,
    mut ctx: RenderContext,
) {
    let Some(pipelines) = pipelines else {
        return;
    };
    let (view_target, flare_texture, settings_index) = view.into_inner();

    let Some(pipeline) = pipeline_cache.get_render_pipeline(pipelines.prefilter) else {
        return;
    };
    let Some(settings_binding) = uniforms.uniforms().binding() else {
        return;
    };

    let bind_group = ctx.render_device().create_bind_group(
        "lens_flare_prefilter_bind_group",
        &pipeline_cache.get_bind_group_layout(&pipelines.prefilter_layout),
        &BindGroupEntries::sequential((
            view_target.main_texture_view(),
            &pipelines.sampler,
            settings_binding.clone(),
        )),
    );

    let mut render_pass = ctx
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("lens_flare_prefilter_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &flare_texture.0.default_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

    render_pass.set_pipeline(pipeline);
    render_pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
    render_pass.draw(0..3, 0..1);
}

fn composite_pass(
    view: ViewQuery<(
        &ViewTarget,
        &LensFlareTexture,
        &DynamicUniformIndex<LensFlare>,
    )>,
    pipelines: Option<Res<LensFlarePipelines>>,
    pipeline_cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<LensFlare>>,
    mut ctx: RenderContext,
) {
    let Some(pipelines) = pipelines else {
        return;
    };
    let (view_target, flare_texture, settings_index) = view.into_inner();

    let Some(pipeline) = pipeline_cache.get_render_pipeline(pipelines.composite) else {
        return;
    };
    let Some(settings_binding) = uniforms.uniforms().binding() else {
        return;
    };

    let post_process = view_target.post_process_write();

    let bind_group = ctx.render_device().create_bind_group(
        "lens_flare_composite_bind_group",
        &pipeline_cache.get_bind_group_layout(&pipelines.composite_layout),
        &BindGroupEntries::sequential((
            post_process.source,
            &pipelines.sampler,
            settings_binding.clone(),
            &flare_texture.0.default_view,
        )),
    );

    let mut render_pass = ctx
        .command_encoder()
        .begin_render_pass(&RenderPassDescriptor {
            label: Some("lens_flare_composite_pass"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: post_process.destination,
                depth_slice: None,
                resolve_target: None,
                ops: Operations::default(),
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

    render_pass.set_pipeline(pipeline);
    render_pass.set_bind_group(0, &bind_group, &[settings_index.index()]);
    render_pass.draw(0..3, 0..1);
}

/// The bright emitter the analytic mode tracks.
#[derive(Component)]
struct FlareSource;

fn mode_name(mode: u32) -> &'static str {
    if mode == FlareMode::Analytic as u32 {
        "analytic"
    } else {
        "screen-space"
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mode = if std::env::var("LENS_FLARE_MODE").as_deref() == Ok("analytic") {
        FlareMode::Analytic as u32
    } else {
        FlareMode::ScreenSpace as u32
    };

    commands.spawn((
        Camera3d::default(),
        Hdr,
        Transform::from_xyz(0.0, 1.5, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
        Bloom::NATURAL,
        // `LENS_FLARE_MODE=analytic` selects the other mode at startup, so that
        // screenshots of both can be captured without keyboard input.
        LensFlare { mode, ..default() },
    ));

    // A very bright emissive sphere standing in for the sun.
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(0.6))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::BLACK,
            emissive: LinearRgba::rgb(30.0, 26.0, 18.0),
            ..default()
        })),
        Transform::from_xyz(-3.0, 2.5, -6.0),
        FlareSource,
    ));

    // Geometry to occlude the source as the camera orbits.
    for (x, y, z) in [(-1.6, 1.0, -2.0), (1.4, 0.4, -1.0), (0.2, 1.8, -4.0)] {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(1.2, 1.2, 1.2))),
            MeshMaterial3d(materials.add(Color::srgb(0.35, 0.37, 0.42))),
            Transform::from_xyz(x, y, z),
        ));
    }

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(40.0, 40.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.2, 0.21, 0.24))),
        Transform::from_xyz(0.0, -0.6, 0.0),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 4_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(-3.0, 2.5, -6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Text::new(format!(
            "Space: switch mode\nLeft/Right: orbit\n\nMode: {}",
            mode_name(mode)
        )),
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            ..default()
        },
    ));
}

fn orbit_camera(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut camera: Single<&mut Transform, With<Camera3d>>,
) {
    let mut angle = 0.0;
    if keys.pressed(KeyCode::ArrowLeft) {
        angle += 1.0;
    }
    if keys.pressed(KeyCode::ArrowRight) {
        angle -= 1.0;
    }

    if angle != 0.0 {
        let rotation = Quat::from_rotation_y(angle * 0.8 * time.delta_secs());
        camera.translation = rotation * camera.translation;
        camera.look_at(Vec3::ZERO, Vec3::Y);
    }
}

fn switch_mode(
    keys: Res<ButtonInput<KeyCode>>,
    mut flare: Single<&mut LensFlare>,
    mut text: Single<&mut Text>,
) {
    if !keys.just_pressed(KeyCode::Space) {
        return;
    }

    let (mode, name) = if flare.mode == FlareMode::ScreenSpace as u32 {
        (FlareMode::Analytic, "analytic")
    } else {
        (FlareMode::ScreenSpace, "screen-space")
    };

    flare.mode = mode as u32;
    text.0 = format!("Space: switch mode\nLeft/Right: orbit\n\nMode: {name}");
}

/// Projects the flare source into screen space for the analytic mode, and keeps
/// the aspect ratio in sync so that ghosts stay circular.
fn project_flare_source(
    camera: Single<(&Camera, &GlobalTransform, &mut LensFlare)>,
    source: Single<&GlobalTransform, With<FlareSource>>,
) {
    let (camera, camera_transform, mut flare) = camera.into_inner();

    let Some(size) = camera.logical_viewport_size() else {
        return;
    };
    flare.aspect = size.x / size.y;

    if let Ok(viewport) = camera.world_to_viewport(camera_transform, source.translation()) {
        flare.light_uv = viewport / size;
    }
}
