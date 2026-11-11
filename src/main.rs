use std::f32::consts::FRAC_PI_2;
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
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

#[test]
fn test_circle_aabb_collision() {
    let b_center = Vec2::new(10.0, 0.0);
    let half_size = Vec2::new(4.0, 4.0);
    let soldier_radius = 0.6;

    let pos = Vec2::new(6.2, 0.0);
    let closest = Vec2::new(
        pos.x.clamp(b_center.x - half_size.x, b_center.x + half_size.x),
        pos.y.clamp(b_center.y - half_size.y, b_center.y + half_size.y),
    );
    let diff = pos - closest;
    let dist_sq = diff.length_squared();
    assert!(dist_sq < soldier_radius * soldier_radius);
}

#[test]
fn test_enemy_visibility_in_fog() {
    let player_pos = Vec2::ZERO;
    let player_dir = Vec2::new(0.0, -1.0);
    let vision_range = 26.0;
    let vision_half_angle = 45.0_f32.to_radians();
    let inner_radius = 2.5;

    let north_point = Vec2::new(0.0, -10.0);
    assert!(is_point_visible_to_player(
        player_pos,
        player_dir,
        north_point,
        vision_range,
        vision_half_angle,
        inner_radius,
        &[],
    ));

    let south_point = Vec2::new(0.0, 10.0);
    assert!(!is_point_visible_to_player(
        player_pos,
        player_dir,
        south_point,
        vision_range,
        vision_half_angle,
        inner_radius,
        &[],
    ));
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
        .add_systems(
            Update,
            (
                update_soldier_and_fog,
                update_enemy,
                update_bullets,
                update_enemy_visibility,
            ),
        )
        .run();
}

#[derive(Component, PartialEq, Eq, Clone, Copy, Debug)]
enum Team {
    Player,
    Enemy,
}

#[derive(Component)]
struct Player;

#[derive(Component)]
struct Enemy {
    patrol_min_x: f32,
    patrol_max_x: f32,
    speed: f32,
    dir: f32,
    shoot_timer: Timer,
}

#[derive(Component)]
struct Soldier {
    facing_dir: Vec2,
    shoot_cooldown: Timer,
}

#[derive(Component)]
struct Bullet {
    direction: Vec2,
    speed: f32,
    lifetime: f32,
    team: Team,
}

#[derive(Resource)]
struct BulletAssets {
    mesh: Handle<Mesh>,
    player_material: Handle<StandardMaterial>,
    enemy_material: Handle<StandardMaterial>,
}

#[derive(Component)]
struct Building;

#[derive(Component)]
struct BuildingCollider {
    half_size: Vec2,
}

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

