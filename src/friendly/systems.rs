use crate::combat::bullet::{Bullet, BulletAssets};
use crate::common::geometry::{
    apply_shoot_spread, can_see_target, compute_flank_waypoint, directional_speed_multiplier,
    find_avoidance_direction, find_cover_position_with_offset, has_clear_shot_past_cover,
    has_low_wall_in_line_of_fire, is_near_low_wall, resolve_circle_obstacle_collision_bounded,
};
use crate::common::types::{
    CommanderOrder, CurrentLevel, GunshotEvent, LevelEntity, RTS_SOLDIER_WORLD_BOUND,
    SOLDIER_RADIUS, SOLDIER_WORLD_BOUND, Soldier, SquadCommand, Stance, Suppression, Team,
};
use crate::enemy::ai::{Enemy, EnemyAiState};
use crate::fog::mesh::create_vision_cone_mesh;
use crate::friendly::ai::{
    Friendly, FriendlyAiState, SquadDoctrineState, SquadId, SquadLeaderDecision, SquadTacticalPhase,
};
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
            let (ft1_anchor, ft1_facing) =
                ft1_lead_info.unwrap_or_else(|| (p_pos - p_facing * 2.5, p_facing));
            let ft1_right = Vec2::new(-ft1_facing.y, ft1_facing.x);
            let (lateral, behind, angle) = match slot {
                1 => (-2.2, 1.8, -35.0_f32.to_radians()),
                2 => (2.2, 1.8, 35.0_f32.to_radians()),
                _ => (0.0, 3.6, 180.0_f32.to_radians()),
            };
            (
                ft1_anchor + ft1_right * lateral - ft1_facing * behind,
                ft1_facing,
                angle,
            )
        }
    } else {
        // 2nd Fire Team (Bravo) follows as trail element of the squad
        if slot == 0 {
            // Bravo Team Leader trails behind 1st Fire Team (7.5m behind Player)
            (p_pos - p_facing * 7.5, p_facing, 0.0)
        } else {
            // Bravo wingmen and rear security follow Bravo Team Leader
            let (ft2_anchor, ft2_facing) =
                ft2_lead_info.unwrap_or_else(|| (p_pos - p_facing * 7.5, p_facing));
            let ft2_right = Vec2::new(-ft2_facing.y, ft2_facing.x);
            let (lateral, behind, angle) = match slot {
                1 => (-2.2, 1.8, -35.0_f32.to_radians()),
                2 => (2.2, 1.8, 35.0_f32.to_radians()),
                _ => (0.0, 3.6, 180.0_f32.to_radians()),
            };
            (
                ft2_anchor + ft2_right * lateral - ft2_facing * behind,
                ft2_facing,
                angle,
            )
        }
    }
}

/// Positions for the 9-man friendly squad on the road south of the village in `test_level_4`.
pub const RTS_SOUTH_ROAD_FRIENDLY_CONFIGS: [(usize, usize, Vec2, Vec2); 9] = [
    // Squad Leader leading at the head of the road formation
    (1, 0, Vec2::new(0.0, 178.0), Vec2::new(0.0, -1.0)),
    // 1st Fire Team (Alpha) on the road
    (1, 1, Vec2::new(-2.2, 180.0), Vec2::new(0.0, -1.0)),
    (1, 2, Vec2::new(2.2, 180.0), Vec2::new(0.0, -1.0)),
    (1, 3, Vec2::new(-1.2, 182.0), Vec2::new(0.0, -1.0)),
    (1, 4, Vec2::new(1.2, 182.0), Vec2::new(0.0, -1.0)),
    // 2nd Fire Team (Bravo) trailing on the road
    (2, 0, Vec2::new(0.0, 184.5), Vec2::new(0.0, -1.0)),
    (2, 1, Vec2::new(-2.2, 186.3), Vec2::new(0.0, -1.0)),
    (2, 2, Vec2::new(2.2, 186.3), Vec2::new(0.0, -1.0)),
    (2, 3, Vec2::new(0.0, 188.0), Vec2::new(0.0, 1.0)),
];

// Spawns friendly squad members organized into two fire teams:
// - Fire Team 1 (Alpha): friendlies flanking and trailing the Player (Squad Leader)
// - Fire Team 2 (Bravo): friendlies following behind Fire Team 1
pub fn setup_friendly(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut squad_command: Option<ResMut<SquadCommand>>,
    current_level: Option<Res<CurrentLevel>>,
) {
    if let Some(ref mut cmd) = squad_command {
        **cmd = SquadCommand::Formation;
    }
    let level = current_level.as_deref().copied().unwrap_or_default();
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

    // Spawn friendly squad members:
    // - In Level 1–3: 8 friendly squad members trailing the Player (Squad Leader) at Z = 50.5..57.9
    // - In Level 4 (RTS): 9-man friendly squad on the road a fair way south of the village (Z = 178.0..188.0)
    let standard_configs = [
        // === 1st Fire Team (Alpha) - Follows Player (Squad Leader) ===
        // Alpha Team Leader: fireteam 1, slot 0 (trails Player directly)
        (1, 0, Vec2::new(0.0, 50.5), Vec2::new(0.0, -1.0)),
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
        (1, 3, Vec2::new(0.0, 54.1), Vec2::new(0.0, 1.0)),
        // === 2nd Fire Team (Bravo) - Follows 1st Fire Team ===
        // Bravo Team Leader: fireteam 2, slot 0 (trails FT1)
        (2, 0, Vec2::new(0.0, 55.5), Vec2::new(0.0, -1.0)),
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
        (2, 3, Vec2::new(0.0, 57.9), Vec2::new(0.0, 1.0)),
    ];

    let friendly_configs: &[(usize, usize, Vec2, Vec2)] = if level == CurrentLevel::Level4 {
        &RTS_SOUTH_ROAD_FRIENDLY_CONFIGS
    } else {
        &standard_configs
    };

    for &(fireteam, slot, pos, facing) in friendly_configs {
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
                SquadId(0),
                Soldier {
                    facing_dir: facing,
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
                Suppression::default(),
            ))
            .id();

        // Attach direction pointer to friendly soldier (local +Z is world +Y, above soldier body)
        commands.spawn((
            Mesh3d(pointer_mesh.clone()),
            MeshMaterial3d(friendly_pointer_material.clone()),
            Transform::from_xyz(0.0, 0.42, 0.015),
            ChildOf(friendly),
        ));

        // Attach visible vision cone to friendly soldier (local -Z is world -Y, below soldier body and above road/ground)
        commands.spawn((
            Mesh3d(friendly_cone_mesh.clone()),
            MeshMaterial3d(cone_mat),
            Transform::from_xyz(0.0, 0.0, -0.015),
            ChildOf(friendly),
        ));
    }
}

/// Rapid METT-TC (Mission, Enemy, Terrain, Troops, Time, Civilians) evaluation performed by an
/// infantry Squad Leader upon making contact with an enemy force.
/// Selects one of the five doctrinal U.S. Army Squad Leader tactical decisions:
/// 1. `SquadAttack` (Flank and Destroy)
/// 2. `BreakContact` (Disengage and Withdraw)
/// 3. `FixAndSupport` (Hold in Place)
/// 4. `Bypass` (Avoid and Report)
/// 5. `HastyAmbush` (Wait and Spring)
pub fn evaluate_squad_leader_mett_tc(
    squad_leader_pos: Vec2,
    contact_center: Vec2,
    squad_alive_count: usize,
    perceived_enemy_count: usize,
    suppressed_count: usize,
    is_undetected: bool,
    enemy_approaching: bool,
    squad_objective: Option<Vec2>,
    buildings: &[(&Transform, &BuildingCollider)],
    low_walls: &[(&Transform, &LowWallCollider)],
    world_bound: f32,
) -> SquadLeaderDecision {
    let has_nearby_cover = buildings.iter().any(|(tf, _)| {
        let c = Vec2::new(tf.translation.x, tf.translation.z);
        (c - squad_leader_pos).length() < 36.0 || (c - contact_center).length() < 36.0
    }) || low_walls.iter().any(|(tf, _)| {
        let c = Vec2::new(tf.translation.x, tf.translation.z);
        (c - squad_leader_pos).length() < 36.0 || (c - contact_center).length() < 36.0
    });

    // 1. Stealth / Unspotted Contact Checks (Visual Contact Only)
    if is_undetected {
        // Decision 5: Hasty Ambush (Wait and Spring)
        // Chosen when the squad detects an enemy element moving toward them before being detected,
        // and the terrain offers nearby concealment/cover along the enemy's route.
        if enemy_approaching
            && (has_nearby_cover || (contact_center - squad_leader_pos).length() <= 28.0)
        {
            return SquadLeaderDecision::HastyAmbush {
                kill_zone: contact_center,
                triggered: false,
                trigger_timer: 0.0,
            };
        }

        // Decision 4: Bypass (Avoid and Report)
        // Chosen when the squad spots the enemy before being detected, and the Commander's objective
        // prioritizes reaching a deeper position (> 45m beyond the enemy pocket).
        if let Some(obj) = squad_objective {
            let obj_dist_from_enemy = (obj - contact_center).length();
            let sl_to_enemy = contact_center - squad_leader_pos;
            let sl_to_obj = obj - squad_leader_pos;
            if obj_dist_from_enemy > 45.0
                && sl_to_obj.length() > sl_to_enemy.length() + 20.0
                && sl_to_enemy.dot(sl_to_obj) > 0.0
            {
                let forward = sl_to_obj.normalize_or_zero();
                let right_perp = Vec2::new(-forward.y, forward.x);
                // Pick the lateral side further from the enemy center
                let side_sign = if (squad_leader_pos - contact_center).dot(right_perp) >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                let raw_detour = contact_center + right_perp * (side_sign * 36.0);
                let detour_waypoint = resolve_circle_obstacle_collision_bounded(
                    raw_detour,
                    SOLDIER_RADIUS,
                    buildings,
                    low_walls,
                    world_bound,
                );
                return SquadLeaderDecision::Bypass {
                    detour_waypoint,
                    threat_center: contact_center,
                };
            }
        }
    }

    // 2. Relative Combat Power, Casualties & Suppression Assessment
    // Decision 2: Break Contact (Disengage and Withdraw)
    // Chosen when the squad is heavily outnumbered/outgunned, has taken heavy casualties (<= 4 alive
    // against an equal/larger force), or has > 50% of the squad pinned by enemy suppression.
    let heavily_outnumbered = perceived_enemy_count > squad_alive_count + 3;
    let decimated_under_fire =
        squad_alive_count <= 4 && perceived_enemy_count >= squad_alive_count.max(2);
    let majority_pinned = suppressed_count * 2 > squad_alive_count && perceived_enemy_count >= 3;

    if heavily_outnumbered || decimated_under_fire || majority_pinned {
        let away_dir = (squad_leader_pos - contact_center).normalize_or(Vec2::new(0.0, 1.0));
        let raw_rally = squad_leader_pos + away_dir * 42.0;
        let rally_point = resolve_circle_obstacle_collision_bounded(
            raw_rally,
            SOLDIER_RADIUS,
            buildings,
            low_walls,
            world_bound,
        );
        return SquadLeaderDecision::BreakContact {
            rally_point,
            active_bound_ft: 2,
            bound_timer: 3.0,
        };
    }

    // Decision 3: Fix and Call for Support (Hold in Place)
    // Chosen when the enemy position is too large or fortified for a single squad to assault alone
    // (>= 6 perceived enemies vs equal/smaller squad, or >= 4 enemies with no cover to maneuver).
    let large_entrenched_enemy =
        perceived_enemy_count >= 6 && perceived_enemy_count + 1 >= squad_alive_count;
    let no_covered_flank = perceived_enemy_count >= 4 && !has_nearby_cover;
    if large_entrenched_enemy || no_covered_flank {
        return SquadLeaderDecision::FixAndSupport {
            target_center: contact_center,
        };
    }

    // Decision 1: Squad Attack (Flank and Destroy)
    // Chosen when the squad outnumbers or matches the enemy position and has cover to establish
    // a Fire Team 1 base of fire while the Squad Leader personally leads Fire Team 2 on a flank assault.
    let flank_right = squad_leader_pos.x >= contact_center.x;
    SquadLeaderDecision::SquadAttack {
        flank_right,
        target_center: contact_center,
    }
}

