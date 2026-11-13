use crate::combat::bullet::{Bullet, BulletAssets};
use crate::common::geometry::{
    apply_shoot_spread, can_see_target, directional_speed_multiplier, find_avoidance_direction,
    resolve_circle_obstacle_collision,
};
use crate::common::types::{GunshotEvent, LevelEntity, SOLDIER_RADIUS, Soldier, Stance, Team};
use crate::enemy::ai::Enemy;
use crate::fog::mesh::create_vision_cone_mesh;
use crate::friendly::ai::{Friendly, FriendlyAiState};
use crate::player::Player;
use crate::world::buildings::{Building, BuildingCollider, LowWall, LowWallCollider};
use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

/// Compute `(formation_spot, base_facing, flank_angle)` for a friendly soldier based on fireteam and slot.
fn compute_formation_target(
    fireteam: usize,
    slot: usize,
    p_pos: Vec2,
    p_facing: Vec2,
    ft1_lead_info: Option<(Vec2, Vec2)>,
    ft2_lead_info: Option<(Vec2, Vec2)>,
) -> (Vec2, Vec2, f32) {
    if fireteam == 1 {
        // 1st Fire Team (Alpha) follows Player (Squad Leader) in tactical wedge
        if slot == 0 {
            // Alpha Team Leader trails Player directly (2.5m behind)
            (p_pos - p_facing * 2.5, p_facing, 0.0)
        } else {
            // Alpha wingmen and rear guard follow Alpha Team Leader
            let (ft1_anchor, ft1_facing) = ft1_lead_info.unwrap_or_else(|| {
                (p_pos - p_facing * 2.5, p_facing)
            });
            let ft1_right = Vec2::new(-ft1_facing.y, ft1_facing.x);
            let (lateral, behind, angle) = match slot {
                1 => (-2.2, 1.8, -35.0_f32.to_radians()),
                2 => (2.2, 1.8, 35.0_f32.to_radians()),
                _ => (0.0, 3.6, 180.0_f32.to_radians()),
            };
            (ft1_anchor + ft1_right * lateral - ft1_facing * behind, ft1_facing, angle)
        }
    } else {
        // 2nd Fire Team (Bravo) follows as trail element of the squad
        if slot == 0 {
            // Bravo Team Leader trails behind 1st Fire Team (7.5m behind Player)
            (p_pos - p_facing * 7.5, p_facing, 0.0)
        } else {
            // Bravo wingmen and rear security follow Bravo Team Leader
            let (ft2_anchor, ft2_facing) = ft2_lead_info.unwrap_or_else(|| {
                (p_pos - p_facing * 7.5, p_facing)
            });
            let ft2_right = Vec2::new(-ft2_facing.y, ft2_facing.x);
            let (lateral, behind, angle) = match slot {
                1 => (-2.2, 1.8, -35.0_f32.to_radians()),
                2 => (2.2, 1.8, 35.0_f32.to_radians()),
                _ => (0.0, 3.6, 180.0_f32.to_radians()),
            };
            (ft2_anchor + ft2_right * lateral - ft2_facing * behind, ft2_facing, angle)
        }
    }
}

