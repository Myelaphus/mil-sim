use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;
use crate::combat::bullet::{Bullet, BulletAssets, CorpseAssets};
use crate::common::geometry::{can_see_target, resolve_circle_obstacle_collision_bounded};
use crate::common::types::{
    ApcOrder, AppState, BloodDecal, CommanderOrder, Corpse, CurrentLevel, GunshotEvent,
    LevelEntity, RTS_SOLDIER_WORLD_BOUND, SOLDIER_RADIUS, SOLDIER_WORLD_BOUND, Soldier, Stance,
    Suppression, Team,
};
use crate::enemy::ai::{Enemy, Zombie};
use crate::fog::mesh::create_vision_cone_mesh;
use crate::friendly::{Friendly, FriendlyAiState};
use crate::player::Player;
use crate::world::buildings::{Building, BuildingCollider, LowWall, LowWallCollider};

/// Realistic scale constants for the Armored Personnel Carrier (APC) (8x8 IFV/Stryker-class):
/// - Hull width (`X` in local space): `3.4m` (2 soldiers side-by-side + armor plating)
/// - Hull height (`Y` in local space): `2.3m`
/// - Hull length (`Z` in local space): `7.6m` (front engine/driver/gunner compartment + rear 9-man troop bay)
/// - Turret radius: `1.15m` circular 360° rotating turret mounted on the hull roof
pub const APC_HULL_WIDTH: f32 = 3.4;
pub const APC_HULL_HEIGHT: f32 = 2.3;
pub const APC_HULL_LENGTH: f32 = 7.6;
pub const APC_TURRET_RADIUS: f32 = 1.15;
pub const APC_TURRET_HEIGHT: f32 = 0.65;
pub const APC_BARREL_LENGTH: f32 = 2.4;
pub const APC_BARREL_WIDTH: f32 = 0.22;

/// Minimum lateral clearance radius required for an APC to pass safely between obstacles:
/// `APC_HULL_WIDTH * 0.5 + 0.35 = 2.05m` radius (`4.10m` minimum passable gap width).
/// Any gap narrower than `4.10m` (which infantry at `1.2m` diameter can easily walk through)
/// is recognized as too small for the APC to pass.
pub const APC_CLEARANCE_RADIUS: f32 = APC_HULL_WIDTH * 0.5 + 0.35;

/// Minimum APC speed (`m/s`) at which colliding with an on-foot soldier is lethal, killing the
/// soldier on impact and allowing the APC to drive directly through them.
pub const APC_LETHAL_IMPACT_SPEED: f32 = 4.2;

/// Default spawn position for the friendly APC on the road south of the infantry squad in `test_level_4`.
pub const LEVEL_4_FRIENDLY_APC_SPAWN_POS: Vec2 = Vec2::new(0.0, 202.0);
pub const LEVEL_4_FRIENDLY_APC_FACING: Vec2 = Vec2::new(0.0, -1.0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VehicleCrewRole {
    Driver,
    Gunner,
    Passenger,
}

/// Attached to a soldier who is part of a vehicle's dedicated operating crew (or passenger complement).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct VehicleCrewMember {
    pub vehicle: Entity,
    pub role: VehicleCrewRole,
}

/// Marker component placed on a soldier while physically inside a vehicle.
/// While `MountedInVehicle` is present, the soldier is hidden inside the hull, excluded from
/// on-foot AI / bullet hits / pairwise collisions, and their transform tracks the vehicle.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountedInVehicle {
    pub vehicle: Entity,
    pub role: VehicleCrewRole,
}

/// Orders that can be issued to a crewed `ApcVehicle` (either by player/commander UI or AI).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum VehicleOrder {
    #[default]
    Hold,
    /// Drive realistically to the target world XZ coordinate (using forward acceleration, braking,
    /// speed-coupled Ackermann steering, or reversing for short rear adjustments).
    MoveTo(Vec2),
    /// Halt the vehicle and have the Driver and Gunner dismount through the side crew hatches.
    DismountCrew,
    /// Recall dismounted Driver and Gunner back to the vehicle hatches to remount and operate it.
    MountCrew,
}

/// Reusable Armored Personnel Carrier (APC) component supporting both `Team::Player` and `Team::Enemy`.
#[derive(Component, Debug, Clone)]
pub struct ApcVehicle {
    pub hull_half_size: Vec2,
    pub heading: Vec2,
    pub current_speed: f32,
    pub max_forward_speed: f32,
    pub max_reverse_speed: f32,
    pub acceleration: f32,
    pub reverse_acceleration: f32,
    pub braking_deceleration: f32,
    pub max_turn_rate_rad: f32,
    pub turret_yaw_local: f32,
    pub turret_target_yaw_local: f32,
    pub turret_turn_rate_rad: f32,
    pub turret_shoot_timer: Timer,
    pub turret_vision_range: f32,
    pub turret_vision_half_angle: f32,
    pub driver: Option<Entity>,
    pub gunner: Option<Entity>,
    pub passengers: Vec<Entity>,
    pub max_passengers: usize,
    pub order: VehicleOrder,
}

impl Default for ApcVehicle {
    fn default() -> Self {
        Self {
            hull_half_size: Vec2::new(APC_HULL_WIDTH * 0.5, APC_HULL_LENGTH * 0.5),
            heading: Vec2::new(0.0, -1.0),
            current_speed: 0.0,
            max_forward_speed: 14.5,
            max_reverse_speed: 6.5,
            acceleration: 7.5,
            reverse_acceleration: 4.8,
            braking_deceleration: 12.0,
            max_turn_rate_rad: 1.15,
            turret_yaw_local: 0.0,
            turret_target_yaw_local: 0.0,
            turret_turn_rate_rad: 1.85,
            turret_shoot_timer: Timer::from_seconds(0.28, TimerMode::Repeating),
            turret_vision_range: 24.0,
            turret_vision_half_angle: 45.0_f32.to_radians(),
            driver: None,
            gunner: None,
            passengers: Vec::new(),
            max_passengers: 9,
            order: VehicleOrder::Hold,
        }
    }
}

impl ApcVehicle {
    pub fn has_driver(&self) -> bool {
        self.driver.is_some()
    }

    pub fn has_gunner(&self) -> bool {
        self.gunner.is_some()
    }

    pub fn is_fully_crewed(&self) -> bool {
        self.driver.is_some() && self.gunner.is_some()
    }

    /// World-space right vector perpendicular to `heading` in the XZ plane.
    pub fn right_dir(&self) -> Vec2 {
        let forward = self.heading.normalize_or(Vec2::new(0.0, -1.0));
        Vec2::new(-forward.y, forward.x)
    }

    /// World-space hatch exit/entry position for a given crew role (`Driver`, `Gunner`, or `Passenger` rear ramp).
    pub fn hatch_world_pos(&self, vehicle_pos: Vec2, role: VehicleCrewRole) -> Vec2 {
        let forward = self.heading.normalize_or(Vec2::new(0.0, -1.0));
        let right = self.right_dir();
        let side_standoff = self.hull_half_size.x + SOLDIER_RADIUS + 0.45;
        let rear_standoff = self.hull_half_size.y + SOLDIER_RADIUS + 0.55;
        match role {
            // Driver exits/enters via front-left crew hatch
            VehicleCrewRole::Driver => vehicle_pos - right * side_standoff + forward * 1.3,
            // Gunner exits/enters via mid-right turret/hull hatch
            VehicleCrewRole::Gunner => vehicle_pos + right * side_standoff - forward * 0.2,
            // Infantry passengers load/unload via the rear troop ramp
            VehicleCrewRole::Passenger => vehicle_pos - forward * rear_standoff,
        }
    }

    /// World-space 2D facing vector of the 360° turret.
    pub fn turret_world_dir(&self) -> Vec2 {
        let hull_yaw = f32::atan2(-self.heading.x, -self.heading.y);
        let total_yaw = hull_yaw + self.turret_yaw_local;
        Vec2::new(-total_yaw.sin(), -total_yaw.cos()).normalize_or(self.heading)
    }
}

/// Marker component for the 360° rotating circular turret child entity on an `ApcVehicle`.
#[derive(Component, Debug, Clone, Copy)]
pub struct ApcTurret {
    pub vehicle: Entity,
}

pub struct VehiclePlugin;

