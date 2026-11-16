pub mod buildings;

use crate::common::types::{
    AppState, Corpse, CurrentLevel, LevelEntity, RTS_ROAD_WIDTH, RTS_WORLD_HALF_SIZE,
    RTS_WORLD_SIZE, Soldier, WORLD_HALF_SIZE, WORLD_SIZE,
};
use bevy::ecs::message::MessageCursor;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::light::NotShadowCaster;
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
            .add_systems(
                Update,
                (update_camera, sync_elevation_visual_scale).run_if(in_state(AppState::InGame)),
            );
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

    // North-to-South road, broad West-to-East river far south of the village, and road bridge for test_level_4.
    // The road stretch that would intersect the river (`Z in [104.0, 132.0]`) is replaced by the bridge.
    if level == CurrentLevel::Level4 {
        let road_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.28, 0.28, 0.30),
            perceptual_roughness: 0.92,
            ..default()
        });

        let north_road_len = LEVEL_4_RIVER_NORTH_BANK_Z - (-RTS_WORLD_HALF_SIZE);
        let north_road_center_z = 0.5 * (-RTS_WORLD_HALF_SIZE + LEVEL_4_RIVER_NORTH_BANK_Z);
        let south_road_len = RTS_WORLD_HALF_SIZE - LEVEL_4_RIVER_SOUTH_BANK_Z;
        let south_road_center_z = 0.5 * (LEVEL_4_RIVER_SOUTH_BANK_Z + RTS_WORLD_HALF_SIZE);

        let road_root = commands
            .spawn((
                LevelEntity,
                Road,
                Visibility::default(),
                Transform::from_xyz(0.0, 0.01, 0.0),
            ))
            .id();

        // North road stretch (`Z in [-600.0, 104.0]`, stopping at the north riverbank)
        commands.spawn((
            Mesh3d(
                meshes.add(
                    Plane3d::default()
                        .mesh()
                        .size(RTS_ROAD_WIDTH, north_road_len),
                ),
            ),
            MeshMaterial3d(road_material.clone()),
            Transform::from_xyz(0.0, 0.0, north_road_center_z),
            ChildOf(road_root),
        ));

        // South road stretch (`Z in [132.0, +600.0]`, starting at the south riverbank)
        commands.spawn((
            Mesh3d(
                meshes.add(
                    Plane3d::default()
                        .mesh()
                        .size(RTS_ROAD_WIDTH, south_road_len),
                ),
            ),
            MeshMaterial3d(road_material.clone()),
            Transform::from_xyz(0.0, 0.0, south_road_center_z),
            ChildOf(road_root),
        ));

        // Broad, deep West-to-East River far south of the village (`Z = 118.0`, width `28.0m`)
        let riverbank_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.32, 0.28, 0.20),
            perceptual_roughness: 0.95,
            ..default()
        });
        let river_outer_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.11, 0.30, 0.42),
            perceptual_roughness: 0.35,
            metallic: 0.1,
            ..default()
        });
        let river_deep_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.06, 0.19, 0.31),
            perceptual_roughness: 0.25,
            metallic: 0.15,
            ..default()
        });

        commands.spawn((
            LevelEntity,
            River,
            NotShadowCaster,
            Mesh3d(
                meshes.add(
                    Plane3d::default()
                        .mesh()
                        .size(RTS_WORLD_SIZE, LEVEL_4_RIVER_WIDTH + 3.2),
                ),
            ),
            MeshMaterial3d(riverbank_mat),
            Transform::from_xyz(0.0, 0.004, LEVEL_4_RIVER_CENTER_Z),
        ));
        commands.spawn((
            LevelEntity,
            River,
            NotShadowCaster,
            Mesh3d(
                meshes.add(
                    Plane3d::default()
                        .mesh()
                        .size(RTS_WORLD_SIZE, LEVEL_4_RIVER_WIDTH),
                ),
            ),
            MeshMaterial3d(river_outer_mat),
            Transform::from_xyz(0.0, 0.007, LEVEL_4_RIVER_CENTER_Z),
        ));
        commands.spawn((
            LevelEntity,
            River,
            NotShadowCaster,
            Mesh3d(
                meshes.add(
                    Plane3d::default()
                        .mesh()
                        .size(RTS_WORLD_SIZE, LEVEL_4_RIVER_WIDTH * 0.64),
                ),
            ),
            MeshMaterial3d(river_deep_mat),
            Transform::from_xyz(0.0, 0.010, LEVEL_4_RIVER_CENTER_Z),
        ));

        // Impassable West and East river channel colliders, leaving only the central bridge (`X in [-4.6, 4.6]`) open
        let channel_half_x = (RTS_WORLD_HALF_SIZE - LEVEL_4_BRIDGE_HALF_WIDTH_X) * 0.5;
        let channel_center_x = LEVEL_4_BRIDGE_HALF_WIDTH_X + channel_half_x;
        for side in [-1.0_f32, 1.0_f32] {
            commands.spawn((
                LevelEntity,
                River,
                Transform::from_xyz(side * channel_center_x, 0.0, LEVEL_4_RIVER_CENTER_Z),
                LowWallCollider {
                    half_size: Vec2::new(channel_half_x, LEVEL_4_RIVER_HALF_WIDTH),
                },
            ));
        }

        // Bridge replacing the intersecting stretch of road (`Z in [104.0, 132.0]`)
        let bridge_span_z = LEVEL_4_RIVER_WIDTH + 4.0;
        let bridge_deck_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.48, 0.49, 0.51),
            perceptual_roughness: 0.85,
            ..default()
        });
        let bridge_parapet_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.58, 0.59, 0.61),
            perceptual_roughness: 0.80,
            ..default()
        });

        let bridge_entity = commands
            .spawn((
                LevelEntity,
                Bridge,
                NotShadowCaster,
                Mesh3d(
                    meshes.add(
                        Plane3d::default()
                            .mesh()
                            .size(LEVEL_4_BRIDGE_HALF_WIDTH_X * 2.1, bridge_span_z),
                    ),
                ),
                MeshMaterial3d(bridge_deck_mat.clone()),
                Transform::from_xyz(0.0, 0.014, LEVEL_4_RIVER_CENTER_Z),
            ))
            .id();

        // Central bridge roadway surface matching the North/South road width
        commands.spawn((
            NotShadowCaster,
            Mesh3d(
                meshes.add(
                    Plane3d::default()
                        .mesh()
                        .size(RTS_ROAD_WIDTH, bridge_span_z),
                ),
            ),
            MeshMaterial3d(road_material),
            Transform::from_xyz(0.0, 0.004, 0.0),
            ChildOf(bridge_entity),
        ));

        // Left and right raised stone/concrete bridge parapets along the outer edges of the bridge
        let parapet_mesh = meshes.add(Cuboid::new(0.45, 0.85, bridge_span_z));
        let abutment_post_mesh = meshes.add(Cuboid::new(0.75, 1.15, 0.75));
        for side in [-1.0_f32, 1.0_f32] {
            let rail_x = side * (LEVEL_4_BRIDGE_HALF_WIDTH_X + 0.25);
            commands.spawn((
                Mesh3d(parapet_mesh.clone()),
                MeshMaterial3d(bridge_parapet_mat.clone()),
                Transform::from_xyz(rail_x, 0.42, 0.0),
                ChildOf(bridge_entity),
            ));
            for end_z in [-bridge_span_z * 0.5, bridge_span_z * 0.5] {
                commands.spawn((
                    Mesh3d(abutment_post_mesh.clone()),
                    MeshMaterial3d(bridge_parapet_mat.clone()),
                    Transform::from_xyz(rail_x, 0.56, end_z),
                    ChildOf(bridge_entity),
                ));
            }
        }
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
        CurrentLevel::Level4 => &VILLAGE_LOW_WALLS,
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

    // Spawn the dense patch of forest on the right (+X) side of the test_level_4 map.
    // Each tree has a trunk collider (`LowWallCollider`) spaced so infantry can pass through
    // between the trees while the APC cannot drive through the forest.
    if level == CurrentLevel::Level4 {
        let trunk_mesh = meshes.add(Cylinder::new(TREE_TRUNK_HALF_SIZE, 2.6));
        let canopy_sphere_mesh = meshes.add(Sphere::new(1.0));
        let trunk_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.36, 0.24, 0.14),
            perceptual_roughness: 0.92,
            ..default()
        });
        let canopy_dark_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.14, 0.31, 0.15),
            perceptual_roughness: 0.88,
            ..default()
        });
        let canopy_light_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.20, 0.40, 0.18),
            perceptual_roughness: 0.85,
            ..default()
        });

        for (idx, tree_pos) in level_4_forest_trees().into_iter().enumerate() {
            let scale_var = 0.92 + ((idx * 37) % 17) as f32 * 0.01;
            let tree_entity = commands
                .spawn((
                    LevelEntity,
                    Tree,
                    LowWallCollider {
                        half_size: Vec2::splat(TREE_TRUNK_HALF_SIZE),
                    },
                    Mesh3d(trunk_mesh.clone()),
                    MeshMaterial3d(trunk_material.clone()),
                    Transform::from_xyz(tree_pos.x, 1.3, tree_pos.y),
                ))
                .id();

            // Lower foliage crown
            commands.spawn((
                Mesh3d(canopy_sphere_mesh.clone()),
                MeshMaterial3d(if idx % 2 == 0 {
                    canopy_dark_material.clone()
                } else {
                    canopy_light_material.clone()
                }),
                Transform::from_xyz(0.0, 1.45, 0.0)
                    .with_scale(Vec3::new(1.42 * scale_var, 1.25 * scale_var, 1.42 * scale_var)),
                ChildOf(tree_entity),
            ));

            // Upper foliage crown
            commands.spawn((
                Mesh3d(canopy_sphere_mesh.clone()),
                MeshMaterial3d(if idx % 2 == 0 {
                    canopy_light_material.clone()
                } else {
                    canopy_dark_material.clone()
                }),
                Transform::from_xyz(0.0, 2.35, 0.0)
                    .with_scale(Vec3::new(1.02 * scale_var, 1.08 * scale_var, 1.02 * scale_var)),
                ChildOf(tree_entity),
            ));
        }

        // Spawn the Western Ridgeline on the left (-X) side of the Level 4 village (`X in [-96.0, -44.0]`)
        // using stacked topographic contour layers with progressive highland colors and contour rims.
        let contour_rim_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.10, 0.14, 0.09, 0.55),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.95,
            ..default()
        });
        let ridge_tiers: [(f32, Color); 6] = [
            (1.0, Color::srgb(0.25, 0.38, 0.22)),
            (2.2, Color::srgb(0.30, 0.40, 0.23)),
            (3.4, Color::srgb(0.36, 0.42, 0.25)),
            (4.6, Color::srgb(0.42, 0.44, 0.28)),
            (5.8, Color::srgb(0.49, 0.47, 0.32)),
            (6.7, Color::srgb(0.56, 0.52, 0.37)),
        ];
        for (tier_idx, (elev_m, tier_color)) in ridge_tiers.into_iter().enumerate() {
            // Invert `(1 - u^2)^2 = elev_m / RIDGE_MAX_HEIGHT` to get exact contour band width
            let ratio = (elev_m / RIDGE_MAX_HEIGHT).clamp(0.01, 0.99);
            let u_bound = (1.0 - ratio.sqrt().sqrt()).max(0.05).sqrt();
            let east_x = RIDGE_CREST_X + u_bound * RIDGE_EAST_WIDTH;
            let west_x = RIDGE_CREST_X - u_bound * RIDGE_WEST_WIDTH;
            let width_x = (east_x - west_x).max(2.0);
            let center_x = 0.5 * (east_x + west_x);
            let half_z = RIDGE_Z_FLAT_HALF + u_bound * RIDGE_Z_SHOULDER;
            let length_z = half_z * 2.0;

            let y_rim = 0.003 + (tier_idx as f32) * 0.0042;
            let y_fill = y_rim + 0.0018;

            // Dark topographic contour outline rim
            commands.spawn((
                LevelEntity,
                Ridgeline,
                NotShadowCaster,
                Mesh3d(
                    meshes.add(
                        Plane3d::default()
                            .mesh()
                            .size(width_x + 0.7, length_z + 0.7),
                    ),
                ),
                MeshMaterial3d(contour_rim_mat.clone()),
                Transform::from_xyz(center_x, y_rim, RIDGE_Z_CENTER),
            ));

            // Topographic elevation plateau fill
            let tier_mat = materials.add(StandardMaterial {
                base_color: tier_color,
                perceptual_roughness: 0.92,
                ..default()
            });
            commands.spawn((
                LevelEntity,
                Ridgeline,
                NotShadowCaster,
                Mesh3d(meshes.add(Plane3d::default().mesh().size(width_x, length_z))),
                MeshMaterial3d(tier_mat),
                Transform::from_xyz(center_x, y_fill, RIDGE_Z_CENTER),
            ));
        }

        // Spawn the negative-elevation sunken trench depression (`h < 0.0m`) between the ridgeline toe and the village
        let trench_outer_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.18, 0.16, 0.12),
            perceptual_roughness: 0.96,
            ..default()
        });
        let trench_floor_mat = materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.11, 0.08),
            perceptual_roughness: 0.98,
            ..default()
        });
        commands.spawn((
            LevelEntity,
            Trench,
            NotShadowCaster,
            Mesh3d(meshes.add(Plane3d::default().mesh().size(
                LEVEL_4_TRENCH_HALF_WIDTH_X * 2.0,
                (LEVEL_4_TRENCH_FLAT_HALF_Z + LEVEL_4_TRENCH_SHOULDER_Z) * 2.0,
            ))),
            MeshMaterial3d(trench_outer_mat),
            Transform::from_xyz(LEVEL_4_TRENCH_CENTER.x, 0.005, LEVEL_4_TRENCH_CENTER.y),
        ));
        commands.spawn((
            LevelEntity,
            Trench,
            NotShadowCaster,
            Mesh3d(meshes.add(Plane3d::default().mesh().size(
                LEVEL_4_TRENCH_HALF_WIDTH_X * 1.25,
                LEVEL_4_TRENCH_FLAT_HALF_Z * 1.85,
            ))),
            MeshMaterial3d(trench_floor_mat),
            Transform::from_xyz(LEVEL_4_TRENCH_CENTER.x, 0.008, LEVEL_4_TRENCH_CENTER.y),
        ));
    }
}

