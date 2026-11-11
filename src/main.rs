use std::f32::consts::FRAC_PI_2;
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Mil-Sim".to_string(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .add_systems(Startup, setup)
        .run();
}

#[derive(Component)]
struct Soldier;

#[derive(Component)]
struct Building;

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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

    // Soldier: 2D blue circle seen from above (lying flat on ground plane)
    let soldier_mesh = meshes.add(Circle::new(0.6));
    let soldier_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.15, 0.50, 0.95),
        perceptual_roughness: 0.5,
        ..default()
    });

    commands.spawn((
        Mesh3d(soldier_mesh),
        MeshMaterial3d(soldier_material),
        Transform::from_xyz(0.0, 0.05, 0.0)
            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
        Soldier,
    ));
}