impl Plugin for VehiclePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_level_vehicles)
            .add_systems(
                Update,
                (
                    handle_vehicle_crew_mount_commands,
                    update_crewed_apc_vehicles,
                    update_dismounted_crew_return,
                    resolve_soldier_apc_collisions,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

/// Spawns a reusable Armored Personnel Carrier (APC) with a rectangular armored hull, a 360°
/// rotating circular turret + autocannon barrel, and a mounted 2-man operating crew (Driver + Gunner)
/// that can exit and re-enter the vehicle.
pub fn spawn_apc_with_crew(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    team: Team,
    spawn_pos: Vec2,
    facing: Vec2,
) -> (Entity, Entity, Entity) {
    let forward = facing.normalize_or(Vec2::new(0.0, -1.0));
    let hull_yaw = f32::atan2(-forward.x, -forward.y);

    let (hull_color, turret_color, barrel_color, crew_color, cone_color) = match team {
        Team::Player => (
            Color::srgb(0.24, 0.38, 0.30),
            Color::srgb(0.20, 0.33, 0.25),
            Color::srgb(0.14, 0.18, 0.16),
            Color::srgb(0.18, 0.68, 0.88),
            Color::srgba(0.20, 0.75, 1.0, 0.16),
        ),
        Team::Enemy => (
            Color::srgb(0.42, 0.24, 0.22),
            Color::srgb(0.35, 0.18, 0.16),
            Color::srgb(0.18, 0.14, 0.14),
            Color::srgb(0.90, 0.15, 0.15),
            Color::srgba(0.95, 0.20, 0.20, 0.18),
        ),
    };

    let hull_mesh = meshes.add(Cuboid::new(APC_HULL_WIDTH, APC_HULL_HEIGHT, APC_HULL_LENGTH));
    let hull_mat = materials.add(StandardMaterial {
        base_color: hull_color,
        perceptual_roughness: 0.75,
        metallic: 0.2,
        ..default()
    });

    // Front glacis / driver viewport accent strip so front vs rear of hull is immediately readable
    let glacis_mesh = meshes.add(Cuboid::new(APC_HULL_WIDTH * 0.82, 0.18, 0.55));
    let glacis_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.70, 0.82, 0.88),
        perceptual_roughness: 0.3,
        ..default()
    });

    // Rear troop ramp accent bar
    let ramp_mesh = meshes.add(Cuboid::new(APC_HULL_WIDTH * 0.76, APC_HULL_HEIGHT * 0.72, 0.20));
    let ramp_mat = materials.add(StandardMaterial {
        base_color: barrel_color,
        perceptual_roughness: 0.85,
        ..default()
    });

    // Circular 360° turret cylinder + forward autocannon barrel
    let turret_mesh = meshes.add(Cylinder::new(APC_TURRET_RADIUS, APC_TURRET_HEIGHT));
    let turret_mat = materials.add(StandardMaterial {
        base_color: turret_color,
        perceptual_roughness: 0.65,
        metallic: 0.25,
        ..default()
    });
    let barrel_mesh = meshes.add(Cuboid::new(APC_BARREL_WIDTH, APC_BARREL_WIDTH, APC_BARREL_LENGTH));
    let barrel_mat = materials.add(StandardMaterial {
        base_color: barrel_color,
        perceptual_roughness: 0.5,
        metallic: 0.4,
        ..default()
    });

    let turret_vision_cone_mesh = meshes.add(create_vision_cone_mesh(24.0, FRAC_PI_2, 16));
    let turret_vision_cone_mat = materials.add(StandardMaterial {
        base_color: cone_color,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    let apc_entity = commands
        .spawn((
            LevelEntity,
            Mesh3d(hull_mesh),
            MeshMaterial3d(hull_mat),
            Transform::from_xyz(spawn_pos.x, APC_HULL_HEIGHT * 0.5, spawn_pos.y)
                .with_rotation(Quat::from_rotation_y(hull_yaw)),
            team,
            ApcVehicle {
                heading: forward,
                ..default()
            },
        ))
        .id();

    // Front driver viewport / glacis marker (local -Z is forward)
    commands.spawn((
        Mesh3d(glacis_mesh),
        MeshMaterial3d(glacis_mat),
        Transform::from_xyz(0.0, APC_HULL_HEIGHT * 0.45, -APC_HULL_LENGTH * 0.42),
        ChildOf(apc_entity),
    ));

    // Rear troop ramp marker (local +Z is rear)
    commands.spawn((
        Mesh3d(ramp_mesh),
        MeshMaterial3d(ramp_mat),
        Transform::from_xyz(0.0, -0.1, APC_HULL_LENGTH * 0.49),
        ChildOf(apc_entity),
    ));

    // 360-degree rotating circular turret mounted on top of the hull
    let turret_entity = commands
        .spawn((
            ApcTurret {
                vehicle: apc_entity,
            },
            Mesh3d(turret_mesh),
            MeshMaterial3d(turret_mat),
            Transform::from_xyz(0.0, APC_HULL_HEIGHT * 0.5 + APC_TURRET_HEIGHT * 0.5, -0.35),
            ChildOf(apc_entity),
        ))
        .id();

    // Autocannon barrel extending forward (-Z) from the circular turret
    commands.spawn((
        Mesh3d(barrel_mesh),
        MeshMaterial3d(barrel_mat),
        Transform::from_xyz(
            0.0,
            0.05,
            -(APC_TURRET_RADIUS + APC_BARREL_LENGTH * 0.45),
        ),
        ChildOf(turret_entity),
    ));

    // Vision cone attached to turret so the gunner's 360° sector is visible on the ground
    commands.spawn((
        Mesh3d(turret_vision_cone_mesh),
        MeshMaterial3d(turret_vision_cone_mat.clone()),
        Transform::from_xyz(
            0.0,
            -(APC_HULL_HEIGHT * 0.5 + APC_TURRET_HEIGHT * 0.5 - 0.04),
            0.0,
        )
        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
        ChildOf(turret_entity),
    ));

    // Spawn the 2 crew soldiers (Driver and Gunner) initially mounted inside the APC
    let soldier_mesh = meshes.add(Circle::new(SOLDIER_RADIUS));
    let pointer_mesh = meshes.add(Circle::new(0.15));
    let soldier_mat = materials.add(StandardMaterial {
        base_color: crew_color,
        perceptual_roughness: 0.5,
        ..default()
    });
    let pointer_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.98, 1.0),
        unlit: true,
        ..default()
    });
    let crew_cone_mesh = meshes.add(create_vision_cone_mesh(24.0, FRAC_PI_2, 16));

    let mut spawn_crew_soldier = |role: VehicleCrewRole, slot_idx: usize| -> Entity {
        let crew_ent = commands
            .spawn((
                LevelEntity,
                Mesh3d(soldier_mesh.clone()),
                MeshMaterial3d(soldier_mat.clone()),
                Transform::from_xyz(spawn_pos.x, 0.05, spawn_pos.y)
                    .with_rotation(Quat::from_rotation_y(hull_yaw) * Quat::from_rotation_x(-FRAC_PI_2)),
                Visibility::Hidden,
                team,
                Soldier {
                    facing_dir: forward,
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
                Suppression::default(),
                VehicleCrewMember {
                    vehicle: apc_entity,
                    role,
                },
                MountedInVehicle {
                    vehicle: apc_entity,
                    role,
                },
            ))
            .id();

        if team == Team::Player {
            commands.entity(crew_ent).insert((
                Friendly {
                    fireteam: 3, // Dedicated mechanized crew element
                    slot: slot_idx,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.35, TimerMode::Repeating),
                    speed: 11.0,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                crate::friendly::SquadId(1), // Separate crew ID so infantry squad doctrine ignores APC crew
            ));
        }

        commands.spawn((
            Mesh3d(pointer_mesh.clone()),
            MeshMaterial3d(pointer_mat.clone()),
            Transform::from_xyz(0.0, 0.42, 0.015),
            ChildOf(crew_ent),
        ));
        commands.spawn((
            Mesh3d(crew_cone_mesh.clone()),
            MeshMaterial3d(turret_vision_cone_mat.clone()),
            Transform::from_xyz(0.0, 0.0, -0.015),
            ChildOf(crew_ent),
        ));

        crew_ent
    };

    let driver_entity = spawn_crew_soldier(VehicleCrewRole::Driver, 0);
    let gunner_entity = spawn_crew_soldier(VehicleCrewRole::Gunner, 1);

    commands.entity(apc_entity).insert(ApcVehicle {
        heading: forward,
        driver: Some(driver_entity),
        gunner: Some(gunner_entity),
        ..default()
    });

    (apc_entity, driver_entity, gunner_entity)
}

/// Spawns the friendly APC on the road south of the village in `test_level_4`.
pub fn setup_level_vehicles(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    current_level: Option<Res<CurrentLevel>>,
) {
    if current_level.as_deref().copied().unwrap_or_default() != CurrentLevel::Level4 {
        return;
    }

    spawn_apc_with_crew(
        &mut commands,
        &mut meshes,
        &mut materials,
        Team::Player,
        LEVEL_4_FRIENDLY_APC_SPAWN_POS,
        LEVEL_4_FRIENDLY_APC_FACING,
    );
}

