pub mod buildings;

use bevy::prelude::*;
pub use buildings::*;
use crate::common::types::{WORLD_HALF_SIZE, WORLD_SIZE};

// Camera framing constants derived mathematically from viewport aspect ratio and world dimensions
pub const CAMERA_VIEWPORT_HEIGHT: f32 = 68.0;
// Maximum Z offset for camera before showing area beyond north/south world edges
pub const CAMERA_MAX_Z: f32 = (WORLD_HALF_SIZE - (CAMERA_VIEWPORT_HEIGHT * 0.5)).max(0.0);
// Maximum X offset for camera in standard 16:9 widescreen before showing area beyond east/west world edges
pub const CAMERA_MAX_X: f32 = (WORLD_HALF_SIZE - (CAMERA_VIEWPORT_HEIGHT * 0.5 * (16.0 / 9.0))).max(0.0);

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_world)
            .add_systems(Update, update_camera);
    }
}

pub fn update_camera(
    player_q: Query<&Transform, (With<crate::player::Player>, Without<Camera3d>)>,
    mut camera_q: Query<&mut Transform, With<Camera3d>>,
) {
    if let Ok(player_tf) = player_q.single() {
        if let Ok(mut cam_tf) = camera_q.single_mut() {
            // Smoothly tracks player, bounded by world dimensions and camera viewport size
            let target_z = player_tf.translation.z.clamp(-CAMERA_MAX_Z, CAMERA_MAX_Z);
            let target_x = player_tf.translation.x.clamp(-CAMERA_MAX_X, CAMERA_MAX_X);
            cam_tf.translation.z = cam_tf.translation.z.lerp(target_z, 0.1);
            cam_tf.translation.x = cam_tf.translation.x.lerp(target_x, 0.1);
        }
    }
}

fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Top-down camera (orthographic projection looking straight down with pulled-back zoom)
    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::FixedVertical {
                viewport_height: CAMERA_VIEWPORT_HEIGHT, // Pulled back zoom to view expanded combat sectors
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 60.0, CAMERA_MAX_Z).looking_at(Vec3::new(0.0, 0.0, CAMERA_MAX_Z), Vec3::NEG_Z),
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

    // Ground plane sized to unified WORLD_SIZE
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(WORLD_SIZE, WORLD_SIZE))),
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

