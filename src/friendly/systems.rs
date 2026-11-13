use crate::combat::bullet::{Bullet, BulletAssets};
use crate::common::geometry::{
    can_see_target, directional_speed_multiplier, find_avoidance_direction,
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

// Spawns 2 friendly squad members flanking the player in a wedge formation
pub fn setup_friendly(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let pointer_mesh = meshes.add(Circle::new(0.15));
    let friendly_soldier_mesh = meshes.add(Circle::new(SOLDIER_RADIUS));

    // Friendly soldiers: tactical squad teal/blue color
    let friendly_soldier_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.68, 0.88),
        perceptual_roughness: 0.5,
        ..default()
    });

    let friendly_pointer_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.85, 0.95, 1.0),
        unlit: true,
        ..default()
    });

    // Friendly vision cone mesh & material (cyan translucent 90° cone, 24 units range)
    let friendly_cone_mesh = meshes.add(create_vision_cone_mesh(24.0, FRAC_PI_2, 16));
    let friendly_cone_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.20, 0.75, 1.0, 0.16),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    // Spawn 2 friendlies flanking player's starting gate position (0.0, 48.0)
    let friendly_configs = [
        // Left wingman: slot 0, offset (-2.4, 2.2) behind-left
        (
            Vec2::new(-2.4, 50.2),
            Vec2::new(-0.35, -0.94).normalize(),
            0,
        ),
        // Right wingman: slot 1, offset (2.4, 2.2) behind-right
        (Vec2::new(2.4, 50.2), Vec2::new(0.35, -0.94).normalize(), 1),
    ];

    for (pos, facing, slot) in friendly_configs {
        let yaw = f32::atan2(-facing.x, -facing.y);
        let friendly = commands
            .spawn((
                LevelEntity,
                Mesh3d(friendly_soldier_mesh.clone()),
                MeshMaterial3d(friendly_soldier_material.clone()),
                Transform::from_xyz(pos.x, 0.05, pos.y)
                    .with_rotation(Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2)),
                Friendly {
                    slot,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(
                        0.38 + (slot as f32 * 0.08),
                        TimerMode::Repeating,
                    ),
                    speed: 5.6,
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
                    ammo: crate::common::types::WeaponAmmo::default(),
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
            MeshMaterial3d(friendly_cone_material.clone()),
            Transform::from_xyz(0.0, 0.0, 0.005),
            ChildOf(friendly),
        ));
    }
}