/// Dismounts all crew and passengers currently inside `vehicle_entity`, placing them outside their
/// respective hatches (`Driver` front-left, `Gunner` mid-right, `Passenger` rear ramp).
pub fn dismount_vehicle_crew(
    commands: &mut Commands,
    vehicle_entity: Entity,
    apc: &mut ApcVehicle,
    vehicle_pos: Vec2,
    buildings: &[(&Transform, &BuildingCollider)],
    low_walls: &[(&Transform, &LowWallCollider)],
    world_bound: f32,
    crew_q: &mut Query<
        (&mut Transform, &mut Soldier, &mut Visibility, Option<&mut Friendly>),
        (With<VehicleCrewMember>, Without<ApcVehicle>),
    >,
) {
    let driver_ent = apc.driver.take();
    let gunner_ent = apc.gunner.take();
    let passengers: Vec<Entity> = apc.passengers.drain(..).collect();

    let mut dismount_one = |crew_ent: Entity, role: VehicleCrewRole| {
        let raw_exit = apc.hatch_world_pos(vehicle_pos, role);
        let exit_pos = resolve_circle_obstacle_collision_bounded(
            raw_exit,
            SOLDIER_RADIUS,
            buildings,
            low_walls,
            world_bound,
        );
        let outward_facing = (exit_pos - vehicle_pos).normalize_or(apc.heading);
        let yaw = f32::atan2(-outward_facing.x, -outward_facing.y);
        if let Ok((mut c_tf, mut c_soldier, mut c_vis, friendly_opt)) = crew_q.get_mut(crew_ent) {
            c_tf.translation.x = exit_pos.x;
            c_tf.translation.y = 0.05;
            c_tf.translation.z = exit_pos.y;
            c_tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2);
            c_soldier.facing_dir = outward_facing;
            c_soldier.stance = Stance::Standing;
            *c_vis = Visibility::Inherited;
            if let Some(mut f) = friendly_opt {
                f.state = FriendlyAiState::Follow;
                f.target_pos = None;
            }
        }
        commands.entity(crew_ent).remove::<MountedInVehicle>();
    };

    if let Some(d_ent) = driver_ent {
        dismount_one(d_ent, VehicleCrewRole::Driver);
    }
    if let Some(g_ent) = gunner_ent {
        dismount_one(g_ent, VehicleCrewRole::Gunner);
    }
    for p_ent in passengers {
        dismount_one(p_ent, VehicleCrewRole::Passenger);
    }
    let _ = vehicle_entity;
}

/// Handles `[G]` hotkey or `VehicleOrder::DismountCrew` / `VehicleOrder::MountCrew` transitions,
/// and syncs friendly APC movement orders from its dedicated `ApcOrder` group resource.
pub fn handle_vehicle_crew_mount_commands(
    mut commands: Commands,
    current_level: Option<Res<CurrentLevel>>,
    keyboard_input: Option<Res<ButtonInput<KeyCode>>>,
    apc_order: Option<Res<ApcOrder>>,
    mut apc_q: Query<(Entity, &Transform, &Team, &mut ApcVehicle)>,
    mut crew_q: Query<
        (&mut Transform, &mut Soldier, &mut Visibility, Option<&mut Friendly>),
        (With<VehicleCrewMember>, Without<ApcVehicle>),
    >,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (With<Building>, Without<ApcVehicle>, Without<VehicleCrewMember>),
    >,
    low_walls_q: Query<
        (&Transform, &LowWallCollider),
        (With<LowWall>, Without<ApcVehicle>, Without<VehicleCrewMember>),
    >,
) {
    let level = current_level.as_deref().copied().unwrap_or_default();
    let world_bound = if level == CurrentLevel::Level4 {
        RTS_SOLDIER_WORLD_BOUND
    } else {
        SOLDIER_WORLD_BOUND
    };
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();
    let low_walls: Vec<(&Transform, &LowWallCollider)> = low_walls_q.iter().collect();

    let toggle_crew_key = keyboard_input
        .as_ref()
        .is_some_and(|keys| keys.just_pressed(KeyCode::KeyG));

    for (v_entity, v_tf, v_team, mut apc) in &mut apc_q {
        let v_pos = Vec2::new(v_tf.translation.x, v_tf.translation.z);

        // Allow toggling crew mount/dismount on friendly APCs with [G] in Level 4
        if toggle_crew_key && *v_team == Team::Player && level == CurrentLevel::Level4 {
            if apc.has_driver() || apc.has_gunner() {
                apc.order = VehicleOrder::DismountCrew;
            } else {
                apc.order = VehicleOrder::MountCrew;
            }
        }

        // Sync friendly APC movement order with its dedicated ApcOrder resource (independent from Infantry Squad)
        if *v_team == Team::Player && !matches!(apc.order, VehicleOrder::DismountCrew | VehicleOrder::MountCrew) {
            if let Some(ref cmd_order) = apc_order {
                match cmd_order.0 {
                    CommanderOrder::Hold => {
                        if matches!(apc.order, VehicleOrder::MoveTo(_)) {
                            apc.order = VehicleOrder::Hold;
                        }
                    }
                    CommanderOrder::TakePosition(target) => {
                        apc.order = VehicleOrder::MoveTo(target);
                    }
                }
            }
        }

        if apc.order == VehicleOrder::DismountCrew && apc.current_speed.abs() < 0.8 {
            dismount_vehicle_crew(
                &mut commands,
                v_entity,
                &mut apc,
                v_pos,
                &buildings,
                &low_walls,
                world_bound,
                &mut crew_q,
            );
            apc.order = VehicleOrder::Hold;
        }
    }
}

/// Checks whether a corridor starting at `origin` along `dir` for `probe_dist` meters is blocked
/// for an APC (`APC_CLEARANCE_RADIUS = 2.05m` -> `4.10m` minimum passable gap width).
/// Any gap between two obstacles narrower than `4.10m` is detected as impassable for the APC.
pub fn is_apc_corridor_blocked(
    origin: Vec2,
    dir: Vec2,
    probe_dist: f32,
    buildings: &[(&Transform, &BuildingCollider)],
    low_walls: &[(&Transform, &LowWallCollider)],
) -> bool {
    let d = dir.normalize_or_zero();
    if d.length_squared() < 0.0001 {
        return false;
    }
    let check_r_sq = APC_CLEARANCE_RADIUS * APC_CLEARANCE_RADIUS;
    // Sample densely along the corridor, extending slightly ahead for the APC's 3.8m front overhang
    let max_step = (probe_dist + 1.8).max(2.0);
    let num_samples = ((max_step / 1.35).ceil() as usize).max(2);
    for i in 1..=num_samples {
        let step = (i as f32 / num_samples as f32) * max_step;
        let probe = origin + d * step;
        for (b_tf, collider) in buildings {
            let b_center = Vec2::new(b_tf.translation.x, b_tf.translation.z);
            let closest = Vec2::new(
                probe.x.clamp(
                    b_center.x - collider.half_size.x,
                    b_center.x + collider.half_size.x,
                ),
                probe.y.clamp(
                    b_center.y - collider.half_size.y,
                    b_center.y + collider.half_size.y,
                ),
            );
            if (probe - closest).length_squared() < check_r_sq {
                return true;
            }
        }
        for (w_tf, collider) in low_walls {
            let w_center = Vec2::new(w_tf.translation.x, w_tf.translation.z);
            let closest = Vec2::new(
                probe.x.clamp(
                    w_center.x - collider.half_size.x,
                    w_center.x + collider.half_size.x,
                ),
                probe.y.clamp(
                    w_center.y - collider.half_size.y,
                    w_center.y + collider.half_size.y,
                ),
            );
            if (probe - closest).length_squared() < check_r_sq {
                return true;
            }
        }
    }
    false
}

/// Checks whether moving from `origin` along `dir` for `probe_dist` meters would run into any
/// friendly on-foot troops (`friendly_positions`).
pub fn is_apc_corridor_hitting_friendlies(
    origin: Vec2,
    dir: Vec2,
    probe_dist: f32,
    friendly_positions: &[Vec2],
) -> bool {
    if friendly_positions.is_empty() {
        return false;
    }
    let d = dir.normalize_or_zero();
    if d.length_squared() < 0.0001 {
        return false;
    }
    let right = Vec2::new(-d.y, d.x);
    let lateral_clearance = APC_HULL_WIDTH * 0.5 + SOLDIER_RADIUS + 0.65;
    let max_ahead = probe_dist + APC_HULL_LENGTH * 0.5;
    for &f_pos in friendly_positions {
        let rel = f_pos - origin;
        let along = rel.dot(d);
        let lat = rel.dot(right).abs();
        if along > -1.0 && along <= max_ahead && lat < lateral_clearance {
            return true;
        }
    }
    false
}