// Generates a 2D circular sector mesh (vision cone fan)
fn create_vision_cone_mesh(radius: f32, fov_radians: f32, segments: u32) -> Mesh {
    let mut positions = vec![Vec3::ZERO];
    let mut normals = vec![Vec3::Z];
    let half_fov = fov_radians * 0.5;
    let angle_step = fov_radians / segments as f32;

    for i in 0..=segments {
        let angle = -half_fov + i as f32 * angle_step;
        positions.push(Vec3::new(radius * angle.sin(), radius * angle.cos(), 0.005));
        normals.push(Vec3::Z);
    }

    let mut indices = Vec::new();
    for i in 1..=segments {
        indices.push(0);
        indices.push(i);
        indices.push(i + 1);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));
    mesh
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
            BuildingCollider {
                half_size: Vec2::new(size.x * 0.5, size.z * 0.5),
            },
        ));
    }

    // Bullet assets resource
    let bullet_mesh = meshes.add(Circle::new(0.14));
    let player_bullet_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.85, 0.2),
        unlit: true,
        ..default()
    });
    let enemy_bullet_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.25, 0.25),
        unlit: true,
        ..default()
    });
    commands.insert_resource(BulletAssets {
        mesh: bullet_mesh,
        player_material: player_bullet_material,
        enemy_material: enemy_bullet_material,
    });

    // Shared token pointer notch mesh
    let pointer_mesh = meshes.add(Circle::new(0.15));

    // ==========================================
    // 1. PLAYER SOLDIER (Blue circle)
    // ==========================================
    let player_soldier_mesh = meshes.add(Circle::new(0.6));
    let player_soldier_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.15, 0.50, 0.95),
        perceptual_roughness: 0.5,
        ..default()
    });
    let player_pointer_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.95, 1.0),
        unlit: true,
        ..default()
    });

    let player = commands
        .spawn((
            Mesh3d(player_soldier_mesh),
            MeshMaterial3d(player_soldier_material),
            Transform::from_xyz(0.0, 0.05, 0.0)
                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0), // Facing North (-Z)
                shoot_cooldown: Timer::from_seconds(0.12, TimerMode::Once),
            },
        ))
        .id();

    commands.spawn((
        Mesh3d(pointer_mesh.clone()),
        MeshMaterial3d(player_pointer_material),
        Transform::from_xyz(0.0, 0.42, 0.01),
        ChildOf(player),
    ));

    // ==========================================
    // 2. ENEMY SOLDIER (Red circle)
    // ==========================================
    let enemy_soldier_mesh = meshes.add(Circle::new(0.6));
    let enemy_soldier_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.90, 0.15, 0.15),
        perceptual_roughness: 0.5,
        ..default()
    });
    let enemy_pointer_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.9, 0.9),
        unlit: true,
        ..default()
    });

    // Enemy vision cone mesh & material (90° cone, 16 units range)
    let enemy_cone_mesh = meshes.add(create_vision_cone_mesh(16.0, FRAC_PI_2, 16));
    let enemy_cone_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.95, 0.2, 0.2, 0.18),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    let enemy = commands
        .spawn((
            Mesh3d(enemy_soldier_mesh),
            MeshMaterial3d(enemy_soldier_material),
            Transform::from_xyz(0.0, 0.05, -15.0)
                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
            Visibility::Hidden,
            Enemy {
                patrol_min_x: -6.0,
                patrol_max_x: 6.0,
                speed: 4.5,
                dir: 1.0,
                shoot_timer: Timer::from_seconds(1.2, TimerMode::Repeating),
            },
            Team::Enemy,
            Soldier {
                facing_dir: Vec2::new(1.0, 0.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
            },
        ))
        .id();

    // Attach direction indicator to enemy
    commands.spawn((
        Mesh3d(pointer_mesh),
        MeshMaterial3d(enemy_pointer_material),
        Transform::from_xyz(0.0, 0.42, 0.01),
        ChildOf(enemy),
    ));

    // Attach visible vision cone to enemy
    commands.spawn((
        Mesh3d(enemy_cone_mesh),
        MeshMaterial3d(enemy_cone_material),
        Transform::from_xyz(0.0, 0.0, 0.005),
        ChildOf(enemy),
    ));

    // ==========================================
    // 3. FOG OF WAR OVERLAY PLANE
    // ==========================================
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
    mut commands: Commands,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    bullet_assets: Res<BulletAssets>,
    mut player_q: Query<(&mut Transform, &mut Soldier), With<Player>>,
    buildings_q: Query<(&Transform, &BuildingCollider), Without<Player>>,
    mut fog_materials: ResMut<Assets<FogMaterial>>,
    fog_q: Query<&MeshMaterial3d<FogMaterial>, With<FogOverlay>>,
) {
    let Ok((mut soldier_tf, mut soldier)) = player_q.single_mut() else {
        return;
    };

    // 1. WASD / Arrow keys movement with building collision
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
        let delta = move_delta.normalize() * speed * time.delta_secs();
        let soldier_radius = 0.6;

        let mut new_pos = Vec2::new(
            soldier_tf.translation.x + delta.x,
            soldier_tf.translation.z + delta.z,
        );

        // Resolve circle vs AABB collision against all buildings
        for _ in 0..4 {
            for (b_tf, collider) in &buildings_q {
                let b_center = Vec2::new(b_tf.translation.x, b_tf.translation.z);
                let closest = Vec2::new(
                    new_pos.x.clamp(
                        b_center.x - collider.half_size.x,
                        b_center.x + collider.half_size.x,
                    ),
                    new_pos.y.clamp(
                        b_center.y - collider.half_size.y,
                        b_center.y + collider.half_size.y,
                    ),
                );

                let diff = new_pos - closest;
                let dist_sq = diff.length_squared();

                if dist_sq < soldier_radius * soldier_radius {
                    if dist_sq > 0.00001 {
                        let dist = dist_sq.sqrt();
                        let overlap = soldier_radius - dist;
                        new_pos += (diff / dist) * overlap;
                    } else {
                        // Soldier center is inside the building: push out to the nearest face
                        let d_left = (new_pos.x - (b_center.x - collider.half_size.x)).abs();
                        let d_right = ((b_center.x + collider.half_size.x) - new_pos.x).abs();
                        let d_top = (new_pos.y - (b_center.y - collider.half_size.y)).abs();
                        let d_bottom = ((b_center.y + collider.half_size.y) - new_pos.y).abs();
                        let min_d = d_left.min(d_right).min(d_top).min(d_bottom);
                        if min_d == d_left {
                            new_pos.x = b_center.x - collider.half_size.x - soldier_radius;
                        } else if min_d == d_right {
                            new_pos.x = b_center.x + collider.half_size.x + soldier_radius;
                        } else if min_d == d_top {
                            new_pos.y = b_center.y - collider.half_size.y - soldier_radius;
                        } else {
                            new_pos.y = b_center.y + collider.half_size.y + soldier_radius;
                        }
                    }
                }
            }
        }

        // Keep soldier on the map
        soldier_tf.translation.x = new_pos.x.clamp(-28.0, 28.0);
        soldier_tf.translation.z = new_pos.y.clamp(-28.0, 28.0);
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

    // 3. Spacebar shooting (Player team)
    soldier.shoot_cooldown.tick(time.delta());
    let should_shoot = keyboard_input.just_pressed(KeyCode::Space)
        || (keyboard_input.pressed(KeyCode::Space) && soldier.shoot_cooldown.is_finished());

    if should_shoot {
        soldier.shoot_cooldown.reset();
        let spawn_pos = Vec2::new(soldier_tf.translation.x, soldier_tf.translation.z)
            + soldier.facing_dir * 0.85;

        commands.spawn((
            Mesh3d(bullet_assets.mesh.clone()),
            MeshMaterial3d(bullet_assets.player_material.clone()),
            Transform::from_xyz(spawn_pos.x, 0.08, spawn_pos.y)
                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
            Bullet {
                direction: soldier.facing_dir,
                speed: 45.0,
                lifetime: 1.5,
                team: Team::Player,
            },
        ));
    }

    // 4. Update Fog of War shader uniforms with current soldier position and facing
    let soldier_xz = Vec2::new(soldier_tf.translation.x, soldier_tf.translation.z);
    for fog_handle in &fog_q {
        if let Some(mut fog_mat) = fog_materials.get_mut(fog_handle) {
            fog_mat.soldier_pos = soldier_xz;
            fog_mat.soldier_dir = soldier.facing_dir;
        }
    }
}

