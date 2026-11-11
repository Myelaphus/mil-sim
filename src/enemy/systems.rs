use std::f32::consts::FRAC_PI_2;
use bevy::prelude::*;
use crate::combat::bullet::{Bullet, BulletAssets};
use crate::common::geometry::{can_see_target, find_avoidance_direction, resolve_circle_building_collision};
use crate::common::types::{Soldier, Team};
use crate::enemy::ai::{Enemy, EnemyAiState, ENEMY_WAYPOINTS};
use crate::fog::mesh::create_vision_cone_mesh;
use crate::player::Player;
use crate::world::buildings::BuildingCollider;

pub fn setup_enemy(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let pointer_mesh = meshes.add(Circle::new(0.15));
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

    // Spawn 5 enemy soldiers with distributed compound patrol assignments
    let enemy_configs = [
        // 1. North Perimeter Guard
        (Vec2::new(0.0, -18.0), Vec2::new(-1.0, 0.0), 0),
        // 2. Northwest Road Guard
        (Vec2::new(-20.0, -8.0), Vec2::new(0.0, 1.0), 1),
        // 3. Central Plaza Sentry
        (Vec2::new(0.0, -6.0), Vec2::new(0.0, 1.0), 2),
        // 4. Northeast Depot Guard
        (Vec2::new(17.0, -10.0), Vec2::new(0.0, -1.0), 7),
        // 5. East Flank Guard
        (Vec2::new(17.0, 8.0), Vec2::new(0.0, 1.0), 4),
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
                    vision_range: 16.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                },
                Team::Enemy,
                Soldier {
                    facing_dir: facing,
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
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

// Enemy AI: patrols compound, hunts player when spotted in vision cone, and shoots
pub fn update_enemy(
    mut commands: Commands,
    time: Res<Time>,
    mut enemy_q: Query<(&mut Transform, &mut Enemy, &mut Soldier), (With<Enemy>, Without<Player>)>,
    player_q: Query<&Transform, (With<Player>, Without<Enemy>)>,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (With<crate::world::buildings::Building>, Without<Player>, Without<Enemy>),
    >,
    bullet_assets: Res<BulletAssets>,
) {
    let dt = time.delta_secs();
    let soldier_radius = 0.6;
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();
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

                // Advance if player is beyond close firing range
                if dist > 6.0 {
                    let desired_move = soldier.facing_dir;
                    let move_dir = find_avoidance_direction(
                        enemy_pos,
                        desired_move,
                        &buildings,
                        soldier_radius,
                    );
                    let next_pos = enemy_pos + move_dir * (enemy.speed * 0.75) * dt;
                    let resolved =
                        resolve_circle_building_collision(next_pos, soldier_radius, &buildings);
                    tf.translation.x = resolved.x;
                    tf.translation.z = resolved.y;
                }

                // Fire at player
                enemy.shoot_timer.tick(time.delta());
                if enemy.shoot_timer.is_finished() {
                    enemy.shoot_timer.reset();
                    let spawn_pos =
                        Vec2::new(tf.translation.x, tf.translation.z) + soldier.facing_dir * 0.85;

                    commands.spawn((
                        Mesh3d(bullet_assets.mesh.clone()),
                        MeshMaterial3d(bullet_assets.enemy_material.clone()),
                        Transform::from_xyz(spawn_pos.x, 0.08, spawn_pos.y)
                            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                        Bullet {
                            direction: soldier.facing_dir,
                            speed: 38.0,
                            lifetime: 1.6,
                            team: Team::Enemy,
                        },
                    ));
                }
            }
            EnemyAiState::Investigate => {
                if let Some(target) = enemy.last_known_player_pos {
                    let to_target = target - enemy_pos;
                    let dist = to_target.length();

                    if dist > 1.2 {
                        // Move toward last known position with obstacle steering
                        let desired_move = to_target / dist;
                        let move_dir = find_avoidance_direction(
                            enemy_pos,
                            desired_move,
                            &buildings,
                            soldier_radius,
                        );
                        soldier.facing_dir = move_dir;
                        let next_pos = enemy_pos + move_dir * enemy.speed * dt;
                        let resolved =
                            resolve_circle_building_collision(next_pos, soldier_radius, &buildings);
                        tf.translation.x = resolved.x;
                        tf.translation.z = resolved.y;
                    } else {
                        // At last known position: look around and search
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
                let target_wp = ENEMY_WAYPOINTS[enemy.current_waypoint_idx];
                let to_wp = target_wp - enemy_pos;
                let dist = to_wp.length();

                if dist < 1.2 {
                    enemy.current_waypoint_idx =
                        (enemy.current_waypoint_idx + 1) % ENEMY_WAYPOINTS.len();
                } else {
                    let desired_move = to_wp / dist;
                    let move_dir = find_avoidance_direction(
                        enemy_pos,
                        desired_move,
                        &buildings,
                        soldier_radius,
                    );
                    soldier.facing_dir = move_dir;
                    let next_pos = enemy_pos + move_dir * enemy.speed * dt;
                    let resolved =
                        resolve_circle_building_collision(next_pos, soldier_radius, &buildings);
                    tf.translation.x = resolved.x;
                    tf.translation.z = resolved.y;
                }
            }
        }

        // Apply visual rotation from facing direction
        let yaw = f32::atan2(-soldier.facing_dir.x, -soldier.facing_dir.y);
        tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2);
    }
}

