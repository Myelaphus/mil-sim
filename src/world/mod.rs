pub mod buildings;

use crate::common::types::{
    AppState, CurrentLevel, LevelEntity, RTS_ROAD_WIDTH, RTS_WORLD_HALF_SIZE, RTS_WORLD_SIZE,
    WORLD_HALF_SIZE, WORLD_SIZE,
};
use bevy::ecs::message::MessageCursor;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
pub use buildings::*;

// Camera framing constants derived mathematically from viewport aspect ratio and world dimensions
pub const CAMERA_VIEWPORT_HEIGHT: f32 = 68.0;
// Maximum Z offset for camera before showing area beyond north/south world edges
pub const CAMERA_MAX_Z: f32 = (WORLD_HALF_SIZE - (CAMERA_VIEWPORT_HEIGHT * 0.5)).max(0.0);
// Maximum X offset for camera in standard 16:9 widescreen before showing area beyond east/west world edges
pub const CAMERA_MAX_X: f32 =
    (WORLD_HALF_SIZE - (CAMERA_VIEWPORT_HEIGHT * 0.5 * (16.0 / 9.0))).max(0.0);

// RTS Camera constants for test_level_4
pub const RTS_CAMERA_DEFAULT_ZOOM: f32 = 140.0;
pub const RTS_CAMERA_MIN_ZOOM: f32 = 25.0;
pub const RTS_CAMERA_MAX_ZOOM: f32 = 900.0;
pub const RTS_CAMERA_PAN_SPEED_FACTOR: f32 = 1.25;
pub const RTS_CAMERA_KEY_ZOOM_SPEED_FACTOR: f32 = 1.2;
pub const RTS_CAMERA_WHEEL_ZOOM_SENSITIVITY: f32 = 0.12;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_world_environment)
            .add_systems(
                OnEnter(AppState::InGame),
                (setup_level_world, reset_camera_to_spawn),
            )
            .add_systems(Update, update_camera.run_if(in_state(AppState::InGame)));
    }
}

