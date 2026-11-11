use std::f32::consts::FRAC_PI_2;
use bevy::prelude::*;
use crate::combat::bullet::{Bullet, BulletAssets};
use crate::common::geometry::{can_see_target, find_avoidance_direction, resolve_circle_obstacle_collision};
use crate::common::types::{Soldier, Stance, Team, SOLDIER_RADIUS};
use crate::enemy::ai::{Enemy, EnemyAiState, ENEMY_WAYPOINTS};
use crate::fog::mesh::create_vision_cone_mesh;
use crate::player::Player;
use crate::world::buildings::{Building, BuildingCollider, LowWall, LowWallCollider};

pub fn setup_enemy(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let pointer_mesh = meshes.add(Circle::new(0.15));
    let enemy_soldier_mesh = meshes.add(Circle::new(SOLDIER_RADIUS));
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

    // Enemy vision cone mesh & material (90° cone, 24 units range)
    let enemy_cone_mesh = meshes.add(create_vision_cone_mesh(24.0, FRAC_PI_2, 16));
    let enemy_cone_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.95, 0.2, 0.2, 0.18),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    // Spawn 5 enemy soldiers with distributed compound patrol assignments
    let enemy_configs = [
        // 1. North Central Guard (on North Central Avenue)
        (Vec2::new(0.0, -25.0), Vec2::new(0.0, 1.0), 1),
        // 2. Northwest Road Guard (on Northwest Avenue west of HQ)
        (Vec2::new(-35.0, -20.0), Vec2::new(0.0, -1.0), 0),
        // 3. Northeast Radar Guard (on Northeast Avenue)
        (Vec2::new(35.0, -20.0), Vec2::new(0.0, 1.0), 2),
        // 4. Southeast Armory Guard (on Southeast Avenue)
        (Vec2::new(35.0, 20.0), Vec2::new(0.0, 1.0), 4),
        // 5. Southwest Motor Pool Guard (on Southwest Avenue)
        (Vec2::new(-35.0, 20.0), Vec2::new(0.0, -1.0), 6),
    ];

    for (pos, facing, start_wp_idx) in enemy_configs {
        let yaw = f32::atan2(-facing.x, -facing.y);
        let enemy = commands
            .spawn((
                Mesh3d(enemy_soldier_mesh.clone()),
                MeshMaterial3d(enemy_soldier_material.clone()),
                Transform::from_xyz(pos.x, 0.05, pos.y)
                    .with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2)),
                Visibility::Hidden,
                Enemy {
                    state: EnemyAiState::Search,
                    current_waypoint_idx: start_wp_idx,
                    last_known_player_pos: None,
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 5.5,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    stuck_timer: 0.0,
                    last_pos: pos,
                },
                Team::Enemy,
                Soldier {
                    facing_dir: facing,
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                },
            ))
            .id();

        // Attach direction indicator to enemy
        commands.spawn((
            Mesh3d(pointer_mesh.clone()),
            MeshMaterial3d(enemy_pointer_material.clone()),
            Transform::from_xyz(0.0, 0.42, 0.01),
            ChildOf(enemy),
        ));

        // Attach visible vision cone to enemy
        commands.spawn((
            Mesh3d(enemy_cone_mesh.clone()),
            MeshMaterial3d(enemy_cone_material.clone()),
            Transform::from_xyz(0.0, 0.0, 0.005),
            ChildOf(enemy),
        ));
    }
}