// Friendly squad AI: follows player in tactical wedge formation, scans flanks,
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

    let heard_gunshots: Vec<GunshotEvent> = gunshot_mutator.read().map(|e| *e).collect();

    for (mut tf, mut friendly, mut soldier) in &mut friendly_q {
        soldier.ammo.tick_reload(time.delta());
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

                    // Check proximity to low wall for cover
                    let near_low_wall = low_walls.iter().any(|(w_tf, _)| {
                        let w_pos = Vec2::new(w_tf.translation.x, w_tf.translation.z);
                        (w_pos - friendly_pos).length() < 3.0
                    });

                    if near_low_wall {
                        // Take cover behind low wall
                        soldier.stance = Stance::Crouching;
                    } else if dist < 14.0 {
                        // Tactical crouch at medium range in open field
                        soldier.stance = Stance::Crouching;
                    } else {
                        soldier.stance = Stance::Standing;
                    }

                    // Fire weapon at target enemy
                    friendly.shoot_timer.tick(time.delta());
                    if friendly.shoot_timer.is_finished() {
                        if soldier.ammo.current_mag == 0 {
                            soldier.ammo.start_reload();
                        } else if soldier.ammo.can_fire() {
                            friendly.shoot_timer.reset();
                            soldier.ammo.current_mag -= 1;

                            // Pop up to standing momentarily to fire over low wall
                            if near_low_wall {
                                soldier.stance = Stance::Standing;
                            }

                            let spawn_pos = friendly_pos + soldier.facing_dir * 0.85;
                            commands.spawn((
                                LevelEntity,
                                Mesh3d(bullet_assets.mesh.clone()),
                                MeshMaterial3d(bullet_assets.player_material.clone()),
                                Transform::from_xyz(spawn_pos.x, 0.08, spawn_pos.y)
                                    .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                                Bullet {
                                    origin: friendly_pos,
                                    direction: soldier.facing_dir,
                                    speed: 45.0,
                                    lifetime: 1.5,
                                    team: Team::Player,
                                    shooter_stance: soldier.stance,
                                    missed_entities: Vec::new(),
                                },
                            ));

                            gunshot_mutator.write(GunshotEvent {
                                position: friendly_pos,
                                shooter_team: Team::Player,
                            });
                        }
                    }
                }
            }
            FriendlyAiState::Follow => {
                if let Some((p_pos, p_facing, p_stance)) = player_info {
                    // Right vector perpendicular to player's facing direction
                    let right = Vec2::new(-p_facing.y, p_facing.x);

                    // Wedge formation offset:
                    // Slot 0 (Left flank): -2.4m right (left), -2.0m facing (behind)
                    // Slot 1 (Right flank): +2.4m right, -2.0m facing (behind)
                    let (lateral_offset, flank_angle) = if friendly.slot == 0 {
                        (-2.4, -35.0_f32.to_radians())
                    } else {
                        (2.4, 35.0_f32.to_radians())
                    };

                    let formation_spot = p_pos + right * lateral_offset - p_facing * 2.0;

                    // Flank guard facing direction (rotates player facing direction by flank angle)
                    let cos_a = flank_angle.cos();
                    let sin_a = flank_angle.sin();
                    let guard_facing = Vec2::new(
                        p_facing.x * cos_a - p_facing.y * sin_a,
                        p_facing.x * sin_a + p_facing.y * cos_a,
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

                        // If close to spot, face guard flank; if far, face movement direction
                        if dist_to_spot < 2.0 {
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
                        let catch_up = if dist_to_spot > 3.5 { 1.25 } else { 1.0 };
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
                ammo: crate::common::types::WeaponAmmo::default(),
            },
        ));

        // Spawn friendly far behind at (0, 15.0)
        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 15.0),
                Friendly {
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
                    ammo: crate::common::types::WeaponAmmo::default(),
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
                ammo: crate::common::types::WeaponAmmo::default(),
            },
        ));

        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(-2.4, 0.05, 2.0),
                Friendly {
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
                    ammo: crate::common::types::WeaponAmmo::default(),
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
                ammo: crate::common::types::WeaponAmmo::default(),
            },
        ));

        // Friendly at (0.0, 2.0) facing North (0.0, -1.0)
        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 2.0),
                Friendly {
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
                    ammo: crate::common::types::WeaponAmmo::default(),
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
                ammo: crate::common::types::WeaponAmmo::default(),
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
                ammo: crate::common::types::WeaponAmmo::default(),
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
                    ammo: crate::common::types::WeaponAmmo::default(),
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
                ammo: crate::common::types::WeaponAmmo::default(),
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
                ammo: crate::common::types::WeaponAmmo::default(),
            },
        ));

        // Friendly with 0 rounds in current magazine
        let mut empty_ammo = crate::common::types::WeaponAmmo::default();
        empty_ammo.current_mag = 0;

        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 2.0),
                Friendly {
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
                    ammo: empty_ammo,
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
                ammo: crate::common::types::WeaponAmmo::default(),
            },
        ));

        app.add_systems(Update, update_friendly);

        // Frame 1: Triggers shoot check, discovers mag is empty, starts reloading
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(60));
        app.update();

        let soldier = app.world().get::<Soldier>(friendly_entity).unwrap();
        assert!(soldier.ammo.is_reloading(), "Friendly should start reloading when mag is empty");
        assert_eq!(soldier.ammo.current_mag, 0);

        // No bullets should have been fired
        let mut bullets_q = app.world_mut().query::<&Bullet>();
        assert_eq!(bullets_q.iter(app.world()).count(), 0);

        // Advance 2.1 seconds to finish reload
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(2100));
        app.update();

        let soldier = app.world().get::<Soldier>(friendly_entity).unwrap();
        assert!(!soldier.ammo.is_reloading(), "Reload should be finished");
        // Reloaded to 30 rounds, and immediately resumed combat by firing 1 round
        assert_eq!(soldier.ammo.current_mag, 29);
        assert_eq!(soldier.ammo.reserve_mags, 5);
    }
}