/// Scales the visual size of infantry (`Soldier`) and ground vehicles (`ApcVehicle`, `FieldArtillery`)
/// based on their terrain elevation (`h` in meters relative to base elevation `0.0m`):
/// - Higher elevations (`h > 0.0m`, e.g. on the Western Ridgeline) appear slightly larger.
/// - Below base elevation (`h < 0.0m`, e.g. in a sunken trench or artillery crater) appear slightly smaller.
pub fn sync_elevation_visual_scale(
    current_level: Option<Res<CurrentLevel>>,
    depressions_q: Query<&ElevationDepression, Without<Camera3d>>,
    mut soldiers_q: Query<
        &mut Transform,
        (
            With<Soldier>,
            Without<Corpse>,
            Without<crate::vehicle::MountedInVehicle>,
            Without<crate::vehicle::ApcVehicle>,
            Without<crate::vehicle::FieldArtillery>,
            Without<Camera3d>,
        ),
    >,
    mut vehicles_q: Query<
        &mut Transform,
        (
            Or<(
                With<crate::vehicle::ApcVehicle>,
                With<crate::vehicle::FieldArtillery>,
            )>,
            Without<Soldier>,
            Without<Camera3d>,
        ),
    >,
) {
    let level = current_level.as_deref().copied().unwrap_or_default();
    let depressions: Vec<ElevationDepression> = depressions_q.iter().copied().collect();

    for mut s_tf in &mut soldiers_q {
        let pos = Vec2::new(s_tf.translation.x, s_tf.translation.z);
        let elev = terrain_elevation_with_depressions(pos, level, &depressions);
        let scale = elevation_visual_scale(elev);
        s_tf.scale = Vec3::splat(scale);
    }

    for mut v_tf in &mut vehicles_q {
        let pos = Vec2::new(v_tf.translation.x, v_tf.translation.z);
        let elev = terrain_elevation_with_depressions(pos, level, &depressions);
        let scale = elevation_visual_scale(elev);
        v_tf.scale = Vec3::splat(scale);
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

        let low_wall_count = app
            .world_mut()
            .query_filtered::<&Transform, With<LowWall>>()
            .iter(app.world())
            .count();
        assert_eq!(
            low_wall_count,
            VILLAGE_LOW_WALLS.len(),
            "test_level_4 must spawn the free-standing waist-high walls around the village"
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

    #[test]
    fn test_level_4_western_ridgeline_and_elevation_scaling_for_infantry_and_vehicles() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.insert_resource(CurrentLevel::Level4);
        app.add_systems(Startup, setup_level_world);
        app.add_systems(Update, sync_elevation_visual_scale);
        app.update();

        // 1. Verify the Western Ridgeline contour layers spawned on the left (-X) side of the village
        let ridge_layers: Vec<&Transform> = app
            .world_mut()
            .query_filtered::<&Transform, With<Ridgeline>>()
            .iter(app.world())
            .collect();
        assert!(
            ridge_layers.len() >= 10,
            "Expected stacked topographic contour layers for the Western Ridgeline, got {}",
            ridge_layers.len()
        );
        for tf in &ridge_layers {
            assert!(
                tf.translation.x < -40.0,
                "Western Ridgeline must be on the left (-X) side of the village, got x={}",
                tf.translation.x
            );
        }

        // 2. Verify terrain_elevation values at crest (+7.0m), base valley (0.0m), and sunken trench (-1.8m)
        let crest_pos = Vec2::new(RIDGE_CREST_X, RIDGE_Z_CENTER);
        let base_pos = Vec2::new(0.0, 0.0);
        let trench_pos = LEVEL_4_TRENCH_CENTER;

        let h_crest = terrain_elevation(crest_pos, CurrentLevel::Level4);
        let h_base = terrain_elevation(base_pos, CurrentLevel::Level4);
        let h_trench = terrain_elevation(trench_pos, CurrentLevel::Level4);

        assert!(
            (h_crest - RIDGE_MAX_HEIGHT).abs() < 0.05,
            "Ridgeline crest should have elevation ~{:.1}m, got {:.2}m",
            RIDGE_MAX_HEIGHT,
            h_crest
        );
        assert!(
            h_base.abs() < 1e-4,
            "Valley floor should be at base elevation 0.0m, got {:.2}m",
            h_base
        );
        assert!(
            h_trench < -1.5,
            "Sunken trench should be below base elevation (< -1.5m), got {:.2}m",
            h_trench
        );

        // 3. Spawn infantry and vehicles at high elevation (ridge crest), base elevation (valley),
        //    and below base elevation (trench + artillery crater) and verify visual scaling
        let soldier_high = app
            .world_mut()
            .spawn((
                Transform::from_xyz(crest_pos.x, 0.05, crest_pos.y),
                Soldier::default(),
            ))
            .id();
        let soldier_base = app
            .world_mut()
            .spawn((
                Transform::from_xyz(base_pos.x, 0.05, base_pos.y),
                Soldier::default(),
            ))
            .id();
        let soldier_trench = app
            .world_mut()
            .spawn((
                Transform::from_xyz(trench_pos.x, 0.05, trench_pos.y),
                Soldier::default(),
            ))
            .id();

        let crater_pos = Vec2::new(12.0, 85.0);
        app.world_mut().spawn(ElevationDepression {
            center: crater_pos,
            radius: 4.0,
            depth: 1.5,
        });
        let soldier_crater = app
            .world_mut()
            .spawn((
                Transform::from_xyz(crater_pos.x, 0.05, crater_pos.y),
                Soldier::default(),
            ))
            .id();

        let apc_high = app
            .world_mut()
            .spawn((
                Transform::from_xyz(crest_pos.x, 1.15, crest_pos.y + 8.0),
                crate::vehicle::ApcVehicle::default(),
            ))
            .id();
        let apc_base = app
            .world_mut()
            .spawn((
                Transform::from_xyz(base_pos.x, 1.15, base_pos.y + 8.0),
                crate::vehicle::ApcVehicle::default(),
            ))
            .id();
        let apc_trench = app
            .world_mut()
            .spawn((
                Transform::from_xyz(trench_pos.x, 1.15, trench_pos.y),
                crate::vehicle::ApcVehicle::default(),
            ))
            .id();

        app.update();

        let s_high_scale = app.world().get::<Transform>(soldier_high).unwrap().scale.x;
        let s_base_scale = app.world().get::<Transform>(soldier_base).unwrap().scale.x;
        let s_trench_scale = app.world().get::<Transform>(soldier_trench).unwrap().scale.x;
        let s_crater_scale = app.world().get::<Transform>(soldier_crater).unwrap().scale.x;

        assert!(
            s_high_scale > s_base_scale && (s_base_scale - 1.0).abs() < 1e-4,
            "Infantry at higher elevation ({:.3}) must appear slightly larger than at base elevation ({:.3})",
            s_high_scale,
            s_base_scale
        );
        assert!(
            s_trench_scale < s_base_scale && s_crater_scale < s_base_scale,
            "Infantry in a trench ({:.3}) or crater ({:.3}) below base elevation must appear slightly smaller than at base ({:.3})",
            s_trench_scale,
            s_crater_scale,
            s_base_scale
        );

        let v_high_scale = app.world().get::<Transform>(apc_high).unwrap().scale.x;
        let v_base_scale = app.world().get::<Transform>(apc_base).unwrap().scale.x;
        let v_trench_scale = app.world().get::<Transform>(apc_trench).unwrap().scale.x;

        assert!(
            v_high_scale > v_base_scale && v_trench_scale < v_base_scale,
            "Vehicles on the ridgeline ({:.3}) must appear larger than base ({:.3}) and smaller in depressions ({:.3})",
            v_high_scale,
            v_base_scale,
            v_trench_scale
        );

        // 4. Verify realistic ridgeline line-of-sight occlusion and slope movement physics:
        //    - Observer in valley cannot see through the ridgeline crest to a target on the reverse slope.
        //    - Observer on the ridgeline crest CAN see down into the valley and gets extended vision range.
        let valley_observer = Vec2::new(-40.0, 0.0);
        let reverse_slope_target = Vec2::new(-82.0, 0.0);
        assert!(
            !crate::common::geometry::can_see_target(
                valley_observer,
                Vec2::new(-1.0, 0.0),
                reverse_slope_target,
                50.0,
                45.0_f32.to_radians(),
                2.5,
                &[],
            ),
            "Observer on valley floor should not see through the ridgeline to the reverse slope"
        );
        assert!(
            crate::common::geometry::can_see_target(
                crest_pos,
                Vec2::new(1.0, 0.0),
                Vec2::new(crest_pos.x + 29.0, crest_pos.y),
                24.0,
                45.0_f32.to_radians(),
                2.5,
                &[],
            ),
            "Spotter on the +7m ridgeline crest should see 29m down into the valley thanks to the high-ground vision bonus"
        );

        let slope_pos = Vec2::new(-56.0, 0.0); // Forward eastern slope of the ridgeline
        let uphill_mult =
            elevation_movement_multiplier(slope_pos, Vec2::new(-1.0, 0.0), CurrentLevel::Level4);
        let downhill_mult =
            elevation_movement_multiplier(slope_pos, Vec2::new(1.0, 0.0), CurrentLevel::Level4);
        assert!(
            uphill_mult < 1.0 && downhill_mult > 1.0,
            "Moving uphill ({:.2}) should be slower than flat ground and downhill ({:.2}) faster",
            uphill_mult,
            downhill_mult
        );
    }

    #[test]
    fn test_level_4_broad_river_is_impassable_except_across_bridge() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.insert_resource(CurrentLevel::Level4);
        app.add_systems(Startup, setup_level_world);
        app.update();

        // 1. Verify the River and Bridge entities spawned far south of the village (`Z = 118.0 > 55.0`),
        //    and the Road entity has 2 child segments (North and South of the river) with the Bridge replacing the crossing
        let river_count = app
            .world_mut()
            .query_filtered::<&Transform, With<River>>()
            .iter(app.world())
            .count();
        assert!(
            river_count >= 5,
            "Expected River visual layers and West/East channel colliders, got {}",
            river_count
        );

        let bridge_tf = app
            .world_mut()
            .query_filtered::<&Transform, With<Bridge>>()
            .single(app.world())
            .expect("test_level_4 must spawn a Bridge across the river");
        assert!(
            (bridge_tf.translation.x - 0.0).abs() < 1e-4
                && (bridge_tf.translation.z - LEVEL_4_RIVER_CENTER_Z).abs() < 1e-4,
            "Bridge should be centered on the road at (0.0, {}), got {:?}",
            LEVEL_4_RIVER_CENTER_Z,
            bridge_tf.translation
        );

        let road_children = app
            .world_mut()
            .query_filtered::<&Children, With<Road>>()
            .single(app.world())
            .expect("Road entity should have North and South road segment children");
        assert_eq!(
            road_children.len(),
            2,
            "Road should be split into North and South stretches with the Bridge replacing the river intersection"
        );

        // 2. Collect river/obstacle colliders and verify the river is impassable outside the bridge
        //    but passable across the bridge (`X = 0.0`) for both infantry and the APC
        let low_wall_pairs: Vec<(Transform, LowWallCollider)> = app
            .world_mut()
            .query::<(&Transform, &LowWallCollider)>()
            .iter(app.world())
            .map(|(t, c)| (*t, *c))
            .collect();
        let w_refs: Vec<(&Transform, &LowWallCollider)> =
            low_wall_pairs.iter().map(|(t, c)| (t, c)).collect();

        // Attempting to step into the river away from the bridge (`X = 35.0, Z = 131.5`) pushes the soldier back onto the south bank (`Z >= 132.6`)
        let in_water_attempt = Vec2::new(35.0, LEVEL_4_RIVER_SOUTH_BANK_Z - 0.5);
        let resolved_bank = crate::common::geometry::resolve_circle_obstacle_collision_bounded(
            in_water_attempt,
            crate::common::types::SOLDIER_RADIUS,
            &[],
            &w_refs,
            crate::common::types::RTS_SOLDIER_WORLD_BOUND,
        );
        assert!(
            resolved_bank.y >= LEVEL_4_RIVER_SOUTH_BANK_Z + crate::common::types::SOLDIER_RADIUS - 0.05,
            "Soldier attempting to enter the river outside the bridge must be stopped at the riverbank, got {:?}",
            resolved_bank
        );

        // Walking across the bridge (`X = 0.0, Z = 118.0`) is completely unobstructed
        let on_bridge_pos = Vec2::new(0.0, LEVEL_4_RIVER_CENTER_Z);
        let resolved_bridge = crate::common::geometry::resolve_circle_obstacle_collision_bounded(
            on_bridge_pos,
            crate::common::types::SOLDIER_RADIUS,
            &[],
            &w_refs,
            crate::common::types::RTS_SOLDIER_WORLD_BOUND,
        );
        assert!(
            (resolved_bridge - on_bridge_pos).length() < 1e-4,
            "Soldier on the bridge must not collide with the river, got {:?}",
            resolved_bridge
        );

        // APC corridor check: blocked across the river at X = 35.0, clear across the bridge at X = 0.0
        assert!(
            crate::vehicle::is_apc_corridor_blocked(
                Vec2::new(35.0, LEVEL_4_RIVER_SOUTH_BANK_Z + 4.0),
                Vec2::new(0.0, -1.0),
                12.0,
                &[],
                &w_refs,
            ),
            "APC must be blocked from driving into the river away from the bridge"
        );
        assert!(
            !crate::vehicle::is_apc_corridor_blocked(
                Vec2::new(0.0, LEVEL_4_RIVER_SOUTH_BANK_Z + 4.0),
                Vec2::new(0.0, -1.0),
                12.0,
                &[],
                &w_refs,
            ),
            "APC must be able to drive across the bridge"
        );

        // Off-road cross-river orders route onto the bridge approach first
        let waypoint = route_across_river_bridge(
            Vec2::new(40.0, 165.0),
            Vec2::new(25.0, 45.0),
            CurrentLevel::Level4,
        );
        assert!(
            waypoint.x.abs() < 3.0 && waypoint.y > LEVEL_4_RIVER_SOUTH_BANK_Z,
            "Off-road South-to-North movement should route to the South Bridge approach first, got {:?}",
            waypoint
        );
    }
}
