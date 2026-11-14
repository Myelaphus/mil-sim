use crate::combat::bullet::{Bullet, BulletAssets};
use crate::common::geometry::{
    apply_shoot_spread, can_see_target, directional_speed_multiplier, find_avoidance_direction,
    find_cover_position, has_low_wall_in_line_of_fire, is_near_low_wall,
    resolve_circle_obstacle_collision,
};
use crate::common::types::{
    CurrentLevel, GunshotEvent, LevelEntity, SOLDIER_RADIUS, SOLDIER_WORLD_BOUND, Soldier, Stance,
    Team,
};
use crate::enemy::ai::{ENEMY_WAYPOINTS, Enemy, EnemyAiState, Zombie};
use crate::fog::mesh::create_vision_cone_mesh;
use crate::player::Player;
use crate::world::buildings::{
    Building, BuildingCollider, COMPOUND_BUILDINGS, COMPOUND_LOW_WALLS, LowWall, LowWallCollider,
};
use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

pub const DEFAULT_ZOMBIE_COUNT: usize = 360;

#[derive(Resource, Clone, Copy, Debug)]
pub struct ZombieSpawnConfig {
    pub count: usize,
}

impl Default for ZombieSpawnConfig {
    fn default() -> Self {
        Self {
            count: DEFAULT_ZOMBIE_COUNT,
        }
    }
}

pub fn setup_enemy(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    current_level: Res<CurrentLevel>,
    zombie_config: Option<Res<ZombieSpawnConfig>>,
    time: Option<Res<Time>>,
) {
    match *current_level {
        CurrentLevel::Level1 => setup_guards(&mut commands, &mut meshes, &mut materials),
        CurrentLevel::Level2 => {
            let count = zombie_config.map_or(DEFAULT_ZOMBIE_COUNT, |c| c.count);
            let seed = time
                .as_ref()
                .map_or(42, |t| (t.elapsed().as_nanos() as u64) ^ 0x517cc1b727220a95);
            setup_zombies(&mut commands, &mut meshes, &mut materials, count, seed);
        }
        CurrentLevel::Level3 => {
            setup_courtyard_fireteam(&mut commands, &mut meshes, &mut materials);
        }
    }
}

pub fn setup_guards(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
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
    spawn_enemy_soldiers(commands, meshes, materials, &enemy_configs);
}

/// Spawn a 4-man enemy fire team on the north side of the courtyard for `test_level_3`.
pub fn setup_courtyard_fireteam(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let south = Vec2::new(0.0, 1.0);
    let fireteam_configs = [
        (Vec2::new(-3.0, -10.5), south, 0),
        (Vec2::new(-1.0, -11.5), south, 0),
        (Vec2::new(1.0, -11.5), south, 0),
        (Vec2::new(3.0, -10.5), south, 0),
    ];
    spawn_enemy_soldiers(commands, meshes, materials, &fireteam_configs);
}

fn spawn_enemy_soldiers(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    enemy_configs: &[(Vec2, Vec2, usize)],
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

    for &(pos, facing, start_wp_idx) in enemy_configs {
        let yaw = f32::atan2(-facing.x, -facing.y);
        let enemy = commands
            .spawn((
                LevelEntity,
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
                    inventory: crate::common::types::Inventory::default(),
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

// Fast, robust, non-cryptographic PRNG (64-bit LCG) for deterministic dynamic placement
#[derive(Clone, Copy, Debug)]
pub struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x853c49e6748fea9b } else { seed },
        }
    }

    pub fn next_u32(&mut self) -> u32 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.state >> 32) as u32
    }

    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f32) / (u32::MAX as f32)
    }

    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next_f32() * (max - min)
    }
}

// Validates whether a candidate 2D position is clear of all world collision objects,
// safe from player spawn, within world bounds, and separated from already placed spawns.
pub fn is_spawn_position_valid(
    pos: Vec2,
    soldier_radius: f32,
    existing_spawns: &[Vec2],
    min_separation: f32,
    player_safe_pos: Vec2,
    player_safe_radius: f32,
) -> bool {
    // 1. World boundary collision check with safety buffer
    let max_bound = SOLDIER_WORLD_BOUND - 1.5;
    if pos.x.abs() > max_bound || pos.y.abs() > max_bound {
        return false;
    }

    // 2. Safe perimeter around player fire team spawn
    if (pos - player_safe_pos).length_squared() < player_safe_radius * player_safe_radius {
        return false;
    }

    // 3. Collision check against all compound buildings (Circle vs AABB)
    let min_obstacle_dist = soldier_radius + 0.35;
    let min_obstacle_dist_sq = min_obstacle_dist * min_obstacle_dist;

    for (size, position) in COMPOUND_BUILDINGS {
        let center = Vec2::new(position.x, position.z);
        let half_size = Vec2::new(size.x * 0.5, size.z * 0.5);
        let closest = Vec2::new(
            pos.x.clamp(center.x - half_size.x, center.x + half_size.x),
            pos.y.clamp(center.y - half_size.y, center.y + half_size.y),
        );
        if (pos - closest).length_squared() < min_obstacle_dist_sq {
            return false;
        }
    }

    // 4. Collision check against all compound low walls (Circle vs AABB)
    for (size, position) in COMPOUND_LOW_WALLS {
        let center = Vec2::new(position.x, position.z);
        let half_size = Vec2::new(size.x * 0.5, size.z * 0.5);
        let closest = Vec2::new(
            pos.x.clamp(center.x - half_size.x, center.x + half_size.x),
            pos.y.clamp(center.y - half_size.y, center.y + half_size.y),
        );
        if (pos - closest).length_squared() < min_obstacle_dist_sq {
            return false;
        }
    }

    // 5. Collision check against other spawned soldiers (prevent overlapping)
    let min_sep_sq = min_separation * min_separation;
    for &other in existing_spawns {
        if (pos - other).length_squared() < min_sep_sq {
            return false;
        }
    }

    true
}

