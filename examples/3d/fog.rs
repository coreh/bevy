//! This example shows how to use distance fog

use bevy::{
    pbr::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_startup_system(setup_camera_fog)
        .add_startup_system(setup_pyramid_scene)
        .add_startup_system(setup_instructions)
        .add_system(update_system)
        .run();
}

fn setup_camera_fog(mut commands: Commands) {
    commands.spawn((
        Camera3dBundle::default(),
        Fog {
            color: Color::rgba(0.05, 0.05, 0.05, 1.0),
            mode: FogMode::Linear {
                start: 5.0,
                end: 20.0,
            },
        },
    ));
}

fn setup_pyramid_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let stone = materials.add(StandardMaterial {
        base_color: Color::hex("28221B").unwrap(),
        perceptual_roughness: 1.0,
        ..default()
    });

    // pillars
    for (x, z) in &[(-1.5, -1.5), (1.5, -1.5), (1.5, 1.5), (-1.5, 1.5)] {
        commands.spawn(PbrBundle {
            mesh: meshes.add(Mesh::from(shape::Box {
                min_x: -0.5,
                max_x: 0.5,
                min_z: -0.5,
                max_z: 0.5,
                min_y: 0.0,
                max_y: 3.0,
            })),
            material: stone.clone(),
            transform: Transform::from_xyz(*x, 0.0, *z),
            ..default()
        });
    }

    // orb
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Mesh::from(shape::Icosphere::default())),
            material: materials.add(StandardMaterial {
                base_color: Color::hex("126212CC").unwrap(),
                reflectance: 1.0,
                perceptual_roughness: 0.0,
                metallic: 0.5,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            transform: Transform::from_scale(Vec3::splat(1.75))
                .with_translation(Vec3::new(0.0, 4.0, 0.0)),
            ..default()
        },
        NotShadowCaster,
        NotShadowReceiver,
    ));

    // steps
    for i in 0..50 {
        let size = i as f32 / 2.0 + 3.0;
        let y = -i as f32 / 2.0;
        commands.spawn(PbrBundle {
            mesh: meshes.add(Mesh::from(shape::Box {
                min_x: -size,
                max_x: size,
                min_z: -size,
                max_z: size,
                min_y: 0.0,
                max_y: 0.5,
            })),
            material: stone.clone(),
            transform: Transform::from_xyz(0.0, y, 0.0),
            ..default()
        });
    }

    // sky
    commands.spawn(PbrBundle {
        mesh: meshes.add(Mesh::from(shape::Box::default())),
        material: materials.add(StandardMaterial {
            base_color: Color::hex("28221B").unwrap(),
            cull_mode: None,
            perceptual_roughness: 1.0,
            ..default()
        }),
        transform: Transform::from_scale(Vec3::splat(10_000_000.0)),
        ..default()
    });

    // light
    commands.spawn(PointLightBundle {
        transform: Transform::from_xyz(0.0, 1.0, 0.0),
        point_light: PointLight {
            intensity: 1500.,
            range: 100.,
            shadows_enabled: true,
            ..default()
        },
        ..default()
    });
}

fn setup_instructions(mut commands: Commands, asset_server: Res<AssetServer>) {
    // UI camera
    commands.spawn(Camera2dBundle {
        camera: Camera {
            priority: -1,
            ..default()
        },
        ..default()
    });

    commands.spawn((TextBundle::from_section(
        "",
        TextStyle {
            font: asset_server.load("fonts/FiraMono-Medium.ttf"),
            font_size: 12.0,
            color: Color::WHITE,
        },
    )
    .with_style(Style {
        position_type: PositionType::Absolute,
        position: UiRect {
            top: Val::Px(10.0),
            left: Val::Px(10.0),
            ..default()
        },
        ..default()
    }),));
}