pub fn update_camera(
    current_level: Option<Res<CurrentLevel>>,
    keyboard_input: Option<Res<ButtonInput<KeyCode>>>,
    mouse_wheel_messages: Option<Res<Messages<MouseWheel>>>,
    mut wheel_cursor: Local<MessageCursor<MouseWheel>>,
    time: Option<Res<Time>>,
    player_q: Query<&Transform, (With<crate::player::Player>, Without<Camera3d>)>,
    mut camera_q: Query<(&mut Transform, Option<&mut Projection>), With<Camera3d>>,
) {
    let level = current_level.as_deref().copied().unwrap_or_default();

    if level == CurrentLevel::Level4 {
        let Ok((mut cam_tf, mut projection_opt)) = camera_q.single_mut() else {
            return;
        };
        let dt = time.as_ref().map_or(1.0 / 60.0, |t| t.delta_secs());

        // Read current zoom level from orthographic viewport_height
        let mut current_zoom = match projection_opt.as_deref() {
            Some(Projection::Orthographic(ortho)) => match ortho.scaling_mode {
                bevy::camera::ScalingMode::FixedVertical { viewport_height } => {
                    viewport_height * ortho.scale
                }
                _ => RTS_CAMERA_DEFAULT_ZOOM,
            },
            _ => RTS_CAMERA_DEFAULT_ZOOM,
        };

        // 1. Zoom up (out) and down (in) via mouse scroll wheel and keyboard keys
        let mut zoom_delta = 0.0;
        if let Some(ref wheel_msgs) = mouse_wheel_messages {
            for ev in wheel_cursor.read(wheel_msgs) {
                let scroll_lines = match ev.unit {
                    MouseScrollUnit::Line => ev.y,
                    MouseScrollUnit::Pixel => ev.y / 32.0,
                };
                // Scrolling up (positive y) zooms down closer; scrolling down (negative y) zooms up higher
                zoom_delta -= scroll_lines * current_zoom * RTS_CAMERA_WHEEL_ZOOM_SENSITIVITY;
            }
        }

        if let Some(ref keys) = keyboard_input {
            let mut key_zoom_dir = 0.0;
            // Zoom out / up higher
            if keys.pressed(KeyCode::KeyQ)
                || keys.pressed(KeyCode::Minus)
                || keys.pressed(KeyCode::NumpadSubtract)
                || keys.pressed(KeyCode::PageUp)
            {
                key_zoom_dir += 1.0;
            }
            // Zoom in / down closer
            if keys.pressed(KeyCode::KeyE)
                || keys.pressed(KeyCode::Equal)
                || keys.pressed(KeyCode::NumpadAdd)
                || keys.pressed(KeyCode::PageDown)
            {
                key_zoom_dir -= 1.0;
            }
            zoom_delta += key_zoom_dir * current_zoom * RTS_CAMERA_KEY_ZOOM_SPEED_FACTOR * dt;
        }

        if zoom_delta.abs() > 0.0 {
            current_zoom =
                (current_zoom + zoom_delta).clamp(RTS_CAMERA_MIN_ZOOM, RTS_CAMERA_MAX_ZOOM);
            if let Some(Projection::Orthographic(ortho)) = projection_opt.as_deref_mut() {
                ortho.scaling_mode = bevy::camera::ScalingMode::FixedVertical {
                    viewport_height: current_zoom,
                };
                ortho.scale = 1.0;
            }
        }

        // 2. Move camera with WASD and Arrow keys
        if let Some(ref keys) = keyboard_input {
            let mut pan_dir = Vec2::ZERO;
            if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
                pan_dir.y -= 1.0;
            }
            if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
                pan_dir.y += 1.0;
            }
            if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
                pan_dir.x -= 1.0;
            }
            if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
                pan_dir.x += 1.0;
            }

            if pan_dir.length_squared() > 0.0 {
                let pan_speed = current_zoom * RTS_CAMERA_PAN_SPEED_FACTOR;
                let delta = pan_dir.normalize() * pan_speed * dt;
                cam_tf.translation.x += delta.x;
                cam_tf.translation.z += delta.y;
            }
        }

        // Keep Y fixed at 60.0 (within DirectionalLight shadow cascade bounds) while clamping X/Z
        cam_tf.translation.y = 60.0;
        let max_z = (RTS_WORLD_HALF_SIZE - current_zoom * 0.5).max(0.0);
        let max_x = (RTS_WORLD_HALF_SIZE - current_zoom * 0.5 * (16.0 / 9.0)).max(0.0);
        cam_tf.translation.x = cam_tf.translation.x.clamp(-max_x, max_x);
        cam_tf.translation.z = cam_tf.translation.z.clamp(-max_z, max_z);
        return;
    }

    if let Ok(player_tf) = player_q.single() {
        if let Ok((mut cam_tf, _)) = camera_q.single_mut() {
            // Smoothly tracks player, bounded by world dimensions and camera viewport size
            let target_z = player_tf.translation.z.clamp(-CAMERA_MAX_Z, CAMERA_MAX_Z);
            let target_x = player_tf.translation.x.clamp(-CAMERA_MAX_X, CAMERA_MAX_X);
            cam_tf.translation.z = cam_tf.translation.z.lerp(target_z, 0.1);
            cam_tf.translation.x = cam_tf.translation.x.lerp(target_x, 0.1);
        }
    }
}