/// Computes an obstacle-avoiding steering direction for the APC:
/// - Rejects gaps between obstacles narrower than `2 * APC_CLEARANCE_RADIUS` (`4.10m`).
/// - Steers around friendly on-foot soldiers (`friendly_positions`) when possible.
/// - Evaluates wide detour corridors and scores them by alignment with `desired_dir`, commitment
///   to `current_heading` (preventing left/right oscillation), and 2nd-leg clearance toward `goal`.
pub fn find_apc_avoidance_direction(
    origin: Vec2,
    desired_dir: Vec2,
    current_heading: Vec2,
    dist_to_goal: f32,
    buildings: &[(&Transform, &BuildingCollider)],
    low_walls: &[(&Transform, &LowWallCollider)],
    friendly_positions: &[Vec2],
) -> Vec2 {
    if desired_dir.length_squared() < 0.0001 {
        return current_heading;
    }
    let desired_norm = desired_dir.normalize();
    let goal = origin + desired_norm * dist_to_goal;
    let lookahead = dist_to_goal.clamp(5.0, 14.0);

    // 1. If the direct corridor is wide enough for the APC and clear of friendly troops, drive straight toward the goal
    if !is_apc_corridor_blocked(origin, desired_norm, lookahead, buildings, low_walls)
        && !is_apc_corridor_hitting_friendlies(
            origin,
            desired_norm,
            lookahead.min(11.0),
            friendly_positions,
        )
    {
        return desired_norm;
    }

    const ANGLES: [f32; 16] = [
        0.26, -0.26, // ~15 deg
        0.52, -0.52, // ~30 deg
        0.79, -0.79, // ~45 deg
        1.05, -1.05, // ~60 deg
        1.31, -1.31, // ~75 deg
        1.57, -1.57, // ~90 deg
        1.92, -1.92, // ~110 deg
        2.36, -2.36, // ~135 deg
    ];

    for probe_tier in [12.5_f32.min(lookahead.max(8.5)), 7.5, 4.5] {
        let mut best_cand: Option<(Vec2, f32)> = None;
        for angle in ANGLES {
            let (s, c) = angle.sin_cos();
            let cand = Vec2::new(
                desired_norm.x * c - desired_norm.y * s,
                desired_norm.x * s + desired_norm.y * c,
            );
            if !is_apc_corridor_blocked(origin, cand, probe_tier, buildings, low_walls) {
                let hits_friendly = is_apc_corridor_hitting_friendlies(
                    origin,
                    cand,
                    probe_tier.min(10.0),
                    friendly_positions,
                );
                let wp = origin + cand * probe_tier;
                let to_goal_from_wp = (goal - wp).normalize_or(cand);
                let second_leg_dist = (goal - wp).length().clamp(4.0, 10.0);
                let second_leg_clear = !is_apc_corridor_blocked(
                    wp,
                    to_goal_from_wp,
                    second_leg_dist,
                    buildings,
                    low_walls,
                );
                let friendly_penalty = if hits_friendly { -4.5 } else { 0.0 };
                let score = cand.dot(desired_norm) * 2.0
                    + cand.dot(current_heading) * 0.65
                    + if second_leg_clear { 1.4 } else { 0.0 }
                    + friendly_penalty;
                if best_cand.map_or(true, |(_, best_score)| score > best_score) {
                    best_cand = Some((cand, score));
                }
            }
        }
        if let Some((best_dir, _)) = best_cand {
            return best_dir;
        }
    }

    desired_norm
}

/// Resolves full-hull collisions for the `3.4m × 7.6m` APC against buildings and low walls
/// using a 3-disc hull capsule (front, center, rear) along `heading`.
pub fn resolve_apc_hull_obstacle_collision(
    mut pos: Vec2,
    heading: Vec2,
    buildings: &[(&Transform, &BuildingCollider)],
    low_walls: &[(&Transform, &LowWallCollider)],
    world_bound: f32,
) -> Vec2 {
    let fwd = heading.normalize_or(Vec2::new(0.0, -1.0));
    let hull_r = APC_HULL_WIDTH * 0.52;
    for offset in [0.0, 1.9, -1.9] {
        let disc_pos = pos + fwd * offset;
        let resolved_disc = resolve_circle_obstacle_collision_bounded(
            disc_pos,
            hull_r,
            buildings,
            low_walls,
            world_bound,
        );
        pos += resolved_disc - disc_pos;
    }
    pos
}

