use std::f32::consts::FRAC_PI_2;
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

#[test]
fn test_wgsl_validation() {
    let mut source = include_str!("../assets/shaders/fog.wgsl").to_string();
    source = source.replace("#{MATERIAL_BIND_GROUP}", "3");
    source = source.replace(
        "#import bevy_pbr::forward_io::VertexOutput",
        "struct VertexOutput { @builtin(position) position: vec4<f32>, @location(0) world_position: vec4<f32>, @location(1) world_normal: vec3<f32>, };",
    );

    let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| {
        panic!("WGSL parse error: {}", e.emit_to_string(&source));
    });

    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    if let Err(e) = validator.validate(&module) {
        panic!("Naga validation error: {:?}", e);
    }
    println!("Naga validation SUCCESSFUL!");
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Mil-Sim - Vision Cone & Fog of War".to_string(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(MaterialPlugin::<FogMaterial>::default())
        .add_systems(Startup, setup)
        .add_systems(Update, update_soldier_and_fog)
        .run();
}

#[derive(Component)]
struct Soldier {
    facing_dir: Vec2,
}

#[derive(Component)]
struct Building;

#[derive(Component)]
struct FogOverlay;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
struct FogMaterial {
    #[uniform(0)]
    soldier_pos: Vec2,
    #[uniform(0)]
    soldier_dir: Vec2,
    #[uniform(0)]
    vision_range: f32,
    #[uniform(0)]
    vision_angle: f32,
    #[uniform(0)]
    inner_radius: f32,
    #[uniform(0)]
    fog_alpha: f32,
    #[uniform(0)]
    building_count: u32,
    #[uniform(0)]
    _pad0: f32,
    #[uniform(0)]
    _pad1: f32,
    #[uniform(0)]
    _pad2: f32,
    #[uniform(0)]
    buildings: [Vec4; 16],
}

impl Material for FogMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/fog.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut fog_materials: ResMut<Assets<FogMaterial>>,
) {
    // Top-down camera (orthographic projection looking straight down)
    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::FixedVertical {
                viewport_height: 48.0,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 50.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z),
    ));

    // Sun / Directional Light
    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.8, 0.5, 0.0)),
    ));

    // Ground plane
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(60.0, 60.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.22, 0.35, 0.22))),
    ));

    // Common white building material
    let building_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.95, 0.95),
        perceptual_roughness: 0.85,
        ..default()
    });

    // 4 Buildings in a realistic compound layout (flanking a central access road/plaza)
    let buildings = [
        // Medium building 1: Command / HQ (Northwest)
        (Vec3::new(10.0, 3.0, 8.0), Vec3::new(-12.0, 1.5, -8.0)),
        // Medium building 2: Barracks / Depot (Northeast)
        (Vec3::new(8.0, 2.5, 8.0), Vec3::new(10.0, 1.25, -7.0)),
        // Small building 3: Armory / Workshop (Southeast)
        (Vec3::new(5.0, 2.2, 5.0), Vec3::new(11.0, 1.1, 7.0)),
        // Small building 4: Guard post / Checkpoint (Southwest)
        (Vec3::new(4.0, 2.0, 4.0), Vec3::new(-8.0, 1.0, 8.0)),
    ];

    for (size, position) in buildings {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            MeshMaterial3d(building_material.clone()),
            Transform::from_translation(position),
            Building,
        ));
    }

    // Soldier: 2D blue circle seen from above
    let soldier_mesh = meshes.add(Circle::new(0.6));
    let soldier_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.15, 0.50, 0.95),
        perceptual_roughness: 0.5,
        ..default()
    });

    // Front direction indicator notch on the soldier token
    let pointer_mesh = meshes.add(Circle::new(0.15));
    let pointer_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.95, 1.0),
        unlit: true,
        ..default()
    });

    let soldier = commands
        .spawn((
            Mesh3d(soldier_mesh),
            MeshMaterial3d(soldier_material),
            Transform::from_xyz(0.0, 0.05, 0.0)
                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0), // Facing North (-Z)
            },
        ))
        .id();

    commands.spawn((
        Mesh3d(pointer_mesh),
        MeshMaterial3d(pointer_material),
        Transform::from_xyz(0.0, 0.42, 0.01),
        ChildOf(soldier),
    ));

    // Fog of war overlay plane (spanning the map at Y = 4.0, above buildings & soldier)
    let mut building_boxes = [Vec4::ZERO; 16];
    for (i, (size, position)) in buildings.iter().enumerate() {
        let half_x = size.x * 0.5;
        let half_z = size.z * 0.5;
        building_boxes[i] = Vec4::new(
            position.x - half_x,
            position.z - half_z,
            position.x + half_x,
            position.z + half_z,
        );
    }

    let fog_material = fog_materials.add(FogMaterial {
        soldier_pos: Vec2::ZERO,
        soldier_dir: Vec2::new(0.0, -1.0),
        vision_range: 26.0,
        vision_angle: 45.0_f32.to_radians(), // 90° vision cone (+/- 45°)
        inner_radius: 2.5,                   // Close-range personal awareness circle
        fog_alpha: 0.82,                     // Dark/greyed-out opacity outside vision cone
        building_count: buildings.len() as u32,
        _pad0: 0.0,
        _pad1: 0.0,
        _pad2: 0.0,
        buildings: building_boxes,
    });

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(120.0, 120.0))),
        MeshMaterial3d(fog_material),
        Transform::from_xyz(0.0, 4.0, 0.0),
        FogOverlay,
    ));
}