pub fn reset_camera_to_spawn(
    current_level: Option<Res<CurrentLevel>>,
    mut camera_q: Query<(&mut Transform, Option<&mut Projection>), With<Camera3d>>,
) {
    let level = current_level.as_deref().copied().unwrap_or_default();
    if let Ok((mut cam_tf, mut projection_opt)) = camera_q.single_mut() {
        if level == CurrentLevel::Level4 {
            *cam_tf = Transform::from_xyz(0.0, 60.0, 0.0).looking_at(Vec3::ZERO, Vec3::NEG_Z);
            if let Some(Projection::Orthographic(ortho)) = projection_opt.as_deref_mut() {
                ortho.scaling_mode = bevy::camera::ScalingMode::FixedVertical {
                    viewport_height: RTS_CAMERA_DEFAULT_ZOOM,
                };
                ortho.scale = 1.0;
            }
        } else {
            *cam_tf = Transform::from_xyz(0.0, 60.0, CAMERA_MAX_Z)
                .looking_at(Vec3::new(0.0, 0.0, CAMERA_MAX_Z), Vec3::NEG_Z);
            if let Some(Projection::Orthographic(ortho)) = projection_opt.as_deref_mut() {
                ortho.scaling_mode = bevy::camera::ScalingMode::FixedVertical {
                    viewport_height: CAMERA_VIEWPORT_HEIGHT,
                };
                ortho.scale = 1.0;
            }
        }
    }
}