/// Realistic wheeled/tracked APC kinematics, obstacle & gap navigation, and 360° turret firing:
/// - Requires a mounted `Driver` to accelerate, reverse, or steer; without a driver the vehicle brakes to a halt.
/// - Navigates around buildings and low walls while detecting and avoiding gaps narrower than `4.10m`.
/// - Requires a mounted `Gunner` to slew the 360° circular turret (`ApcTurret`) and fire at any enemies in view.
pub fn update_crewed_apc_vehicles(
    mut commands: Commands,
    time: Res<Time>,
    current_level: Option<Res<CurrentLevel>>,
    bullet_assets: Option<Res<BulletAssets>>,
    mut gunshot_mutator: Option<MessageWriter<GunshotEvent>>,
    mut apc_q: Query<
        (Entity, &mut Transform, &Team, &mut ApcVehicle),
        (Without<ApcTurret>, Without<MountedInVehicle>),
    >,
    mut turret_q: Query<(&ApcTurret, &mut Transform), (Without<ApcVehicle>, Without<MountedInVehicle>)>,
    mut mounted_crew_q: Query<
        (&MountedInVehicle, &mut Transform, &mut Soldier, &mut Visibility),
        (Without<ApcVehicle>, Without<ApcTurret>),
    >,
    soldiers_q: Query<
        (&Transform, &Team),
        (With<Soldier>, Without<ApcVehicle>, Without<ApcTurret>, Without<MountedInVehicle>),
    >,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (With<Building>, Without<ApcVehicle>, Without<ApcTurret>, Without<MountedInVehicle>),
    >,
    low_walls_q: Query<
        (&Transform, &LowWallCollider),
        (With<LowWall>, Without<ApcVehicle>, Without<ApcTurret>, Without<MountedInVehicle>),
    >,
) {
    let dt = time.delta_secs();
    let level = current_level.as_deref().copied().unwrap_or_default();
    let world_bound = if level == CurrentLevel::Level4 {
        RTS_SOLDIER_WORLD_BOUND - APC_HULL_LENGTH * 0.5
    } else {
        SOLDIER_WORLD_BOUND - APC_HULL_LENGTH * 0.5
    };
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();
    let low_walls: Vec<(&Transform, &LowWallCollider)> = low_walls_q.iter().collect();

    let mut vehicle_states: Vec<(Entity, Vec2, Vec2, Vec2)> = Vec::new();

    for (v_entity, mut v_tf, v_team, mut apc) in &mut apc_q {
        let mut pos = Vec2::new(v_tf.translation.x, v_tf.translation.z);
        let mut heading = apc.heading.normalize_or(Vec2::new(0.0, -1.0));

        // Gather friendly on-foot troop positions so the APC steers around friendly infantry
        let friendly_troops: Vec<Vec2> = soldiers_q
            .iter()
            .filter(|(_, s_team)| **s_team == *v_team)
            .map(|(s_tf, _)| Vec2::new(s_tf.translation.x, s_tf.translation.z))
            .collect();

        // 1. Hull Driving Kinematics & Obstacle/Friendly-Troop Avoidance (requires mounted Driver)
        if apc.has_driver() {
            match apc.order {
                VehicleOrder::MoveTo(goal) => {
                    let to_goal = goal - pos;
                    let dist = to_goal.length();
                    let stop_dist = (apc.current_speed.abs() * apc.current_speed.abs())
                        / (2.0 * apc.braking_deceleration.max(1.0))
                        + 1.2;

                    if dist <= 1.5 {
                        // Arrived: brake to a halt
                        if apc.current_speed.abs() <= apc.braking_deceleration * dt {
                            apc.current_speed = 0.0;
                            apc.order = VehicleOrder::Hold;
                        } else {
                            apc.current_speed -= apc.current_speed.signum() * apc.braking_deceleration * dt;
                        }
                    } else {
                        let raw_dir = to_goal / dist;
                        let raw_forward_dot = heading.dot(raw_dir);

                        // Choose reverse gear only when backing straight up into an unobstructed nearby rear point
                        let use_reverse = raw_forward_dot < -0.25
                            && dist <= 24.0
                            && !is_apc_corridor_blocked(pos, raw_dir, dist, &buildings, &low_walls)
                            && !is_apc_corridor_hitting_friendlies(pos, raw_dir, dist, &friendly_troops);

                        let desired_dir = if use_reverse {
                            raw_dir
                        } else {
                            find_apc_avoidance_direction(
                                pos,
                                raw_dir,
                                heading,
                                dist,
                                &buildings,
                                &low_walls,
                                &friendly_troops,
                            )
                        };

                        let forward_dot = heading.dot(desired_dir);
                        let right = Vec2::new(-heading.y, heading.x);
                        let right_dot = right.dot(desired_dir);

                        // Check if any friendly soldier is immediately ahead of the APC bumper while turning;
                        // if so, cap speed below APC_LETHAL_IMPACT_SPEED while steering around them.
                        let friendly_close_ahead = friendly_troops.iter().any(|&f_pos| {
                            let rel = f_pos - pos;
                            let along = rel.dot(heading);
                            let lat = rel.dot(right).abs();
                            along > 0.0
                                && along < APC_HULL_LENGTH * 0.5 + 3.5
                                && lat < APC_HULL_WIDTH * 0.5 + SOLDIER_RADIUS + 0.45
                        });

                        if use_reverse {
                            let target_speed = if dist <= stop_dist {
                                -1.5_f32.min(dist)
                            } else {
                                -apc.max_reverse_speed
                            };
                            if apc.current_speed > target_speed {
                                let decel = if apc.current_speed > 0.0 {
                                    apc.braking_deceleration
                                } else {
                                    apc.reverse_acceleration
                                };
                                apc.current_speed = (apc.current_speed - decel * dt).max(target_speed);
                            } else {
                                apc.current_speed =
                                    (apc.current_speed + apc.braking_deceleration * dt).min(target_speed);
                            }

                            // Rear-end steering alignment while backing up
                            let rear_dir = -heading;
                            let rear_right = Vec2::new(-rear_dir.y, rear_dir.x);
                            let steer_err = rear_right.dot(desired_dir).clamp(-1.0, 1.0);
                            let speed_factor = (apc.current_speed.abs() / 2.5).clamp(0.25, 1.0);
                            let yaw_delta = steer_err * apc.max_turn_rate_rad * speed_factor * dt;
                            let (s, c) = yaw_delta.sin_cos();
                            heading = Vec2::new(
                                heading.x * c - heading.y * s,
                                heading.x * s + heading.y * c,
                            )
                            .normalize_or(heading);
                        } else {
                            // Forward gear with realistic turning radius and braking curve
                            let turn_error = right_dot.atan2(forward_dot);
                            let sharp_turn_slowdown = if turn_error.abs() > 0.65 { 0.48 } else { 1.0 };
                            let mut target_speed = if dist <= stop_dist {
                                (dist * 1.4).min(apc.max_forward_speed * 0.35)
                            } else {
                                apc.max_forward_speed * sharp_turn_slowdown
                            };
                            if friendly_close_ahead {
                                target_speed = target_speed.min(APC_LETHAL_IMPACT_SPEED * 0.65);
                            }

                            if apc.current_speed < target_speed {
                                let accel = if apc.current_speed < 0.0 {
                                    apc.braking_deceleration
                                } else {
                                    apc.acceleration
                                };
                                apc.current_speed = (apc.current_speed + accel * dt).min(target_speed);
                            } else {
                                apc.current_speed = (apc.current_speed
                                    - apc.braking_deceleration * dt)
                                    .max(target_speed);
                            }

                            // Steer hull heading proportionally to forward speed
                            let speed_factor = (apc.current_speed.abs() / 3.0).clamp(0.35, 1.0);
                            let max_step = apc.max_turn_rate_rad * speed_factor * dt;
                            let yaw_step = turn_error.clamp(-max_step, max_step);
                            let (s, c) = yaw_step.sin_cos();
                            heading = Vec2::new(
                                heading.x * c - heading.y * s,
                                heading.x * s + heading.y * c,
                            )
                            .normalize_or(heading);
                        }
                    }
                }
                VehicleOrder::Hold | VehicleOrder::DismountCrew | VehicleOrder::MountCrew => {
                    if apc.current_speed.abs() <= apc.braking_deceleration * dt {
                        apc.current_speed = 0.0;
                    } else {
                        apc.current_speed -=
                            apc.current_speed.signum() * apc.braking_deceleration * dt;
                    }
                }
            }
        } else {
            // No Driver inside: vehicle brakes to a halt
            if apc.current_speed.abs() <= apc.braking_deceleration * dt {
                apc.current_speed = 0.0;
            } else {
                apc.current_speed -= apc.current_speed.signum() * apc.braking_deceleration * dt;
            }
        }

        if apc.current_speed.abs() > 0.0001 {
            let next_pos = pos + heading * apc.current_speed * dt;
            let resolved = resolve_apc_hull_obstacle_collision(
                next_pos,
                heading,
                &buildings,
                &low_walls,
                world_bound,
            );
            if (resolved - next_pos).length_squared() > 0.04 {
                apc.current_speed *= 0.35;
            }
            pos = resolved;
        }

        apc.heading = heading;
        let hull_yaw = f32::atan2(-heading.x, -heading.y);
        v_tf.translation.x = pos.x;
        v_tf.translation.z = pos.y;
        v_tf.rotation = Quat::from_rotation_y(hull_yaw);

        // 2. 360-degree Circular Turret Target Acquisition, Rotation & Firing (requires mounted Gunner)
        if apc.has_gunner() {
            apc.turret_shoot_timer.tick(time.delta());
            let current_turret_dir = apc.turret_world_dir();

            // Scan for any enemies in view of the turret's vision cone or driver's forward vision cone
            let mut nearest_visible_enemy: Option<(Vec2, f32)> = None;
            for (s_tf, s_team) in &soldiers_q {
                if *s_team == *v_team {
                    continue;
                }
                let e_pos = Vec2::new(s_tf.translation.x, s_tf.translation.z);
                let seen_by_turret = can_see_target(
                    pos,
                    current_turret_dir,
                    e_pos,
                    apc.turret_vision_range,
                    apc.turret_vision_half_angle,
                    3.0,
                    &buildings,
                );
                let seen_by_driver = can_see_target(
                    pos,
                    heading,
                    e_pos,
                    apc.turret_vision_range,
                    apc.turret_vision_half_angle,
                    3.0,
                    &buildings,
                );
                if seen_by_turret || seen_by_driver {
                    let d2 = (e_pos - pos).length_squared();
                    if nearest_visible_enemy.map_or(true, |(_, best_d2)| d2 < best_d2) {
                        nearest_visible_enemy = Some((e_pos, d2));
                    }
                }
            }

            if let Some((enemy_pos, _)) = nearest_visible_enemy {
                let to_enemy = enemy_pos - pos;
                if to_enemy.length_squared() > 0.001 {
                    let desired_world_yaw = f32::atan2(-to_enemy.x, -to_enemy.y);
                    let mut desired_local_yaw = desired_world_yaw - hull_yaw;
                    while desired_local_yaw > std::f32::consts::PI {
                        desired_local_yaw -= std::f32::consts::TAU;
                    }
                    while desired_local_yaw < -std::f32::consts::PI {
                        desired_local_yaw += std::f32::consts::TAU;
                    }
                    apc.turret_target_yaw_local = desired_local_yaw;
                }
            }

            // Shortest-angle wrap in [-PI, PI] so the turret spins 360° smoothly in either direction
            let mut angle_diff = apc.turret_target_yaw_local - apc.turret_yaw_local;
            while angle_diff > std::f32::consts::PI {
                angle_diff -= std::f32::consts::TAU;
            }
            while angle_diff < -std::f32::consts::PI {
                angle_diff += std::f32::consts::TAU;
            }
            let max_turret_step = apc.turret_turn_rate_rad * dt;
            if angle_diff.abs() <= max_turret_step {
                apc.turret_yaw_local = apc.turret_target_yaw_local;
            } else {
                apc.turret_yaw_local += angle_diff.signum() * max_turret_step;
            }
            // Normalize into [-PI, PI]
            while apc.turret_yaw_local > std::f32::consts::PI {
                apc.turret_yaw_local -= std::f32::consts::TAU;
            }
            while apc.turret_yaw_local < -std::f32::consts::PI {
                apc.turret_yaw_local += std::f32::consts::TAU;
            }

            // Fire autocannon when an enemy is in view and the turret barrel is aligned
            let updated_turret_dir = apc.turret_world_dir();
            if let Some((enemy_pos, _)) = nearest_visible_enemy {
                let aim_dir = (enemy_pos - pos).normalize_or(updated_turret_dir);
                if updated_turret_dir.dot(aim_dir) >= 0.95 && apc.turret_shoot_timer.is_finished()
                {
                    apc.turret_shoot_timer.reset();
                    let muzzle_pos =
                        pos + updated_turret_dir * (APC_TURRET_RADIUS + APC_BARREL_LENGTH);
                    let shot_dir = (enemy_pos - muzzle_pos).normalize_or(updated_turret_dir);

                    if let Some(ref assets) = bullet_assets {
                        let mat = match *v_team {
                            Team::Player => assets.player_material.clone(),
                            Team::Enemy => assets.enemy_material.clone(),
                        };
                        commands.spawn((
                            LevelEntity,
                            Mesh3d(assets.mesh.clone()),
                            MeshMaterial3d(mat),
                            Transform::from_xyz(muzzle_pos.x, 0.08, muzzle_pos.y)
                                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                            Bullet {
                                origin: muzzle_pos,
                                direction: shot_dir,
                                speed: 58.0,
                                lifetime: 1.3,
                                team: *v_team,
                                shooter_stance: Stance::Standing,
                                missed_entities: Vec::new(),
                            },
                        ));
                    }

                    if let Some(ref mut writer) = gunshot_mutator {
                        writer.write(GunshotEvent {
                            position: muzzle_pos,
                            shooter_team: *v_team,
                        });
                    }
                }
            }
        }

        let turret_dir = apc.turret_world_dir();
        vehicle_states.push((v_entity, pos, heading, turret_dir));

        for (turret, mut t_tf) in &mut turret_q {
            if turret.vehicle == v_entity {
                t_tf.rotation = Quat::from_rotation_y(apc.turret_yaw_local);
            }
        }
    }

    // Sync mounted crew transforms and facing directions so Fog of War / spotter queries see
    // the Driver looking ahead and the Gunner looking along the 360° turret orientation
    for (mounted, mut c_tf, mut c_soldier, mut c_vis) in &mut mounted_crew_q {
        if let Some(&(_, v_pos, v_heading, v_turret_dir)) = vehicle_states
            .iter()
            .find(|(ent, _, _, _)| *ent == mounted.vehicle)
        {
            c_tf.translation.x = v_pos.x;
            c_tf.translation.y = 0.05;
            c_tf.translation.z = v_pos.y;
            c_soldier.facing_dir = match mounted.role {
                VehicleCrewRole::Driver => v_heading,
                VehicleCrewRole::Gunner => v_turret_dir,
                VehicleCrewRole::Passenger => v_heading,
            };
            *c_vis = Visibility::Hidden;
        }
    }
}