fn update_soldier_and_fog(
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut soldier_q: Query<(&mut Transform, &mut Soldier)>,
    mut fog_materials: ResMut<Assets<FogMaterial>>,
    fog_q: Query<&MeshMaterial3d<FogMaterial>, With<FogOverlay>>,
) {
    let Ok((mut soldier_tf, mut soldier)) = soldier_q.single_mut() else {
        return;
    };

    // 1. WASD / Arrow keys movement
    let mut move_delta = Vec3::ZERO;
    if keyboard_input.pressed(KeyCode::KeyW) || keyboard_input.pressed(KeyCode::ArrowUp) {
        move_delta.z -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyS) || keyboard_input.pressed(KeyCode::ArrowDown) {
        move_delta.z += 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyA) || keyboard_input.pressed(KeyCode::ArrowLeft) {
        move_delta.x -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyD) || keyboard_input.pressed(KeyCode::ArrowRight) {
        move_delta.x += 1.0;
    }

    if move_delta.length_squared() > 0.0 {
        let speed = 12.0;
        soldier_tf.translation += move_delta.normalize() * speed * time.delta_secs();
        // Keep soldier on the map
        soldier_tf.translation.x = soldier_tf.translation.x.clamp(-28.0, 28.0);
        soldier_tf.translation.z = soldier_tf.translation.z.clamp(-28.0, 28.0);
    }

    // 2. Mouse aim direction for vision cone & soldier orientation
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, camera_gt)) = camera_q.single() else {
        return;
    };

    if let Some(cursor_pos) = window.cursor_position() {
        if let Ok(ray) = camera.viewport_to_world(camera_gt, cursor_pos) {
            if let Some(dist) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) {
                let ground_pos = ray.get_point(dist);
                let diff_x = ground_pos.x - soldier_tf.translation.x;
                let diff_z = ground_pos.z - soldier_tf.translation.z;
                let aim_2d = Vec2::new(diff_x, diff_z);

                if aim_2d.length_squared() > 0.01 {
                    let dir = aim_2d.normalize();
                    soldier.facing_dir = dir;

                    // Rotate soldier token to face mouse cursor
                    let yaw = f32::atan2(-dir.x, -dir.y);
                    soldier_tf.rotation =
                        Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2);
                }
            }
        }
    }

    // 3. Update Fog of War shader uniforms with current soldier position and facing
    let soldier_xz = Vec2::new(soldier_tf.translation.x, soldier_tf.translation.z);
    for fog_handle in &fog_q {
        if let Some(mut fog_mat) = fog_materials.get_mut(fog_handle) {
            fog_mat.soldier_pos = soldier_xz;
            fog_mat.soldier_dir = soldier.facing_dir;
        }
    }
}
