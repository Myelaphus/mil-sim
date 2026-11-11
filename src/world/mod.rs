pub mod buildings;

use bevy::prelude::*;
pub use buildings::*;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_world);
    }
}

fn setup_world(
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

    // Spawn 4 buildings
    for (size, position) in COMPOUND_BUILDINGS {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            MeshMaterial3d(building_material.clone()),
            Transform::from_translation(position),
            Building,
            BuildingCollider {
                half_size: Vec2::new(size.x * 0.5, size.z * 0.5),
            },
        ));
    }

    // Waist-high low wall material (sandbag / concrete barrier tan-grey)
    let low_wall_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.68, 0.64, 0.52),
        perceptual_roughness: 0.9,
        ..default()
    });

    // Spawn waist-height low walls
    for (size, position) in COMPOUND_LOW_WALLS {
        commands.spawn((
            Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
            MeshMaterial3d(low_wall_material.clone()),
            Transform::from_translation(position),
            LowWall,
            LowWallCollider {
                half_size: Vec2::new(size.x * 0.5, size.z * 0.5),
            },
        ));
    }
}