// Dynamically distributes `count` spawn positions across the map without colliding
// with buildings, walls, player spawn, or each other.
pub fn generate_dynamic_zombie_spawns(count: usize, seed: u64) -> Vec<Vec2> {
    let mut rng = SimpleRng::new(seed);
    let mut spawns = Vec::with_capacity(count);

    let soldier_radius = SOLDIER_RADIUS;
    let player_safe_pos = Vec2::new(0.0, 48.0);
    let player_safe_radius = 28.0;

    let mut current_min_separation = 4.5_f32;
    let absolute_min_separation = soldier_radius * 2.4; // 1.44m (strictly > 2 * SOLDIER_RADIUS)

    let spawn_bounds_x = (-48.0_f32, 48.0_f32);
    let spawn_bounds_y = (-48.0_f32, 20.0_f32);

    for _ in 0..count {
        let mut placed = false;
        let mut attempts = 0;

        while !placed && attempts < 1000 {
            attempts += 1;
            let candidate = Vec2::new(
                rng.range(spawn_bounds_x.0, spawn_bounds_x.1),
                rng.range(spawn_bounds_y.0, spawn_bounds_y.1),
            );

            if is_spawn_position_valid(
                candidate,
                soldier_radius,
                &spawns,
                current_min_separation,
                player_safe_pos,
                player_safe_radius,
            ) {
                spawns.push(candidate);
                placed = true;
            } else if attempts % 80 == 0 && current_min_separation > absolute_min_separation {
                // If space is tight at high density, gradually relax separation down to physical minimum
                current_min_separation =
                    (current_min_separation * 0.88).max(absolute_min_separation);
            }
        }

        // Fallback pass if separation constraint wasn't met
        if !placed {
            for _ in 0..300 {
                let candidate = Vec2::new(
                    rng.range(spawn_bounds_x.0, spawn_bounds_x.1),
                    rng.range(spawn_bounds_y.0, spawn_bounds_y.1),
                );
                if is_spawn_position_valid(
                    candidate,
                    soldier_radius,
                    &spawns,
                    absolute_min_separation,
                    player_safe_pos,
                    player_safe_radius,
                ) {
                    spawns.push(candidate);
                    break;
                }
            }
        }
    }

    spawns
}