fn setup_world_environment(mut commands: Commands) {
    // Top-down camera (orthographic projection looking straight down with pulled-back zoom)
    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::FixedVertical {
                viewport_height: CAMERA_VIEWPORT_HEIGHT, // Pulled back zoom to view expanded combat sectors
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_xyz(0.0, 60.0, CAMERA_MAX_Z)
            .looking_at(Vec3::new(0.0, 0.0, CAMERA_MAX_Z), Vec3::NEG_Z),
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
}

pub fn setup_level_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    current_level: Option<Res<CurrentLevel>>,
) {
    let level = current_level.as_deref().copied().unwrap_or_default();
    let map_size = if level == CurrentLevel::Level4 {
        RTS_WORLD_SIZE
    } else {
        WORLD_SIZE
    };

    // Ground plane sized to level map dimensions
    commands.spawn((
        LevelEntity,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(map_size, map_size))),
        MeshMaterial3d(materials.add(Color::srgb(0.22, 0.35, 0.22))),
    ));

    // North-to-South road for test_level_4 extending from top (-Z) to bottom (+Z) of map through the village.
    // Placed as a flat surface at Y = 0.01 (above ground at Y = 0.0, below vision cones and soldiers at Y >= 0.035).
    if level == CurrentLevel::Level4 {
        let road_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.28, 0.28, 0.30),
            perceptual_roughness: 0.92,
            ..default()
        });
        commands.spawn((
            LevelEntity,
            Road,
            Mesh3d(
                meshes.add(
                    Plane3d::default()
                        .mesh()
                        .size(RTS_ROAD_WIDTH, RTS_WORLD_SIZE),
                ),
            ),
            MeshMaterial3d(road_material),
            Transform::from_xyz(0.0, 0.01, 0.0),
        ));
    }

    // Common white building material
    let building_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.95, 0.95),
        perceptual_roughness: 0.85,
        ..default()
    });

    // Spawn buildings for the active level
    let buildings: &[(Vec3, Vec3)] = match level {
        CurrentLevel::Level1 | CurrentLevel::Level2 => &COMPOUND_BUILDINGS,
        CurrentLevel::Level3 => &[],
        CurrentLevel::Level4 => &VILLAGE_BUILDINGS,
    };
    for &(size, position) in buildings {
        commands.spawn((
            LevelEntity,
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
    let low_walls: &[(Vec3, Vec3)] = match level {
        CurrentLevel::Level1 | CurrentLevel::Level2 => &COMPOUND_LOW_WALLS,
        CurrentLevel::Level3 => &COURTYARD_LOW_WALLS,
        CurrentLevel::Level4 => &[],
    };
    for &(size, position) in low_walls {
        commands.spawn((
            LevelEntity,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_level_4_spawns_10_village_buildings_and_north_south_road() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.insert_resource(CurrentLevel::Level4);
        app.add_systems(Startup, setup_level_world);
        app.update();

        let building_count = app
            .world_mut()
            .query_filtered::<&Transform, With<Building>>()
            .iter(app.world())
            .count();
        assert_eq!(
            building_count, 10,
            "test_level_4 must spawn exactly 10 village buildings"
        );

        let road_count = app
            .world_mut()
            .query_filtered::<&Transform, With<Road>>()
            .iter(app.world())
            .count();
        assert_eq!(
            road_count, 1,
            "test_level_4 must spawn the north-to-south road"
        );

        // Verify none of the 10 village buildings block the north-south road (X in [-4, 4])
        for (tf, collider) in app
            .world_mut()
            .query::<(&Transform, &BuildingCollider)>()
            .iter(app.world())
        {
            let min_x = tf.translation.x - collider.half_size.x;
            let max_x = tf.translation.x + collider.half_size.x;
            assert!(
                max_x < -RTS_ROAD_WIDTH * 0.5 || min_x > RTS_ROAD_WIDTH * 0.5,
                "Village building at X={} overlaps the north-to-south road",
                tf.translation.x
            );
        }
    }

    #[test]
    fn test_level_4_camera_moves_with_wasd_and_arrows_and_zooms() {
        let mut app = App::new();
        app.insert_resource(CurrentLevel::Level4);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<Messages<MouseWheel>>();
        app.init_resource::<Time>();
        app.add_systems(
            Startup,
            (setup_world_environment, reset_camera_to_spawn).chain(),
        );
        app.add_systems(Update, update_camera);
        app.update();

        let cam_entity = app
            .world_mut()
            .query_filtered::<Entity, With<Camera3d>>()
            .single(app.world())
            .unwrap();

        // 1. Move North (-Z) with W and East (+X) with ArrowRight
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyW);
            keys.press(KeyCode::ArrowRight);
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(200));
        }
        app.update();

        let tf_after_move = *app.world().get::<Transform>(cam_entity).unwrap();
        assert!(
            tf_after_move.translation.z < -1.0,
            "W key should move RTS camera North (-Z), got Z={}",
            tf_after_move.translation.z
        );
        assert!(
            tf_after_move.translation.x > 1.0,
            "Right arrow should move RTS camera East (+X), got X={}",
            tf_after_move.translation.x
        );
        assert!(
            (tf_after_move.translation.y - 60.0).abs() < 1e-4,
            "Camera Y should remain fixed at 60.0 for orthographic shadow bounds, got Y={}",
            tf_after_move.translation.y
        );

        // 2. Zoom out (up) via scroll wheel down
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<Messages<MouseWheel>>()
            .write(MouseWheel {
                unit: MouseScrollUnit::Line,
                x: 0.0,
                y: -3.0,
                window: Entity::PLACEHOLDER,
                phase: bevy::input::touch::TouchPhase::Moved,
            });
        app.update();

        let zoom_after_out = match app.world().get::<Projection>(cam_entity).unwrap() {
            Projection::Orthographic(ortho) => match ortho.scaling_mode {
                bevy::camera::ScalingMode::FixedVertical { viewport_height } => viewport_height,
                _ => panic!("Expected FixedVertical scaling mode"),
            },
            _ => panic!("Expected Orthographic projection"),
        };
        assert!(
            zoom_after_out > RTS_CAMERA_DEFAULT_ZOOM,
            "Scrolling down should zoom camera out to larger viewport height, got {}",
            zoom_after_out
        );

        // 3. Zoom in (down) via scroll wheel up
        app.world_mut()
            .resource_mut::<Messages<MouseWheel>>()
            .write(MouseWheel {
                unit: MouseScrollUnit::Line,
                x: 0.0,
                y: 5.0,
                window: Entity::PLACEHOLDER,
                phase: bevy::input::touch::TouchPhase::Moved,
            });
        app.update();

        let zoom_after_in = match app.world().get::<Projection>(cam_entity).unwrap() {
            Projection::Orthographic(ortho) => match ortho.scaling_mode {
                bevy::camera::ScalingMode::FixedVertical { viewport_height } => viewport_height,
                _ => panic!("Expected FixedVertical scaling mode"),
            },
            _ => panic!("Expected Orthographic projection"),
        };
        assert!(
            zoom_after_in < zoom_after_out,
            "Scrolling up should zoom camera in closer to the ground, got {} vs prev={}",
            zoom_after_in,
            zoom_after_out
        );
    }
}