// Enemy AI: patrols compound, hunts player when spotted in vision cone, takes cover & shoots
pub fn update_enemy(
    mut commands: Commands,
    time: Res<Time>,
    mut enemy_q: Query<(&mut Transform, &mut Enemy, &mut Soldier), (With<Enemy>, Without<Player>)>,
    player_q: Query<&Transform, (With<Player>, Without<Enemy>)>,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (With<Building>, Without<Player>, Without<Enemy>),
    >,
    low_walls_q: Query<
        (&Transform, &LowWallCollider),
        (With<LowWall>, Without<Player>, Without<Enemy>),
    >,
    bullet_assets: Res<BulletAssets>,
) {
    let dt = time.delta_secs();
    let soldier_radius = SOLDIER_RADIUS;
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();
    let low_walls: Vec<(&Transform, &LowWallCollider)> = low_walls_q.iter().collect();
    let player_pos = player_q
        .iter()
        .next()
        .map(|tf| Vec2::new(tf.translation.x, tf.translation.z));

    for (mut tf, mut enemy, mut soldier) in &mut enemy_q {
        let enemy_pos = Vec2::new(tf.translation.x, tf.translation.z);

        // 1. Check if the enemy can see the player
        let can_see_player = if let Some(target) = player_pos {
            can_see_target(
                enemy_pos,
                soldier.facing_dir,
                target,
                enemy.vision_range,
                enemy.vision_half_angle,
                enemy.inner_radius,
                &buildings,
            )
        } else {
            false
        };

        // 2. State transitions
        if can_see_player {
            if enemy.state != EnemyAiState::Engage {
                // Just acquired target: reset shoot timer so player gets a reaction delay
                enemy.shoot_timer.reset();
            }
            enemy.state = EnemyAiState::Engage;
            enemy.last_known_player_pos = player_pos;
            enemy.investigate_timer.reset();
        } else if enemy.state == EnemyAiState::Engage {
            // Lost sight of player - switch to investigating last known location
            enemy.state = EnemyAiState::Investigate;
            enemy.investigate_timer.reset();
        }

        // 3. State behaviors
        match enemy.state {
            EnemyAiState::Engage => {
                let target = player_pos.unwrap_or(enemy_pos);
                let to_target = target - enemy_pos;
                let dist = to_target.length();

                if dist > 0.001 {
                    soldier.facing_dir = to_target / dist;
                }

                // Check proximity to any low wall for cover
                let near_low_wall = low_walls.iter().any(|(w_tf, _)| {
                    let w_pos = Vec2::new(w_tf.translation.x, w_tf.translation.z);
                    (w_pos - enemy_pos).length() < 2.5
                });

                if dist > 14.0 {
                    // Long-range engagement: drop prone to snipe with high cover profile (80% miss protection)
                    soldier.stance = Stance::Prone;
                    let desired_move = soldier.facing_dir;
                    let move_dir = find_avoidance_direction(
                        enemy_pos,
                        desired_move,
                        &buildings,
                        &low_walls,
                        soldier_radius,
                    );
                    let crawl_speed = enemy.speed * 0.32;
                    let next_pos = enemy_pos + move_dir * crawl_speed * dt;
                    let resolved =
                        resolve_circle_obstacle_collision(next_pos, soldier_radius, &buildings, &low_walls);
                    tf.translation.x = resolved.x;
                    tf.translation.z = resolved.y;
                } else if dist > 6.0 {
                    // Medium-long range: advance at standing speed towards player
                    soldier.stance = Stance::Standing;
                    let desired_move = soldier.facing_dir;
                    let move_dir = find_avoidance_direction(
                        enemy_pos,
                        desired_move,
                        &buildings,
                        &low_walls,
                        soldier_radius,
                    );
                    let next_pos = enemy_pos + move_dir * (enemy.speed * 0.75) * dt;
                    let resolved =
                        resolve_circle_obstacle_collision(next_pos, soldier_radius, &buildings, &low_walls);
                    tf.translation.x = resolved.x;
                    tf.translation.z = resolved.y;
                } else if near_low_wall || dist > 3.5 {
                    // Duck down into crouch behind low wall or medium range
                    soldier.stance = Stance::Crouching;
                } else {
                    soldier.stance = Stance::Standing;
                }

                // Fire at player
                enemy.shoot_timer.tick(time.delta());
                if enemy.shoot_timer.is_finished() {
                    enemy.shoot_timer.reset();

                    // If crouched behind a low wall, pop up to standing to shoot over the wall
                    if near_low_wall {
                        soldier.stance = Stance::Standing;
                    }

                    let spawn_pos =
                        Vec2::new(tf.translation.x, tf.translation.z) + soldier.facing_dir * 0.85;

                    commands.spawn((
                        Mesh3d(bullet_assets.mesh.clone()),
                        MeshMaterial3d(bullet_assets.enemy_material.clone()),
                        Transform::from_xyz(spawn_pos.x, 0.08, spawn_pos.y)
                            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                        Bullet {
                            origin: spawn_pos,
                            direction: soldier.facing_dir,
                            speed: 38.0,
                            lifetime: 1.6,
                            team: Team::Enemy,
                            shooter_stance: soldier.stance,
                            missed_entities: Vec::new(),
                        },
                    ));
                }
            }
            EnemyAiState::Investigate => {
                if let Some(target) = enemy.last_known_player_pos {
                    let to_target = target - enemy_pos;
                    let dist = to_target.length();

                    if dist > 1.2 {
                        // Move toward last known position at full standing speed
                        soldier.stance = Stance::Standing;
                        let desired_move = to_target / dist;
                        let move_dir = find_avoidance_direction(
                            enemy_pos,
                            desired_move,
                            &buildings,
                            &low_walls,
                            soldier_radius,
                        );
                        soldier.facing_dir = move_dir;
                        let next_pos = enemy_pos + move_dir * enemy.speed * dt;
                        let resolved =
                            resolve_circle_obstacle_collision(next_pos, soldier_radius, &buildings, &low_walls);
                        tf.translation.x = resolved.x;
                        tf.translation.z = resolved.y;
                    } else {
                        // At last known position: crouch low, look around and search
                        soldier.stance = Stance::Crouching;
                        enemy.investigate_timer.tick(time.delta());
                        let scan_angle = time.elapsed_secs() * 3.0;
                        soldier.facing_dir = Vec2::new(scan_angle.cos(), scan_angle.sin());

                        if enemy.investigate_timer.is_finished() {
                            enemy.last_known_player_pos = None;
                            enemy.state = EnemyAiState::Search;
                        }
                    }
                } else {
                    enemy.state = EnemyAiState::Search;
                }
            }
            EnemyAiState::Search => {
                // Standing during routine perimeter patrol
                soldier.stance = Stance::Standing;
                let target_wp = ENEMY_WAYPOINTS[enemy.current_waypoint_idx];
                let to_wp = target_wp - enemy_pos;
                let dist = to_wp.length();

                // Stuck watchdog: track displacement to ensure AI never remains pinned against obstacles
                let moved_dist = (enemy_pos - enemy.last_pos).length();
                if moved_dist < 0.25 * dt * enemy.speed {
                    enemy.stuck_timer += dt;
                } else {
                    enemy.stuck_timer = 0.0;
                    enemy.last_pos = enemy_pos;
                }

                // If reached waypoint OR stuck for > 1.5 seconds, advance to the next patrol waypoint
                if dist < 1.6 || enemy.stuck_timer > 1.5 {
                    enemy.current_waypoint_idx =
                        (enemy.current_waypoint_idx + 1) % ENEMY_WAYPOINTS.len();
                    enemy.stuck_timer = 0.0;
                    enemy.last_pos = enemy_pos;
                } else {
                    let desired_move = to_wp / dist;
                    let move_dir = find_avoidance_direction(
                        enemy_pos,
                        desired_move,
                        &buildings,
                        &low_walls,
                        soldier_radius,
                    );
                    soldier.facing_dir = move_dir;
                    let next_pos = enemy_pos + move_dir * enemy.speed * dt;
                    let resolved =
                        resolve_circle_obstacle_collision(next_pos, soldier_radius, &buildings, &low_walls);
                    tf.translation.x = resolved.x;
                    tf.translation.z = resolved.y;
                }
            }
        }

        // Apply visual rotation from facing direction and scale according to stance
        let yaw = f32::atan2(-soldier.facing_dir.x, -soldier.facing_dir.y);
        tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2);
        tf.scale = match soldier.stance {
            Stance::Standing => Vec3::ONE,
            Stance::Crouching => Vec3::splat(0.72),
            Stance::Prone => Vec3::new(0.55, 1.15, 0.55),
        };
    }
}