// Spawns friendly squad members organized into two fire teams:
// - Fire Team 1 (Alpha): 3 friendlies flanking and trailing the Player (Squad Leader)
// - Fire Team 2 (Bravo): 4 friendlies following behind Fire Team 1
pub fn setup_friendly(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let pointer_mesh = meshes.add(Circle::new(0.15));
    let friendly_soldier_mesh = meshes.add(Circle::new(SOLDIER_RADIUS));

    // FT1 (Alpha): tactical cyan/teal color
    let ft1_soldier_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.68, 0.88),
        perceptual_roughness: 0.5,
        ..default()
    });

    // FT2 (Bravo): tactical cobalt/azure color
    let ft2_soldier_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.24, 0.54, 0.90),
        perceptual_roughness: 0.5,
        ..default()
    });

    let friendly_pointer_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.85, 0.95, 1.0),
        unlit: true,
        ..default()
    });

    // Vision cone meshes & materials for both fire teams
    let friendly_cone_mesh = meshes.add(create_vision_cone_mesh(24.0, FRAC_PI_2, 16));
    let ft1_cone_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.20, 0.75, 1.0, 0.16),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    let ft2_cone_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.26, 0.60, 1.0, 0.16),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    // Spawn 8 friendly squad members:
    // - 1st Fire Team (Alpha, 4 friendlies) follows Player (Squad Leader)
    // - 2nd Fire Team (Bravo, 4 friendlies) follows 1st Fire Team
    // Completing a 9-man rifle squad with the Player leading
    let friendly_configs = [
        // === 1st Fire Team (Alpha) - Follows Player (Squad Leader) ===
        // Alpha Team Leader: fireteam 1, slot 0 (trails Player directly)
        (
            1,
            0,
            Vec2::new(0.0, 50.5),
            Vec2::new(0.0, -1.0),
        ),
        // Alpha Left Flank: fireteam 1, slot 1
        (
            1,
            1,
            Vec2::new(-2.2, 52.3),
            Vec2::new(-0.35, -0.94).normalize(),
        ),
        // Alpha Right Flank: fireteam 1, slot 2
        (
            1,
            2,
            Vec2::new(2.2, 52.3),
            Vec2::new(0.35, -0.94).normalize(),
        ),
        // Alpha Rear Guard: fireteam 1, slot 3 (covers FT1 rear)
        (
            1,
            3,
            Vec2::new(0.0, 54.1),
            Vec2::new(0.0, 1.0),
        ),

        // === 2nd Fire Team (Bravo) - Follows 1st Fire Team ===
        // Bravo Team Leader: fireteam 2, slot 0 (trails FT1)
        (
            2,
            0,
            Vec2::new(0.0, 55.5),
            Vec2::new(0.0, -1.0),
        ),
        // Bravo Left Flank: fireteam 2, slot 1
        (
            2,
            1,
            Vec2::new(-2.2, 57.3),
            Vec2::new(-0.35, -0.94).normalize(),
        ),
        // Bravo Right Flank: fireteam 2, slot 2
        (
            2,
            2,
            Vec2::new(2.2, 57.3),
            Vec2::new(0.35, -0.94).normalize(),
        ),
        // Bravo Rear Security: fireteam 2, slot 3 (squad rear security facing South)
        (
            2,
            3,
            Vec2::new(0.0, 57.9),
            Vec2::new(0.0, 1.0),
        ),
    ];

    for (fireteam, slot, pos, facing) in friendly_configs {
        let yaw = f32::atan2(-facing.x, -facing.y);
        let soldier_mat = if fireteam == 1 {
            ft1_soldier_material.clone()
        } else {
            ft2_soldier_material.clone()
        };
        let cone_mat = if fireteam == 1 {
            ft1_cone_material.clone()
        } else {
            ft2_cone_material.clone()
        };

        let friendly = commands
            .spawn((
                LevelEntity,
                Mesh3d(friendly_soldier_mesh.clone()),
                MeshMaterial3d(soldier_mat),
                Transform::from_xyz(pos.x, 0.05, pos.y)
                    .with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2)),
                Friendly {
                    fireteam,
                    slot,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(
                        0.35 + (((fireteam - 1) * 4 + slot) as f32 * 0.05),
                        TimerMode::Repeating,
                    ),
                    speed: 12.0,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                Soldier {
                    facing_dir: facing,
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        // Attach direction pointer to friendly soldier
        commands.spawn((
            Mesh3d(pointer_mesh.clone()),
            MeshMaterial3d(friendly_pointer_material.clone()),
            Transform::from_xyz(0.0, 0.42, 0.01),
            ChildOf(friendly),
        ));

        // Attach visible vision cone to friendly soldier
        commands.spawn((
            Mesh3d(friendly_cone_mesh.clone()),
            MeshMaterial3d(cone_mat),
            Transform::from_xyz(0.0, 0.0, 0.005),
            ChildOf(friendly),
        ));
    }
}

// Friendly fire team AI: follows player in tactical wedge formation, scans flanks and rear,
// takes cover behind low walls, and engages visible enemies with coordinated fire.
pub fn update_friendly(
    mut commands: Commands,
    time: Res<Time>,
    mut friendly_q: Query<
        (&mut Transform, &mut Friendly, &mut Soldier),
        (With<Friendly>, Without<Player>, Without<Enemy>),
    >,
    player_q: Query<(&Transform, &Soldier), (With<Player>, Without<Friendly>, Without<Enemy>)>,
    enemy_q: Query<(&Transform, &Soldier), (With<Enemy>, Without<Friendly>, Without<Player>)>,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (
            With<Building>,
            Without<Friendly>,
            Without<Player>,
            Without<Enemy>,
        ),
    >,
    low_walls_q: Query<
        (&Transform, &LowWallCollider),
        (
            With<LowWall>,
            Without<Friendly>,
            Without<Player>,
            Without<Enemy>,
        ),
    >,
    bullet_assets: Res<BulletAssets>,
    mut gunshot_mutator: MessageMutator<GunshotEvent>,
) {
    let dt = time.delta_secs();
    let soldier_radius = SOLDIER_RADIUS;
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();
    let low_walls: Vec<(&Transform, &LowWallCollider)> = low_walls_q.iter().collect();

    let player_info = player_q.iter().next().map(|(tf, s)| {
        (
            Vec2::new(tf.translation.x, tf.translation.z),
            s.facing_dir,
            s.stance,
        )
    });

    // Capture snapshot of living friendlies to coordinate squad and fire team following
    let friendly_snapshots: Vec<(usize, usize, Vec2, Vec2)> = friendly_q
        .iter()
        .map(|(tf, f, s)| {
            (
                f.fireteam,
                f.slot,
                Vec2::new(tf.translation.x, tf.translation.z),
                s.facing_dir,
            )
        })
        .collect();
    let ft1_lead_info = friendly_snapshots
        .iter()
        .find(|(ft, slot, _, _)| *ft == 1 && *slot == 0)
        .map(|(_, _, pos, facing)| (*pos, *facing));
    let ft2_lead_info = friendly_snapshots
        .iter()
        .find(|(ft, slot, _, _)| *ft == 2 && *slot == 0)
        .map(|(_, _, pos, facing)| (*pos, *facing));

    let heard_gunshots: Vec<GunshotEvent> = gunshot_mutator.read().map(|e| *e).collect();

    for (mut tf, mut friendly, mut soldier) in &mut friendly_q {
        soldier.inventory.tick(time.delta());
        let friendly_pos = Vec2::new(tf.translation.x, tf.translation.z);

        // 1. Target acquisition: scan vision cone for living enemies in line of sight
        let mut visible_enemies: Vec<(Vec2, f32)> = Vec::new();
        for (e_tf, _) in &enemy_q {
            let e_pos = Vec2::new(e_tf.translation.x, e_tf.translation.z);
            if can_see_target(
                friendly_pos,
                soldier.facing_dir,
                e_pos,
                friendly.vision_range,
                friendly.vision_half_angle,
                friendly.inner_radius,
                &buildings,
            ) {
                let dist_sq = (e_pos - friendly_pos).length_squared();
                visible_enemies.push((e_pos, dist_sq));
            }
        }
        visible_enemies.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // 2. React to enemy gunfire if no target is currently visible
        if visible_enemies.is_empty() && friendly.state != FriendlyAiState::Engage {
            for event in &heard_gunshots {
                if event.shooter_team == Team::Enemy {
                    let to_gunshot = event.position - friendly_pos;
                    let dist = to_gunshot.length();
                    if dist < 50.0 && dist > 0.1 {
                        // Swivel facing direction toward hostile gunfire to investigate
                        soldier.facing_dir = to_gunshot.normalize();
                        break;
                    }
                }
            }
        }

        // 3. State transitions
        if let Some(&(nearest_enemy_pos, _)) = visible_enemies.first() {
            friendly.state = FriendlyAiState::Engage;
            friendly.target_pos = Some(nearest_enemy_pos);
            friendly.target_lost_timer = 0.0;
        } else if friendly.state == FriendlyAiState::Engage {
            friendly.target_lost_timer += dt;
            if friendly.target_lost_timer > 1.5 {
                // Lost sight of enemy: return to formation follow
                friendly.state = FriendlyAiState::Follow;
                friendly.target_pos = None;
                friendly.target_lost_timer = 0.0;
            }
        }

        // 4. State behaviors
        match friendly.state {
            FriendlyAiState::Engage => {
                if let Some(target) = friendly.target_pos {
                    let to_target = target - friendly_pos;
                    let dist = to_target.length();

                    if dist > 0.01 {
                        soldier.facing_dir = to_target / dist;
                    }

                    // Keep moving toward formation if far out of position while engaging
                    let mut is_moving = false;
                    if let Some((p_pos, p_facing, _)) = player_info {
                        let (formation_spot, _, _) = compute_formation_target(
                            friendly.fireteam,
                            friendly.slot,
                            p_pos,
                            p_facing,
                            ft1_lead_info,
                            ft2_lead_info,
                        );
                        let to_spot = formation_spot - friendly_pos;
                        let dist_to_spot = to_spot.length();
                        if dist_to_spot > 1.5 {
                            is_moving = true;
                            soldier.stance = Stance::Standing;
                            let desired_move = to_spot / dist_to_spot;
                            let move_dir = find_avoidance_direction(
                                friendly_pos,
                                desired_move,
                                &buildings,
                                &low_walls,
                                soldier_radius,
                            );
                            let catch_up = if dist_to_spot > 5.0 {
                                1.35
                            } else if dist_to_spot > 2.0 {
                                1.18
                            } else {
                                1.0
                            };
                            let dir_mult = directional_speed_multiplier(soldier.facing_dir, move_dir);
                            let speed = friendly.speed * catch_up * dir_mult;
                            let next_pos = friendly_pos + move_dir * speed * dt;
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

                    let current_pos = Vec2::new(tf.translation.x, tf.translation.z);
                    let near_low_wall = !is_moving
                        && low_walls.iter().any(|(w_tf, _)| {
                            let w_pos = Vec2::new(w_tf.translation.x, w_tf.translation.z);
                            (w_pos - current_pos).length() < 3.0
                        });

                    if !is_moving {
                        if near_low_wall {
                            // Take cover behind low wall
                            soldier.stance = Stance::Crouching;
                        } else if dist < 14.0 {
                            // Tactical crouch at medium range in open field
                            soldier.stance = Stance::Crouching;
                        } else {
                            soldier.stance = Stance::Standing;
                        }
                    }

                    // If active weapon is completely out of ammunition and soldier has a sidearm, switch to sidearm
                    if let Some(active) = soldier.inventory.active_weapon() {
                        if active.ammo.is_completely_empty() && soldier.inventory.has_sidearm() && !soldier.inventory.is_swapping() {
                            soldier.inventory.start_swap();
                        }
                    }

                    // Fire weapon at target enemy if not swapping
                    friendly.shoot_timer.tick(time.delta());
                    if friendly.shoot_timer.is_finished() && !soldier.inventory.is_swapping() {
                        let mut should_fire = false;
                        let mut bullet_speed = 45.0;
                        let mut bullet_lifetime = 1.5;

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
                            friendly.shoot_timer.reset();

                            // Pop up to standing momentarily to fire over low wall
                            if near_low_wall {
                                soldier.stance = Stance::Standing;
                            }

                            let spread_seed = time.elapsed_secs().to_bits() as u64
                                ^ (friendly.slot as u64)
                                ^ ((friendly.fireteam as u64) << 8)
                                ^ (current_pos.x.to_bits() as u64)
                                ^ ((current_pos.y.to_bits() as u64) << 16);
                            let shot_dir = apply_shoot_spread(soldier.facing_dir, is_moving, spread_seed);
                            let spawn_pos = current_pos + soldier.facing_dir * 0.85;
                            commands.spawn((
                                LevelEntity,
                                Mesh3d(bullet_assets.mesh.clone()),
                                MeshMaterial3d(bullet_assets.player_material.clone()),
                                Transform::from_xyz(spawn_pos.x, 0.08, spawn_pos.y)
                                    .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                                Bullet {
                                    origin: current_pos,
                                    direction: shot_dir,
                                    speed: bullet_speed,
                                    lifetime: bullet_lifetime,
                                    team: Team::Player,
                                    shooter_stance: soldier.stance,
                                    missed_entities: Vec::new(),
                                },
                            ));

                            gunshot_mutator.write(GunshotEvent {
                                position: current_pos,
                                shooter_team: Team::Player,
                            });
                        }
                    }
                }
            }
            FriendlyAiState::Follow => {
                if let Some((p_pos, p_facing, p_stance)) = player_info {
                    let (formation_spot, base_facing, flank_angle) = compute_formation_target(
                        friendly.fireteam,
                        friendly.slot,
                        p_pos,
                        p_facing,
                        ft1_lead_info,
                        ft2_lead_info,
                    );

                    // Flank guard facing direction (rotates base facing direction by flank angle)
                    let cos_a = flank_angle.cos();
                    let sin_a = flank_angle.sin();
                    let guard_facing = Vec2::new(
                        base_facing.x * cos_a - base_facing.y * sin_a,
                        base_facing.x * sin_a + base_facing.y * cos_a,
                    )
                    .normalize();

                    let to_spot = formation_spot - friendly_pos;
                    let dist_to_spot = to_spot.length();

                    // Inherit player stance during patrol/follow
                    soldier.stance = p_stance;

                    if dist_to_spot > 0.85 {
                        // Moving to maintain formation
                        let desired_move = to_spot / dist_to_spot;
                        let move_dir = find_avoidance_direction(
                            friendly_pos,
                            desired_move,
                            &buildings,
                            &low_walls,
                            soldier_radius,
                        );

                        // When actively moving to catch up, face the movement direction;
                        // when arriving at formation spot, orient towards guard flank
                        if dist_to_spot < 1.2 {
                            soldier.facing_dir = guard_facing;
                        } else {
                            soldier.facing_dir = move_dir;
                        }

                        let speed_multiplier = match soldier.stance {
                            Stance::Standing => 1.0,
                            Stance::Crouching => 0.6,
                            Stance::Prone => 0.28,
                        };

                        // Catch-up acceleration if fallen behind
                        let catch_up = if dist_to_spot > 5.0 {
                            1.35
                        } else if dist_to_spot > 2.0 {
                            1.18
                        } else {
                            1.0
                        };
                        // Moving forward is normal speed; lateral or backwards is 50% normal speed
                        let dir_mult = directional_speed_multiplier(soldier.facing_dir, move_dir);
                        let speed = friendly.speed * speed_multiplier * catch_up * dir_mult;

                        let next_pos = friendly_pos + move_dir * speed * dt;
                        let resolved = resolve_circle_obstacle_collision(
                            next_pos,
                            soldier_radius,
                            &buildings,
                            &low_walls,
                        );
                        tf.translation.x = resolved.x;
                        tf.translation.z = resolved.y;
                    } else {
                        // In formation spot: hold flank guard orientation
                        soldier.facing_dir = guard_facing;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_app() -> App {
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
        app
    }

    #[test]
    fn test_friendly_follows_player_in_formation() {
        let mut app = create_test_app();

        // Spawn player at (0, 0) facing North (0, -1)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, 0.0),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        // Spawn friendly far behind at (0, 15.0)
        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 15.0),
                Friendly {
                    fireteam: 1,
                    slot: 0, // Left wingman
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.4, TimerMode::Repeating),
                    speed: 5.6,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        app.add_systems(Update, update_friendly);

        // Advance a few frames
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(200));

        app.update();

        let friendly_tf = app.world().get::<Transform>(friendly_entity).unwrap();

        // Friendly should have moved forward towards the player (Z < 15.0)
        assert!(
            friendly_tf.translation.z < 15.0,
            "Friendly should move forward to follow player, Z was: {}",
            friendly_tf.translation.z
        );
    }

    #[test]
    fn test_friendly_inherits_player_stance_in_follow_mode() {
        let mut app = create_test_app();

        // Spawn player crouching
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, 0.0),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Crouching,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(-2.4, 0.05, 2.0),
                Friendly {
                    fireteam: 1,
                    slot: 0,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.4, TimerMode::Repeating),
                    speed: 5.6,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        app.add_systems(Update, update_friendly);
        app.update();

        let soldier = app.world().get::<Soldier>(friendly_entity).unwrap();
        assert_eq!(
            soldier.stance,
            Stance::Crouching,
            "Friendly should match player's crouching stance in formation"
        );
    }

    #[test]
    fn test_friendly_engages_visible_enemy_and_fires() {
        let mut app = create_test_app();

        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, 0.0),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        // Friendly at (0.0, 2.0) facing North (0.0, -1.0)
        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 2.0),
                Friendly {
                    fireteam: 1,
                    slot: 0,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.1, TimerMode::Repeating),
                    speed: 5.6,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        // Enemy at (0.0, -10.0) (directly in friendly's vision cone)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, -10.0),
            Enemy {
                state: crate::enemy::ai::EnemyAiState::Search,
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
        ));

        app.add_systems(Update, update_friendly);

        // Advance time enough to trigger shooting
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(200));

        app.update();

        let friendly = app.world().get::<Friendly>(friendly_entity).unwrap();
        let soldier = app.world().get::<Soldier>(friendly_entity).unwrap();

        // 1. Transitioned to Engage state
        assert_eq!(friendly.state, FriendlyAiState::Engage);

        // 2. Facing enemy (North: y ≈ -1.0)
        assert!(soldier.facing_dir.y < -0.9);

        // 3. Fired a bullet
        let mut bullets_q = app.world_mut().query::<&Bullet>();
        let bullet_count = bullets_q.iter(app.world()).count();
        assert!(bullet_count > 0, "Friendly should fire at visible enemy");
    }

    #[test]
    fn test_friendly_takes_cover_behind_low_wall_when_engaging() {
        let mut app = create_test_app();

        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, 0.0),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        // Spawn low wall at (0.0, 1.0)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.4, 1.0),
            LowWall,
            LowWallCollider {
                half_size: Vec2::new(2.0, 0.4),
            },
        ));

        // Friendly at (0.0, 2.0) (within 1.0m of low wall)
        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 2.0),
                Friendly {
                    fireteam: 1,
                    slot: 0,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 5.6,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        // Enemy at (0.0, -10.0)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, -10.0),
            Enemy {
                state: crate::enemy::ai::EnemyAiState::Search,
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
        ));

        app.add_systems(Update, update_friendly);
        app.update();

        let soldier = app.world().get::<Soldier>(friendly_entity).unwrap();
        assert_eq!(
            soldier.stance,
            Stance::Crouching,
            "Friendly should take cover (crouch) behind low wall during combat"
        );
    }

    #[test]
    fn test_friendly_reloads_when_magazine_empty() {
        let mut app = create_test_app();

        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, 0.0),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        // Friendly with 0 rounds in current magazine
        let mut empty_inv = crate::common::types::Inventory::default();
        empty_inv.active_weapon_mut().unwrap().ammo.current_mag = 0;

        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 2.0),
                Friendly {
                    fireteam: 1,
                    slot: 0,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.05, TimerMode::Repeating),
                    speed: 5.6,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: empty_inv,
                },
            ))
            .id();

        // Enemy at (0.0, -10.0)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, -10.0),
            Enemy {
                state: crate::enemy::ai::EnemyAiState::Search,
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
        ));

        app.add_systems(Update, update_friendly);

        // Frame 1: Triggers shoot check, discovers mag is empty, starts reloading
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(60));
        app.update();

        let soldier = app.world().get::<Soldier>(friendly_entity).unwrap();
        let weapon = soldier.inventory.active_weapon().unwrap();
        assert!(weapon.ammo.is_reloading(), "Friendly should start reloading when mag is empty");
        assert_eq!(weapon.ammo.current_mag, 0);

        // No bullets should have been fired
        let mut bullets_q = app.world_mut().query::<&Bullet>();
        assert_eq!(bullets_q.iter(app.world()).count(), 0);

        // Advance 2.1 seconds to finish reload
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(2100));
        app.update();

        let soldier = app.world().get::<Soldier>(friendly_entity).unwrap();
        let weapon = soldier.inventory.active_weapon().unwrap();
        assert!(!weapon.ammo.is_reloading(), "Reload should be finished");
        // Reloaded to 30 rounds, and immediately resumed combat by firing 1 round
        assert_eq!(weapon.ammo.current_mag, 29);
        assert_eq!(weapon.ammo.reserve_mags, 5);
    }

    #[test]
    fn test_friendly_switches_to_sidearm_when_main_weapon_runs_out_of_ammo() {
        let mut app = create_test_app();

        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, 0.0),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        // Friendly with Rifle (completely empty: 0 in mag, 0 reserve) + Pistol (17 rounds)
        let mut rifle = crate::common::types::Weapon::rifle();
        rifle.ammo.current_mag = 0;
        rifle.ammo.reserve_mags = 0;
        let pistol = crate::common::types::Weapon::pistol();
        let inv = crate::common::types::Inventory::new(vec![rifle, pistol]);

        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 2.0),
                Friendly {
                    fireteam: 1,
                    slot: 0,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.05, TimerMode::Repeating),
                    speed: 5.6,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: inv,
                },
            ))
            .id();

        // Enemy at (0.0, -10.0)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, -10.0),
            Enemy {
                state: crate::enemy::ai::EnemyAiState::Search,
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
        ));

        app.add_systems(Update, update_friendly);

        // Frame 1: Friendly detects enemy, engages, sees rifle has 0 ammo, triggers swap to pistol!
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(60));
        app.update();

        let soldier = app.world().get::<Soldier>(friendly_entity).unwrap();
        assert!(soldier.inventory.is_swapping(), "Friendly should be swapping to sidearm");
        assert_eq!(
            soldier.inventory.pending_weapon().unwrap().weapon_type,
            crate::common::types::WeaponType::Pistol
        );

        // Advance 2.1s to finish swap
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(2100));
        app.update();

        let soldier = app.world().get::<Soldier>(friendly_entity).unwrap();
        assert!(!soldier.inventory.is_swapping());
        let active = soldier.inventory.active_weapon().unwrap();
        assert_eq!(active.weapon_type, crate::common::types::WeaponType::Pistol);
        // Pistol fired 1 round
        assert_eq!(active.ammo.current_mag, 16);
    }

    #[test]
    fn test_setup_friendly_spawns_squad_with_two_fireteams() {
        let mut app = create_test_app();
        app.add_systems(Update, setup_friendly);
        app.update();

        let friendlies: Vec<(&Friendly, &Soldier, &Transform)> = app
            .world_mut()
            .query::<(&Friendly, &Soldier, &Transform)>()
            .iter(app.world())
            .collect();

        // 8 friendly members spawned (4 in FT1 + 4 in FT2, completing 9-man rifle squad with player as Squad Leader)
        assert_eq!(
            friendlies.len(),
            8,
            "Squad must spawn exactly 8 friendly members across 2 fire teams"
        );

        let ft1_members: Vec<&Friendly> = friendlies.iter().map(|(f, _, _)| *f).filter(|f| f.fireteam == 1).collect();
        let ft2_members: Vec<&Friendly> = friendlies.iter().map(|(f, _, _)| *f).filter(|f| f.fireteam == 2).collect();

        assert_eq!(ft1_members.len(), 4, "1st Fire Team (Alpha) must have 4 members");
        assert_eq!(ft2_members.len(), 4, "2nd Fire Team (Bravo) must have 4 members");

        let mut ft1_slots: Vec<usize> = ft1_members.iter().map(|f| f.slot).collect();
        ft1_slots.sort();
        assert_eq!(ft1_slots, vec![0, 1, 2, 3]);

        let mut ft2_slots: Vec<usize> = ft2_members.iter().map(|f| f.slot).collect();
        ft2_slots.sort();
        assert_eq!(ft2_slots, vec![0, 1, 2, 3]);

        // FT1 Slot 0 is Alpha Team Leader directly trailing player
        let ft1_slot0 = friendlies.iter().find(|(f, _, _)| f.fireteam == 1 && f.slot == 0).unwrap();
        assert_eq!(ft1_slot0.2.translation.x, 0.0);
        assert_eq!(ft1_slot0.2.translation.z, 50.5);
        assert!(
            ft1_slot0.1.facing_dir.y < -0.9,
            "FT1 Slot 0 must face North"
        );

        // FT1 Slot 3 is Alpha Rear Guard
        let ft1_slot3 = friendlies.iter().find(|(f, _, _)| f.fireteam == 1 && f.slot == 3).unwrap();
        assert_eq!(ft1_slot3.2.translation.x, 0.0);
        assert_eq!(ft1_slot3.2.translation.z, 54.1);
        assert!(
            ft1_slot3.1.facing_dir.y > 0.9,
            "FT1 Slot 3 must face South (rear guard)"
        );

        // FT2 Slot 0 is Bravo Leader trailing behind FT1
        let ft2_slot0 = friendlies.iter().find(|(f, _, _)| f.fireteam == 2 && f.slot == 0).unwrap();
        assert_eq!(ft2_slot0.2.translation.x, 0.0);
        assert_eq!(ft2_slot0.2.translation.z, 55.5);
        assert!(
            ft2_slot0.1.facing_dir.y < -0.9,
            "FT2 Leader must face forward (North)"
        );

        // FT2 Slot 3 is squad rear security facing South
        let ft2_slot3 = friendlies.iter().find(|(f, _, _)| f.fireteam == 2 && f.slot == 3).unwrap();
        assert_eq!(ft2_slot3.2.translation.x, 0.0);
        assert_eq!(ft2_slot3.2.translation.z, 57.9);
        assert!(
            ft2_slot3.1.facing_dir.y > 0.9,
            "FT2 Slot 3 must face South for squad rear security"
        );
    }

    #[test]
    fn test_friendly_slot_3_provides_squad_rear_security_in_formation() {
        let mut app = create_test_app();

        // Spawn player (Squad Leader) at (0, 0) facing North (0, -1)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, 0.0),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        // Spawn friendly FT2 slot 3 near its formation spot
        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 12.2),
                Friendly {
                    fireteam: 2,
                    slot: 3, // Rear security
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 12.0,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0), // Facing rear
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        app.add_systems(Update, update_friendly);
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(50));
        app.update();

        let soldier = app.world().get::<Soldier>(friendly_entity).unwrap();
        // Since player faces North (0, -1), rear guard facing must be South (0, 1)
        assert!(
            soldier.facing_dir.y > 0.9,
            "Slot 3 must hold rear guard facing (South), was: {:?}",
            soldier.facing_dir
        );
    }

    #[test]
    fn test_second_fireteam_follows_first_fireteam_in_squad_formation() {
        let mut app = create_test_app();

        // Player (Squad Leader) at (0, 0) facing North (0, -1)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, 0.0),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        // FT1 Leader (Slot 0) far behind at (0.0, 10.0)
        let ft1_lead_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 10.0),
                Friendly {
                    fireteam: 1,
                    slot: 0,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 12.0,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        // FT2 Leader (Slot 0) far behind at (0.0, 20.0)
        let ft2_lead_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 20.0),
                Friendly {
                    fireteam: 2,
                    slot: 0,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 12.0,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        app.add_systems(Update, update_friendly);

        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(300));
        app.update();

        let ft1_lead_tf = app.world().get::<Transform>(ft1_lead_entity).unwrap();
        let ft2_lead_tf = app.world().get::<Transform>(ft2_lead_entity).unwrap();

        // FT1 Leader must advance forward toward Player (Z < 10.0)
        assert!(
            ft1_lead_tf.translation.z < 10.0,
            "FT1 Leader should move forward toward Player, Z was: {}",
            ft1_lead_tf.translation.z
        );

        // FT2 Leader must advance towards FT1 (Z < 20.0)
        assert!(
            ft2_lead_tf.translation.z < 20.0,
            "FT2 Leader should move forward toward FT1, Z was: {}",
            ft2_lead_tf.translation.z
        );
    }

    #[test]
    fn test_squad_moves_together_with_player_when_player_moves_around() {
        let mut app = create_test_app();

        // Player (Squad Leader) at (0, 0) facing North (0, -1)
        let player_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 0.0),
                Player,
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        // Spawn Alpha Leader (slot 0) at (0.0, 2.5)
        let alpha_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 2.5),
                Friendly {
                    fireteam: 1,
                    slot: 0,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 12.0,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        // Spawn Bravo Leader (slot 0) at (0.0, 8.6)
        let bravo_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 8.6),
                Friendly {
                    fireteam: 2,
                    slot: 0,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 12.0,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        app.add_systems(Update, update_friendly);

        // Player moves North by 10 meters: translation.z = -10.0
        let mut player_tf = app.world_mut().get_mut::<Transform>(player_entity).unwrap();
        player_tf.translation.z = -10.0;

        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(500));
        app.update();

        let alpha_tf = app.world().get::<Transform>(alpha_entity).unwrap();
        let bravo_tf = app.world().get::<Transform>(bravo_entity).unwrap();

        // Both Alpha and Bravo must actively advance North following the player
        assert!(
            alpha_tf.translation.z < 2.5,
            "Alpha should move forward with player, was: {}",
            alpha_tf.translation.z
        );
        assert!(
            bravo_tf.translation.z < 8.6,
            "Bravo should move forward with player, was: {}",
            bravo_tf.translation.z
        );
    }

    #[test]
    fn test_full_squad_follows_player_through_gate_in_world() {
        let mut app = create_test_app();
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.init_state::<crate::common::types::AppState>();
        app.init_resource::<crate::common::types::CurrentLevel>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.add_plugins((
            crate::player::PlayerPlugin,
            crate::friendly::FriendlyPlugin,
        ));
        app.add_systems(
            OnEnter(crate::common::types::AppState::InGame),
            crate::world::setup_level_world,
        );
        app.add_systems(
            Update,
            crate::common::geometry::resolve_soldier_pairwise_collisions
                .after(crate::player::update_player)
                .after(update_friendly)
                .run_if(in_state(crate::common::types::AppState::InGame)),
        );

        // Transition MainMenu -> InGame
        app.update();
        app.world_mut()
            .resource_mut::<NextState<crate::common::types::AppState>>()
            .set(crate::common::types::AppState::InGame);
        app.update();

        // Hold W for 120 frames (2.0s at 60fps)
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        for _ in 0..120 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
            app.update();
        }

        let player_z = app
            .world_mut()
            .query_filtered::<&Transform, With<Player>>()
            .single(app.world())
            .unwrap()
            .translation
            .z;

        let friendlies: Vec<(&Friendly, &Transform)> = app
            .world_mut()
            .query::<(&Friendly, &Transform)>()
            .iter(app.world())
            .collect();

        assert_eq!(friendlies.len(), 8);
        for (f, tf) in &friendlies {
            let dz = tf.translation.z - player_z;
            assert!(
                tf.translation.z < 42.0,
                "FT{} slot {} stuck! Expected it to pass Z=44 gate with player (player_z={}), actual z={}",
                f.fireteam,
                f.slot,
                player_z,
                tf.translation.z
            );
            assert!(
                dz < 16.0,
                "FT{} slot {} fell too far behind player! dz={}",
                f.fireteam,
                f.slot,
                dz
            );
        }
    }

    #[test]
    fn test_friendly_fires_while_moving_back_into_formation() {
        let mut app = create_test_app();

        // Player at origin facing North
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, 0.0),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        // Friendly far out of formation at z=12.0 (expected formation spot is z=2.5)
        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 12.0),
                Friendly {
                    fireteam: 1,
                    slot: 0,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.1, TimerMode::Repeating),
                    speed: 12.0,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        // Enemy visible in front at z=-6.0 (18m away, within 24m vision range)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, -6.0),
            Enemy {
                state: crate::enemy::ai::EnemyAiState::Search,
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
        ));

        app.add_systems(Update, update_friendly);

        for _ in 0..10 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        let tf = app.world().get::<Transform>(friendly_entity).unwrap();
        assert!(
            tf.translation.z < 9.0,
            "Friendly should move toward formation (z=2.5) while engaging, got z={}",
            tf.translation.z
        );

        let bullets: Vec<&Bullet> = app
            .world_mut()
            .query::<&Bullet>()
            .iter(app.world())
            .collect();
        assert!(
            !bullets.is_empty(),
            "Friendly should fire at visible enemy while moving back into formation"
        );
    }
}
