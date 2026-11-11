use std::f32::consts::FRAC_PI_2;
use bevy::prelude::*;
use crate::combat::bullet::{Bullet, BulletAssets};
use crate::common::geometry::{can_see_target, find_avoidance_direction, resolve_circle_obstacle_collision};
use crate::common::types::{GunshotEvent, Soldier, Stance, Team, SOLDIER_RADIUS};
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
    mut enemy_q: Query<
        (&mut Transform, &mut Enemy, &mut Soldier),
        (With<Enemy>, Without<Player>, Without<crate::friendly::Friendly>),
    >,
    player_q: Query<
        &Transform,
        (With<Player>, Without<Enemy>, Without<crate::friendly::Friendly>),
    >,
    friendly_q: Query<
        &Transform,
        (With<crate::friendly::Friendly>, Without<Enemy>, Without<Player>),
    >,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (
            With<Building>,
            Without<Player>,
            Without<Enemy>,
            Without<crate::friendly::Friendly>,
        ),
    >,
    low_walls_q: Query<
        (&Transform, &LowWallCollider),
        (
            With<LowWall>,
            Without<Player>,
            Without<Enemy>,
            Without<crate::friendly::Friendly>,
        ),
    >,
    bullets_q: Query<
        (&Transform, &Bullet),
        (
            With<Bullet>,
            Without<Enemy>,
            Without<Player>,
            Without<crate::friendly::Friendly>,
        ),
    >,
    bullet_assets: Res<BulletAssets>,
    mut gunshot_reader: MessageReader<GunshotEvent>,
) {
    let dt = time.delta_secs();
    let soldier_radius = SOLDIER_RADIUS;
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();
    let low_walls: Vec<(&Transform, &LowWallCollider)> = low_walls_q.iter().collect();
    let player_pos = player_q
        .iter()
        .next()
        .map(|tf| Vec2::new(tf.translation.x, tf.translation.z));

    // Collect all gunshot messages heard this frame so each enemy can process them
    let heard_gunshots: Vec<GunshotEvent> = gunshot_reader.read().copied().collect();

    for (mut tf, mut enemy, mut soldier) in &mut enemy_q {
        let enemy_pos = Vec2::new(tf.translation.x, tf.translation.z);

        // 0. React to nearby gunfire or active near-miss bullets if not already engaged in combat
        if enemy.state != EnemyAiState::Engage {
            let mut alert_source: Option<Vec2> = None;

            // Check for nearby bullets flying past (near-miss suppression within 4.0m)
            for (b_tf, bullet) in &bullets_q {
                if bullet.team != Team::Enemy {
                    let b_pos = Vec2::new(b_tf.translation.x, b_tf.translation.z);
                    if (b_pos - enemy_pos).length_squared() < 16.0 {
                        alert_source = Some(bullet.origin);
                        break;
                    }
                }
            }

            // Check acoustic hearing of gunshots across compound
            if alert_source.is_none() {
                for event in &heard_gunshots {
                    if event.shooter_team != Team::Enemy {
                        let to_gunshot = event.position - enemy_pos;
                        let dist = to_gunshot.length();

                        // Sound check: clear line-of-sight sound carries up to 55m;
                        // sound blocked by solid concrete buildings is dampened to 35m.
                        let has_los = can_see_target(
                            enemy_pos,
                            to_gunshot.normalize_or_zero(),
                            event.position,
                            60.0,
                            std::f32::consts::PI, // Omnidirectional hearing (360 degrees)
                            0.0,
                            &buildings,
                        );
                        let max_hearing_dist = if has_los { 55.0 } else { 35.0 };

                        if dist <= max_hearing_dist && dist > 0.05 {
                            alert_source = Some(event.position);
                            break;
                        }
                    }
                }
            }

            if let Some(source_pos) = alert_source {
                let to_source = source_pos - enemy_pos;
                if to_source.length_squared() > 0.01 {
                    // 1. Immediately turn facing towards the source of the gunfire
                    soldier.facing_dir = to_source.normalize();

                    // 2. Optionally take cover:
                    // If a low wall is within 3.5m, immediately duck behind it
                    let near_low_wall = low_walls.iter().any(|(w_tf, _)| {
                        let w_pos = Vec2::new(w_tf.translation.x, w_tf.translation.z);
                        (w_pos - enemy_pos).length() < 3.5
                    });

                    if near_low_wall {
                        soldier.stance = Stance::Crouching;
                    } else if to_source.length() < 18.0 {
                        // Close gunfire in the open: duck into crouch to reduce hit silhouette
                        soldier.stance = Stance::Crouching;
                    }

                    // 3. Alert AI to investigate the gunfire position
                    enemy.state = EnemyAiState::Investigate;
                    enemy.last_known_player_pos = Some(source_pos);
                    enemy.investigate_timer.reset();
                }
            }
        }

        // 1. Check if the enemy can see any squad member (player or friendly)
        let mut visible_squad_targets: Vec<(Vec2, f32)> = Vec::new();
        if let Some(target) = player_pos {
            if can_see_target(
                enemy_pos,
                soldier.facing_dir,
                target,
                enemy.vision_range,
                enemy.vision_half_angle,
                enemy.inner_radius,
                &buildings,
            ) {
                visible_squad_targets.push((target, (target - enemy_pos).length_squared()));
            }
        }
        for f_tf in &friendly_q {
            let f_pos = Vec2::new(f_tf.translation.x, f_tf.translation.z);
            if can_see_target(
                enemy_pos,
                soldier.facing_dir,
                f_pos,
                enemy.vision_range,
                enemy.vision_half_angle,
                enemy.inner_radius,
                &buildings,
            ) {
                visible_squad_targets.push((f_pos, (f_pos - enemy_pos).length_squared()));
            }
        }
        visible_squad_targets.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let can_see_squad_target = visible_squad_targets.first().map(|(pos, _)| *pos);

        // 2. State transitions
        if let Some(target) = can_see_squad_target {
            if enemy.state != EnemyAiState::Engage {
                // Just acquired target: reset shoot timer so squad gets a reaction delay
                enemy.shoot_timer.reset();
            }
            enemy.state = EnemyAiState::Engage;
            enemy.last_known_player_pos = Some(target);
            enemy.investigate_timer.reset();
        } else if enemy.state == EnemyAiState::Engage {
            // Lost sight of squad target - switch to investigating last known location
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

                    // Check if near a low wall for tactical cover during investigation
                    let near_low_wall = low_walls.iter().any(|(w_tf, _)| {
                        let w_pos = Vec2::new(w_tf.translation.x, w_tf.translation.z);
                        (w_pos - enemy_pos).length() < 3.0
                    });

                    if dist > 1.2 {
                        // If taking cover or near a low wall, crouch-walk for tactical cover; otherwise standing advance
                        if near_low_wall {
                            soldier.stance = Stance::Crouching;
                        } else if soldier.stance != Stance::Crouching && soldier.stance != Stance::Prone {
                            soldier.stance = Stance::Standing;
                        }
                        let desired_move = to_target / dist;
                        let move_dir = find_avoidance_direction(
                            enemy_pos,
                            desired_move,
                            &buildings,
                            &low_walls,
                            soldier_radius,
                        );
                        soldier.facing_dir = move_dir;
                        let speed = match soldier.stance {
                            Stance::Standing => enemy.speed,
                            Stance::Crouching => enemy.speed * 0.6,
                            Stance::Prone => enemy.speed * 0.3,
                        };
                        let next_pos = enemy_pos + move_dir * speed * dt;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enemy_turns_and_takes_cover_from_nearby_gunfire() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Time>();
        app.add_message::<GunshotEvent>();

        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let bullet_mesh = meshes.add(Circle::new(0.12));
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let dummy_mat = materials.add(Color::srgb(1.0, 1.0, 1.0));
        app.insert_resource(BulletAssets {
            mesh: bullet_mesh,
            player_material: dummy_mat.clone(),
            enemy_material: dummy_mat,
        });

        // Spawn low wall at (0.0, 1.0)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.4, 1.0),
            LowWall,
            LowWallCollider {
                half_size: Vec2::new(2.0, 0.4),
            },
        ));

        // Spawn enemy at (0, 0) facing South (0, 1), in Search mode standing
        let enemy_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 0.0),
                Enemy {
                    state: EnemyAiState::Search,
                    current_waypoint_idx: 0,
                    last_known_player_pos: None,
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 5.5,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    stuck_timer: 0.0,
                    last_pos: Vec2::ZERO,
                },
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0), // South
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                },
            ))
            .id();

        app.add_systems(Update, update_enemy);

        // Send a gunshot message from East (15.0, 0.0) fired by Player
        app.world_mut().write_message(GunshotEvent {
            position: Vec2::new(15.0, 0.0),
            shooter_team: Team::Player,
        });

        app.update();

        let enemy = app.world().get::<Enemy>(enemy_entity).unwrap();
        let soldier = app.world().get::<Soldier>(enemy_entity).unwrap();

        // 1. Turned towards source of gunfire (East: x ≈ 1.0, y ≈ 0.0)
        assert!(
            soldier.facing_dir.x > 0.95,
            "Enemy should face gunfire source, facing_dir: {:?}",
            soldier.facing_dir
        );

        // 2. Took cover behind nearby low wall (crouching)
        assert_eq!(
            soldier.stance,
            Stance::Crouching,
            "Enemy should take cover (crouch) near low wall"
        );

        // 3. Switched state to Investigate with target position recorded
        assert_eq!(enemy.state, EnemyAiState::Investigate);
        assert_eq!(enemy.last_known_player_pos, Some(Vec2::new(15.0, 0.0)));
    }

    #[test]
    fn test_enemy_reacts_to_near_miss_bullet() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Time>();
        app.add_message::<GunshotEvent>();

        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let bullet_mesh = meshes.add(Circle::new(0.12));
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let dummy_mat = materials.add(Color::srgb(1.0, 1.0, 1.0));
        app.insert_resource(BulletAssets {
            mesh: bullet_mesh,
            player_material: dummy_mat.clone(),
            enemy_material: dummy_mat,
        });

        // Spawn enemy at (0, 0) facing South (0, 1) in open field standing
        let enemy_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 0.0),
                Enemy {
                    state: EnemyAiState::Search,
                    current_waypoint_idx: 0,
                    last_known_player_pos: None,
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 5.5,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    stuck_timer: 0.0,
                    last_pos: Vec2::ZERO,
                },
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                },
            ))
            .id();

        // Spawn an active player bullet whizzing 1.5m to the right (within 4m near-miss threshold)
        // with origin at (10.0, 0.0)
        app.world_mut().spawn((
            Transform::from_xyz(1.5, 0.08, 0.0),
            Bullet {
                origin: Vec2::new(10.0, 0.0),
                direction: Vec2::new(-1.0, 0.0),
                speed: 45.0,
                lifetime: 1.5,
                team: Team::Player,
                shooter_stance: Stance::Standing,
                missed_entities: Vec::new(),
            },
        ));

        app.add_systems(Update, update_enemy);
        app.update();

        let enemy = app.world().get::<Enemy>(enemy_entity).unwrap();
        let soldier = app.world().get::<Soldier>(enemy_entity).unwrap();

        // Should turn towards bullet origin (East: x ≈ 1.0)
        assert!(
            soldier.facing_dir.x > 0.95,
            "Enemy should face incoming bullet origin, facing_dir: {:?}",
            soldier.facing_dir
        );

        // Near-miss close in open field triggers defensive crouch
        assert_eq!(
            soldier.stance,
            Stance::Crouching,
            "Enemy should duck into crouch upon near-miss"
        );
        assert_eq!(enemy.state, EnemyAiState::Investigate);
    }

    #[test]
    fn test_enemy_spots_player_and_engages_after_turning_towards_gunfire() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Time>();
        app.add_message::<GunshotEvent>();

        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let bullet_mesh = meshes.add(Circle::new(0.12));
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let dummy_mat = materials.add(Color::srgb(1.0, 1.0, 1.0));
        app.insert_resource(BulletAssets {
            mesh: bullet_mesh,
            player_material: dummy_mat.clone(),
            enemy_material: dummy_mat,
        });

        // Enemy initially faces South (0, 1) at (0, 0)
        let enemy_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 0.0),
                Enemy {
                    state: EnemyAiState::Search,
                    current_waypoint_idx: 0,
                    last_known_player_pos: None,
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 5.5,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    stuck_timer: 0.0,
                    last_pos: Vec2::ZERO,
                },
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0), // Facing South, cannot see North
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                },
            ))
            .id();

        // Player is behind enemy at (0.0, -10.0) (North)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, -10.0),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, 1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
            },
        ));

        app.add_systems(Update, update_enemy);

        // Player fires gun at (0.0, -10.0)
        app.world_mut().write_message(GunshotEvent {
            position: Vec2::new(0.0, -10.0),
            shooter_team: Team::Player,
        });

        app.update();

        let enemy = app.world().get::<Enemy>(enemy_entity).unwrap();
        let soldier = app.world().get::<Soldier>(enemy_entity).unwrap();

        // Enemy swiveled to face North towards gunshot (0.0, -1.0)
        assert!(
            soldier.facing_dir.y < -0.9,
            "Enemy should swivel to face North, facing_dir: {:?}",
            soldier.facing_dir
        );

        // Because player is at (0, -10) in clear line-of-sight, enemy spotted player and engaged!
        assert_eq!(
            enemy.state,
            EnemyAiState::Engage,
            "Enemy should transition directly to Engage upon spotting player"
        );
    }

    #[test]
    fn test_enemy_ignores_friendly_gunfire() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Time>();
        app.add_message::<GunshotEvent>();

        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let bullet_mesh = meshes.add(Circle::new(0.12));
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let dummy_mat = materials.add(Color::srgb(1.0, 1.0, 1.0));
        app.insert_resource(BulletAssets {
            mesh: bullet_mesh,
            player_material: dummy_mat.clone(),
            enemy_material: dummy_mat,
        });

        // Enemy facing South (0, 1) in Search
        let enemy_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 0.0),
                Enemy {
                    state: EnemyAiState::Search,
                    current_waypoint_idx: 0,
                    last_known_player_pos: None,
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 5.5,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    stuck_timer: 0.0,
                    last_pos: Vec2::ZERO,
                },
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                },
            ))
            .id();

        app.add_systems(Update, update_enemy);

        // Friendly enemy fires from West (-10.0, 0.0)
        app.world_mut().write_message(GunshotEvent {
            position: Vec2::new(-10.0, 0.0),
            shooter_team: Team::Enemy,
        });

        app.update();

        let enemy = app.world().get::<Enemy>(enemy_entity).unwrap();
        let soldier = app.world().get::<Soldier>(enemy_entity).unwrap();

        // Enemy should ignore friendly gunfire and stay on Search patrol
        assert_eq!(enemy.state, EnemyAiState::Search);
        assert_eq!(soldier.stance, Stance::Standing);
    }
}