// Enemy movement (patrol back and forth along X) and shooting mechanics
fn update_enemy(
    mut commands: Commands,
    time: Res<Time>,
    mut enemy_q: Query<(&mut Transform, &mut Enemy, &mut Soldier), With<Enemy>>,
    bullet_assets: Res<BulletAssets>,
) {
    let dt = time.delta_secs();
    for (mut tf, mut enemy, mut soldier) in &mut enemy_q {
        // 1. Move side-to-side along X
        tf.translation.x += enemy.dir * enemy.speed * dt;
        if tf.translation.x >= enemy.patrol_max_x {
            tf.translation.x = enemy.patrol_max_x;
            enemy.dir = -1.0;
        } else if tf.translation.x <= enemy.patrol_min_x {
            tf.translation.x = enemy.patrol_min_x;
            enemy.dir = 1.0;
        }

        // Face in patrol movement direction (+X when moving right, -X when moving left)
        let facing = Vec2::new(enemy.dir, 0.0);
        soldier.facing_dir = facing;

        let yaw = f32::atan2(-facing.x, -facing.y);
        tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2);

        // 2. Periodic enemy shooting (Enemy team)
        enemy.shoot_timer.tick(time.delta());
        if enemy.shoot_timer.is_finished() {
            enemy.shoot_timer.reset();
            let spawn_pos = Vec2::new(tf.translation.x, tf.translation.z) + facing * 0.85;

            commands.spawn((
                Mesh3d(bullet_assets.mesh.clone()),
                MeshMaterial3d(bullet_assets.enemy_material.clone()),
                Transform::from_xyz(spawn_pos.x, 0.08, spawn_pos.y)
                    .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                Bullet {
                    direction: facing,
                    speed: 35.0,
                    lifetime: 1.8,
                    team: Team::Enemy,
                },
            ));
        }
    }
}

// Bullet flight, obstacle collision, and player/enemy death handling
fn update_bullets(
    mut commands: Commands,
    time: Res<Time>,
    mut bullet_q: Query<(Entity, &mut Transform, &Bullet)>,
    buildings_q: Query<(&Transform, &BuildingCollider), (With<Building>, Without<Bullet>)>,
    characters_q: Query<(Entity, &Transform, &Team), Without<Bullet>>,
) {
    let dt = time.delta_secs();
    let soldier_radius = 0.6;
    let bullet_radius = 0.14;
    let hit_dist_sq = (soldier_radius + bullet_radius) * (soldier_radius + bullet_radius);

    for (b_entity, mut tf, bullet) in &mut bullet_q {
        // Lifetime check
        if bullet.lifetime <= 0.0 {
            commands.entity(b_entity).despawn();
            continue;
        }

        tf.translation.x += bullet.direction.x * bullet.speed * dt;
        tf.translation.z += bullet.direction.y * bullet.speed * dt;

        // Map boundaries check
        if tf.translation.x.abs() > 30.0 || tf.translation.z.abs() > 30.0 {
            commands.entity(b_entity).despawn();
            continue;
        }

        let bullet_pos = Vec2::new(tf.translation.x, tf.translation.z);

        // 1. Check building collision
        let mut hit_building = false;
        for (b_tf, collider) in &buildings_q {
            let b_center = Vec2::new(b_tf.translation.x, b_tf.translation.z);
            if bullet_pos.x >= b_center.x - collider.half_size.x
                && bullet_pos.x <= b_center.x + collider.half_size.x
                && bullet_pos.y >= b_center.y - collider.half_size.y
                && bullet_pos.y <= b_center.y + collider.half_size.y
            {
                hit_building = true;
                break;
            }
        }
        if hit_building {
            commands.entity(b_entity).despawn();
            continue;
        }

        // 2. Check hit on opposing team characters (Player / Enemy)
        let mut hit_character = false;
        for (char_entity, char_tf, char_team) in &characters_q {
            if bullet.team != *char_team {
                let char_pos = Vec2::new(char_tf.translation.x, char_tf.translation.z);
                if (bullet_pos - char_pos).length_squared() < hit_dist_sq {
                    // Character dies when hit by opposing bullet!
                    commands.entity(char_entity).despawn_children();
                    commands.entity(char_entity).despawn();
                    hit_character = true;
                    break;
                }
            }
        }

        if hit_character {
            commands.entity(b_entity).despawn();
        }
    }
}