fn update_system(
    mut camera: Query<(&mut Fog, &mut Transform)>,
    mut text: Query<&mut Text>,
    time: Res<Time>,
    keycode: Res<Input<KeyCode>>,
) {
    let now = time.elapsed().as_millis() as f32 / 1000.0;
    let delta = time.delta().as_millis() as f32 / 1000.0;

    let (mut fog, mut transform) = camera.single_mut();
    let mut text = text.single_mut();

    // Orbit camera around pyramid
    let orbit_scale = 8.0 + (now / 10.0).sin() * 7.0;
    *transform = Transform::from_xyz(
        (now / 5.0).cos() * orbit_scale,
        12.0 - orbit_scale / 2.0,
        (now / 5.0).sin() * orbit_scale,
    )
    .looking_at(Vec3::ZERO, Vec3::Y);

    // Fog Information
    text.sections[0].value = format!("Fog Mode: {:?}\nFog Color: {:?}", fog.mode, fog.color);

    // Fog Mode Switching
    text.sections[0]
        .value
        .push_str("\n\n1 / 2 / 3 - Switch Fog Mode");

    if keycode.pressed(KeyCode::Key1) {
        fog.mode = match fog.mode {
            FogMode::Linear { start, end } => FogMode::Linear { start, end },
            _ => FogMode::Linear {
                start: 5.0,
                end: 20.0,
            },
        };
    }

    if keycode.pressed(KeyCode::Key2) {
        fog.mode = match fog.mode {
            FogMode::Exponential { density } | FogMode::ExponentialSquared { density } => {
                FogMode::Exponential { density }
            }
            _ => FogMode::Exponential { density: 0.07 },
        };
    }

    if keycode.pressed(KeyCode::Key3) {
        fog.mode = match fog.mode {
            FogMode::Exponential { density } | FogMode::ExponentialSquared { density } => {
                FogMode::ExponentialSquared { density }
            }
            _ => FogMode::ExponentialSquared { density: 0.07 },
        };
    }

    // Linear Fog Controls
    if let FogMode::Linear {
        ref mut start,
        ref mut end,
    } = &mut fog.mode
    {
        text.sections[0]
            .value
            .push_str("\nA / S - Move Start Distance\nZ / X - Move End Distance");

        if keycode.pressed(KeyCode::A) {
            *start -= delta * 3.0;
        }
        if keycode.pressed(KeyCode::S) {
            *start += delta * 3.0;
        }
        if keycode.pressed(KeyCode::Z) {
            *end -= delta * 3.0;
        }
        if keycode.pressed(KeyCode::X) {
            *end += delta * 3.0;
        }
    }

    // Exponential Fog Controls
    if let FogMode::Exponential { ref mut density } = &mut fog.mode {
        text.sections[0].value.push_str("\nA / S - Change Density");

        if keycode.pressed(KeyCode::A) {
            *density -= delta * 0.5 * *density;
            if *density < 0.0 {
                *density = 0.0;
            }
        }
        if keycode.pressed(KeyCode::S) {
            *density += delta * 0.5 * *density;
        }
    }

    // ExponentialSquared Fog Controls
    if let FogMode::ExponentialSquared { ref mut density } = &mut fog.mode {
        text.sections[0].value.push_str("\nA / S - Change Density");

        if keycode.pressed(KeyCode::A) {
            *density -= delta * 0.5 * *density;
            if *density < 0.0 {
                *density = 0.0;
            }
        }
        if keycode.pressed(KeyCode::S) {
            *density += delta * 0.5 * *density;
        }
    }

    // RGBA Controls
    text.sections[0]
        .value
        .push_str("\n\n- / = - Red\n[ / ] - Green\n; / ' - Blue\n. / ? - Alpha");

    if keycode.pressed(KeyCode::Minus) {
        let r = (fog.color.r() - 0.1 * delta).max(0.0);
        fog.color.set_r(r);
    }

    if keycode.pressed(KeyCode::Equals) {
        let r = (fog.color.r() + 0.1 * delta).min(1.0);
        fog.color.set_r(r);
    }

    if keycode.pressed(KeyCode::LBracket) {
        let r = (fog.color.g() - 0.1 * delta).max(0.0);
        fog.color.set_g(r);
    }

    if keycode.pressed(KeyCode::RBracket) {
        let r = (fog.color.g() + 0.1 * delta).min(1.0);
        fog.color.set_g(r);
    }

    if keycode.pressed(KeyCode::Semicolon) {
        let r = (fog.color.b() - 0.1 * delta).max(0.0);
        fog.color.set_b(r);
    }

    if keycode.pressed(KeyCode::Apostrophe) {
        let r = (fog.color.b() + 0.1 * delta).min(1.0);
        fog.color.set_b(r);
    }

    if keycode.pressed(KeyCode::Period) {
        let r = (fog.color.a() - 0.1 * delta).max(0.0);
        fog.color.set_a(r);
    }

    if keycode.pressed(KeyCode::Slash) {
        let r = (fog.color.a() + 0.1 * delta).min(1.0);
        fog.color.set_a(r);
    }
}