// Friendly fire team AI: follows player in tactical wedge formation, scans flanks and rear,
// takes cover behind low walls, and engages visible enemies with coordinated fire.
// When operating autonomously without a Player entity, the Squad Leader executes React to Contact
// and METT-TC decision making across the 5 doctrinal squad orders.
pub fn update_friendly(
    mut commands: Commands,
    time: Res<Time>,
    mut friendly_q: Query<
        (
            &mut Transform,
            &mut Friendly,
            &mut Soldier,
            &mut Suppression,
            Option<&SquadId>,
        ),
        (With<Friendly>, Without<Player>, Without<Enemy>),
    >,
    player_q: Query<
        (Entity, &Transform, &Soldier),
        (With<Player>, Without<Friendly>, Without<Enemy>),
    >,
    enemy_q: Query<
        (&Transform, &Soldier, &Enemy),
        (With<Enemy>, Without<Friendly>, Without<Player>),
    >,
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
    bullets_q: Query<
        (&Transform, &Bullet),
        (
            With<Bullet>,
            Without<Friendly>,
            Without<Player>,
            Without<Enemy>,
        ),
    >,
    bullet_assets: Res<BulletAssets>,
    mut gunshot_mutator: MessageMutator<GunshotEvent>,
    mut formation_state: Local<Option<(Entity, Vec2, Vec2)>>,
    mut local_doctrine: Local<SquadDoctrineState>,
    mut squad_doctrine: Option<ResMut<SquadDoctrineState>>,
    mut squad_command: Option<ResMut<SquadCommand>>,
    commander_order: Option<Res<CommanderOrder>>,
    current_level: Option<Res<CurrentLevel>>,
) {
    let level = current_level.as_deref().copied().unwrap_or_default();
    let world_bound = if level == CurrentLevel::Level4 {
        RTS_SOLDIER_WORLD_BOUND
    } else {
        SOLDIER_WORLD_BOUND
    };
    let active_commander_order = commander_order.as_deref().copied().unwrap_or_default();
    let dt = time.delta_secs();
    let soldier_radius = SOLDIER_RADIUS;
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();
    let low_walls: Vec<(&Transform, &LowWallCollider)> = low_walls_q.iter().collect();

    // Reset flanking command automatically if all enemies in the level have been eliminated
    if enemy_q.is_empty() {
        if let Some(ref mut cmd) = squad_command {
            if **cmd != SquadCommand::Formation {
                **cmd = SquadCommand::Formation;
            }
        }
    }
    let squad_cmd = squad_command.as_deref().copied().unwrap_or_default();
    let is_flanking_order = matches!(
        squad_cmd,
        SquadCommand::FlankLeft | SquadCommand::FlankRight
    );

    let player_entity_info = player_q.iter().next().map(|(e, tf, s)| {
        (
            e,
            Vec2::new(tf.translation.x, tf.translation.z),
            s.facing_dir,
            s.stance,
        )
    });
    let player_info = player_entity_info.map(|(_, pos, facing, stance)| (pos, facing, stance));

    // Only update formation heading when the player actually moves, so looking around while
    // standing still does not cause the squad to constantly reposition around the player.
    // Also resets cleanly whenever a new Player entity is spawned on level start.
    let formation_heading = if let Some((p_entity, p_pos, p_facing, _)) = player_entity_info {
        let state = formation_state.get_or_insert((p_entity, p_pos, p_facing));
        if state.0 != p_entity || (p_pos - state.1).length_squared() > 0.0001 {
            *state = (p_entity, p_pos, p_facing);
        }
        state.2
    } else {
        Vec2::new(0.0, -1.0)
    };

    // Capture snapshot of living friendlies to coordinate squad and fire team following:
    // `(squad_id, fireteam, slot, pos, facing_dir, vision_range, vision_half_angle, inner_radius, is_suppressed)`
    let friendly_snapshots: Vec<(usize, usize, usize, Vec2, Vec2, f32, f32, f32, bool)> =
        friendly_q
            .iter()
            .map(|(tf, f, s, sup, sq_opt)| {
                (
                    sq_opt.map_or(0, |sq| sq.0),
                    f.fireteam,
                    f.slot,
                    Vec2::new(tf.translation.x, tf.translation.z),
                    s.facing_dir,
                    f.vision_range,
                    f.vision_half_angle,
                    f.inner_radius,
                    sup.is_suppressed(),
                )
            })
            .collect();
    let ft1_lead_info = friendly_snapshots
        .iter()
        .find(|(_, ft, slot, _, _, _, _, _, _)| *ft == 1 && *slot == 0)
        .map(|(_, _, _, pos, _, _, _, _, _)| (*pos, formation_heading));
    let ft2_lead_info = friendly_snapshots
        .iter()
        .find(|(_, ft, slot, _, _, _, _, _, _)| *ft == 2 && *slot == 0)
        .map(|(_, _, _, pos, _, _, _, _, _)| (*pos, formation_heading));

    // Track how many Fire Team 1 (Alpha) members are currently able to fire so they can
    // stagger suppressive fire and reloads (ensuring at least 2 members are always firing).
    let mut ft1_able_to_fire: usize = friendly_q
        .iter()
        .filter(|(_, f, s, _, _)| {
            f.fireteam == 1
                && !s.inventory.is_swapping()
                && s.inventory
                    .active_weapon()
                    .map_or(false, |w| w.ammo.can_fire())
        })
        .count();

    // Shared squad target for coordinated flanking maneuvers
    let squad_anchor = player_info
        .map(|(p, _, _)| p)
        .or_else(|| ft1_lead_info.map(|(p, _)| p))
        .unwrap_or(Vec2::ZERO);
    let shared_flank_target: Option<Vec2> = if is_flanking_order {
        enemy_q
            .iter()
            .map(|(e_tf, _, _)| Vec2::new(e_tf.translation.x, e_tf.translation.z))
            .min_by(|a, b| {
                (*a - squad_anchor)
                    .length_squared()
                    .partial_cmp(&(*b - squad_anchor).length_squared())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    } else {
        None
    };

    let heard_gunshots: Vec<GunshotEvent> = gunshot_mutator.read().map(|e| *e).collect();

    // Update autonomous Squad Leader state machine (React to Contact -> METT-TC -> 5 Decisions)
    let doctrine_state: &mut SquadDoctrineState = match squad_doctrine.as_deref_mut() {
        Some(res) => res,
        None => &mut *local_doctrine,
    };

    if player_info.is_none() && !is_flanking_order {
        let mut squad_ids: Vec<usize> = friendly_snapshots
            .iter()
            .map(|(id, _, _, _, _, _, _, _, _)| *id)
            .collect();
        squad_ids.sort_unstable();
        squad_ids.dedup();

        for &sq_id in &squad_ids {
            let sq_members: Vec<&(usize, usize, usize, Vec2, Vec2, f32, f32, f32, bool)> =
                friendly_snapshots
                    .iter()
                    .filter(|(id, _, _, _, _, _, _, _, _)| *id == sq_id)
                    .collect();
            let squad_alive_count = sq_members.len();
            if squad_alive_count == 0 {
                continue;
            }
            // Only activate multi-stage squad doctrine when operating in Level 4, when the
            // SquadDoctrineState resource is present, or when a multi-soldier squad is present.
            if level != CurrentLevel::Level4
                && squad_alive_count < 2
                && doctrine_state.squads.is_empty()
            {
                continue;
            }

            let suppressed_count = sq_members
                .iter()
                .filter(|(_, _, _, _, _, _, _, _, sup)| *sup)
                .count();
            let leader_pos = sq_members
                .iter()
                .min_by_key(|(_, ft, slot, _, _, _, _, _, _)| (*ft, *slot))
                .map(|(_, _, _, pos, _, _, _, _, _)| *pos)
                .unwrap_or(Vec2::ZERO);

            let squad_obj = match active_commander_order {
                CommanderOrder::Hold => None,
                CommanderOrder::TakePosition(base_obj) => {
                    if sq_id == 0 {
                        Some(base_obj)
                    } else {
                        let side = if sq_id % 2 == 1 { 1.0 } else { -1.0 };
                        let tier = ((sq_id + 1) / 2) as f32;
                        Some(base_obj + Vec2::new(side * tier * 14.0, 0.0))
                    }
                }
            };

            // Aggregate squad perception: visible enemies, enemy awareness, approaching enemies, and hostile fire
            let mut visible_enemy_positions: Vec<Vec2> = Vec::new();
            let mut any_enemy_aware_or_seeing_squad = false;
            let mut any_enemy_approaching = false;
            let mut min_enemy_dist = f32::MAX;

            for (e_tf, e_soldier, e_ai) in &enemy_q {
                let e_pos = Vec2::new(e_tf.translation.x, e_tf.translation.z);
                let mut seen_by_squad = false;
                for (_, _, _, m_pos, m_facing, v_range, v_angle, v_inner, _) in &sq_members {
                    if can_see_target(
                        *m_pos, *m_facing, e_pos, *v_range, *v_angle, *v_inner, &buildings,
                    ) {
                        seen_by_squad = true;
                        break;
                    }
                }
                if seen_by_squad {
                    visible_enemy_positions.push(e_pos);
                    let d = (e_pos - leader_pos).length();
                    if d < min_enemy_dist {
                        min_enemy_dist = d;
                    }
                    if e_ai.state != EnemyAiState::Search {
                        any_enemy_aware_or_seeing_squad = true;
                    }
                    for (_, _, _, m_pos, _, _, _, _, _) in &sq_members {
                        if can_see_target(
                            e_pos,
                            e_soldier.facing_dir,
                            *m_pos,
                            e_ai.vision_range,
                            e_ai.vision_half_angle,
                            e_ai.inner_radius,
                            &buildings,
                        ) {
                            any_enemy_aware_or_seeing_squad = true;
                            break;
                        }
                    }
                    let move_delta = e_pos - e_ai.last_pos;
                    let to_squad = (leader_pos - e_pos).normalize_or_zero();
                    if move_delta.length_squared() > 0.0001 && move_delta.dot(to_squad) > 0.0 {
                        any_enemy_approaching = true;
                    }
                }
            }

            let mut recent_gunshot_origins: Vec<Vec2> = Vec::new();
            for event in &heard_gunshots {
                if event.shooter_team == Team::Enemy
                    && (event.position - leader_pos).length() < 55.0
                {
                    if !recent_gunshot_origins
                        .iter()
                        .any(|p| (*p - event.position).length_squared() < 9.0)
                    {
                        recent_gunshot_origins.push(event.position);
                    }
                }
            }

            let mut under_enemy_bullet_fire = false;
            let mut bullet_threat_origin: Option<Vec2> = None;
            for (b_tf, bullet) in &bullets_q {
                if bullet.team == Team::Enemy {
                    let b_pos = Vec2::new(b_tf.translation.x, b_tf.translation.z);
                    for (_, _, _, m_pos, _, _, _, _, _) in &sq_members {
                        let to_m = *m_pos - b_pos;
                        let d2 = to_m.length_squared();
                        if d2 < 36.0 || (d2 < 196.0 && to_m.dot(bullet.direction) > 0.0) {
                            under_enemy_bullet_fire = true;
                            bullet_threat_origin = Some(bullet.origin);
                            break;
                        }
                    }
                }
            }

            let any_hostile_fire = under_enemy_bullet_fire || !recent_gunshot_origins.is_empty();
            let is_undetected = !visible_enemy_positions.is_empty()
                && !any_enemy_aware_or_seeing_squad
                && !any_hostile_fire;

            // Perceived enemy count combines visible enemies and distinct unspotted gunshot origins
            let mut perceived_positions = visible_enemy_positions.clone();
            for g_pos in &recent_gunshot_origins {
                if !perceived_positions
                    .iter()
                    .any(|p| (*p - *g_pos).length_squared() < 16.0)
                {
                    perceived_positions.push(*g_pos);
                }
            }
            if let Some(b_orig) = bullet_threat_origin {
                if !perceived_positions
                    .iter()
                    .any(|p| (*p - b_orig).length_squared() < 16.0)
                {
                    perceived_positions.push(b_orig);
                }
            }
            let perceived_enemy_count = perceived_positions.len();
            let has_contact = !perceived_positions.is_empty();
            let contact_center = if !visible_enemy_positions.is_empty() {
                let sum = visible_enemy_positions
                    .iter()
                    .copied()
                    .fold(Vec2::ZERO, |a, b| a + b);
                sum / (visible_enemy_positions.len() as f32)
            } else if !perceived_positions.is_empty() {
                let sum = perceived_positions
                    .iter()
                    .copied()
                    .fold(Vec2::ZERO, |a, b| a + b);
                sum / (perceived_positions.len() as f32)
            } else {
                leader_pos
            };

            let entry = doctrine_state.entry_mut(sq_id);
            if has_contact {
                entry.perceived_enemy_count =
                    perceived_enemy_count.max(entry.perceived_enemy_count);
                entry.contact_lost_timer = 0.0;
            } else {
                entry.contact_lost_timer += dt;
            }

            match entry.phase {
                SquadTacticalPhase::Normal => {
                    if has_contact {
                        let has_deeper_bypass_obj = squad_obj.is_some_and(|obj| {
                            (obj - contact_center).length() > 45.0
                                && (obj - leader_pos).length()
                                    > (contact_center - leader_pos).length() + 20.0
                        });
                        let silent_stealth_assessment =
                            is_undetected && (any_enemy_approaching || has_deeper_bypass_obj);
                        entry.perceived_enemy_count = perceived_enemy_count;
                        entry.phase = SquadTacticalPhase::ReactToContact {
                            assess_timer: if silent_stealth_assessment { 0.45 } else { 1.0 },
                            contact_center,
                            spotted_unaware_enemy: silent_stealth_assessment,
                        };
                    }
                }
                SquadTacticalPhase::ReactToContact {
                    mut assess_timer,
                    contact_center: prev_center,
                    mut spotted_unaware_enemy,
                } => {
                    let updated_center = if has_contact {
                        contact_center
                    } else {
                        prev_center
                    };
                    if !is_undetected {
                        spotted_unaware_enemy = false;
                    }
                    assess_timer -= dt;
                    if assess_timer <= 0.0 {
                        let effective_enemy_count = perceived_enemy_count
                            .max(entry.perceived_enemy_count)
                            .max(1);
                        let decision = evaluate_squad_leader_mett_tc(
                            leader_pos,
                            updated_center,
                            squad_alive_count,
                            effective_enemy_count,
                            suppressed_count,
                            spotted_unaware_enemy && is_undetected,
                            any_enemy_approaching,
                            squad_obj,
                            &buildings,
                            &low_walls,
                            world_bound,
                        );
                        entry.phase = SquadTacticalPhase::DecisionActive(decision);
                    } else {
                        entry.phase = SquadTacticalPhase::ReactToContact {
                            assess_timer,
                            contact_center: updated_center,
                            spotted_unaware_enemy,
                        };
                    }
                }
                SquadTacticalPhase::DecisionActive(mut decision) => {
                    match &mut decision {
                        SquadLeaderDecision::SquadAttack {
                            flank_right: _,
                            target_center,
                        } => {
                            if has_contact {
                                *target_center = contact_center;
                            }
                            if perceived_enemy_count > squad_alive_count + 3
                                || (squad_alive_count <= 4
                                    && perceived_enemy_count >= squad_alive_count.max(2))
                                || (suppressed_count * 2 > squad_alive_count
                                    && perceived_enemy_count >= 3)
                            {
                                let away_dir =
                                    (leader_pos - *target_center).normalize_or(Vec2::new(0.0, 1.0));
                                let rally_point = resolve_circle_obstacle_collision_bounded(
                                    leader_pos + away_dir * 42.0,
                                    SOLDIER_RADIUS,
                                    &buildings,
                                    &low_walls,
                                    world_bound,
                                );
                                entry.phase = SquadTacticalPhase::DecisionActive(
                                    SquadLeaderDecision::BreakContact {
                                        rally_point,
                                        active_bound_ft: 2,
                                        bound_timer: 3.0,
                                    },
                                );
                            } else if perceived_enemy_count >= 6
                                && perceived_enemy_count + 1 >= squad_alive_count
                            {
                                entry.phase = SquadTacticalPhase::DecisionActive(
                                    SquadLeaderDecision::FixAndSupport {
                                        target_center: *target_center,
                                    },
                                );
                            } else if entry.contact_lost_timer > 2.0 {
                                entry.phase = SquadTacticalPhase::Normal;
                                entry.perceived_enemy_count = 0;
                            } else {
                                entry.phase = SquadTacticalPhase::DecisionActive(decision);
                            }
                        }
                        SquadLeaderDecision::FixAndSupport { target_center } => {
                            if has_contact {
                                *target_center = contact_center;
                            }
                            if squad_alive_count <= 4
                                || (suppressed_count * 2 > squad_alive_count
                                    && perceived_enemy_count >= 3)
                            {
                                let away_dir =
                                    (leader_pos - *target_center).normalize_or(Vec2::new(0.0, 1.0));
                                let rally_point = resolve_circle_obstacle_collision_bounded(
                                    leader_pos + away_dir * 42.0,
                                    SOLDIER_RADIUS,
                                    &buildings,
                                    &low_walls,
                                    world_bound,
                                );
                                entry.phase = SquadTacticalPhase::DecisionActive(
                                    SquadLeaderDecision::BreakContact {
                                        rally_point,
                                        active_bound_ft: 2,
                                        bound_timer: 3.0,
                                    },
                                );
                            } else if has_contact
                                && perceived_enemy_count <= 4
                                && squad_alive_count >= 5
                            {
                                // Enemy whittled down: Squad Leader transitions from Fix to Squad Attack!
                                entry.phase = SquadTacticalPhase::DecisionActive(
                                    SquadLeaderDecision::SquadAttack {
                                        flank_right: leader_pos.x >= target_center.x,
                                        target_center: *target_center,
                                    },
                                );
                            } else if entry.contact_lost_timer > 2.0 {
                                entry.phase = SquadTacticalPhase::Normal;
                                entry.perceived_enemy_count = 0;
                            } else {
                                entry.phase = SquadTacticalPhase::DecisionActive(decision);
                            }
                        }
                        SquadLeaderDecision::BreakContact {
                            rally_point,
                            active_bound_ft,
                            bound_timer,
                        } => {
                            *bound_timer -= dt;
                            if *bound_timer <= 0.0 {
                                *active_bound_ft = if *active_bound_ft == 1 { 2 } else { 1 };
                                *bound_timer = 3.0;
                            }
                            let all_at_rally =
                                sq_members.iter().all(|(_, _, _, m_pos, _, _, _, _, _)| {
                                    (*m_pos - *rally_point).length() < 9.0
                                });
                            if (all_at_rally && entry.contact_lost_timer > 1.2)
                                || enemy_q.is_empty()
                            {
                                entry.phase = SquadTacticalPhase::Normal;
                                entry.perceived_enemy_count = 0;
                            } else {
                                entry.phase = SquadTacticalPhase::DecisionActive(decision);
                            }
                        }
                        SquadLeaderDecision::Bypass {
                            detour_waypoint,
                            threat_center: _,
                        } => {
                            if any_hostile_fire || any_enemy_aware_or_seeing_squad {
                                // Spotted or fired upon while bypassing: immediately React to Contact!
                                entry.phase = SquadTacticalPhase::ReactToContact {
                                    assess_timer: 0.8,
                                    contact_center,
                                    spotted_unaware_enemy: false,
                                };
                            } else if (leader_pos - *detour_waypoint).length() < 5.5
                                || enemy_q.is_empty()
                            {
                                entry.phase = SquadTacticalPhase::Normal;
                                entry.perceived_enemy_count = 0;
                            } else {
                                entry.phase = SquadTacticalPhase::DecisionActive(decision);
                            }
                        }
                        SquadLeaderDecision::HastyAmbush {
                            kill_zone,
                            triggered,
                            trigger_timer,
                        } => {
                            if has_contact {
                                *kill_zone = contact_center;
                            }
                            *trigger_timer += dt;
                            if !*triggered {
                                if min_enemy_dist <= 15.0
                                    || any_hostile_fire
                                    || any_enemy_aware_or_seeing_squad
                                    || *trigger_timer >= 5.0
                                {
                                    *triggered = true;
                                    *trigger_timer = 0.0;
                                }
                                entry.phase = SquadTacticalPhase::DecisionActive(decision);
                            } else if *trigger_timer >= 3.5 {
                                // Transition into SquadAttack to sweep through the ambush kill zone
                                entry.phase = SquadTacticalPhase::DecisionActive(
                                    SquadLeaderDecision::SquadAttack {
                                        flank_right: leader_pos.x >= kill_zone.x,
                                        target_center: *kill_zone,
                                    },
                                );
                            } else if entry.contact_lost_timer > 2.0 {
                                entry.phase = SquadTacticalPhase::Normal;
                                entry.perceived_enemy_count = 0;
                            } else {
                                entry.phase = SquadTacticalPhase::DecisionActive(decision);
                            }
                        }
                    }
                }
            }
        }
    }

    for (mut tf, mut friendly, mut soldier, mut suppression, sq_opt) in &mut friendly_q {
        soldier.inventory.tick(time.delta());
        suppression.decay(dt);
        let friendly_pos = Vec2::new(tf.translation.x, tf.translation.z);
        let sq_id = sq_opt.map_or(0, |sq| sq.0);

        // Per-squad autonomous leadership & shared contact when executing CommanderOrder::TakePosition
        let squad_take_pos_objective = match active_commander_order {
            CommanderOrder::Hold => None,
            CommanderOrder::TakePosition(base_obj) => {
                if sq_id == 0 {
                    Some(base_obj)
                } else {
                    let side = if sq_id % 2 == 1 { 1.0 } else { -1.0 };
                    let tier = ((sq_id + 1) / 2) as f32;
                    Some(base_obj + Vec2::new(side * tier * 14.0, 0.0))
                }
            }
        };

        let squad_leader_entry = friendly_snapshots
            .iter()
            .filter(|(id, _, _, _, _, _, _, _, _)| *id == sq_id)
            .min_by_key(|(_, ft, slot, _, _, _, _, _, _)| (*ft, *slot))
            .map(|(_, ft, slot, pos, _, _, _, _, _)| (*ft, *slot, *pos));
        let is_squad_leader = squad_leader_entry
            .is_some_and(|(lft, lslot, _)| friendly.fireteam == lft && friendly.slot == lslot);

        let active_squad_phase = if player_info.is_none() && !is_flanking_order {
            doctrine_state
                .squads
                .get(&sq_id)
                .map(|e| e.phase)
                .unwrap_or(SquadTacticalPhase::Normal)
        } else {
            SquadTacticalPhase::Normal
        };

        let shared_squad_contact: Option<Vec2> = if squad_take_pos_objective.is_some()
            || active_squad_phase != SquadTacticalPhase::Normal
        {
            let mut best_contact: Option<(Vec2, f32)> = None;
            for (id, _, _, m_pos, m_facing, v_range, v_angle, v_inner, _) in &friendly_snapshots {
                if *id != sq_id {
                    continue;
                }
                for (e_tf, _, _) in &enemy_q {
                    let e_pos = Vec2::new(e_tf.translation.x, e_tf.translation.z);
                    if can_see_target(
                        *m_pos, *m_facing, e_pos, *v_range, *v_angle, *v_inner, &buildings,
                    ) {
                        let d2 = (e_pos - friendly_pos).length_squared();
                        if best_contact.map_or(true, |(_, best_d2)| d2 < best_d2) {
                            best_contact = Some((e_pos, d2));
                        }
                    }
                }
            }
            best_contact.map(|(pos, _)| pos)
        } else {
            None
        };

        // 1. Target acquisition: scan vision cone for living enemies in line of sight
        let mut visible_enemies: Vec<(Vec2, f32)> = Vec::new();
        for (e_tf, _, _) in &enemy_q {
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

        // 2. Detect incoming enemy fire (near-miss bullets or nearby hostile gunshots)
        let mut under_fire_source: Option<Vec2> = None;
        for (b_tf, bullet) in &bullets_q {
            if bullet.team == Team::Enemy {
                let b_pos = Vec2::new(b_tf.translation.x, b_tf.translation.z);
                let to_friendly = friendly_pos - b_pos;
                let dist_sq = to_friendly.length_squared();
                if dist_sq < 36.0 || (dist_sq < 196.0 && to_friendly.dot(bullet.direction) > 0.0) {
                    under_fire_source = Some(bullet.origin);
                    break;
                }
            }
        }
        if under_fire_source.is_none() {
            for event in &heard_gunshots {
                if event.shooter_team == Team::Enemy {
                    let to_gunshot = event.position - friendly_pos;
                    let dist = to_gunshot.length();
                    if dist < 50.0 && dist > 0.1 {
                        under_fire_source = Some(event.position);
                        break;
                    }
                }
            }
        }

        // 3. State transitions (taking fire, spotting an enemy, or executing a squad tactical decision)
        let doctrine_target: Option<Vec2> = match active_squad_phase {
            SquadTacticalPhase::Normal => None,
            SquadTacticalPhase::ReactToContact { contact_center, .. } => Some(contact_center),
            SquadTacticalPhase::DecisionActive(decision) => match decision {
                SquadLeaderDecision::SquadAttack { target_center, .. } => Some(target_center),
                SquadLeaderDecision::FixAndSupport { target_center } => Some(target_center),
                SquadLeaderDecision::BreakContact { rally_point, .. } => {
                    shared_squad_contact.or(Some(rally_point))
                }
                SquadLeaderDecision::Bypass { .. } => None,
                SquadLeaderDecision::HastyAmbush { kill_zone, .. } => Some(kill_zone),
            },
        };

        if matches!(
            active_squad_phase,
            SquadTacticalPhase::DecisionActive(SquadLeaderDecision::Bypass { .. })
        ) {
            // Bypass order: stay in Follow/maneuver state and hold fire to detour around enemy
            friendly.state = FriendlyAiState::Follow;
            friendly.target_pos = None;
            friendly.target_lost_timer = 0.0;
        } else if let Some(&(nearest_enemy_pos, _)) = visible_enemies.first() {
            friendly.state = FriendlyAiState::Engage;
            friendly.target_pos = Some(nearest_enemy_pos);
            friendly.target_lost_timer = 0.0;
        } else if let Some(flank_target) = shared_flank_target {
            friendly.state = FriendlyAiState::Engage;
            friendly.target_pos = Some(flank_target);
            friendly.target_lost_timer = 0.0;
        } else if let Some(threat_pos) = under_fire_source {
            friendly.state = FriendlyAiState::Engage;
            friendly.target_pos = Some(threat_pos);
            friendly.target_lost_timer = 0.0;
        } else if let Some(squad_contact) = shared_squad_contact {
            friendly.state = FriendlyAiState::Engage;
            friendly.target_pos = Some(squad_contact);
            friendly.target_lost_timer = 0.0;
        } else if let Some(doc_target) = doctrine_target {
            friendly.state = FriendlyAiState::Engage;
            friendly.target_pos = Some(doc_target);
            friendly.target_lost_timer = 0.0;
        } else if friendly.state == FriendlyAiState::Engage {
            friendly.target_lost_timer += dt;
            if friendly.target_lost_timer > 1.5 {
                // Lost sight of enemy: return to formation follow / objective advance
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

                    let mut is_moving = false;
                    let mut has_clear_flank_shot = false;
                    let mut hold_fire = false;

                    let is_ft2_flanking = is_flanking_order && friendly.fireteam == 2;
                    let mut is_ft1_suppressing = is_flanking_order && friendly.fireteam == 1;

                    let lateral_offset = match (friendly.fireteam, friendly.slot) {
                        (1, 0) => 0.0,
                        (1, 1) => -1.8,
                        (1, 2) => 1.8,
                        (1, 3) => -0.9,
                        (1, _) => 0.9,
                        (2, 0) => -0.8,
                        (2, 1) => -2.2,
                        (2, 2) => 2.2,
                        _ => 0.8,
                    };

                    if active_squad_phase != SquadTacticalPhase::Normal
                        && !suppression.is_suppressed()
                    {
                        match active_squad_phase {
                            SquadTacticalPhase::ReactToContact {
                                contact_center,
                                spotted_unaware_enemy,
                                ..
                            } => {
                                // Immediate React to Contact Battle Drill:
                                // - If visual contact only on unaware enemy (`spotted_unaware_enemy`),
                                //   halt, crouch, and hold fire while Squad Leader finishes METT-TC.
                                // - Otherwise, drop to nearest cover and return initial baseline fire.
                                if spotted_unaware_enemy {
                                    hold_fire = true;
                                    soldier.stance = Stance::Crouching;
                                    let to_c = contact_center - friendly_pos;
                                    if to_c.length_squared() > 0.001 {
                                        soldier.facing_dir = to_c.normalize();
                                    }
                                } else {
                                    is_ft1_suppressing = friendly.fireteam == 1;
                                    if let Some(cover_spot) = find_cover_position_with_offset(
                                        friendly_pos,
                                        target,
                                        &buildings,
                                        &low_walls,
                                        soldier_radius,
                                        14.0,
                                        lateral_offset,
                                    ) {
                                        let to_cover = cover_spot - friendly_pos;
                                        let dist_to_cover = to_cover.length();
                                        if dist_to_cover > 0.35 {
                                            is_moving = true;
                                            soldier.stance = Stance::Crouching;
                                            let desired_move = to_cover / dist_to_cover;
                                            let move_dir = find_avoidance_direction(
                                                friendly_pos,
                                                desired_move,
                                                &buildings,
                                                &low_walls,
                                                soldier_radius,
                                            );
                                            let dir_mult = directional_speed_multiplier(
                                                soldier.facing_dir,
                                                move_dir,
                                            );
                                            let speed = friendly.speed * dir_mult;
                                            let next_pos = friendly_pos + move_dir * speed * dt;
                                            let resolved =
                                                resolve_circle_obstacle_collision_bounded(
                                                    next_pos,
                                                    soldier_radius,
                                                    &buildings,
                                                    &low_walls,
                                                    world_bound,
                                                );
                                            tf.translation.x = resolved.x;
                                            tf.translation.z = resolved.y;
                                        } else {
                                            soldier.stance = Stance::Crouching;
                                            has_clear_flank_shot = true;
                                        }
                                    } else {
                                        soldier.stance = Stance::Crouching;
                                        has_clear_flank_shot = dist <= 24.0;
                                    }
                                }
                            }
                            SquadTacticalPhase::DecisionActive(
                                SquadLeaderDecision::SquadAttack {
                                    flank_right,
                                    target_center,
                                },
                            ) => {
                                // Decision 1: Squad Attack (Flank and Destroy)
                                // - Fire Team 1 (`fireteam == 1 && !is_squad_leader`) establishes a stationary
                                //   support-by-fire position to suppress the enemy.
                                // - Squad Leader (`is_squad_leader`) personally leads Fire Team 2 (`fireteam == 2`)
                                //   along a covered flank route to assault through the enemy position.
                                let is_assault_element = is_squad_leader || friendly.fireteam == 2;
                                is_ft1_suppressing = !is_assault_element;

                                let clear_shot = has_clear_shot_past_cover(
                                    friendly_pos,
                                    target,
                                    &buildings,
                                    &low_walls,
                                );

                                if !is_assault_element {
                                    // Fire Team 1 Support-by-Fire Base
                                    let low_wall_cover = if dist <= 25.0 {
                                        find_cover_position_with_offset(
                                            friendly_pos,
                                            target,
                                            &[],
                                            &low_walls,
                                            soldier_radius,
                                            14.0,
                                            lateral_offset,
                                        )
                                    } else {
                                        None
                                    };

                                    if let Some(cover_spot) = low_wall_cover {
                                        let to_cover = cover_spot - friendly_pos;
                                        let dist_to_cover = to_cover.length();
                                        if dist_to_cover > 0.35 {
                                            is_moving = true;
                                            soldier.stance = if dist_to_cover < 1.5 {
                                                Stance::Crouching
                                            } else {
                                                Stance::Standing
                                            };
                                            let desired_move = to_cover / dist_to_cover;
                                            let move_dir = find_avoidance_direction(
                                                friendly_pos,
                                                desired_move,
                                                &buildings,
                                                &low_walls,
                                                soldier_radius,
                                            );
                                            let dir_mult = directional_speed_multiplier(
                                                soldier.facing_dir,
                                                move_dir,
                                            );
                                            let next_pos = friendly_pos
                                                + move_dir * (friendly.speed * dir_mult) * dt;
                                            let resolved =
                                                resolve_circle_obstacle_collision_bounded(
                                                    next_pos,
                                                    soldier_radius,
                                                    &buildings,
                                                    &low_walls,
                                                    world_bound,
                                                );
                                            tf.translation.x = resolved.x;
                                            tf.translation.z = resolved.y;
                                        } else {
                                            soldier.stance = Stance::Crouching;
                                            has_clear_flank_shot = true;
                                        }
                                    } else if clear_shot && dist <= 22.0 {
                                        soldier.stance = Stance::Crouching;
                                        has_clear_flank_shot = true;
                                    } else {
                                        let sbf_goal =
                                            target + Vec2::new(lateral_offset * 1.4, 0.0);
                                        let to_goal = sbf_goal - friendly_pos;
                                        let dist_to_goal = to_goal.length();
                                        if dist_to_goal > 0.5 {
                                            is_moving = true;
                                            soldier.stance = Stance::Standing;
                                            let desired_move = to_goal / dist_to_goal;
                                            let move_dir = find_avoidance_direction(
                                                friendly_pos,
                                                desired_move,
                                                &buildings,
                                                &low_walls,
                                                soldier_radius,
                                            );
                                            soldier.facing_dir = if clear_shot && dist <= 24.0 {
                                                (target - friendly_pos).normalize_or_zero()
                                            } else {
                                                move_dir
                                            };
                                            let dir_mult = directional_speed_multiplier(
                                                soldier.facing_dir,
                                                move_dir,
                                            );
                                            let next_pos = friendly_pos
                                                + move_dir * (friendly.speed * dir_mult) * dt;
                                            let resolved =
                                                resolve_circle_obstacle_collision_bounded(
                                                    next_pos,
                                                    soldier_radius,
                                                    &buildings,
                                                    &low_walls,
                                                    world_bound,
                                                );
                                            tf.translation.x = resolved.x;
                                            tf.translation.z = resolved.y;
                                        }
                                    }
                                } else {
                                    // Squad Leader + Fire Team 2 Flanking Assault Element
                                    let flank_slot = if is_squad_leader {
                                        0
                                    } else {
                                        friendly.slot + 1
                                    };
                                    if clear_shot && dist <= 17.5 {
                                        has_clear_flank_shot = true;
                                        soldier.stance = Stance::Crouching;
                                    } else {
                                        let step_goal = if !clear_shot {
                                            compute_flank_waypoint(
                                                friendly_pos,
                                                target_center,
                                                flank_right,
                                                flank_slot,
                                                &buildings,
                                                &low_walls,
                                                soldier_radius,
                                            )
                                        } else {
                                            let side = if flank_right { 1.0 } else { -1.0 };
                                            target
                                                + Vec2::new(
                                                    side * (2.5 + flank_slot as f32 * 1.1),
                                                    0.0,
                                                )
                                        };
                                        let to_goal = step_goal - friendly_pos;
                                        let dist_to_goal = to_goal.length();
                                        if dist_to_goal > 0.5 {
                                            is_moving = true;
                                            soldier.stance = Stance::Standing;
                                            let desired_move = to_goal / dist_to_goal;
                                            let move_dir = find_avoidance_direction(
                                                friendly_pos,
                                                desired_move,
                                                &buildings,
                                                &low_walls,
                                                soldier_radius,
                                            );
                                            soldier.facing_dir = if clear_shot && dist <= 22.0 {
                                                (target - friendly_pos).normalize_or_zero()
                                            } else {
                                                move_dir
                                            };
                                            let dir_mult = directional_speed_multiplier(
                                                soldier.facing_dir,
                                                move_dir,
                                            );
                                            let speed = friendly.speed * 1.12 * dir_mult;
                                            let next_pos = friendly_pos + move_dir * speed * dt;
                                            let resolved =
                                                resolve_circle_obstacle_collision_bounded(
                                                    next_pos,
                                                    soldier_radius,
                                                    &buildings,
                                                    &low_walls,
                                                    world_bound,
                                                );
                                            tf.translation.x = resolved.x;
                                            tf.translation.z = resolved.y;
                                        }
                                    }
                                }
                            }
                            SquadTacticalPhase::DecisionActive(
                                SquadLeaderDecision::BreakContact {
                                    rally_point,
                                    active_bound_ft,
                                    ..
                                },
                            ) => {
                                // Decision 2: Break Contact (Reverse Bounding Overwatch)
                                // Active bounding fire team bounds rearward toward `rally_point`;
                                // Overwatch fire team halts, crouches, and lays maximum suppressive fire.
                                if friendly.fireteam == active_bound_ft {
                                    let slot_offset = Vec2::new(
                                        lateral_offset * 1.3,
                                        (friendly.slot as f32) * 1.2,
                                    );
                                    let dest = rally_point + slot_offset;
                                    let to_rally = dest - friendly_pos;
                                    let dist_to_rally = to_rally.length();
                                    if dist_to_rally > 2.0 {
                                        is_moving = true;
                                        hold_fire = true;
                                        soldier.stance = Stance::Standing;
                                        let desired_move = to_rally / dist_to_rally;
                                        let move_dir = find_avoidance_direction(
                                            friendly_pos,
                                            desired_move,
                                            &buildings,
                                            &low_walls,
                                            soldier_radius,
                                        );
                                        soldier.facing_dir = move_dir;
                                        let speed = friendly.speed * 1.2;
                                        let next_pos = friendly_pos + move_dir * speed * dt;
                                        let resolved = resolve_circle_obstacle_collision_bounded(
                                            next_pos,
                                            soldier_radius,
                                            &buildings,
                                            &low_walls,
                                            world_bound,
                                        );
                                        tf.translation.x = resolved.x;
                                        tf.translation.z = resolved.y;
                                    } else {
                                        soldier.stance = Stance::Crouching;
                                        has_clear_flank_shot = true;
                                    }
                                } else {
                                    // Overwatch fire team: hold position and pour maximum suppressive fire
                                    is_ft1_suppressing = true;
                                    soldier.stance = Stance::Crouching;
                                    has_clear_flank_shot = true;
                                    friendly
                                        .shoot_timer
                                        .set_duration(std::time::Duration::from_secs_f32(0.14));
                                }
                            }
                            SquadTacticalPhase::DecisionActive(
                                SquadLeaderDecision::FixAndSupport { target_center },
                            ) => {
                                // Decision 3: Fix and Call for Support (Hold in Place)
                                // Both Fire Team 1 and Fire Team 2 take cover and maintain sustained
                                // staggered suppressive fire without charging into the open.
                                is_ft1_suppressing = true;
                                let fix_lateral = if friendly.fireteam == 1 {
                                    lateral_offset - 3.5
                                } else {
                                    lateral_offset + 3.5
                                };
                                let cover_candidate = find_cover_position_with_offset(
                                    friendly_pos,
                                    target_center,
                                    &[],
                                    &low_walls,
                                    soldier_radius,
                                    18.0,
                                    fix_lateral,
                                )
                                .or_else(|| {
                                    find_cover_position_with_offset(
                                        friendly_pos,
                                        target_center,
                                        &buildings,
                                        &low_walls,
                                        soldier_radius,
                                        14.0,
                                        fix_lateral,
                                    )
                                });

                                if let Some(cover_spot) = cover_candidate {
                                    let to_cover = cover_spot - friendly_pos;
                                    let dist_to_cover = to_cover.length();
                                    if dist_to_cover > 0.4 {
                                        is_moving = true;
                                        soldier.stance = if dist_to_cover < 2.0 {
                                            Stance::Crouching
                                        } else {
                                            Stance::Standing
                                        };
                                        let desired_move = to_cover / dist_to_cover;
                                        let move_dir = find_avoidance_direction(
                                            friendly_pos,
                                            desired_move,
                                            &buildings,
                                            &low_walls,
                                            soldier_radius,
                                        );
                                        let dir_mult = directional_speed_multiplier(
                                            soldier.facing_dir,
                                            move_dir,
                                        );
                                        let next_pos = friendly_pos
                                            + move_dir * (friendly.speed * dir_mult) * dt;
                                        let resolved = resolve_circle_obstacle_collision_bounded(
                                            next_pos,
                                            soldier_radius,
                                            &buildings,
                                            &low_walls,
                                            world_bound,
                                        );
                                        tf.translation.x = resolved.x;
                                        tf.translation.z = resolved.y;
                                    } else {
                                        soldier.stance = Stance::Crouching;
                                        has_clear_flank_shot = true;
                                    }
                                } else if dist > 23.5 {
                                    // Close only to outer stand-off firing range (23m) and halt in place
                                    let stand_off_dir = (friendly_pos - target_center)
                                        .normalize_or(Vec2::new(0.0, 1.0));
                                    let stand_off_spot = target_center
                                        + stand_off_dir * 22.0
                                        + Vec2::new(fix_lateral, 0.0);
                                    let to_spot = stand_off_spot - friendly_pos;
                                    let d_spot = to_spot.length();
                                    if d_spot > 0.6 {
                                        is_moving = true;
                                        soldier.stance = Stance::Crouching;
                                        let desired_move = to_spot / d_spot;
                                        let move_dir = find_avoidance_direction(
                                            friendly_pos,
                                            desired_move,
                                            &buildings,
                                            &low_walls,
                                            soldier_radius,
                                        );
                                        let dir_mult = directional_speed_multiplier(
                                            soldier.facing_dir,
                                            move_dir,
                                        );
                                        let next_pos = friendly_pos
                                            + move_dir * (friendly.speed * dir_mult) * dt;
                                        let resolved = resolve_circle_obstacle_collision_bounded(
                                            next_pos,
                                            soldier_radius,
                                            &buildings,
                                            &low_walls,
                                            world_bound,
                                        );
                                        tf.translation.x = resolved.x;
                                        tf.translation.z = resolved.y;
                                    } else {
                                        soldier.stance = Stance::Crouching;
                                        has_clear_flank_shot = true;
                                    }
                                } else {
                                    soldier.stance = Stance::Crouching;
                                    has_clear_flank_shot = true;
                                }
                            }
                            SquadTacticalPhase::DecisionActive(
                                SquadLeaderDecision::HastyAmbush {
                                    kill_zone,
                                    triggered,
                                    ..
                                },
                            ) => {
                                // Decision 5: Hasty Ambush (Wait and Spring)
                                let to_kz = kill_zone - friendly_pos;
                                if to_kz.length_squared() > 0.001 {
                                    soldier.facing_dir = to_kz.normalize();
                                }
                                if !triggered {
                                    hold_fire = true;
                                    if let Some(cover_spot) = find_cover_position_with_offset(
                                        friendly_pos,
                                        kill_zone,
                                        &buildings,
                                        &low_walls,
                                        soldier_radius,
                                        7.5,
                                        lateral_offset,
                                    ) {
                                        let to_cover = cover_spot - friendly_pos;
                                        let d_cov = to_cover.length();
                                        if d_cov > 0.35 {
                                            is_moving = true;
                                            soldier.stance = Stance::Crouching;
                                            let desired_move = to_cover / d_cov;
                                            let move_dir = find_avoidance_direction(
                                                friendly_pos,
                                                desired_move,
                                                &buildings,
                                                &low_walls,
                                                soldier_radius,
                                            );
                                            let dir_mult = directional_speed_multiplier(
                                                soldier.facing_dir,
                                                move_dir,
                                            );
                                            let next_pos = friendly_pos
                                                + move_dir
                                                    * (friendly.speed * 0.75 * dir_mult)
                                                    * dt;
                                            let resolved =
                                                resolve_circle_obstacle_collision_bounded(
                                                    next_pos,
                                                    soldier_radius,
                                                    &buildings,
                                                    &low_walls,
                                                    world_bound,
                                                );
                                            tf.translation.x = resolved.x;
                                            tf.translation.z = resolved.y;
                                        } else {
                                            soldier.stance = Stance::Crouching;
                                        }
                                    } else {
                                        soldier.stance = Stance::Prone;
                                    }
                                } else {
                                    // Ambush sprung: open rapid simultaneous fire into the kill zone!
                                    is_ft1_suppressing = true;
                                    has_clear_flank_shot = true;
                                    soldier.stance = Stance::Crouching;
                                    friendly
                                        .shoot_timer
                                        .set_duration(std::time::Duration::from_secs_f32(0.12));
                                }
                            }
                            SquadTacticalPhase::DecisionActive(SquadLeaderDecision::Bypass {
                                ..
                            })
                            | SquadTacticalPhase::Normal => {}
                        }
                    } else if is_ft2_flanking {
                        // Fire Team 2 (Bravo): maneuver along cover towards an enfilade position
                        // with a clear shot perpendicular to the enemy's cover.
                        let flank_right = squad_cmd == SquadCommand::FlankRight;
                        let clear_shot = dist < 22.0
                            && has_clear_shot_past_cover(
                                friendly_pos,
                                target,
                                &buildings,
                                &low_walls,
                            );
                        if !clear_shot {
                            let flank_wp = compute_flank_waypoint(
                                friendly_pos,
                                target,
                                flank_right,
                                friendly.slot,
                                &buildings,
                                &low_walls,
                                soldier_radius,
                            );
                            let to_wp = flank_wp - friendly_pos;
                            let dist_to_wp = to_wp.length();
                            if dist_to_wp > 0.45 {
                                is_moving = true;
                                soldier.stance = Stance::Standing;
                                let desired_move = to_wp / dist_to_wp;
                                let move_dir = find_avoidance_direction(
                                    friendly_pos,
                                    desired_move,
                                    &buildings,
                                    &low_walls,
                                    soldier_radius,
                                );
                                // Face movement direction while flanking to move at 100% forward speed
                                soldier.facing_dir = move_dir;
                                let speed = friendly.speed * 1.15;
                                let next_pos = friendly_pos + move_dir * speed * dt;
                                let resolved = resolve_circle_obstacle_collision_bounded(
                                    next_pos,
                                    soldier_radius,
                                    &buildings,
                                    &low_walls,
                                    world_bound,
                                );
                                tf.translation.x = resolved.x;
                                tf.translation.z = resolved.y;
                            } else {
                                has_clear_flank_shot = true;
                            }
                        } else {
                            has_clear_flank_shot = true;
                        }
                    } else {
                        // Compute per-slot lateral offset along wide cover walls when multiple friendlies are present
                        let lateral_offset = if friendly_snapshots.len() > 1 {
                            match (friendly.fireteam, friendly.slot) {
                                (1, 0) => -0.8,
                                (1, 1) => -2.5,
                                (1, 2) => 2.5,
                                (1, 3) => -4.2,
                                (2, 0) => 0.8,
                                (2, 1) => -5.8,
                                (2, 2) => 4.2,
                                _ => 5.8,
                            }
                        } else {
                            0.0
                        };

                        // Taking cover behind an obstruction between soldier and enemy fire overrules formation
                        if let Some(cover_spot) = find_cover_position_with_offset(
                            friendly_pos,
                            target,
                            &buildings,
                            &low_walls,
                            soldier_radius,
                            18.0,
                            lateral_offset,
                        ) {
                            let to_cover = cover_spot - friendly_pos;
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
                                        friendly_pos,
                                        desired_move,
                                        &buildings,
                                        &low_walls,
                                        soldier_radius,
                                    )
                                };
                                let dir_mult =
                                    directional_speed_multiplier(soldier.facing_dir, move_dir);
                                let speed = friendly.speed * 1.15 * dir_mult;
                                let next_pos = friendly_pos + move_dir * speed * dt;
                                let resolved = resolve_circle_obstacle_collision_bounded(
                                    next_pos,
                                    soldier_radius,
                                    &buildings,
                                    &low_walls,
                                    world_bound,
                                );
                                tf.translation.x = resolved.x;
                                tf.translation.z = resolved.y;
                            } else {
                                // Directly behind cover: crouch low behind the obstruction
                                soldier.stance = Stance::Crouching;
                            }
                        } else if let Some((p_pos, _, _)) = player_info {
                            // No cover nearby: keep moving toward formation if far out of position
                            let (formation_spot, _, _) = compute_formation_target(
                                friendly.fireteam,
                                friendly.slot,
                                p_pos,
                                formation_heading,
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
                                let dir_mult =
                                    directional_speed_multiplier(soldier.facing_dir, move_dir);
                                let speed = friendly.speed * catch_up * dir_mult;
                                let next_pos = friendly_pos + move_dir * speed * dt;
                                let resolved = resolve_circle_obstacle_collision_bounded(
                                    next_pos,
                                    soldier_radius,
                                    &buildings,
                                    &low_walls,
                                    world_bound,
                                );
                                tf.translation.x = resolved.x;
                                tf.translation.z = resolved.y;
                            }
                        }
                    }

                    let current_pos = Vec2::new(tf.translation.x, tf.translation.z);
                    let near_low_wall = is_near_low_wall(current_pos, &low_walls, 2.5)
                        || has_low_wall_in_line_of_fire(current_pos, target, &low_walls);

                    if !is_moving {
                        if matches!(
                            active_squad_phase,
                            SquadTacticalPhase::DecisionActive(SquadLeaderDecision::HastyAmbush {
                                triggered: false,
                                ..
                            })
                        ) && !near_low_wall
                        {
                            soldier.stance = Stance::Prone;
                        } else if near_low_wall
                            || (dist < 14.0 && !has_clear_flank_shot)
                            || active_squad_phase != SquadTacticalPhase::Normal
                        {
                            // Take cover behind low wall or crouch during active squad battle drill
                            soldier.stance = Stance::Crouching;
                        } else {
                            soldier.stance = Stance::Standing;
                        }
                    }

                    // Stagger Fire Team 1 (Alpha) fire rate and tactical reloads so at least two
                    // members are always firing while the others reload.
                    if friendly.fireteam == 1 && is_ft1_suppressing {
                        let mut target_interval: f32 = match friendly.slot {
                            0 => 0.15,
                            1 => 0.24,
                            2 => 0.18,
                            _ => 0.28,
                        };
                        if ft1_able_to_fire <= 2 {
                            if let Some(w) = soldier.inventory.active_weapon() {
                                let min_interval = 2.05 / (w.ammo.current_mag.max(1) as f32);
                                target_interval = target_interval.max(min_interval);
                            }
                        }
                        friendly
                            .shoot_timer
                            .set_duration(std::time::Duration::from_secs_f32(target_interval));

                        // Proactive staggered reload for Pair A (slots 0 & 2) while >2 FT1 members can still fire
                        let early_reload_threshold = match friendly.slot {
                            0 => Some(15),
                            2 => Some(9),
                            _ => None,
                        };
                        if let Some(thresh) = early_reload_threshold {
                            if ft1_able_to_fire > 2 {
                                if let Some(weapon) = soldier.inventory.active_weapon_mut() {
                                    if weapon.ammo.current_mag <= thresh
                                        && weapon.ammo.can_fire()
                                        && weapon.ammo.start_reload()
                                    {
                                        ft1_able_to_fire = ft1_able_to_fire.saturating_sub(1);
                                    }
                                }
                            }
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

                    // Fire weapon at target enemy if not swapping, not holding fire, and not pinned behind cover by suppression
                    friendly.shoot_timer.tick(time.delta());
                    let pinned_behind_cover =
                        !is_moving && near_low_wall && suppression.is_suppressed();
                    let can_engage_target =
                        !visible_enemies.is_empty() || is_ft1_suppressing || has_clear_flank_shot;
                    if !hold_fire
                        && can_engage_target
                        && !(is_ft2_flanking && is_moving)
                        && !pinned_behind_cover
                        && friendly.shoot_timer.is_finished()
                        && !soldier.inventory.is_swapping()
                    {
                        let mut should_fire = false;
                        let mut bullet_speed = 45.0;
                        let mut bullet_lifetime = 1.5;

                        if let Some(weapon) = soldier.inventory.active_weapon_mut() {
                            if weapon.ammo.current_mag == 0 {
                                if weapon.ammo.start_reload() && friendly.fireteam == 1 {
                                    ft1_able_to_fire = ft1_able_to_fire.saturating_sub(1);
                                }
                            } else if weapon.ammo.can_fire() {
                                weapon.ammo.current_mag -= 1;
                                if weapon.ammo.current_mag == 0 && friendly.fireteam == 1 {
                                    let _ = weapon.ammo.start_reload();
                                    ft1_able_to_fire = ft1_able_to_fire.saturating_sub(1);
                                }
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
                            let shot_dir =
                                apply_shoot_spread(soldier.facing_dir, is_moving, spread_seed);
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
                if let Some((p_pos, _, p_stance)) = player_info {
                    let (formation_spot, base_facing, flank_angle) = compute_formation_target(
                        friendly.fireteam,
                        friendly.slot,
                        p_pos,
                        formation_heading,
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
                        let resolved = resolve_circle_obstacle_collision_bounded(
                            next_pos,
                            soldier_radius,
                            &buildings,
                            &low_walls,
                            world_bound,
                        );
                        tf.translation.x = resolved.x;
                        tf.translation.z = resolved.y;
                    } else {
                        // In formation spot: hold flank guard orientation
                        soldier.facing_dir = guard_facing;
                    }
                } else if let (Some(active_nav_goal), Some((lead_ft, lead_slot, lead_pos))) = (
                    match active_squad_phase {
                        SquadTacticalPhase::DecisionActive(SquadLeaderDecision::Bypass {
                            detour_waypoint,
                            ..
                        }) => Some(detour_waypoint),
                        SquadTacticalPhase::DecisionActive(SquadLeaderDecision::BreakContact {
                            rally_point,
                            ..
                        }) => Some(rally_point),
                        _ => squad_take_pos_objective,
                    },
                    squad_leader_entry,
                ) {
                    let squad_obj = active_nav_goal;
                    // Autonomous squad execution of CommanderOrder::TakePosition or Bypass detour when no Player entity exists
                    let leader_to_obj = squad_obj - lead_pos;
                    let leader_dist_to_obj = leader_to_obj.length();
                    let is_squad_leader =
                        friendly.fireteam == lead_ft && friendly.slot == lead_slot;

                    let (target_spot, guard_facing, target_stance) = if leader_dist_to_obj > 4.5 {
                        // Phase 1: Tactical Approach in squad formation toward `squad_obj`
                        let squad_heading = leader_to_obj / leader_dist_to_obj;
                        if is_squad_leader {
                            (squad_obj, squad_heading, Stance::Standing)
                        } else {
                            let sq_ft1_lead = friendly_snapshots
                                .iter()
                                .filter(|(id, ft, _, _, _, _, _, _, _)| *id == sq_id && *ft == 1)
                                .min_by_key(|(_, _, slot, _, _, _, _, _, _)| *slot)
                                .map(|(_, _, _, pos, _, _, _, _, _)| (*pos, squad_heading));
                            let sq_ft2_lead = friendly_snapshots
                                .iter()
                                .filter(|(id, ft, _, _, _, _, _, _, _)| *id == sq_id && *ft == 2)
                                .min_by_key(|(_, _, slot, _, _, _, _, _, _)| *slot)
                                .map(|(_, _, _, pos, _, _, _, _, _)| (*pos, squad_heading));
                            let (spot, base_facing, flank_angle) = compute_formation_target(
                                friendly.fireteam,
                                friendly.slot,
                                lead_pos,
                                squad_heading,
                                sq_ft1_lead,
                                sq_ft2_lead,
                            );
                            let (sin_a, cos_a) = flank_angle.sin_cos();
                            let facing = Vec2::new(
                                base_facing.x * cos_a - base_facing.y * sin_a,
                                base_facing.x * sin_a + base_facing.y * cos_a,
                            )
                            .normalize_or_zero();
                            (spot, facing, Stance::Standing)
                        }
                    } else {
                        // Phase 4: Consolidate & Hold 360° perimeter around `squad_obj`
                        if is_squad_leader {
                            (squad_obj, Vec2::new(0.0, -1.0), Stance::Crouching)
                        } else {
                            let idx = if friendly.fireteam == 1 {
                                friendly.slot
                            } else {
                                4 + friendly.slot
                            };
                            let angle = (idx as f32) * (std::f32::consts::TAU / 8.0);
                            let radius = if friendly.fireteam == 1 { 4.2 } else { 5.8 };
                            let offset = Vec2::new(angle.sin(), -angle.cos()) * radius;
                            let outward = offset.normalize_or_zero();
                            (squad_obj + offset, outward, Stance::Crouching)
                        }
                    };

                    let to_spot = target_spot - friendly_pos;
                    let dist_to_spot = to_spot.length();
                    if dist_to_spot > 0.85 {
                        soldier.stance = Stance::Standing;
                        let desired_move = to_spot / dist_to_spot;
                        let move_dir = find_avoidance_direction(
                            friendly_pos,
                            desired_move,
                            &buildings,
                            &low_walls,
                            soldier_radius,
                        );
                        soldier.facing_dir = if dist_to_spot < 1.4 {
                            guard_facing
                        } else {
                            move_dir
                        };
                        let catch_up = if !is_squad_leader && dist_to_spot > 5.0 {
                            1.30
                        } else if !is_squad_leader && dist_to_spot > 2.0 {
                            1.15
                        } else {
                            1.0
                        };
                        let dir_mult = directional_speed_multiplier(soldier.facing_dir, move_dir);
                        let speed = friendly.speed * catch_up * dir_mult;
                        let next_pos = friendly_pos + move_dir * speed * dt;
                        let resolved = resolve_circle_obstacle_collision_bounded(
                            next_pos,
                            soldier_radius,
                            &buildings,
                            &low_walls,
                            world_bound,
                        );
                        tf.translation.x = resolved.x;
                        tf.translation.z = resolved.y;
                    } else {
                        soldier.stance = target_stance;
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
        assert!(
            weapon.ammo.is_reloading(),
            "Friendly should start reloading when mag is empty"
        );
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
        assert!(
            soldier.inventory.is_swapping(),
            "Friendly should be swapping to sidearm"
        );
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

        let ft1_members: Vec<&Friendly> = friendlies
            .iter()
            .map(|(f, _, _)| *f)
            .filter(|f| f.fireteam == 1)
            .collect();
        let ft2_members: Vec<&Friendly> = friendlies
            .iter()
            .map(|(f, _, _)| *f)
            .filter(|f| f.fireteam == 2)
            .collect();

        assert_eq!(
            ft1_members.len(),
            4,
            "1st Fire Team (Alpha) must have 4 members"
        );
        assert_eq!(
            ft2_members.len(),
            4,
            "2nd Fire Team (Bravo) must have 4 members"
        );

        let mut ft1_slots: Vec<usize> = ft1_members.iter().map(|f| f.slot).collect();
        ft1_slots.sort();
        assert_eq!(ft1_slots, vec![0, 1, 2, 3]);

        let mut ft2_slots: Vec<usize> = ft2_members.iter().map(|f| f.slot).collect();
        ft2_slots.sort();
        assert_eq!(ft2_slots, vec![0, 1, 2, 3]);

        // FT1 Slot 0 is Alpha Team Leader directly trailing player
        let ft1_slot0 = friendlies
            .iter()
            .find(|(f, _, _)| f.fireteam == 1 && f.slot == 0)
            .unwrap();
        assert_eq!(ft1_slot0.2.translation.x, 0.0);
        assert_eq!(ft1_slot0.2.translation.z, 50.5);
        assert!(
            ft1_slot0.1.facing_dir.y < -0.9,
            "FT1 Slot 0 must face North"
        );

        // FT1 Slot 3 is Alpha Rear Guard
        let ft1_slot3 = friendlies
            .iter()
            .find(|(f, _, _)| f.fireteam == 1 && f.slot == 3)
            .unwrap();
        assert_eq!(ft1_slot3.2.translation.x, 0.0);
        assert_eq!(ft1_slot3.2.translation.z, 54.1);
        assert!(
            ft1_slot3.1.facing_dir.y > 0.9,
            "FT1 Slot 3 must face South (rear guard)"
        );

        // FT2 Slot 0 is Bravo Leader trailing behind FT1
        let ft2_slot0 = friendlies
            .iter()
            .find(|(f, _, _)| f.fireteam == 2 && f.slot == 0)
            .unwrap();
        assert_eq!(ft2_slot0.2.translation.x, 0.0);
        assert_eq!(ft2_slot0.2.translation.z, 55.5);
        assert!(
            ft2_slot0.1.facing_dir.y < -0.9,
            "FT2 Leader must face forward (North)"
        );

        // FT2 Slot 3 is squad rear security facing South
        let ft2_slot3 = friendlies
            .iter()
            .find(|(f, _, _)| f.fireteam == 2 && f.slot == 3)
            .unwrap();
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
        app.add_plugins((crate::player::PlayerPlugin, crate::friendly::FriendlyPlugin));
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

    #[test]
    fn test_squad_does_not_reposition_when_player_stands_still_and_looks_around() {
        let mut app = create_test_app();
        app.add_systems(Startup, (crate::player::setup_player, setup_friendly));
        app.add_systems(Update, update_friendly);
        app.update();

        // Let squad settle for a few frames
        for _ in 0..10 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(16));
            app.update();
        }

        let initial_positions: Vec<(usize, usize, Vec2)> = app
            .world_mut()
            .query::<(&Friendly, &Transform)>()
            .iter(app.world())
            .map(|(f, tf)| {
                (
                    f.fireteam,
                    f.slot,
                    Vec2::new(tf.translation.x, tf.translation.z),
                )
            })
            .collect();
        assert_eq!(initial_positions.len(), 8);

        // Rotate player facing direction 90 deg East, then 180 deg South, while standing still
        for look_dir in [
            Vec2::new(1.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(-1.0, 0.0),
        ] {
            {
                let mut player_q = app
                    .world_mut()
                    .query_filtered::<&mut Soldier, With<Player>>();
                let mut player_soldier = player_q.single_mut(app.world_mut()).unwrap();
                player_soldier.facing_dir = look_dir;
            }
            for _ in 0..20 {
                let mut time = app.world_mut().resource_mut::<Time>();
                time.advance_by(std::time::Duration::from_millis(16));
                app.update();
            }
        }

        let final_positions: Vec<(usize, usize, Vec2)> = app
            .world_mut()
            .query::<(&Friendly, &Transform)>()
            .iter(app.world())
            .map(|(f, tf)| {
                (
                    f.fireteam,
                    f.slot,
                    Vec2::new(tf.translation.x, tf.translation.z),
                )
            })
            .collect();

        for (ft, slot, init_pos) in &initial_positions {
            let final_pos = final_positions
                .iter()
                .find(|(f, s, _)| f == ft && s == slot)
                .map(|(_, _, p)| *p)
                .unwrap();
            let moved = (*init_pos - final_pos).length();
            assert!(
                moved < 0.05,
                "FT{} slot {} repositioned by {}m while player stood still and looked around!",
                ft,
                slot,
                moved
            );
        }
    }

    #[test]
    fn test_friendly_taking_fire_seeks_cover_behind_wall_overruling_formation() {
        let mut app = create_test_app();

        // Player is back at z = 18.0 (so FT1 slot 0 formation spot would be z = 20.5)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, 18.0),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        // South courtyard wall at z = 8.0 (half_size = 8.0 x 0.5)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.375, 8.0),
            LowWall,
            LowWallCollider {
                half_size: Vec2::new(8.0, 0.5),
            },
        ));

        // Friendly at (0.0, 13.0) (5m south of wall, 7.5m ahead of formation spot z=20.5)
        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 13.0),
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

        app.add_systems(Update, update_friendly);

        // Enemy fires from north side of courtyard (0.0, -9.5)
        app.world_mut().write_message(GunshotEvent {
            position: Vec2::new(0.0, -9.5),
            shooter_team: Team::Enemy,
        });

        for _ in 0..15 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        let tf = app.world().get::<Transform>(friendly_entity).unwrap();
        let soldier = app.world().get::<Soldier>(friendly_entity).unwrap();

        // Friendly should overrule formation (z=20.5) and move forward to crouch directly behind the south wall (z ≈ 9.35)
        assert!(
            (tf.translation.z - 9.35).abs() < 0.4,
            "Friendly should move directly behind south wall (z ≈ 9.35) instead of retreating to formation (z=20.5), got z={}",
            tf.translation.z
        );
        assert_eq!(
            soldier.stance,
            Stance::Crouching,
            "Friendly should crouch directly behind the wall"
        );
    }

    #[test]
    fn test_courtyard_flanking_and_ft1_staggered_suppressive_reloads() {
        let mut app = create_test_app();
        app.insert_resource(crate::common::types::CurrentLevel::Level3);
        app.insert_resource(SquadCommand::FlankLeft);

        // Spawn courtyard low walls (4 waist-high walls in a 16x16 square)
        for &(size, pos) in &crate::world::buildings::COURTYARD_LOW_WALLS {
            app.world_mut().spawn((
                Transform::from_translation(pos),
                LowWall,
                LowWallCollider {
                    half_size: Vec2::new(size.x * 0.5, size.z * 0.5),
                },
            ));
        }

        // Player behind south wall at (0.0, 9.5)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, 9.5),
            Player,
            Team::Player,
            Soldier::default(),
        ));

        // Spawn full 8-man friendly squad (4 in Fire Team 1, 4 in Fire Team 2) behind south wall
        for fireteam in 1..=2 {
            for slot in 0..4 {
                let x = (slot as f32 - 1.5) * 1.6;
                let z = if fireteam == 1 { 10.0 } else { 12.0 };
                app.world_mut().spawn((
                    Transform::from_xyz(x, 0.05, z),
                    Friendly {
                        fireteam,
                        slot,
                        state: FriendlyAiState::Follow,
                        shoot_timer: Timer::from_seconds(0.2, TimerMode::Repeating),
                        speed: 12.0,
                        vision_range: 24.0,
                        vision_half_angle: 45.0_f32.to_radians(),
                        inner_radius: 2.0,
                        target_pos: None,
                        target_lost_timer: 0.0,
                    },
                    Team::Player,
                    Soldier::default(),
                ));
            }
        }

        // Spawn enemy behind north courtyard wall at (0.0, -9.35)
        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, -9.35),
            Enemy {
                state: crate::enemy::ai::EnemyAiState::Engage,
                current_waypoint_idx: 0,
                last_known_player_pos: Some(Vec2::new(0.0, 9.5)),
                investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                speed: 5.5,
                vision_range: 24.0,
                vision_half_angle: 45.0_f32.to_radians(),
                inner_radius: 2.0,
                stuck_timer: 0.0,
                last_pos: Vec2::new(0.0, -9.35),
            },
            Team::Enemy,
            Soldier {
                facing_dir: Vec2::new(0.0, 1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Crouching,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        app.add_systems(Update, update_friendly);

        // Simulate 10.0 seconds (200 frames at 50ms) — long enough for FT1 to cycle through reloads
        // and for FT2 to flank around the West wall to a clear-shot position.
        let mut saw_any_ft1_reload = false;
        for frame in 0..200 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();

            // Verify at least 2 members of Fire Team 1 are ALWAYS able to fire (not reloading) on every frame
            let mut ft1_ready = 0;
            let mut ft1_reloading = 0;
            for (f, s) in app
                .world_mut()
                .query::<(&Friendly, &Soldier)>()
                .iter(app.world())
            {
                if f.fireteam == 1 {
                    let w = s.inventory.active_weapon().unwrap();
                    if w.ammo.is_reloading() {
                        ft1_reloading += 1;
                        saw_any_ft1_reload = true;
                    } else if w.ammo.can_fire() {
                        ft1_ready += 1;
                    }
                }
            }
            assert!(
                ft1_ready >= 2,
                "Frame {}: Fire Team 1 had only {} members able to fire ({} reloading)! At least 2 must always be able to fire.",
                frame,
                ft1_ready,
                ft1_reloading
            );
        }

        assert!(
            saw_any_ft1_reload,
            "Fire Team 1 should have performed staggered reloads during 10s of suppressive fire"
        );

        // Verify Fire Team 2 (Bravo) flanked along the West side to a clear-shot position
        let low_wall_entities: Vec<(Transform, LowWallCollider)> = app
            .world_mut()
            .query::<(&Transform, &LowWallCollider)>()
            .iter(app.world())
            .map(|(t, c)| (*t, *c))
            .collect();
        let low_wall_refs: Vec<(&Transform, &LowWallCollider)> =
            low_wall_entities.iter().map(|(t, c)| (t, c)).collect();

        let mut ft2_clear_shots = 0;
        for (f, tf) in app
            .world_mut()
            .query::<(&Friendly, &Transform)>()
            .iter(app.world())
        {
            if f.fireteam == 2 {
                let pos = Vec2::new(tf.translation.x, tf.translation.z);
                if has_clear_shot_past_cover(pos, Vec2::new(0.0, -9.35), &[], &low_wall_refs) {
                    ft2_clear_shots += 1;
                }
            }
        }
        assert!(
            ft2_clear_shots >= 2,
            "Fire Team 2 should maneuver along cover to a clear flanking shot on the enemy (got {} with clear shot)",
            ft2_clear_shots
        );
    }

    #[test]
    fn test_level_4_friendly_squad_spawns_south_of_village_on_road() {
        let mut app = create_test_app();
        app.insert_resource(CurrentLevel::Level4);
        app.insert_resource(SquadCommand::Formation);
        app.add_systems(Startup, setup_friendly);
        app.add_systems(Update, update_friendly);

        app.update();

        let positions: Vec<Vec2> = app
            .world_mut()
            .query_filtered::<&Transform, With<Friendly>>()
            .iter(app.world())
            .map(|tf| Vec2::new(tf.translation.x, tf.translation.z))
            .collect();

        assert_eq!(
            positions.len(),
            RTS_SOUTH_ROAD_FRIENDLY_CONFIGS.len(),
            "test_level_4 should spawn the 9-man friendly squad"
        );
        for pos in &positions {
            assert!(
                pos.x.abs() <= crate::common::types::RTS_ROAD_WIDTH * 0.5,
                "Friendly soldier in test_level_4 should be on the road (|x| <= 4.0), got x = {}",
                pos.x
            );
            assert!(
                pos.y >= 150.0,
                "Friendly soldier in test_level_4 should be a fair way south of the village (z >= 150.0), got z = {}",
                pos.y
            );
        }
    }

    #[test]
    fn test_commander_order_take_position_advances_and_consolidates_squad() {
        let mut app = create_test_app();
        app.insert_resource(CurrentLevel::Level4);
        app.insert_resource(SquadCommand::Formation);
        let objective = Vec2::new(0.0, 140.0);
        app.insert_resource(CommanderOrder::TakePosition(objective));
        app.add_systems(Startup, setup_friendly);
        app.add_systems(Update, update_friendly);

        app.update();

        // Simulate 4.5 seconds (90 frames at 50ms) so the squad advances from Z=178..188 to Z=140
        for _ in 0..90 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        let positions: Vec<Vec2> = app
            .world_mut()
            .query_filtered::<&Transform, With<Friendly>>()
            .iter(app.world())
            .map(|tf| Vec2::new(tf.translation.x, tf.translation.z))
            .collect();

        assert_eq!(positions.len(), 9);
        for pos in &positions {
            let dist_to_obj = (*pos - objective).length();
            assert!(
                dist_to_obj < 10.0,
                "Friendly squad member should advance to and consolidate around TakePosition objective {:?}, got {:?} (dist {:.2})",
                objective,
                pos,
                dist_to_obj
            );
        }
    }

    #[test]
    fn test_squad_leader_mett_tc_evaluates_all_five_decisions() {
        let squad_center = Vec2::new(0.0, 20.0);
        let enemy_centroid = Vec2::new(0.0, 0.0);
        // Wall at (0.0, 14.0) offers immediate concealment near the squad (within 12m) along the enemy's path
        let wall_tf = Transform::from_xyz(0.0, 0.4, 14.0);
        let wall_col = LowWallCollider {
            half_size: Vec2::new(3.0, 0.5),
        };
        let flank_wall_tf = Transform::from_xyz(-8.0, 0.4, 10.0);
        let flank_wall_col = LowWallCollider {
            half_size: Vec2::new(4.0, 0.5),
        };
        let low_walls = vec![(&wall_tf, &wall_col), (&flank_wall_tf, &flank_wall_col)];

        // 1. HastyAmbush: undetected visual contact + enemy moving toward squad
        let ambush = evaluate_squad_leader_mett_tc(
            squad_center,
            enemy_centroid,
            9,
            3,
            0,
            true,
            true,
            None,
            &[],
            &low_walls,
            240.0,
        );
        assert!(
            matches!(ambush, SquadLeaderDecision::HastyAmbush { .. }),
            "Expected HastyAmbush, got {:?}",
            ambush
        );

        // 2. Bypass: undetected visual contact + deeper TakePosition objective (>45m beyond enemy)
        let bypass = evaluate_squad_leader_mett_tc(
            squad_center,
            enemy_centroid,
            9,
            3,
            0,
            true,
            false,
            Some(Vec2::new(0.0, -80.0)),
            &[],
            &low_walls,
            240.0,
        );
        assert!(
            matches!(bypass, SquadLeaderDecision::Bypass { .. }),
            "Expected Bypass, got {:?}",
            bypass
        );

        // 3. BreakContact: heavily outnumbered or critically depleted (<= 4 alive)
        let break_contact = evaluate_squad_leader_mett_tc(
            squad_center,
            enemy_centroid,
            4,
            8,
            2,
            false,
            false,
            Some(Vec2::new(0.0, 0.0)),
            &[],
            &low_walls,
            240.0,
        );
        assert!(
            matches!(break_contact, SquadLeaderDecision::BreakContact { .. }),
            "Expected BreakContact, got {:?}",
            break_contact
        );

        // 4. FixAndSupport: large enemy position (>= 6) vs equal/smaller squad (or >= 4 with no cover)
        let fix_support = evaluate_squad_leader_mett_tc(
            squad_center,
            enemy_centroid,
            8,
            8,
            1,
            false,
            false,
            Some(Vec2::new(0.0, 0.0)),
            &[],
            &[],
            240.0,
        );
        assert!(
            matches!(fix_support, SquadLeaderDecision::FixAndSupport { .. }),
            "Expected FixAndSupport, got {:?}",
            fix_support
        );

        // 5. SquadAttack: favorable combat power and cover/flank available
        let squad_attack = evaluate_squad_leader_mett_tc(
            squad_center,
            enemy_centroid,
            9,
            3,
            0,
            false,
            false,
            Some(Vec2::new(0.0, 0.0)),
            &[],
            &low_walls,
            240.0,
        );
        assert!(
            matches!(squad_attack, SquadLeaderDecision::SquadAttack { .. }),
            "Expected SquadAttack, got {:?}",
            squad_attack
        );
    }

    #[test]
    fn test_squad_leader_react_to_contact_and_decisions_execution() {
        let mut app = create_test_app();
        app.insert_resource(CurrentLevel::Level4);
        app.insert_resource(SquadCommand::Formation);
        app.insert_resource(CommanderOrder::TakePosition(Vec2::new(0.0, 0.0)));
        app.init_resource::<SquadDoctrineState>();
        app.add_systems(Update, update_friendly);

        // Spawn Squad Leader (1, 0), Alpha Fire Team member (1, 1), and Bravo Fire Team member (2, 0)
        // plus 3 more squad members so 6 friendlies outnumber 2 enemies -> SquadAttack
        let mut friendly_entities = Vec::new();
        let configs = [
            (1, 0, Vec3::new(0.0, 0.05, 18.0)),
            (1, 1, Vec3::new(-2.0, 0.05, 19.0)),
            (1, 2, Vec3::new(2.0, 0.05, 19.0)),
            (2, 0, Vec3::new(-1.5, 0.05, 20.0)),
            (2, 1, Vec3::new(1.5, 0.05, 20.0)),
            (2, 2, Vec3::new(0.0, 0.05, 21.0)),
        ];
        for (fireteam, slot, pos) in configs {
            let id = app
                .world_mut()
                .spawn((
                    Transform::from_translation(pos),
                    Friendly {
                        fireteam,
                        slot,
                        state: FriendlyAiState::Follow,
                        shoot_timer: Timer::from_seconds(0.2, TimerMode::Repeating),
                        speed: 8.0,
                        vision_range: 30.0,
                        vision_half_angle: 60.0_f32.to_radians(),
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
            friendly_entities.push(id);
        }

        // Spawn a low wall offering cover on the flank
        app.world_mut().spawn((
            Transform::from_xyz(-8.0, 0.4, 8.0),
            LowWall,
            LowWallCollider {
                half_size: Vec2::new(3.0, 0.5),
            },
        ));

        // Spawn 2 enemies at origin engaged with the squad
        for x in [-1.5, 1.5] {
            app.world_mut().spawn((
                Transform::from_xyz(x, 0.05, 0.0),
                Enemy {
                    state: crate::enemy::ai::EnemyAiState::Engage,
                    current_waypoint_idx: 0,
                    last_known_player_pos: Some(Vec2::new(0.0, 18.0)),
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 5.5,
                    vision_range: 30.0,
                    vision_half_angle: 60.0_f32.to_radians(),
                    inner_radius: 2.0,
                    stuck_timer: 0.0,
                    last_pos: Vec2::new(x, 0.0),
                },
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ));
        }

        // Step 1: First frame of unexpected contact triggers ReactToContact battle drill
        {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
        }
        app.update();

        {
            let doctrine = app.world().resource::<SquadDoctrineState>();
            let entry = doctrine
                .squads
                .get(&0)
                .expect("Squad 0 doctrine entry should exist");
            assert!(
                matches!(entry.phase, SquadTacticalPhase::ReactToContact { .. }),
                "Initial contact should trigger ReactToContact battle drill"
            );
        }

        // Step 2: Advance past the 1.2s METT-TC assessment timer -> Squad Leader chooses SquadAttack
        for _ in 0..30 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        {
            let doctrine = app.world().resource::<SquadDoctrineState>();
            let entry = doctrine.squads.get(&0).unwrap();
            assert!(
                matches!(
                    entry.phase,
                    SquadTacticalPhase::DecisionActive(SquadLeaderDecision::SquadAttack { .. })
                ),
                "Outnumbering 6 vs 2 with flank cover should transition from ReactToContact to SquadAttack, got {:?}",
                entry.phase
            );
        }

        // Verify Squad Leader (1, 0) and Fire Team 2 (2, 0) displaced laterally on the flanking route
        let sl_tf = app.world().get::<Transform>(friendly_entities[0]).unwrap();
        let bravo_tf = app.world().get::<Transform>(friendly_entities[3]).unwrap();
        assert!(
            sl_tf.translation.x.abs() > 0.3 || sl_tf.translation.z < 17.5,
            "Squad Leader (1, 0) should maneuver with the flanking assault element"
        );
        assert!(
            bravo_tf.translation.x.abs() > 1.6 || bravo_tf.translation.z < 19.5,
            "Fire Team 2 should maneuver along the flanking route"
        );
    }
}