pub fn setup_zombies(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    count: usize,
    seed: u64,
) {
    let pointer_mesh = meshes.add(Circle::new(0.15));
    let zombie_mesh = meshes.add(Circle::new(SOLDIER_RADIUS));

    // Green circle material for zombies
    let zombie_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.20, 0.72, 0.28),
        perceptual_roughness: 0.65,
        ..default()
    });
    let zombie_pointer_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.65, 0.95, 0.65),
        unlit: true,
        ..default()
    });

    let spawns = generate_dynamic_zombie_spawns(count, seed);
    let mut rng = SimpleRng::new(seed.wrapping_add(101));

    for &pos in &spawns {
        let angle = rng.range(0.0, std::f32::consts::TAU);
        let facing = Vec2::new(angle.cos(), angle.sin()).normalize_or_zero();
        let facing = if facing.length_squared() > 0.01 {
            facing
        } else {
            Vec2::new(0.0, 1.0)
        };
        let yaw = f32::atan2(-facing.x, -facing.y);

        let zombie = commands
            .spawn((
                LevelEntity,
                Mesh3d(zombie_mesh.clone()),
                MeshMaterial3d(zombie_material.clone()),
                Transform::from_xyz(pos.x, 0.05, pos.y)
                    .with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2)),
                Visibility::Hidden,
                Zombie {
                    wander_dir: facing,
                    wander_timer: Timer::from_seconds(
                        2.0 + rng.range(0.0, 3.0),
                        TimerMode::Repeating,
                    ),
                    target_pos: None,
                    speed: 2.4,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.5,
                    attack_cooldown: {
                        let mut t = Timer::from_seconds(0.8, TimerMode::Once);
                        t.set_elapsed(std::time::Duration::from_secs(1));
                        t
                    },
                },
                Enemy {
                    state: EnemyAiState::Search,
                    current_waypoint_idx: 0,
                    last_known_player_pos: None,
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(999.0, TimerMode::Once),
                    speed: 2.4,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.5,
                    stuck_timer: 0.0,
                    last_pos: pos,
                },
                Team::Enemy,
                Soldier {
                    facing_dir: facing,
                    shoot_cooldown: Timer::from_seconds(999.0, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        // Attach direction pointer so player can tell zombie facing direction
        commands.spawn((
            Mesh3d(pointer_mesh.clone()),
            MeshMaterial3d(zombie_pointer_material.clone()),
            Transform::from_xyz(0.0, 0.42, 0.01),
            ChildOf(zombie),
        ));

        // NOTE: No vision cone child attached — zombies have vision cones, but do not render them!
    }
}

// Enemy AI: patrols compound, hunts player when spotted in vision cone, takes cover & shoots
pub fn update_enemy(
    mut commands: Commands,
    time: Res<Time>,
    mut enemy_q: Query<
        (&mut Transform, &mut Enemy, &mut Soldier),
        (
            With<Enemy>,
            Without<Zombie>,
            Without<Player>,
            Without<crate::friendly::Friendly>,
        ),
    >,
    player_q: Query<
        &Transform,
        (
            With<Player>,
            Without<Enemy>,
            Without<crate::friendly::Friendly>,
        ),
    >,
    friendly_q: Query<
        &Transform,
        (
            With<crate::friendly::Friendly>,
            Without<Enemy>,
            Without<Player>,
        ),
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
    current_level: Option<Res<CurrentLevel>>,
) {
    let level = current_level.as_deref().copied().unwrap_or_default();
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
        soldier.inventory.tick(time.delta());
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
                    let near_low_wall = is_near_low_wall(enemy_pos, &low_walls, 3.5);

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

        // 1. Check if the enemy can see any fire team member (player or friendly)
        let mut visible_fireteam_targets: Vec<(Vec2, f32)> = Vec::new();
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
                visible_fireteam_targets.push((target, (target - enemy_pos).length_squared()));
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
                visible_fireteam_targets.push((f_pos, (f_pos - enemy_pos).length_squared()));
            }
        }
        visible_fireteam_targets
            .sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        let can_see_fireteam_target = visible_fireteam_targets.first().map(|(pos, _)| *pos);

        // 2. State transitions
        if let Some(target) = can_see_fireteam_target {
            if enemy.state != EnemyAiState::Engage {
                // Just acquired target: reset shoot timer so fire team gets a reaction delay
                enemy.shoot_timer.reset();
            }
            enemy.state = EnemyAiState::Engage;
            enemy.last_known_player_pos = Some(target);
            enemy.investigate_timer.reset();
        } else if enemy.state == EnemyAiState::Engage {
            // Lost sight of fire team target - switch to investigating last known location
            enemy.state = EnemyAiState::Investigate;
            enemy.investigate_timer.reset();
        }

        // 3. State behaviors
        match enemy.state {
            EnemyAiState::Engage => {
                let target = can_see_fireteam_target
                    .or(enemy.last_known_player_pos)
                    .or(player_pos)
                    .unwrap_or(enemy_pos);
                let to_target = target - enemy_pos;
                let dist = to_target.length();

                if dist > 0.001 {
                    soldier.facing_dir = to_target / dist;
                }

                // Attempt to take cover behind a nearby obstruction between enemy and threat.
                // Crouching directly behind a low wall avoids going prone and shooting into the wall.
                let mut is_moving = false;
                if let Some(cover_spot) = find_cover_position(
                    enemy_pos,
                    target,
                    &buildings,
                    &low_walls,
                    soldier_radius,
                    16.0,
                ) {
                    let to_cover = cover_spot - enemy_pos;
                    let dist_to_cover = to_cover.length();
                    if dist_to_cover > 0.35 {
                        is_moving = true;
                        soldier.stance = if dist_to_cover < 1.5 {
                            Stance::Crouching
                        } else {
                            Stance::Standing
                        };
                        let desired_move = to_cover / dist_to_cover;
                        let move_dir = if dist_to_cover < 2.0 {
                            desired_move
                        } else {
                            find_avoidance_direction(
                                enemy_pos,
                                desired_move,
                                &buildings,
                                &low_walls,
                                soldier_radius,
                            )
                        };
                        let dir_mult = directional_speed_multiplier(soldier.facing_dir, move_dir);
                        let next_pos = enemy_pos + move_dir * (enemy.speed * dir_mult) * dt;
                        let resolved = resolve_circle_obstacle_collision(
                            next_pos,
                            soldier_radius,
                            &buildings,
                            &low_walls,
                        );
                        tf.translation.x = resolved.x;
                        tf.translation.z = resolved.y;
                    } else {
                        // Directly behind cover: crouch behind the wall
                        soldier.stance = Stance::Crouching;
                    }
                } else {
                    let wall_in_los = is_near_low_wall(enemy_pos, &low_walls, 2.5)
                        || has_low_wall_in_line_of_fire(enemy_pos, target, &low_walls);

                    if dist > 14.0 && !wall_in_los {
                        // Long-range engagement in open field (no wall blocking): drop prone
                        is_moving = true;
                        soldier.stance = Stance::Prone;
                        let desired_move = soldier.facing_dir;
                        let move_dir = find_avoidance_direction(
                            enemy_pos,
                            desired_move,
                            &buildings,
                            &low_walls,
                            soldier_radius,
                        );
                        let dir_mult = directional_speed_multiplier(soldier.facing_dir, move_dir);
                        let crawl_speed = enemy.speed * 0.32 * dir_mult;
                        let next_pos = enemy_pos + move_dir * crawl_speed * dt;
                        let resolved = resolve_circle_obstacle_collision(
                            next_pos,
                            soldier_radius,
                            &buildings,
                            &low_walls,
                        );
                        tf.translation.x = resolved.x;
                        tf.translation.z = resolved.y;
                    } else if dist > 6.0 && !wall_in_los {
                        // Medium-long range: advance at standing speed towards target
                        is_moving = true;
                        soldier.stance = Stance::Standing;
                        let desired_move = soldier.facing_dir;
                        let move_dir = find_avoidance_direction(
                            enemy_pos,
                            desired_move,
                            &buildings,
                            &low_walls,
                            soldier_radius,
                        );
                        let dir_mult = directional_speed_multiplier(soldier.facing_dir, move_dir);
                        let next_pos = enemy_pos + move_dir * (enemy.speed * 0.75 * dir_mult) * dt;
                        let resolved = resolve_circle_obstacle_collision(
                            next_pos,
                            soldier_radius,
                            &buildings,
                            &low_walls,
                        );
                        tf.translation.x = resolved.x;
                        tf.translation.z = resolved.y;
                    } else if wall_in_los || dist > 3.5 {
                        // Duck down into crouch behind low wall or medium range
                        soldier.stance = Stance::Crouching;
                    } else {
                        soldier.stance = Stance::Standing;
                    }
                }

                // If active weapon is completely out of ammunition and soldier has a sidearm, switch to sidearm
                if let Some(active) = soldier.inventory.active_weapon() {
                    if active.ammo.is_completely_empty()
                        && soldier.inventory.has_sidearm()
                        && !soldier.inventory.is_swapping()
                    {
                        soldier.inventory.start_swap();
                    }
                }

                // Fire at target if not swapping
                enemy.shoot_timer.tick(time.delta());
                if enemy.shoot_timer.is_finished() && !soldier.inventory.is_swapping() {
                    let mut should_fire = false;
                    let mut bullet_speed = 38.0;
                    let mut bullet_lifetime = 1.6;

                    if let Some(weapon) = soldier.inventory.active_weapon_mut() {
                        if weapon.ammo.current_mag == 0 {
                            weapon.ammo.start_reload();
                        } else if weapon.ammo.can_fire() {
                            weapon.ammo.current_mag -= 1;
                            bullet_speed = weapon.bullet_speed();
                            bullet_lifetime = weapon.bullet_lifetime();
                            should_fire = true;
                        }
                    }

                    if should_fire {
                        enemy.shoot_timer.reset();

                        let current_pos = Vec2::new(tf.translation.x, tf.translation.z);
                        let behind_low_wall = is_near_low_wall(current_pos, &low_walls, 2.5)
                            || has_low_wall_in_line_of_fire(current_pos, target, &low_walls);

                        // If behind a low wall, pop up to standing to shoot over the wall
                        if behind_low_wall {
                            soldier.stance = Stance::Standing;
                        }

                        let spread_seed = time.elapsed_secs().to_bits() as u64
                            ^ (current_pos.x.to_bits() as u64)
                            ^ ((current_pos.y.to_bits() as u64) << 16);
                        let shot_dir =
                            apply_shoot_spread(soldier.facing_dir, is_moving, spread_seed);
                        let spawn_pos = current_pos + soldier.facing_dir * 0.85;

                        commands.spawn((
                            LevelEntity,
                            Mesh3d(bullet_assets.mesh.clone()),
                            MeshMaterial3d(bullet_assets.enemy_material.clone()),
                            Transform::from_xyz(spawn_pos.x, 0.08, spawn_pos.y)
                                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                            Bullet {
                                origin: spawn_pos,
                                direction: shot_dir,
                                speed: bullet_speed,
                                lifetime: bullet_lifetime,
                                team: Team::Enemy,
                                shooter_stance: soldier.stance,
                                missed_entities: Vec::new(),
                            },
                        ));
                    }
                }
            }
            EnemyAiState::Investigate => {
                if let Some(target) = enemy.last_known_player_pos {
                    let to_target = target - enemy_pos;
                    let dist = to_target.length();

                    // If cover is nearby between the enemy and the gunfire source, move to cover and crouch behind it
                    if let Some(cover_spot) = find_cover_position(
                        enemy_pos,
                        target,
                        &buildings,
                        &low_walls,
                        soldier_radius,
                        16.0,
                    ) {
                        if dist > 0.01 {
                            soldier.facing_dir = to_target / dist;
                        }
                        soldier.stance = Stance::Crouching;
                        let to_cover = cover_spot - enemy_pos;
                        let dist_to_cover = to_cover.length();
                        if dist_to_cover > 0.35 {
                            let desired_move = to_cover / dist_to_cover;
                            let move_dir = if dist_to_cover < 2.0 {
                                desired_move
                            } else {
                                find_avoidance_direction(
                                    enemy_pos,
                                    desired_move,
                                    &buildings,
                                    &low_walls,
                                    soldier_radius,
                                )
                            };
                            let dir_mult =
                                directional_speed_multiplier(soldier.facing_dir, move_dir);
                            let speed = enemy.speed * 0.85 * dir_mult;
                            let next_pos = enemy_pos + move_dir * speed * dt;
                            let resolved = resolve_circle_obstacle_collision(
                                next_pos,
                                soldier_radius,
                                &buildings,
                                &low_walls,
                            );
                            tf.translation.x = resolved.x;
                            tf.translation.z = resolved.y;
                        } else {
                            enemy.investigate_timer.tick(time.delta());
                            if enemy.investigate_timer.is_finished() {
                                enemy.last_known_player_pos = None;
                                enemy.state = EnemyAiState::Search;
                            }
                        }
                    } else {
                        let near_low_wall = is_near_low_wall(enemy_pos, &low_walls, 3.0);

                        if dist > 1.2 {
                            // If taking cover or near a low wall, crouch-walk for tactical cover; otherwise standing advance
                            if near_low_wall {
                                soldier.stance = Stance::Crouching;
                            } else if soldier.stance != Stance::Crouching
                                && soldier.stance != Stance::Prone
                            {
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
                            let dir_mult =
                                directional_speed_multiplier(soldier.facing_dir, move_dir);
                            soldier.facing_dir = move_dir;
                            let base_speed = match soldier.stance {
                                Stance::Standing => enemy.speed,
                                Stance::Crouching => enemy.speed * 0.6,
                                Stance::Prone => enemy.speed * 0.3,
                            };
                            let speed = base_speed * dir_mult;
                            let next_pos = enemy_pos + move_dir * speed * dt;
                            let resolved = resolve_circle_obstacle_collision(
                                next_pos,
                                soldier_radius,
                                &buildings,
                                &low_walls,
                            );
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
                    }
                } else {
                    enemy.state = EnemyAiState::Search;
                }
            }
            EnemyAiState::Search => {
                // Standing during routine perimeter patrol
                soldier.stance = Stance::Standing;
                if level == CurrentLevel::Level3 {
                    // Hold north-side courtyard overwatch facing South
                    soldier.facing_dir = Vec2::new(0.0, 1.0);
                } else {
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
                        let dir_mult = directional_speed_multiplier(soldier.facing_dir, move_dir);
                        soldier.facing_dir = move_dir;
                        let next_pos = enemy_pos + move_dir * (enemy.speed * dir_mult) * dt;
                        let resolved = resolve_circle_obstacle_collision(
                            next_pos,
                            soldier_radius,
                            &buildings,
                            &low_walls,
                        );
                        tf.translation.x = resolved.x;
                        tf.translation.z = resolved.y;
                    }
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

// Zombie AI: wander randomly, slow pursuit on sight or gunfire, never crouch or prone, lethal melee contact
pub fn update_zombies(
    mut commands: Commands,
    time: Res<Time>,
    mut zombie_q: Query<
        (&mut Transform, &mut Zombie, &mut Soldier),
        (
            With<Zombie>,
            With<Enemy>,
            Without<Player>,
            Without<crate::friendly::Friendly>,
        ),
    >,
    player_q: Query<
        (Entity, &Transform),
        (
            With<Player>,
            Without<Zombie>,
            Without<Enemy>,
            Without<crate::friendly::Friendly>,
        ),
    >,
    friendly_q: Query<
        (Entity, &Transform),
        (
            With<crate::friendly::Friendly>,
            Without<Zombie>,
            Without<Enemy>,
            Without<Player>,
        ),
    >,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (
            With<Building>,
            Without<Zombie>,
            Without<Player>,
            Without<Enemy>,
            Without<crate::friendly::Friendly>,
        ),
    >,
    low_walls_q: Query<
        (&Transform, &LowWallCollider),
        (
            With<LowWall>,
            Without<Zombie>,
            Without<Player>,
            Without<Enemy>,
            Without<crate::friendly::Friendly>,
        ),
    >,
    corpse_assets: Res<crate::combat::bullet::CorpseAssets>,
    mut gunshot_reader: MessageReader<GunshotEvent>,
) {
    let dt = time.delta_secs();
    let soldier_radius = SOLDIER_RADIUS;
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();
    let low_walls: Vec<(&Transform, &LowWallCollider)> = low_walls_q.iter().collect();

    // Fire team targets (Player + living Friendlies)
    let mut fireteam_targets: Vec<(Entity, Vec2, Quat)> = Vec::new();
    for (p_ent, p_tf) in &player_q {
        fireteam_targets.push((
            p_ent,
            Vec2::new(p_tf.translation.x, p_tf.translation.z),
            p_tf.rotation,
        ));
    }
    for (f_ent, f_tf) in &friendly_q {
        fireteam_targets.push((
            f_ent,
            Vec2::new(f_tf.translation.x, f_tf.translation.z),
            f_tf.rotation,
        ));
    }

    let heard_gunshots: Vec<GunshotEvent> = gunshot_reader.read().copied().collect();

    for (mut tf, mut zombie, mut soldier) in &mut zombie_q {
        zombie.wander_timer.tick(time.delta());
        zombie.attack_cooldown.tick(time.delta());

        // Zombies NEVER crouch or go prone
        soldier.stance = Stance::Standing;

        let zombie_pos = Vec2::new(tf.translation.x, tf.translation.z);

        // 1. Vision: check if any fire team member is inside zombie's vision cone and line-of-sight
        let mut visible_fireteam: Vec<(Vec2, f32)> = Vec::new();
        for (_, s_pos, _) in &fireteam_targets {
            if can_see_target(
                zombie_pos,
                soldier.facing_dir,
                *s_pos,
                zombie.vision_range,
                zombie.vision_half_angle,
                zombie.inner_radius,
                &buildings,
            ) {
                visible_fireteam.push((*s_pos, (*s_pos - zombie_pos).length_squared()));
            }
        }
        visible_fireteam.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some(&(closest_fireteam_pos, _)) = visible_fireteam.first() {
            // Spotted fire team member: alert and chase target
            zombie.target_pos = Some(closest_fireteam_pos);
        } else if zombie.target_pos.is_none() {
            // 2. Hearing: check if nearby gunfire from player team occurred
            for event in &heard_gunshots {
                if event.shooter_team == Team::Player {
                    let to_gunshot = event.position - zombie_pos;
                    let dist = to_gunshot.length();
                    if dist <= 55.0 && dist > 0.1 {
                        zombie.target_pos = Some(event.position);
                        break;
                    }
                }
            }
        }

        // 3. Movement
        if let Some(target) = zombie.target_pos {
            let to_target = target - zombie_pos;
            let dist = to_target.length();

            if dist > 0.7 {
                let desired_move = to_target / dist;
                let move_dir = find_avoidance_direction(
                    zombie_pos,
                    desired_move,
                    &buildings,
                    &low_walls,
                    soldier_radius,
                );
                let dir_mult = directional_speed_multiplier(soldier.facing_dir, move_dir);
                soldier.facing_dir = move_dir;
                let next_pos = zombie_pos + move_dir * (zombie.speed * dir_mult) * dt;
                let resolved = resolve_circle_obstacle_collision(
                    next_pos,
                    soldier_radius,
                    &buildings,
                    &low_walls,
                );
                tf.translation.x = resolved.x;
                tf.translation.z = resolved.y;
            } else {
                // Reached target position with no target in sight: return to random wander
                zombie.target_pos = None;
                zombie.wander_timer.reset();
            }
        } else {
            // Wandering randomly
            if zombie.wander_timer.is_finished() {
                zombie.wander_timer.reset();
                let seed =
                    time.elapsed_secs() * 11.3 + tf.translation.x * 2.7 + tf.translation.z * 5.9;
                let angle = seed.sin() * std::f32::consts::PI;
                zombie.wander_dir = Vec2::new(angle.cos(), angle.sin()).normalize_or_zero();
                if zombie.wander_dir.length_squared() < 0.01 {
                    zombie.wander_dir = Vec2::new(0.0, 1.0);
                }
            }

            let move_dir = find_avoidance_direction(
                zombie_pos,
                zombie.wander_dir,
                &buildings,
                &low_walls,
                soldier_radius,
            );
            let dir_mult = directional_speed_multiplier(soldier.facing_dir, move_dir);
            soldier.facing_dir = move_dir;
            let wander_speed = zombie.speed * 0.65 * dir_mult;
            let next_pos = zombie_pos + move_dir * wander_speed * dt;
            let resolved =
                resolve_circle_obstacle_collision(next_pos, soldier_radius, &buildings, &low_walls);
            tf.translation.x = resolved.x;
            tf.translation.z = resolved.y;
        }

        // 4. Melee contact attack on player fire team
        let melee_reach = soldier_radius * 2.0 + 0.35; // 1.55m (pairwise collision pushes to ~1.20m)
        let melee_reach_sq = melee_reach * melee_reach;

        if zombie.attack_cooldown.is_finished() {
            let mut victim_idx = None;
            for (idx, &(s_ent, s_pos, s_rot)) in fireteam_targets.iter().enumerate() {
                if (s_pos - zombie_pos).length_squared() <= melee_reach_sq {
                    zombie.attack_cooldown.reset();
                    zombie.target_pos = None;

                    // Lethal melee strike: convert victim soldier to corpse with blood pool
                    let mut entity_cmds = commands.entity(s_ent);
                    entity_cmds.despawn_children();
                    entity_cmds
                        .remove::<Soldier>()
                        .remove::<Team>()
                        .remove::<Player>()
                        .remove::<crate::friendly::Friendly>()
                        .insert((
                            crate::common::types::Corpse,
                            Visibility::Inherited,
                            Transform::from_xyz(s_pos.x, 0.03, s_pos.y)
                                .with_rotation(s_rot)
                                .with_scale(Vec3::new(0.65, 0.95, 0.65)),
                            MeshMaterial3d(corpse_assets.player_corpse_material.clone()),
                        ));

                    // Primary blood pool
                    commands.spawn((
                        LevelEntity,
                        Mesh3d(corpse_assets.blood_pool_mesh.clone()),
                        MeshMaterial3d(corpse_assets.blood_material.clone()),
                        Transform::from_xyz(s_pos.x, 0.015, s_pos.y)
                            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                        crate::common::types::BloodDecal,
                    ));

                    // Small splatter droplets
                    let splatters = [Vec2::new(0.50, 0.40), Vec2::new(-0.45, -0.35)];
                    for offset in splatters {
                        commands.spawn((
                            LevelEntity,
                            Mesh3d(corpse_assets.blood_splatter_mesh.clone()),
                            MeshMaterial3d(corpse_assets.blood_material.clone()),
                            Transform::from_xyz(s_pos.x + offset.x, 0.015, s_pos.y + offset.y)
                                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                            crate::common::types::BloodDecal,
                        ));
                    }

                    victim_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = victim_idx {
                fireteam_targets.swap_remove(idx);
            }
        }

        // Visual orientation
        let yaw = f32::atan2(-soldier.facing_dir.x, -soldier.facing_dir.y);
        tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2);
        tf.scale = Vec3::ONE; // Always standing silhouette
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
                    inventory: crate::common::types::Inventory::default(),
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
                    inventory: crate::common::types::Inventory::default(),
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
                    inventory: crate::common::types::Inventory::default(),
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
                inventory: crate::common::types::Inventory::default(),
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
                    inventory: crate::common::types::Inventory::default(),
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

    #[test]
    fn test_setup_zombies_spawns_20_zombies_without_cone_meshes() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.insert_resource(CurrentLevel::Level2);
        app.insert_resource(ZombieSpawnConfig { count: 20 });

        app.add_systems(Update, setup_enemy);
        app.update();

        let mut zombie_count = 0;

        let mut zombie_q = app
            .world_mut()
            .query::<(Entity, &Zombie, &Soldier, &Enemy)>();
        for (z_entity, _, soldier, _) in zombie_q.iter(app.world()) {
            zombie_count += 1;
            assert_eq!(soldier.stance, Stance::Standing);

            // Verify children: must NOT have a vision cone mesh
            // Vision cone meshes are identified by not being pointer circles (radius != 0.15)
            // or by checking the number of children (only 1 child: the direction pointer)
            let children = app.world().get::<Children>(z_entity);
            if let Some(children) = children {
                // Should only have 1 child (pointer) and NOT 2 children (pointer + cone)
                assert_eq!(
                    children.len(),
                    1,
                    "Zombie should have exactly 1 child (pointer), but found {}",
                    children.len()
                );
            }
        }

        assert_eq!(zombie_count, 20, "Level 2 must spawn exactly 20 zombies");
    }

    #[test]
    fn test_zombie_hears_gunfire_and_pursues_without_crouching() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Time>();
        app.add_message::<GunshotEvent>();

        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let blood_pool_mesh = meshes.add(Circle::new(0.85));
        let blood_splatter_mesh = meshes.add(Circle::new(0.22));
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let blood_mat = materials.add(Color::srgb(0.5, 0.0, 0.0));
        let corpse_mat = materials.add(Color::srgb(0.3, 0.3, 0.3));
        app.insert_resource(crate::combat::bullet::CorpseAssets {
            blood_pool_mesh,
            blood_splatter_mesh,
            blood_material: blood_mat,
            player_corpse_material: corpse_mat.clone(),
            enemy_corpse_material: corpse_mat,
        });

        // Spawn zombie at (0.0, 0.0) facing North (0.0, -1.0)
        let zombie_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 0.0),
                Zombie {
                    wander_dir: Vec2::new(0.0, -1.0),
                    wander_timer: Timer::from_seconds(3.0, TimerMode::Repeating),
                    target_pos: None,
                    speed: 2.4,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.5,
                    attack_cooldown: {
                        let mut t = Timer::from_seconds(0.8, TimerMode::Once);
                        t.set_elapsed(std::time::Duration::from_secs(1));
                        t
                    },
                },
                Enemy {
                    state: EnemyAiState::Search,
                    current_waypoint_idx: 0,
                    last_known_player_pos: None,
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(999.0, TimerMode::Once),
                    speed: 2.4,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.5,
                    stuck_timer: 0.0,
                    last_pos: Vec2::ZERO,
                },
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(999.0, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        app.add_systems(Update, update_zombies);

        // Advance time delta to 0.1s
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(100));

        // Player fires gun at South (0.0, 20.0)
        app.world_mut().write_message(GunshotEvent {
            position: Vec2::new(0.0, 20.0),
            shooter_team: Team::Player,
        });

        app.update();

        let zombie = app.world().get::<Zombie>(zombie_entity).unwrap();
        let soldier = app.world().get::<Soldier>(zombie_entity).unwrap();
        let tf = app.world().get::<Transform>(zombie_entity).unwrap();

        // Zombie heard gunshot, set target_pos
        assert_eq!(zombie.target_pos, Some(Vec2::new(0.0, 20.0)));
        // Zombie never crouches or goes prone
        assert_eq!(soldier.stance, Stance::Standing);
        // Zombie faced towards gunfire (South)
        assert!(
            soldier.facing_dir.y > 0.9,
            "Zombie should turn South toward gunshot"
        );
        // Zombie moved toward gunshot
        assert!(tf.translation.z > 0.0, "Zombie should move toward target");
    }

    #[test]
    fn test_dynamic_zombie_spawns_have_no_collisions() {
        let count = 25;
        let seed = 12345;
        let spawns = generate_dynamic_zombie_spawns(count, seed);

        assert_eq!(
            spawns.len(),
            count,
            "Should generate requested number of spawns"
        );

        let soldier_radius = SOLDIER_RADIUS;
        let min_sep = soldier_radius * 2.0;

        for (i, &pos) in spawns.iter().enumerate() {
            // Must be within world bounds
            assert!(
                pos.x.abs() <= SOLDIER_WORLD_BOUND,
                "Spawn {} ({:?}) out of world bounds X",
                i,
                pos
            );
            assert!(
                pos.y.abs() <= SOLDIER_WORLD_BOUND,
                "Spawn {} ({:?}) out of world bounds Y",
                i,
                pos
            );

            // Safe from player & squad spawn at (0, 48) and outside 24m vision range
            let to_player = pos - Vec2::new(0.0, 48.0);
            assert!(
                to_player.length() >= 28.0,
                "Spawn {} ({:?}) too close to player/squad spawn",
                i,
                pos
            );

            // Must not penetrate any building
            for (b_idx, (size, b_pos)) in COMPOUND_BUILDINGS.iter().enumerate() {
                let center = Vec2::new(b_pos.x, b_pos.z);
                let half_size = Vec2::new(size.x * 0.5, size.z * 0.5);
                let closest = Vec2::new(
                    pos.x.clamp(center.x - half_size.x, center.x + half_size.x),
                    pos.y.clamp(center.y - half_size.y, center.y + half_size.y),
                );
                let dist = (pos - closest).length();
                assert!(
                    dist >= soldier_radius,
                    "Spawn {} ({:?}) penetrates building {} (center: {:?}, half_size: {:?}, dist: {})",
                    i,
                    pos,
                    b_idx,
                    center,
                    half_size,
                    dist
                );
            }

            // Must not penetrate any low wall
            for (w_idx, (size, w_pos)) in COMPOUND_LOW_WALLS.iter().enumerate() {
                let center = Vec2::new(w_pos.x, w_pos.z);
                let half_size = Vec2::new(size.x * 0.5, size.z * 0.5);
                let closest = Vec2::new(
                    pos.x.clamp(center.x - half_size.x, center.x + half_size.x),
                    pos.y.clamp(center.y - half_size.y, center.y + half_size.y),
                );
                let dist = (pos - closest).length();
                assert!(
                    dist >= soldier_radius,
                    "Spawn {} ({:?}) penetrates low wall {} (dist: {})",
                    i,
                    pos,
                    w_idx,
                    dist
                );
            }

            // Must not overlap with any other spawned zombie
            for (j, &other) in spawns.iter().enumerate() {
                if i != j {
                    let sep = (pos - other).length();
                    assert!(
                        sep >= min_sep,
                        "Spawn {} ({:?}) and spawn {} ({:?}) collide (dist: {} < min: {})",
                        i,
                        pos,
                        j,
                        other,
                        sep,
                        min_sep
                    );
                }
            }
        }
    }

    #[test]
    fn test_custom_zombie_spawn_count_via_config() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.insert_resource(CurrentLevel::Level2);
        app.insert_resource(ZombieSpawnConfig { count: 32 });

        app.add_systems(Update, setup_enemy);
        app.update();

        let zombie_count = app
            .world_mut()
            .query::<(&Zombie, &Soldier, &Enemy)>()
            .iter(app.world())
            .count();

        assert_eq!(zombie_count, 32, "Should spawn 32 zombies as configured");
    }

    #[test]
    fn test_enemy_reloads_when_magazine_empty() {
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

        // Spawn player at (0.0, -8.0)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, -8.0),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, 1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        // Spawn enemy at (0, 0) facing North (0, -1) with 0 ammo in mag
        let mut empty_inv = crate::common::types::Inventory::default();
        empty_inv.active_weapon_mut().unwrap().ammo.current_mag = 0;

        let enemy_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 0.0),
                Enemy {
                    state: EnemyAiState::Engage,
                    current_waypoint_idx: 0,
                    last_known_player_pos: Some(Vec2::new(0.0, -8.0)),
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(0.05, TimerMode::Repeating),
                    speed: 5.5,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    stuck_timer: 0.0,
                    last_pos: Vec2::ZERO,
                },
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: empty_inv,
                },
            ))
            .id();

        app.add_systems(Update, update_enemy);

        // Frame 1: Triggers shoot check, discovers mag is empty, starts reloading
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(60));
        app.update();

        let soldier = app.world().get::<Soldier>(enemy_entity).unwrap();
        let weapon = soldier.inventory.active_weapon().unwrap();
        assert!(
            weapon.ammo.is_reloading(),
            "Enemy should start reload when empty"
        );
        assert_eq!(weapon.ammo.current_mag, 0);

        // Advance 2.1 seconds to finish reload
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(2100));
        app.update();

        let soldier = app.world().get::<Soldier>(enemy_entity).unwrap();
        let weapon = soldier.inventory.active_weapon().unwrap();
        assert!(
            !weapon.ammo.is_reloading(),
            "Reload should finish after 2.0 seconds"
        );
        // Reloaded to 30 rounds, and immediately engaged target by firing 1 round
        assert_eq!(weapon.ammo.current_mag, 29);
        assert_eq!(weapon.ammo.reserve_mags, 5);
    }

    #[test]
    fn test_enemy_switches_to_sidearm_when_main_weapon_runs_out_of_ammo() {
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

        // Spawn player at (0.0, -8.0)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, -8.0),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, 1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        // Enemy with Rifle (0 mag, 0 reserve) + Pistol (17 rounds)
        let mut rifle = crate::common::types::Weapon::rifle();
        rifle.ammo.current_mag = 0;
        rifle.ammo.reserve_mags = 0;
        let pistol = crate::common::types::Weapon::pistol();
        let inv = crate::common::types::Inventory::new(vec![rifle, pistol]);

        let enemy_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 0.0),
                Enemy {
                    state: EnemyAiState::Engage,
                    current_waypoint_idx: 0,
                    last_known_player_pos: Some(Vec2::new(0.0, -8.0)),
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(0.05, TimerMode::Repeating),
                    speed: 5.5,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    stuck_timer: 0.0,
                    last_pos: Vec2::ZERO,
                },
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: inv,
                },
            ))
            .id();

        app.add_systems(Update, update_enemy);

        // Frame 1: Enemy engages, detects rifle is out of ammo, triggers swap to sidearm!
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(60));
        app.update();

        let soldier = app.world().get::<Soldier>(enemy_entity).unwrap();
        assert!(soldier.inventory.is_swapping());
        assert_eq!(
            soldier.inventory.pending_weapon().unwrap().weapon_type,
            crate::common::types::WeaponType::Pistol
        );

        // Advance 2.1 seconds to finish swap
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(2100));
        app.update();

        let soldier = app.world().get::<Soldier>(enemy_entity).unwrap();
        assert!(!soldier.inventory.is_swapping());
        let active = soldier.inventory.active_weapon().unwrap();
        assert_eq!(active.weapon_type, crate::common::types::WeaponType::Pistol);
        assert_eq!(active.ammo.current_mag, 16); // Fired 1 pistol round
    }

    #[test]
    fn test_zombie_melee_kills_player_at_collision_distance() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Time>();
        app.add_message::<GunshotEvent>();

        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let blood_pool_mesh = meshes.add(Circle::new(0.85));
        let blood_splatter_mesh = meshes.add(Circle::new(0.22));
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let blood_mat = materials.add(Color::srgb(0.5, 0.0, 0.0));
        let corpse_mat = materials.add(Color::srgb(0.3, 0.3, 0.3));
        app.insert_resource(crate::combat::bullet::CorpseAssets {
            blood_pool_mesh,
            blood_splatter_mesh,
            blood_material: blood_mat,
            player_corpse_material: corpse_mat.clone(),
            enemy_corpse_material: corpse_mat,
        });

        // Player at origin
        let player_entity = app
            .world_mut()
            .spawn((
                Player,
                Team::Player,
                Transform::from_xyz(0.0, 0.05, 0.0)
                    .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        // Zombie at (0.0, 1.25) - within the melee reach (1.55m) and at typical collision contact distance (1.20m)
        let _zombie_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 1.25),
                Zombie {
                    wander_dir: Vec2::new(0.0, -1.0),
                    wander_timer: Timer::from_seconds(3.0, TimerMode::Repeating),
                    target_pos: Some(Vec2::new(0.0, 0.0)),
                    speed: 2.4,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.5,
                    attack_cooldown: {
                        let mut t = Timer::from_seconds(0.8, TimerMode::Once);
                        t.set_elapsed(std::time::Duration::from_secs(1));
                        t
                    },
                },
                Enemy {
                    state: EnemyAiState::Engage,
                    current_waypoint_idx: 0,
                    last_known_player_pos: None,
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(999.0, TimerMode::Once),
                    speed: 2.4,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.5,
                    stuck_timer: 0.0,
                    last_pos: Vec2::ZERO,
                },
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(999.0, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        app.add_systems(Update, update_zombies);

        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(50));

        app.update();

        // Player must have been killed and converted to Corpse
        assert!(
            app.world().get::<Player>(player_entity).is_none(),
            "Player component should be removed on death"
        );
        assert!(
            app.world().get::<Soldier>(player_entity).is_none(),
            "Soldier component should be removed on death"
        );
        assert!(
            app.world()
                .get::<crate::common::types::Corpse>(player_entity)
                .is_some(),
            "Victim should gain Corpse component"
        );

        // Blood decal must be spawned
        let blood_count = app
            .world_mut()
            .query::<&crate::common::types::BloodDecal>()
            .iter(app.world())
            .count();
        assert!(blood_count >= 1, "At least 1 blood decal should be spawned");
    }

    #[test]
    fn test_courtyard_level_3_layout_and_spawns() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.insert_resource(CurrentLevel::Level3);
        app.add_systems(
            Startup,
            (
                crate::world::setup_level_world,
                crate::player::setup_player,
                crate::friendly::setup_friendly,
                setup_enemy,
            ),
        );
        app.update();

        // 1. Courtyard map has 0 buildings and 4 waist-high walls arranged in a square
        let building_count = app
            .world_mut()
            .query::<&Building>()
            .iter(app.world())
            .count();
        assert_eq!(
            building_count, 0,
            "Level 3 courtyard should have no buildings"
        );

        let wall_positions: Vec<Vec2> = app
            .world_mut()
            .query_filtered::<&Transform, With<LowWall>>()
            .iter(app.world())
            .map(|tf| Vec2::new(tf.translation.x, tf.translation.z))
            .collect();
        assert_eq!(
            wall_positions.len(),
            4,
            "Level 3 courtyard should have 4 low walls"
        );

        let north_z = wall_positions
            .iter()
            .map(|p| p.y)
            .fold(f32::INFINITY, f32::min);
        let south_z = wall_positions
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let courtyard_span = south_z - north_z;
        let rifle_range = crate::common::types::Weapon::rifle().bullet_speed()
            * crate::common::types::Weapon::rifle().bullet_lifetime();
        assert!(
            courtyard_span > 10.0 && courtyard_span + 6.0 <= 24.0 && courtyard_span < rifle_range,
            "Courtyard span ({}) should allow firing and seeing across from one side to the other",
            courtyard_span
        );

        // 2. Enemy fire team (4 soldiers) spawns on the north side facing South
        let enemies: Vec<(Vec2, Vec2, f32)> = app
            .world_mut()
            .query::<(&Enemy, &Transform, &Soldier)>()
            .iter(app.world())
            .map(|(e, tf, s)| {
                (
                    Vec2::new(tf.translation.x, tf.translation.z),
                    s.facing_dir,
                    e.vision_range,
                )
            })
            .collect();
        assert_eq!(
            enemies.len(),
            4,
            "Level 3 should spawn a 4-man enemy fire team"
        );

        let player_pos = app
            .world_mut()
            .query_filtered::<&Transform, With<Player>>()
            .single(app.world())
            .map(|tf| Vec2::new(tf.translation.x, tf.translation.z))
            .unwrap();

        for &(e_pos, facing_dir, vision_range) in &enemies {
            assert!(
                e_pos.y < north_z,
                "Enemy fire team member should start on north side of courtyard (z < {}), got {}",
                north_z,
                e_pos.y
            );
            assert!(facing_dir.y > 0.5, "Enemy fire team should face South");
            let dist_to_player = (player_pos - e_pos).length();
            assert!(
                dist_to_player > vision_range + 10.0,
                "Friendly squad should start far enough south to not be in immediate firing range (dist={}, vision={})",
                dist_to_player,
                vision_range
            );
        }
    }

    #[test]
    fn test_courtyard_enemy_crouches_behind_wall_instead_of_prone_and_fires_over_wall() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Time>();
        app.add_message::<GunshotEvent>();
        app.insert_resource(CurrentLevel::Level3);

        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let bullet_mesh = meshes.add(Circle::new(0.12));
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let dummy_mat = materials.add(Color::srgb(1.0, 1.0, 1.0));
        app.insert_resource(BulletAssets {
            mesh: bullet_mesh,
            player_material: dummy_mat.clone(),
            enemy_material: dummy_mat,
        });

        // North courtyard wall at z = -8.0 (half_size = 8.0 x 0.5)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.375, -8.0),
            LowWall,
            LowWallCollider {
                half_size: Vec2::new(8.0, 0.5),
            },
        ));

        // Enemy on north side at (-3.0, -11.5) facing South
        let enemy_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(-3.0, 0.05, -11.5),
                Enemy {
                    state: EnemyAiState::Search,
                    current_waypoint_idx: 0,
                    last_known_player_pos: None,
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(0.25, TimerMode::Repeating),
                    speed: 5.5,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    stuck_timer: 0.0,
                    last_pos: Vec2::new(-3.0, -11.5),
                },
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        // Player on south side of courtyard at (-3.0, 9.5) (21m away -> > 14m, within 24m vision)
        app.world_mut().spawn((
            Transform::from_xyz(-3.0, 0.05, 9.5),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Crouching,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        app.add_systems(Update, update_enemy);

        // Advance several frames so enemy moves directly behind the north wall and fires
        for _ in 0..20 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        let tf = app.world().get::<Transform>(enemy_entity).unwrap();
        let soldier = app.world().get::<Soldier>(enemy_entity).unwrap();

        // Enemy moved directly behind the north wall (z ≈ -9.35) and is NOT Prone
        assert!(
            (tf.translation.z - (-9.35)).abs() < 0.4,
            "Enemy should move directly behind the north wall (z ≈ -9.35), got z={}",
            tf.translation.z
        );
        assert_ne!(
            soldier.stance,
            Stance::Prone,
            "Enemy behind low wall must not go Prone (which would cause shots to hit the wall)"
        );

        // Verify spawned bullets have Standing shooter_stance so they clear the low wall
        let bullets: Vec<&Bullet> = app
            .world_mut()
            .query::<&Bullet>()
            .iter(app.world())
            .collect();
        assert!(
            !bullets.is_empty(),
            "Enemy should fire across the courtyard"
        );
        for b in bullets {
            assert_eq!(
                b.shooter_stance,
                Stance::Standing,
                "Bullets fired from behind a low wall must pop up to Standing to clear the wall"
            );
        }
    }
}