/// Moves dismounted crew members toward their APC hatches and re-mounts them when `VehicleOrder::MountCrew` is active.
pub fn update_dismounted_crew_return(
    mut commands: Commands,
    time: Res<Time>,
    current_level: Option<Res<CurrentLevel>>,
    mut apc_q: Query<(Entity, &Transform, &mut ApcVehicle)>,
    mut dismounted_crew_q: Query<
        (
            Entity,
            &VehicleCrewMember,
            &mut Transform,
            &mut Soldier,
            &mut Visibility,
            Option<&Friendly>,
        ),
        (Without<MountedInVehicle>, Without<ApcVehicle>),
    >,
    buildings_q: Query<(&Transform, &BuildingCollider), (With<Building>, Without<ApcVehicle>, Without<VehicleCrewMember>)>,
    low_walls_q: Query<(&Transform, &LowWallCollider), (With<LowWall>, Without<ApcVehicle>, Without<VehicleCrewMember>)>,
) {
    let dt = time.delta_secs();
    let level = current_level.as_deref().copied().unwrap_or_default();
    let world_bound = if level == CurrentLevel::Level4 {
        RTS_SOLDIER_WORLD_BOUND
    } else {
        SOLDIER_WORLD_BOUND
    };
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();
    let low_walls: Vec<(&Transform, &LowWallCollider)> = low_walls_q.iter().collect();

    for (v_entity, v_tf, mut apc) in &mut apc_q {
        if apc.order != VehicleOrder::MountCrew {
            continue;
        }
        let v_pos = Vec2::new(v_tf.translation.x, v_tf.translation.z);

        for (c_ent, crew_member, mut c_tf, mut c_soldier, mut c_vis, friendly_opt) in
            &mut dismounted_crew_q
        {
            if crew_member.vehicle != v_entity {
                continue;
            }
            let c_pos = Vec2::new(c_tf.translation.x, c_tf.translation.z);
            let hatch_pos = apc.hatch_world_pos(v_pos, crew_member.role);
            let to_hatch = hatch_pos - c_pos;
            let dist = to_hatch.length();

            if dist <= 1.25 {
                // Re-enter the APC through the crew hatch / rear ramp
                *c_vis = Visibility::Hidden;
                c_tf.translation.x = v_pos.x;
                c_tf.translation.z = v_pos.y;
                commands.entity(c_ent).insert(MountedInVehicle {
                    vehicle: v_entity,
                    role: crew_member.role,
                });
                match crew_member.role {
                    VehicleCrewRole::Driver => apc.driver = Some(c_ent),
                    VehicleCrewRole::Gunner => apc.gunner = Some(c_ent),
                    VehicleCrewRole::Passenger => {
                        if !apc.passengers.contains(&c_ent) && apc.passengers.len() < apc.max_passengers {
                            apc.passengers.push(c_ent);
                        }
                    }
                }
            } else {
                let speed = friendly_opt.map_or(8.0, |f| f.speed);
                let move_dir = to_hatch / dist;
                c_soldier.facing_dir = move_dir;
                c_soldier.stance = Stance::Standing;
                let next_pos = c_pos + move_dir * speed * dt;
                let resolved = resolve_circle_obstacle_collision_bounded(
                    next_pos,
                    SOLDIER_RADIUS,
                    &buildings,
                    &low_walls,
                    world_bound,
                );
                c_tf.translation.x = resolved.x;
                c_tf.translation.z = resolved.y;
                let yaw = f32::atan2(-move_dir.x, -move_dir.y);
                c_tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2);
            }
        }

        if apc.is_fully_crewed() {
            apc.order = VehicleOrder::Hold;
        }
    }
}