// Dynamically sets enemy visibility based on player's vision cone and building line-of-sight
fn update_enemy_visibility(
    player_q: Query<(&Transform, &Soldier), With<Player>>,
    mut enemy_q: Query<(&Transform, &mut Visibility), With<Enemy>>,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (With<Building>, Without<Player>, Without<Enemy>),
    >,
) {
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();

    let Ok((player_tf, player_soldier)) = player_q.single() else {
        // Player is dead: enemies remain hidden
        for (_, mut vis) in &mut enemy_q {
            *vis = Visibility::Hidden;
        }
        return;
    };

    let player_pos = Vec2::new(player_tf.translation.x, player_tf.translation.z);

    for (enemy_tf, mut vis) in &mut enemy_q {
        let enemy_pos = Vec2::new(enemy_tf.translation.x, enemy_tf.translation.z);
        let visible = is_point_visible_to_player(
            player_pos,
            player_soldier.facing_dir,
            enemy_pos,
            26.0,
            45.0_f32.to_radians(),
            2.5,
            &buildings,
        );

        *vis = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn is_point_visible_to_player(
    player_pos: Vec2,
    player_dir: Vec2,
    point: Vec2,
    vision_range: f32,
    vision_half_angle: f32,
    inner_radius: f32,
    buildings: &[(&Transform, &BuildingCollider)],
) -> bool {
    let to_point = point - player_pos;
    let dist = to_point.length();

    if dist > vision_range {
        return false;
    }

    let in_inner = dist <= inner_radius;
    let in_cone = if dist > 0.001 {
        let dir = to_point / dist;
        let dot = dir.dot(player_dir.normalize_or_zero());
        dot >= vision_half_angle.cos()
    } else {
        true
    };

    if !in_inner && !in_cone {
        return false;
    }

    // Line of sight check against buildings
    for (b_tf, collider) in buildings {
        let b_center = Vec2::new(b_tf.translation.x, b_tf.translation.z);
        let b_min = b_center - collider.half_size;
        let b_max = b_center + collider.half_size;

        if line_intersects_box(player_pos, point, b_min, b_max) {
            return false;
        }
    }

    true
}

fn line_intersects_box(origin: Vec2, target: Vec2, b_min: Vec2, b_max: Vec2) -> bool {
    // If target point is inside this building, it does not occlude itself
    if target.x >= b_min.x && target.x <= b_max.x && target.y >= b_min.y && target.y <= b_max.y {
        return false;
    }

    let delta = target - origin;
    let mut tmin = 0.0_f32;
    let mut tmax = 1.0_f32;

    if delta.x.abs() > 0.0001 {
        let inv_dx = 1.0 / delta.x;
        let mut t1 = (b_min.x - origin.x) * inv_dx;
        let mut t2 = (b_max.x - origin.x) * inv_dx;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        tmin = tmin.max(t1);
        tmax = tmax.min(t2);
        if tmin > tmax {
            return false;
        }
    } else if origin.x < b_min.x || origin.x > b_max.x {
        return false;
    }

    if delta.y.abs() > 0.0001 {
        let inv_dy = 1.0 / delta.y;
        let mut t1 = (b_min.y - origin.y) * inv_dy;
        let mut t2 = (b_max.y - origin.y) * inv_dy;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        tmin = tmin.max(t1);
        tmax = tmax.min(t2);
        if tmin > tmax {
            return false;
        }
    } else if origin.y < b_min.y || origin.y > b_max.y {
        return false;
    }

    tmin < 0.999 && tmax > 0.001
}