/// Resolves collisions between on-foot soldiers and the APC's rectangular hull:
/// - If the APC collides with any soldier while moving at sufficient speed (`|current_speed| >= APC_LETHAL_IMPACT_SPEED`),
///   the soldier is killed on impact (converted into a `Corpse` with blood decals) and the APC drives right through them!
/// - If the APC is moving slowly (`< APC_LETHAL_IMPACT_SPEED`), soldiers in its path are stepped/pushed laterally (`±right`)
///   aside so the APC never bulldozes a soldier forward across the map.
pub fn resolve_soldier_apc_collisions(
    mut commands: Commands,
    corpse_assets: Option<Res<CorpseAssets>>,
    apc_q: Query<(&Transform, &ApcVehicle), Without<Soldier>>,
    mut soldiers_q: Query<
        (Entity, &mut Transform, &Team),
        (With<Soldier>, Without<MountedInVehicle>, Without<ApcVehicle>),
    >,
) {
    let r = SOLDIER_RADIUS;
    for (v_tf, apc) in &apc_q {
        let v_pos = Vec2::new(v_tf.translation.x, v_tf.translation.z);
        let forward = apc.heading.normalize_or(Vec2::new(0.0, -1.0));
        let right = Vec2::new(-forward.y, forward.x);
        let half_x = apc.hull_half_size.x;
        let half_z = apc.hull_half_size.y;
        let speed_abs = apc.current_speed.abs();

        for (s_ent, mut s_tf, s_team) in &mut soldiers_q {
            let s_pos = Vec2::new(s_tf.translation.x, s_tf.translation.z);
            let rel = s_pos - v_pos;
            // Project relative position into vehicle local axes `(local_x, local_z)`
            let local_x = rel.dot(right);
            let local_z = rel.dot(forward);

            let clamped_x = local_x.clamp(-half_x, half_x);
            let clamped_z = local_z.clamp(-half_z, half_z);
            let diff_local = Vec2::new(local_x - clamped_x, local_z - clamped_z);
            let dist_sq = diff_local.length_squared();

            if dist_sq < r * r {
                if speed_abs >= APC_LETHAL_IMPACT_SPEED {
                    // Lethal high-speed vehicle impact: kill the soldier in place and drive right through them!
                    let mut entity_cmds = commands.entity(s_ent);
                    entity_cmds.despawn_children();
                    entity_cmds
                        .remove::<Soldier>()
                        .remove::<Team>()
                        .remove::<Player>()
                        .remove::<Enemy>()
                        .remove::<Friendly>()
                        .remove::<Zombie>()
                        .remove::<VehicleCrewMember>()
                        .insert((
                            Corpse,
                            Visibility::Inherited,
                            Transform::from_xyz(s_pos.x, 0.03, s_pos.y)
                                .with_rotation(s_tf.rotation)
                                .with_scale(Vec3::new(0.65, 0.95, 0.65)),
                        ));

                    if let Some(ref assets) = corpse_assets {
                        entity_cmds.insert(MeshMaterial3d(if *s_team == Team::Player {
                            assets.player_corpse_material.clone()
                        } else {
                            assets.enemy_corpse_material.clone()
                        }));
                        commands.spawn((
                            LevelEntity,
                            Mesh3d(assets.blood_pool_mesh.clone()),
                            MeshMaterial3d(assets.blood_material.clone()),
                            Transform::from_xyz(s_pos.x, 0.015, s_pos.y)
                                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                            BloodDecal,
                        ));
                        for offset in [Vec2::new(0.50, 0.40), Vec2::new(-0.45, -0.35)] {
                            commands.spawn((
                                LevelEntity,
                                Mesh3d(assets.blood_splatter_mesh.clone()),
                                MeshMaterial3d(assets.blood_material.clone()),
                                Transform::from_xyz(s_pos.x + offset.x, 0.015, s_pos.y + offset.y)
                                    .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                                BloodDecal,
                            ));
                        }
                    }
                } else if speed_abs > 0.15 {
                    // Low-speed contact: push soldier laterally (`±right`) off the side of the hull
                    // so the APC never pushes a soldier forward a long distance along its movement axis.
                    let side_sign = if local_x >= 0.0 { 1.0 } else { -1.0 };
                    let new_local_x = side_sign * (half_x + r + 0.08);
                    let resolved_world = v_pos + right * new_local_x + forward * local_z;
                    s_tf.translation.x = resolved_world.x;
                    s_tf.translation.z = resolved_world.y;
                } else {
                    // Stationary parked APC: standard closest-edge OBB collision
                    let new_local = if dist_sq > 0.00001 {
                        let dist = dist_sq.sqrt();
                        let push = (diff_local / dist) * (r - dist);
                        Vec2::new(local_x + push.x, local_z + push.y)
                    } else {
                        let dx = half_x - local_x.abs();
                        let dz = half_z - local_z.abs();
                        if dx < dz {
                            Vec2::new((half_x + r) * local_x.signum(), local_z)
                        } else {
                            Vec2::new(local_x, (half_z + r) * local_z.signum())
                        }
                    };
                    let resolved_world = v_pos + right * new_local.x + forward * new_local.y;
                    s_tf.translation.x = resolved_world.x;
                    s_tf.translation.z = resolved_world.y;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_vehicle_test_app() -> App {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Time>();
        app.add_message::<GunshotEvent>();
        app.insert_resource(CurrentLevel::Level4);
        let bullet_mesh = app
            .world_mut()
            .resource_mut::<Assets<Mesh>>()
            .add(Circle::new(crate::common::types::BULLET_RADIUS));
        let (player_material, enemy_material) = {
            let mut mats = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
            (
                mats.add(Color::srgb(1.0, 0.9, 0.2)),
                mats.add(Color::srgb(1.0, 0.2, 0.2)),
            )
        };
        app.insert_resource(BulletAssets {
            mesh: bullet_mesh,
            player_material,
            enemy_material,
        });
        app.add_systems(
            Update,
            (
                handle_vehicle_crew_mount_commands,
                update_crewed_apc_vehicles,
                update_dismounted_crew_return,
                resolve_soldier_apc_collisions,
            )
                .chain(),
        );
        app
    }

    #[test]
    fn test_level_4_spawns_friendly_apc_with_mounted_driver_and_gunner() {
        let mut app = create_vehicle_test_app();
        app.add_systems(Startup, setup_level_vehicles);
        app.update();

        let mut apc_count = 0;
        for (tf, team, apc) in app
            .world_mut()
            .query::<(&Transform, &Team, &ApcVehicle)>()
            .iter(app.world())
        {
            apc_count += 1;
            assert_eq!(*team, Team::Player);
            assert!(
                apc.is_fully_crewed(),
                "APC must spawn with both Driver and Gunner mounted inside"
            );
            assert_eq!(apc.hull_half_size, Vec2::new(APC_HULL_WIDTH * 0.5, APC_HULL_LENGTH * 0.5));
            assert!((tf.translation.z - LEVEL_4_FRIENDLY_APC_SPAWN_POS.y).abs() < 0.1);
        }
        assert_eq!(apc_count, 1, "Level 4 should spawn 1 friendly APC");

        let turret_count = app
            .world_mut()
            .query::<&ApcTurret>()
            .iter(app.world())
            .count();
        assert_eq!(turret_count, 1, "APC must have a circular 360-degree rotating turret");
    }

    #[test]
    fn test_apc_drives_forward_and_backward_and_spins_turret_360() {
        let mut app = create_vehicle_test_app();
        app.add_systems(Startup, setup_level_vehicles);
        app.update();

        let apc_entity = app
            .world_mut()
            .query_filtered::<Entity, With<ApcVehicle>>()
            .single(app.world())
            .unwrap();

        // 1. Order APC to drive forward North from Z=202 to Z=170, while rotating turret 200° (3.49 rad)
        {
            let mut apc = app.world_mut().get_mut::<ApcVehicle>(apc_entity).unwrap();
            apc.order = VehicleOrder::MoveTo(Vec2::new(0.0, 170.0));
            apc.turret_target_yaw_local = 2.8;
        }

        for _ in 0..50 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        let mid_z = app.world().get::<Transform>(apc_entity).unwrap().translation.z;
        let apc_after_fwd = app.world().get::<ApcVehicle>(apc_entity).unwrap();
        assert!(
            mid_z < 185.0,
            "APC should accelerate and drive forward North toward Z=170, got z={}",
            mid_z
        );
        assert!(
            (apc_after_fwd.turret_yaw_local - 2.8).abs() < 0.05,
            "Turret should rotate smoothly to target yaw 2.8 rad, got {}",
            apc_after_fwd.turret_yaw_local
        );

        // 2. Order APC to reverse to a nearby point behind it (Z = mid_z + 12.0)
        //    and spin turret across the 180°/-180° boundary to -2.5 rad (full 360° shortest path)
        let reverse_target_z = mid_z + 12.0;
        {
            let mut apc = app.world_mut().get_mut::<ApcVehicle>(apc_entity).unwrap();
            apc.current_speed = 0.0;
            apc.order = VehicleOrder::MoveTo(Vec2::new(0.0, reverse_target_z));
            apc.turret_target_yaw_local = -2.5;
        }

        for _ in 0..45 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        let rev_z = app.world().get::<Transform>(apc_entity).unwrap().translation.z;
        let apc_after_rev = app.world().get::<ApcVehicle>(apc_entity).unwrap();
        assert!(
            rev_z > mid_z + 4.0,
            "APC should reverse southward toward Z={}, got z={}",
            reverse_target_z,
            rev_z
        );
        assert!(
            (apc_after_rev.turret_yaw_local - (-2.5)).abs() < 0.05,
            "Turret should rotate across the 360° wrap boundary to -2.5 rad, got {}",
            apc_after_rev.turret_yaw_local
        );
    }

    #[test]
    fn test_apc_crew_driver_and_gunner_can_exit_and_reenter_vehicle() {
        let mut app = create_vehicle_test_app();
        app.add_systems(Startup, setup_level_vehicles);
        app.update();

        let apc_entity = app
            .world_mut()
            .query_filtered::<Entity, With<ApcVehicle>>()
            .single(app.world())
            .unwrap();

        // 1. Issue DismountCrew order -> Driver and Gunner exit to side hatches and become visible
        {
            let mut apc = app.world_mut().get_mut::<ApcVehicle>(apc_entity).unwrap();
            apc.order = VehicleOrder::DismountCrew;
        }
        {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
        }
        app.update();

        {
            let apc = app.world().get::<ApcVehicle>(apc_entity).unwrap();
            assert!(!apc.has_driver() && !apc.has_gunner(), "Driver and Gunner should have exited the APC");
        }
        let dismounted_count = app
            .world_mut()
            .query_filtered::<Entity, (With<VehicleCrewMember>, Without<MountedInVehicle>)>()
            .iter(app.world())
            .count();
        assert_eq!(dismounted_count, 2, "Both Driver and Gunner should be dismounted on foot");

        // Verify uncrewed APC cannot drive when ordered to MoveTo while Driver is dismounted
        let z_before = app.world().get::<Transform>(apc_entity).unwrap().translation.z;
        {
            let mut apc = app.world_mut().get_mut::<ApcVehicle>(apc_entity).unwrap();
            apc.order = VehicleOrder::MoveTo(Vec2::new(0.0, 150.0));
        }
        for _ in 0..10 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }
        let z_after = app.world().get::<Transform>(apc_entity).unwrap().translation.z;
        assert!(
            (z_after - z_before).abs() < 0.01,
            "APC without a mounted Driver must not move"
        );

        // 2. Issue MountCrew order -> Driver and Gunner walk back to their hatches and re-enter the APC
        {
            let mut apc = app.world_mut().get_mut::<ApcVehicle>(apc_entity).unwrap();
            apc.order = VehicleOrder::MountCrew;
        }
        for _ in 0..20 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        let apc_remounted = app.world().get::<ApcVehicle>(apc_entity).unwrap();
        assert!(
            apc_remounted.is_fully_crewed(),
            "Driver and Gunner should have re-entered the APC"
        );
    }

    #[test]
    fn test_apc_navigates_around_obstacle_to_reach_target() {
        let mut app = create_vehicle_test_app();
        app.add_systems(Startup, setup_level_vehicles);
        app.update();

        let apc_entity = app
            .world_mut()
            .query_filtered::<Entity, With<ApcVehicle>>()
            .single(app.world())
            .unwrap();

        // Place APC at (0.0, 60.0) facing North and place a building at (0.0, 38.0) blocking the direct path to (0.0, 14.0)
        app.world_mut().get_mut::<Transform>(apc_entity).unwrap().translation =
            Vec3::new(0.0, APC_HULL_HEIGHT * 0.5, 60.0);
        app.world_mut().spawn((
            Building,
            BuildingCollider {
                half_size: Vec2::new(4.5, 4.0),
            },
            Transform::from_xyz(0.0, 1.5, 38.0),
        ));

        {
            let mut apc = app.world_mut().get_mut::<ApcVehicle>(apc_entity).unwrap();
            apc.order = VehicleOrder::MoveTo(Vec2::new(0.0, 14.0));
        }

        for _ in 0..140 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        let tf = app.world().get::<Transform>(apc_entity).unwrap();
        let final_pos = Vec2::new(tf.translation.x, tf.translation.z);
        assert!(
            (final_pos - Vec2::new(0.0, 14.0)).length() < 3.5,
            "APC should steer around the obstacle at (0, 38) and reach the goal at (0, 14), ended at {:?}",
            final_pos
        );
    }

    #[test]
    fn test_apc_detects_narrow_gap_is_too_small_and_detours_around_it() {
        let mut app = create_vehicle_test_app();
        app.add_systems(Startup, setup_level_vehicles);
        app.update();

        // Spawn two low walls at Z = 40.0 leaving a 2.6m gap in the middle (X in [-1.3, 1.3]):
        // - Left wall: center X = -4.3, half_size.x = 3.0 -> spans [-7.3, -1.3]
        // - Right wall: center X = +4.3, half_size.x = 3.0 -> spans [+1.3, +7.3]
        // A 1.2m infantry soldier can walk straight through [-1.3, 1.3], but the 3.4m-wide APC cannot!
        let left_tf = Transform::from_xyz(-4.3, 0.375, 40.0);
        let right_tf = Transform::from_xyz(4.3, 0.375, 40.0);
        let wall_col = LowWallCollider {
            half_size: Vec2::new(3.0, 0.5),
        };
        let walls = vec![(&left_tf, &wall_col), (&right_tf, &wall_col)];

        assert!(
            is_apc_corridor_blocked(
                Vec2::new(0.0, 52.0),
                Vec2::new(0.0, -1.0),
                14.0,
                &[],
                &walls
            ),
            "2.6m gap between obstacles must be detected as too small for the 3.4m-wide APC"
        );

        // Verify a 5.2m wide gap IS recognized as passable for the APC
        let wide_left_tf = Transform::from_xyz(-5.6, 0.375, 40.0);
        let wide_right_tf = Transform::from_xyz(5.6, 0.375, 40.0);
        let wide_walls = vec![(&wide_left_tf, &wall_col), (&wide_right_tf, &wall_col)];
        assert!(
            !is_apc_corridor_blocked(
                Vec2::new(0.0, 52.0),
                Vec2::new(0.0, -1.0),
                14.0,
                &[],
                &wide_walls
            ),
            "5.2m wide gap between obstacles should be recognized as passable for the APC"
        );

        // Now spawn the narrow-gap walls in the world and verify the APC detours around the outside to reach (0.0, 20.0)
        app.world_mut().spawn((LowWall, wall_col, left_tf));
        app.world_mut().spawn((LowWall, wall_col, right_tf));

        let apc_entity = app
            .world_mut()
            .query_filtered::<Entity, With<ApcVehicle>>()
            .single(app.world())
            .unwrap();
        app.world_mut().get_mut::<Transform>(apc_entity).unwrap().translation =
            Vec3::new(0.0, APC_HULL_HEIGHT * 0.5, 58.0);
        {
            let mut apc = app.world_mut().get_mut::<ApcVehicle>(apc_entity).unwrap();
            apc.order = VehicleOrder::MoveTo(Vec2::new(0.0, 20.0));
        }

        let mut max_lateral_detour_x = 0.0_f32;
        for _ in 0..150 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
            let x = app.world().get::<Transform>(apc_entity).unwrap().translation.x.abs();
            if x > max_lateral_detour_x {
                max_lateral_detour_x = x;
            }
        }

        let tf = app.world().get::<Transform>(apc_entity).unwrap();
        let final_pos = Vec2::new(tf.translation.x, tf.translation.z);
        assert!(
            max_lateral_detour_x > 7.5,
            "APC must detour around the outside of the narrow gap (|x| > 7.5), max |x| was {:.2}",
            max_lateral_detour_x
        );
        assert!(
            (final_pos - Vec2::new(0.0, 20.0)).length() < 4.0,
            "APC should complete its detour around the narrow gap and reach (0, 20), ended at {:?}",
            final_pos
        );
    }

    #[test]
    fn test_apc_turret_slews_and_fires_at_enemy_in_view() {
        let mut app = create_vehicle_test_app();
        app.add_systems(Startup, setup_level_vehicles);
        app.update();

        let apc_entity = app
            .world_mut()
            .query_filtered::<Entity, With<ApcVehicle>>()
            .single(app.world())
            .unwrap();
        app.world_mut().get_mut::<Transform>(apc_entity).unwrap().translation =
            Vec3::new(0.0, APC_HULL_HEIGHT * 0.5, 50.0);

        // Spawn an enemy soldier at (6.0, 34.0) inside the APC's forward/turret view
        app.world_mut().spawn((
            Transform::from_xyz(6.0, 0.05, 34.0),
            Team::Enemy,
            Soldier {
                facing_dir: Vec2::new(0.0, 1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        for _ in 0..20 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        let bullets_fired = app
            .world_mut()
            .query::<&Bullet>()
            .iter(app.world())
            .count();
        assert!(
            bullets_fired > 0,
            "APC turret with mounted Gunner should slew onto and fire at enemy in view"
        );
    }

    #[test]
    fn test_apc_steers_around_friendly_troops_without_running_them_over_or_pushing_them_far() {
        let mut app = create_vehicle_test_app();
        app.add_systems(Startup, setup_level_vehicles);
        app.update();

        let apc_entity = app
            .world_mut()
            .query_filtered::<Entity, With<ApcVehicle>>()
            .single(app.world())
            .unwrap();
        app.world_mut().get_mut::<Transform>(apc_entity).unwrap().translation =
            Vec3::new(0.0, APC_HULL_HEIGHT * 0.5, 60.0);

        // Spawn a friendly infantry soldier standing at (0.0, 44.0) directly on the road between the APC and (0.0, 20.0)
        let friendly_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 44.0),
                Team::Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        {
            let mut apc = app.world_mut().get_mut::<ApcVehicle>(apc_entity).unwrap();
            apc.order = VehicleOrder::MoveTo(Vec2::new(0.0, 20.0));
        }

        for _ in 0..100 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        assert!(
            app.world().get::<Soldier>(friendly_entity).is_some(),
            "Friendly soldier must not be killed when the APC maneuvers around them"
        );
        let f_tf = app.world().get::<Transform>(friendly_entity).unwrap();
        assert!(
            (f_tf.translation.z - 44.0).abs() < 1.0,
            "Friendly soldier must not be pushed forward along Z by the APC, z was {}",
            f_tf.translation.z
        );
        let apc_tf = app.world().get::<Transform>(apc_entity).unwrap();
        assert!(
            apc_tf.translation.z < 26.0,
            "APC should steer past the friendly soldier toward Z=20, ended at z={}",
            apc_tf.translation.z
        );
    }

    #[test]
    fn test_apc_moving_at_speed_kills_soldier_on_collision_and_drives_through() {
        let mut app = create_vehicle_test_app();
        app.add_systems(Startup, setup_level_vehicles);
        app.update();

        let apc_entity = app
            .world_mut()
            .query_filtered::<Entity, With<ApcVehicle>>()
            .single(app.world())
            .unwrap();
        app.world_mut().get_mut::<Transform>(apc_entity).unwrap().translation =
            Vec3::new(0.0, APC_HULL_HEIGHT * 0.5, 50.0);

        // Spawn an enemy soldier directly in the APC's path at (0.0, 38.0)
        let enemy_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 38.0),
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        {
            let mut apc = app.world_mut().get_mut::<ApcVehicle>(apc_entity).unwrap();
            apc.current_speed = 10.0;
            apc.order = VehicleOrder::MoveTo(Vec2::new(0.0, 20.0));
        }

        for _ in 0..45 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        assert!(
            app.world().get::<Corpse>(enemy_entity).is_some()
                && app.world().get::<Soldier>(enemy_entity).is_none(),
            "Soldier hit by APC at high speed must be killed and converted into a Corpse"
        );
        let corpse_tf = app.world().get::<Transform>(enemy_entity).unwrap();
        assert!(
            (corpse_tf.translation.z - 38.0).abs() < 0.5,
            "Killed soldier must not be pushed down the road, corpse z was {}",
            corpse_tf.translation.z
        );
        let apc_tf = app.world().get::<Transform>(apc_entity).unwrap();
        assert!(
            apc_tf.translation.z < 30.0,
            "APC should drive directly through the killed soldier without stopping, z={}",
            apc_tf.translation.z
        );
    }
}
