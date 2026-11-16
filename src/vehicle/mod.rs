use crate::combat::bullet::{Bullet, BulletAssets, CorpseAssets};
use crate::common::geometry::{
    can_see_target, can_see_target_aerial, resolve_circle_obstacle_collision_bounded,
};
use crate::common::types::{
    ApcOrder, AppState, BloodDecal, CommanderOrder, Corpse, CurrentLevel, GunshotEvent,
    HelicopterOrder, LevelEntity, RTS_SOLDIER_WORLD_BOUND, SOLDIER_RADIUS, SOLDIER_WORLD_BOUND,
    Soldier, Stance, Suppression, Team,
};
use crate::enemy::ai::{Enemy, Zombie};
use crate::fog::mesh::create_vision_cone_mesh;
use crate::friendly::{Friendly, FriendlyAiState};
use crate::player::Player;
use crate::world::buildings::{Building, BuildingCollider, LowWall, LowWallCollider};
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

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

/// Realistic Attack Helicopter scale constants (AH-64 Apache / AH-1Z Viper gunship proportions):
/// - Main fuselage is a large elongated oval (`Sphere::new(1.0)` scaled into an ellipsoid)
/// - Tail boom is a long rectangular section extending rearward (`+Z`) from the fuselage
/// - Top main rotor and rear tail rotor spin rapidly
pub const HELICOPTER_BODY_WIDTH: f32 = 2.9;
pub const HELICOPTER_BODY_HEIGHT: f32 = 2.3;
pub const HELICOPTER_BODY_LENGTH: f32 = 8.8;
pub const HELICOPTER_TAIL_WIDTH: f32 = 0.62;
pub const HELICOPTER_TAIL_HEIGHT: f32 = 0.68;
pub const HELICOPTER_TAIL_LENGTH: f32 = 6.2;
pub const HELICOPTER_ROTOR_DIAMETER: f32 = 11.6;
pub const HELICOPTER_ROTOR_BLADE_CHORD: f32 = 0.38;
pub const HELICOPTER_HOVER_ALTITUDE: f32 = 11.5;
pub const HELICOPTER_MAIN_ROTOR_SPIN_RATE_RAD: f32 = 28.0;
pub const HELICOPTER_TAIL_ROTOR_SPIN_RATE_RAD: f32 = 42.0;

/// Attack Helicopter directional speed hierarchy (`forward > strafe > backward`):
/// - Moving forward is fastest (`22.0 m/s`),
/// - Strafing side-to-side is intermediate (`13.5 m/s`),
/// - Moving backward is slowest (`8.0 m/s`).
pub const HELICOPTER_MAX_FORWARD_SPEED: f32 = 22.0;
pub const HELICOPTER_MAX_STRAFE_SPEED: f32 = 13.5;
pub const HELICOPTER_MAX_BACKWARD_SPEED: f32 = 8.0;
pub const HELICOPTER_ACCEL: f32 = 16.0;
pub const HELICOPTER_DECEL: f32 = 18.0;
pub const HELICOPTER_YAW_RATE_RAD: f32 = 2.4;
/// Distance threshold (`m`) above which the helicopter turns its nose to point in the direction
/// of travel and flies forward nose-first rather than strafing/backing up in place.
pub const HELICOPTER_LONG_TRANSIT_DIST: f32 = 22.0;

/// Limited-arc chin autocannon constants (`±70°` = `140°` total forward traverse arc):
pub const HELICOPTER_CANNON_MAX_YAW_RAD: f32 = 1.2217305; // 70 degrees in radians
pub const HELICOPTER_CANNON_SLEW_RATE_RAD: f32 = 4.5;
pub const HELICOPTER_CANNON_VISION_RANGE: f32 = 38.0;
pub const HELICOPTER_CANNON_FIRE_INTERVAL_SECS: f32 = 0.16;
/// Forward offset (`m`) from the helicopter center (`next_pos`) to the nose-mounted chin autocannon pivot.
pub const HELICOPTER_CANNON_MOUNT_FORWARD: f32 = HELICOPTER_BODY_LENGTH * 0.49;
/// Distance (`m`) from the chin autocannon turret pivot to the barrel muzzle tip along `cannon_world_dir()`.
pub const HELICOPTER_CANNON_BARREL_TIP_OFFSET: f32 = 1.55;

/// Default spawn position for the friendly Attack Helicopter in `test_level_4` (west of the southern approach road).
pub const LEVEL_4_FRIENDLY_HELICOPTER_SPAWN_POS: Vec2 = Vec2::new(-16.0, 192.0);
pub const LEVEL_4_FRIENDLY_HELICOPTER_FACING: Vec2 = Vec2::new(0.0, -1.0);

/// M119A3 105mm Light Towed Field Artillery constants:
/// - Stationary emplacement that rotates on its central firing platform axis very slowly (`0.45 rad/s` ≈ `25.8°/s`).
/// - Firing range covers the entire map (`2000.0 m`).
/// - Slow-firing cadence (`5.5 s` between shells), with shells exploding in a medium circle (`7.5 m` radius = `15.0 m` diameter) on impact.
/// - Long 105mm howitzer barrel (`5.6 m` barrel length).
pub const ARTILLERY_ROTATION_SPEED_RAD: f32 = 0.45;
pub const ARTILLERY_FIRING_RANGE: f32 = 2000.0;
pub const ARTILLERY_RELOAD_SECS: f32 = 5.5;
pub const ARTILLERY_SHELL_SPEED: f32 = 65.0;
pub const ARTILLERY_BLAST_RADIUS: f32 = 7.5;
pub const ARTILLERY_BARREL_LENGTH: f32 = 5.6;
pub const ARTILLERY_BARREL_MUZZLE_OFFSET: f32 = 5.2;

/// Default emplacement position for the friendly M119A3 Field Artillery in `test_level_4` (east of the southern approach road).
pub const LEVEL_4_FRIENDLY_ARTILLERY_SPAWN_POS: Vec2 = Vec2::new(16.0, 206.0);
pub const LEVEL_4_FRIENDLY_ARTILLERY_FACING: Vec2 = Vec2::new(0.0, -1.0);

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

/// Reusable Attack Helicopter entity with an elongated oval fuselage, rectangular tail boom,
/// rapidly spinning top main rotor and rear tail rotor, directional flight kinematics
/// (`forward > strafe > backward`), and a limited-arc rotating chin autocannon.
#[derive(Component, Debug, Clone)]
pub struct AttackHelicopter {
    /// Forward heading unit vector on the XZ plane.
    pub heading: Vec2,
    /// Hover altitude above ground (`Y` coordinate in world space).
    pub hover_altitude: f32,
    /// Current signed longitudinal speed (`m/s`) along `heading`:
    /// positive = flying forward (`[0, max_forward_speed]`),
    /// negative = flying backward (`[-max_backward_speed, 0]`).
    pub forward_speed: f32,
    /// Current signed lateral strafe speed (`m/s`) along `right_dir()`:
    /// positive = strafing right, negative = strafing left (`[-max_strafe_speed, max_strafe_speed]`).
    pub strafe_speed: f32,
    /// Maximum forward flight speed (`m/s`) — fastest movement mode.
    pub max_forward_speed: f32,
    /// Maximum lateral side-to-side strafe speed (`m/s`) — faster than backward, slower than forward.
    pub max_strafe_speed: f32,
    /// Maximum backward flight speed (`m/s`) — slowest movement mode.
    pub max_backward_speed: f32,
    /// Flight thrust acceleration (`m/s²`).
    pub acceleration: f32,
    /// Flight braking/hover deceleration (`m/s²`).
    pub deceleration: f32,
    /// Yaw rotation rate (`rad/s`) when turning the nose toward a long-distance flight direction.
    pub yaw_rate_rad: f32,
    /// When `true`, locks the helicopter's nose heading so it translates purely via forward/backward/strafe.
    /// When `false` (default), traveling long distances (`> HELICOPTER_LONG_TRANSIT_DIST`) rotates the
    /// nose into the direction of movement and flies forward nose-first.
    pub maintain_heading: bool,
    /// Active movement order (`VehicleOrder::Hold` or `VehicleOrder::MoveTo(Vec2)`).
    pub order: VehicleOrder,
    /// Current local yaw angle (`radians`) of the chin autocannon relative to `heading`,
    /// strictly clamped to `[-cannon_max_yaw_rad, cannon_max_yaw_rad]`.
    pub cannon_yaw_local: f32,
    /// Half-angle of the chin autocannon's limited traverse arc (`radians`, default `±70°`).
    pub cannon_max_yaw_rad: f32,
    /// Angular slew speed of the chin autocannon (`rad/s`).
    pub cannon_slew_rate_rad: f32,
    /// Maximum target acquisition and engagement range of the chin autocannon (`m`).
    pub cannon_vision_range: f32,
    /// Repeating fire timer for the 30mm chin autocannon.
    pub cannon_shoot_timer: Timer,
    /// Current main rotor rotation angle (`radians`, around local `Y` axis).
    pub main_rotor_angle: f32,
    /// Angular velocity of the top main rotor (`rad/s`).
    pub main_rotor_spin_rate_rad: f32,
    /// Current tail rotor rotation angle (`radians`, around local `X` axis).
    pub tail_rotor_angle: f32,
    /// Angular velocity of the tail rotor (`rad/s`).
    pub tail_rotor_spin_rate_rad: f32,
}

impl Default for AttackHelicopter {
    fn default() -> Self {
        Self {
            heading: LEVEL_4_FRIENDLY_HELICOPTER_FACING,
            hover_altitude: HELICOPTER_HOVER_ALTITUDE,
            forward_speed: 0.0,
            strafe_speed: 0.0,
            max_forward_speed: HELICOPTER_MAX_FORWARD_SPEED,
            max_strafe_speed: HELICOPTER_MAX_STRAFE_SPEED,
            max_backward_speed: HELICOPTER_MAX_BACKWARD_SPEED,
            acceleration: HELICOPTER_ACCEL,
            deceleration: HELICOPTER_DECEL,
            yaw_rate_rad: HELICOPTER_YAW_RATE_RAD,
            maintain_heading: false,
            order: VehicleOrder::Hold,
            cannon_yaw_local: 0.0,
            cannon_max_yaw_rad: HELICOPTER_CANNON_MAX_YAW_RAD,
            cannon_slew_rate_rad: HELICOPTER_CANNON_SLEW_RATE_RAD,
            cannon_vision_range: HELICOPTER_CANNON_VISION_RANGE,
            cannon_shoot_timer: Timer::from_seconds(
                HELICOPTER_CANNON_FIRE_INTERVAL_SECS,
                TimerMode::Repeating,
            ),
            main_rotor_angle: 0.0,
            main_rotor_spin_rate_rad: HELICOPTER_MAIN_ROTOR_SPIN_RATE_RAD,
            tail_rotor_angle: 0.0,
            tail_rotor_spin_rate_rad: HELICOPTER_TAIL_ROTOR_SPIN_RATE_RAD,
        }
    }
}

impl AttackHelicopter {
    /// World-space right vector perpendicular to `heading` on the XZ plane.
    pub fn right_dir(&self) -> Vec2 {
        let forward = self
            .heading
            .normalize_or(LEVEL_4_FRIENDLY_HELICOPTER_FACING);
        Vec2::new(-forward.y, forward.x)
    }

    /// World-space 2D planar velocity vector (`m/s`) combining forward/backward and strafe movement.
    pub fn velocity_2d(&self) -> Vec2 {
        let forward = self
            .heading
            .normalize_or(LEVEL_4_FRIENDLY_HELICOPTER_FACING);
        let right = self.right_dir();
        forward * self.forward_speed + right * self.strafe_speed
    }

    /// Computes the anisotropic maximum speed (`m/s`) for a body-local direction `(local_forward, local_right)`:
    /// - Pure forward (`(1, 0)`): `self.max_forward_speed` (`22.0 m/s`)
    /// - Pure strafe (`(0, ±1)`): `self.max_strafe_speed` (`13.5 m/s`)
    /// - Pure backward (`(-1, 0)`): `self.max_backward_speed` (`8.0 m/s`)
    pub fn max_speed_for_local_dir(&self, local_forward: f32, local_right: f32) -> f32 {
        let dir = Vec2::new(local_forward, local_right).normalize_or_zero();
        if dir.length_squared() < 1e-6 {
            return 0.0;
        }
        let max_long = if dir.x >= 0.0 {
            self.max_forward_speed.max(0.1)
        } else {
            self.max_backward_speed.max(0.1)
        };
        let max_lat = self.max_strafe_speed.max(0.1);
        let denom = ((dir.x / max_long).powi(2) + (dir.y / max_lat).powi(2)).sqrt();
        if denom > 1e-6 { 1.0 / denom } else { 0.0 }
    }

    /// World-space 2D aiming direction of the limited-arc chin autocannon.
    pub fn cannon_world_dir(&self) -> Vec2 {
        let forward = self
            .heading
            .normalize_or(LEVEL_4_FRIENDLY_HELICOPTER_FACING);
        let fuselage_yaw = f32::atan2(-forward.x, -forward.y);
        let clamped_local = self
            .cannon_yaw_local
            .clamp(-self.cannon_max_yaw_rad, self.cannon_max_yaw_rad);
        let total_yaw = fuselage_yaw + clamped_local;
        Vec2::new(-total_yaw.sin(), -total_yaw.cos()).normalize_or(forward)
    }

    /// World-space XZ position of the nose-mounted chin autocannon turret pivot given the
    /// helicopter's world XZ center position `heli_pos`.
    pub fn cannon_world_pos(&self, heli_pos: Vec2) -> Vec2 {
        let forward = self
            .heading
            .normalize_or(LEVEL_4_FRIENDLY_HELICOPTER_FACING);
        heli_pos + forward * HELICOPTER_CANNON_MOUNT_FORWARD
    }

    /// World-space XZ position of the autocannon barrel muzzle tip given the helicopter's
    /// world XZ center position `heli_pos`.
    pub fn cannon_muzzle_world_pos(&self, heli_pos: Vec2) -> Vec2 {
        let mount_pos = self.cannon_world_pos(heli_pos);
        let cannon_dir = self.cannon_world_dir();
        mount_pos + cannon_dir * HELICOPTER_CANNON_BARREL_TIP_OFFSET
    }
}

/// Marker component for the rapidly spinning top main rotor assembly on an `AttackHelicopter`.
#[derive(Component, Debug, Clone, Copy)]
pub struct HelicopterMainRotor {
    pub helicopter: Entity,
}

/// Marker component for the rapidly spinning tail anti-torque rotor on an `AttackHelicopter`.
#[derive(Component, Debug, Clone, Copy)]
pub struct HelicopterTailRotor {
    pub helicopter: Entity,
}

/// Marker component for the limited-arc rotating chin autocannon turret on an `AttackHelicopter`.
#[derive(Component, Debug, Clone, Copy)]
pub struct HelicopterAutocannon {
    pub helicopter: Entity,
}

/// Attached to one of the 4 soldiers operating a `FieldArtillery` howitzer emplacement.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArtilleryCrewMember {
    pub artillery: Entity,
    pub slot: usize,
}

/// Stationary M119A3 105mm light towed field artillery piece that can rotate very slowly on its
/// central firing platform axis, has full-map indirect-fire range, and fires high-explosive
/// shells that explode in a medium circle on impact when a Squad Leader calls for support
/// and provides target coordinates. Operated by a 4-soldier howitzer section standing behind it.
#[derive(Component, Debug, Clone)]
pub struct FieldArtillery {
    /// Current forward heading unit vector of the howitzer barrel on the XZ plane.
    pub heading: Vec2,
    /// Very slow traverse rotation speed (`rad/s`) on its central axis.
    pub rotation_speed_rad: f32,
    /// Maximum indirect-fire range (`m`) — covers the entire map.
    pub firing_range: f32,
    /// Active target coordinates (`Some(Vec2)`) provided by the Squad Leader calling for support (or assigned directly).
    pub target_coords: Option<Vec2>,
    /// Tracks whether `target_coords` was assigned by a Squad Leader's support call.
    pub called_by_squad_leader: bool,
    /// Slow-firing reload timer between 105mm HE shells.
    pub reload_timer: Timer,
    /// Hydro-pneumatic recoil displacement (`m`, kicks back on firing and smoothly recuperates to `0.0`).
    pub recoil_offset: f32,
    /// The 4 operating crew soldiers stationed behind the howitzer.
    pub crew: Vec<Entity>,
}

impl Default for FieldArtillery {
    fn default() -> Self {
        let mut reload_timer = Timer::from_seconds(ARTILLERY_RELOAD_SECS, TimerMode::Once);
        reload_timer.tick(std::time::Duration::from_secs_f32(ARTILLERY_RELOAD_SECS));
        Self {
            heading: LEVEL_4_FRIENDLY_ARTILLERY_FACING,
            rotation_speed_rad: ARTILLERY_ROTATION_SPEED_RAD,
            firing_range: ARTILLERY_FIRING_RANGE,
            target_coords: None,
            called_by_squad_leader: false,
            reload_timer,
            recoil_offset: 0.0,
            crew: Vec::new(),
        }
    }
}

impl FieldArtillery {
    /// World-space XZ position of the long M119A3 barrel muzzle tip given the artillery emplacement's
    /// world XZ center position `artillery_pos`.
    pub fn muzzle_world_pos(&self, artillery_pos: Vec2) -> Vec2 {
        let forward = self.heading.normalize_or(LEVEL_4_FRIENDLY_ARTILLERY_FACING);
        artillery_pos + forward * ARTILLERY_BARREL_MUZZLE_OFFSET
    }

    /// World-space XZ position and 2D facing vector for one of the 4 operating crew soldiers (`slot 0..4`)
    /// standing behind the howitzer emplacement.
    pub fn crew_station_world_pos_and_facing(
        &self,
        artillery_pos: Vec2,
        slot: usize,
    ) -> (Vec2, Vec2) {
        let forward = self.heading.normalize_or(LEVEL_4_FRIENDLY_ARTILLERY_FACING);
        let right = Vec2::new(-forward.y, forward.x);
        let (right_offset, rear_offset, splay) = match slot % 4 {
            // Slot 0: Gunner (behind left shield / breech)
            0 => (-1.65, 1.95, -0.15),
            // Slot 1: Assistant Gunner / Loader (behind right shield / breech)
            1 => (1.65, 1.95, 0.15),
            // Slot 2: Section Chief (behind left trail spade)
            2 => (-1.35, 3.85, -0.35),
            // Slot 3: Ammunition Bearer (behind right trail spade)
            _ => (1.35, 3.85, 0.35),
        };
        let world_pos = artillery_pos + right * right_offset - forward * rear_offset;
        let world_facing = (forward + right * splay).normalize_or(forward);
        (world_pos, world_facing)
    }
}

/// Marker component for the long M119A3 105mm barrel assembly on a `FieldArtillery` piece.
#[derive(Component, Debug, Clone, Copy)]
pub struct ArtilleryBarrel {
    pub artillery: Entity,
}

/// In-flight 105mm high-explosive artillery shell traveling along a ballistic arc toward `target_pos`.
#[derive(Component, Debug, Clone)]
pub struct ArtilleryShell {
    pub origin: Vec2,
    pub target_pos: Vec2,
    pub speed: f32,
    pub total_distance: f32,
    pub traveled_distance: f32,
    pub blast_radius: f32,
    pub team: Team,
}

/// Expanding medium-circle visual blast effect spawned when an `ArtilleryShell` impacts at `target_pos`.
#[derive(Component, Debug, Clone)]
pub struct ArtilleryExplosion {
    pub timer: Timer,
    pub blast_radius: f32,
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
                    update_attack_helicopters,
                    update_field_artillery,
                    update_artillery_crew,
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

    let hull_mesh = meshes.add(Cuboid::new(
        APC_HULL_WIDTH,
        APC_HULL_HEIGHT,
        APC_HULL_LENGTH,
    ));
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
    let ramp_mesh = meshes.add(Cuboid::new(
        APC_HULL_WIDTH * 0.76,
        APC_HULL_HEIGHT * 0.72,
        0.20,
    ));
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
    let barrel_mesh = meshes.add(Cuboid::new(
        APC_BARREL_WIDTH,
        APC_BARREL_WIDTH,
        APC_BARREL_LENGTH,
    ));
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
        Transform::from_xyz(0.0, 0.05, -(APC_TURRET_RADIUS + APC_BARREL_LENGTH * 0.45)),
        ChildOf(turret_entity),
    ));

    // Vision cone attached to turret so the gunner's 360° sector is visible on the ground
    commands.spawn((
        Mesh3d(turret_vision_cone_mesh),
        MeshMaterial3d(turret_vision_cone_mat.clone()),
        NotShadowCaster,
        NotShadowReceiver,
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
                Transform::from_xyz(spawn_pos.x, 0.05, spawn_pos.y).with_rotation(
                    Quat::from_rotation_y(hull_yaw) * Quat::from_rotation_x(-FRAC_PI_2),
                ),
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

/// Spawns a reusable Attack Helicopter with:
/// - A large elongated oval fuselage (`Sphere::new(1.0)` scaled to `HELICOPTER_BODY_WIDTH × HELICOPTER_BODY_HEIGHT × HELICOPTER_BODY_LENGTH`),
/// - Tandem armored cockpit canopy, stub wings with engine nacelles & under-wing rocket pods, and nose sensor/chin turret,
/// - A long rectangular tail boom (`Cuboid::new(HELICOPTER_TAIL_WIDTH, HELICOPTER_TAIL_HEIGHT, HELICOPTER_TAIL_LENGTH)`) with horizontal stabilizer and vertical tail fin,
/// - Rapidly spinning 4-blade top main rotor (`HelicopterMainRotor`) and anti-torque tail rotor (`HelicopterTailRotor`).
pub fn spawn_attack_helicopter(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    team: Team,
    spawn_pos: Vec2,
    facing: Vec2,
) -> (Entity, Entity, Entity) {
    let forward = facing.normalize_or(Vec2::new(0.0, -1.0));
    let fuselage_yaw = f32::atan2(-forward.x, -forward.y);

    let (body_color, accent_color, canopy_color, rotor_blade_color, rotor_disc_color) = match team {
        Team::Player => (
            Color::srgb(0.22, 0.35, 0.27),
            Color::srgb(0.16, 0.27, 0.21),
            Color::srgb(0.62, 0.78, 0.86),
            Color::srgb(0.12, 0.14, 0.14),
            Color::srgba(0.18, 0.22, 0.20, 0.18),
        ),
        Team::Enemy => (
            Color::srgb(0.40, 0.22, 0.20),
            Color::srgb(0.30, 0.16, 0.15),
            Color::srgb(0.75, 0.62, 0.58),
            Color::srgb(0.14, 0.12, 0.12),
            Color::srgba(0.24, 0.16, 0.16, 0.18),
        ),
    };

    let unit_sphere_mesh = meshes.add(Sphere::new(1.0));
    let body_mat = materials.add(StandardMaterial {
        base_color: body_color,
        perceptual_roughness: 0.68,
        metallic: 0.25,
        ..default()
    });
    let accent_mat = materials.add(StandardMaterial {
        base_color: accent_color,
        perceptual_roughness: 0.62,
        metallic: 0.3,
        ..default()
    });
    let canopy_mat = materials.add(StandardMaterial {
        base_color: canopy_color,
        perceptual_roughness: 0.2,
        metallic: 0.1,
        ..default()
    });
    let dark_metal_mat = materials.add(StandardMaterial {
        base_color: rotor_blade_color,
        perceptual_roughness: 0.45,
        metallic: 0.5,
        ..default()
    });
    let rotor_disc_mat = materials.add(StandardMaterial {
        base_color: rotor_disc_color,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        unlit: true,
        ..default()
    });

    // Root helicopter entity hovered above the battlefield
    let helicopter_entity = commands
        .spawn((
            LevelEntity,
            Transform::from_xyz(spawn_pos.x, HELICOPTER_HOVER_ALTITUDE, spawn_pos.y)
                .with_rotation(Quat::from_rotation_y(fuselage_yaw)),
            Visibility::default(),
            team,
            AttackHelicopter {
                heading: forward,
                hover_altitude: HELICOPTER_HOVER_ALTITUDE,
                ..default()
            },
        ))
        .id();

    // 1. Main Fuselage: large elongated oval (`Sphere::new(1.0)` scaled to 2.9m × 2.3m × 8.8m)
    commands.spawn((
        Mesh3d(unit_sphere_mesh.clone()),
        MeshMaterial3d(body_mat.clone()),
        NotShadowReceiver,
        Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::new(
            HELICOPTER_BODY_WIDTH * 0.5,
            HELICOPTER_BODY_HEIGHT * 0.5,
            HELICOPTER_BODY_LENGTH * 0.5,
        )),
        ChildOf(helicopter_entity),
    ));

    // Tandem cockpit canopy (raised elongated oval glass bubble on forward-upper fuselage)
    commands.spawn((
        Mesh3d(unit_sphere_mesh.clone()),
        MeshMaterial3d(canopy_mat),
        NotShadowCaster,
        NotShadowReceiver,
        Transform::from_xyz(0.0, 0.48, -1.35).with_scale(Vec3::new(0.88, 0.55, 2.35)),
        ChildOf(helicopter_entity),
    ));

    // Nose FLIR/TADS sensor turret dome + rotating limited-arc chin 30mm autocannon (`-Z` is forward)
    let sensor_dome_mesh = meshes.add(Sphere::new(0.42));
    let chin_turret_hub_mesh = meshes.add(Cylinder::new(0.28, 0.30));
    let chin_gun_mesh = meshes.add(Cuboid::new(0.16, 0.16, 1.55));
    let cannon_cone_color = match team {
        Team::Player => Color::srgba(0.25, 0.85, 1.0, 0.12),
        Team::Enemy => Color::srgba(0.95, 0.25, 0.20, 0.12),
    };
    let cannon_vision_cone_mesh = meshes.add(create_vision_cone_mesh(
        HELICOPTER_CANNON_VISION_RANGE,
        35.0_f32.to_radians(),
        16,
    ));
    let cannon_vision_cone_mat = materials.add(StandardMaterial {
        base_color: cannon_cone_color,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    commands.spawn((
        Mesh3d(sensor_dome_mesh),
        MeshMaterial3d(dark_metal_mat.clone()),
        NotShadowReceiver,
        Transform::from_xyz(0.0, -0.18, -HELICOPTER_BODY_LENGTH * 0.49),
        ChildOf(helicopter_entity),
    ));

    let cannon_turret_entity = commands
        .spawn((
            HelicopterAutocannon {
                helicopter: helicopter_entity,
            },
            Mesh3d(chin_turret_hub_mesh),
            MeshMaterial3d(dark_metal_mat.clone()),
            NotShadowReceiver,
            Transform::from_xyz(0.0, -0.34, -HELICOPTER_CANNON_MOUNT_FORWARD),
            ChildOf(helicopter_entity),
        ))
        .id();

    commands.spawn((
        Mesh3d(chin_gun_mesh),
        MeshMaterial3d(dark_metal_mat.clone()),
        NotShadowReceiver,
        Transform::from_xyz(0.0, -0.02, -0.78),
        ChildOf(cannon_turret_entity),
    ));

    commands.spawn((
        Mesh3d(cannon_vision_cone_mesh),
        MeshMaterial3d(cannon_vision_cone_mat),
        NotShadowCaster,
        NotShadowReceiver,
        Transform::from_xyz(0.0, -(HELICOPTER_HOVER_ALTITUDE - 0.34 - 0.035), 0.0)
            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
        ChildOf(cannon_turret_entity),
    ));

    // Stub wings + twin turboshaft engine nacelles + under-wing rocket pods
    let stub_wing_mesh = meshes.add(Cuboid::new(5.2, 0.20, 1.15));
    let nacelle_mesh = meshes.add(Cuboid::new(0.68, 0.65, 2.45));
    let rocket_pod_mesh = meshes.add(Cylinder::new(0.26, 1.75));
    commands.spawn((
        Mesh3d(stub_wing_mesh),
        MeshMaterial3d(accent_mat.clone()),
        NotShadowReceiver,
        Transform::from_xyz(0.0, 0.05, 0.25),
        ChildOf(helicopter_entity),
    ));
    for side in [-1.0_f32, 1.0_f32] {
        // Side engine nacelle
        commands.spawn((
            Mesh3d(nacelle_mesh.clone()),
            MeshMaterial3d(accent_mat.clone()),
            NotShadowReceiver,
            Transform::from_xyz(side * 1.22, 0.34, 0.35),
            ChildOf(helicopter_entity),
        ));
        // Under-wing rocket pod (rotated to point forward along -Z)
        commands.spawn((
            Mesh3d(rocket_pod_mesh.clone()),
            MeshMaterial3d(dark_metal_mat.clone()),
            NotShadowReceiver,
            Transform::from_xyz(side * 2.05, -0.24, 0.15)
                .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            ChildOf(helicopter_entity),
        ));
    }

    // 2. Tail Boom: long rectangular shape extending rearward (+Z) from the back of the oval body
    let tail_boom_mesh = meshes.add(Cuboid::new(
        HELICOPTER_TAIL_WIDTH,
        HELICOPTER_TAIL_HEIGHT,
        HELICOPTER_TAIL_LENGTH,
    ));
    let tail_boom_z = HELICOPTER_BODY_LENGTH * 0.38 + HELICOPTER_TAIL_LENGTH * 0.5;
    commands.spawn((
        Mesh3d(tail_boom_mesh),
        MeshMaterial3d(body_mat),
        NotShadowReceiver,
        Transform::from_xyz(0.0, 0.18, tail_boom_z),
        ChildOf(helicopter_entity),
    ));

    // Horizontal tailplane stabilizer + vertical tail fin at the rear of the rectangular tail boom
    let tail_end_z = tail_boom_z + HELICOPTER_TAIL_LENGTH * 0.45;
    let horiz_stab_mesh = meshes.add(Cuboid::new(2.55, 0.14, 0.65));
    let vert_fin_mesh = meshes.add(Cuboid::new(0.20, 1.40, 0.95));
    commands.spawn((
        Mesh3d(horiz_stab_mesh),
        MeshMaterial3d(accent_mat.clone()),
        NotShadowReceiver,
        Transform::from_xyz(0.0, 0.12, tail_end_z - 0.25),
        ChildOf(helicopter_entity),
    ));
    commands.spawn((
        Mesh3d(vert_fin_mesh),
        MeshMaterial3d(accent_mat),
        NotShadowReceiver,
        Transform::from_xyz(0.0, 0.68, tail_end_z),
        ChildOf(helicopter_entity),
    ));

    // 3. Top Main Rotor Assembly: central rotor mast + 4 crossed blades + non-shadow-casting rotor blur disc
    let rotor_mast_mesh = meshes.add(Cylinder::new(0.28, 0.48));
    let blade_span_x_mesh = meshes.add(Cuboid::new(
        HELICOPTER_ROTOR_DIAMETER,
        0.06,
        HELICOPTER_ROTOR_BLADE_CHORD,
    ));
    let blade_span_z_mesh = meshes.add(Cuboid::new(
        HELICOPTER_ROTOR_BLADE_CHORD,
        0.06,
        HELICOPTER_ROTOR_DIAMETER,
    ));
    let rotor_disc_mesh = meshes.add(Circle::new(HELICOPTER_ROTOR_DIAMETER * 0.5));

    let main_rotor_entity = commands
        .spawn((
            HelicopterMainRotor {
                helicopter: helicopter_entity,
            },
            Mesh3d(rotor_mast_mesh),
            MeshMaterial3d(dark_metal_mat.clone()),
            NotShadowCaster,
            NotShadowReceiver,
            Transform::from_xyz(0.0, HELICOPTER_BODY_HEIGHT * 0.5 + 0.24, 0.15),
            ChildOf(helicopter_entity),
        ))
        .id();

    commands.spawn((
        Mesh3d(blade_span_x_mesh),
        MeshMaterial3d(dark_metal_mat.clone()),
        NotShadowCaster,
        NotShadowReceiver,
        Transform::from_xyz(0.0, 0.14, 0.0),
        ChildOf(main_rotor_entity),
    ));
    commands.spawn((
        Mesh3d(blade_span_z_mesh),
        MeshMaterial3d(dark_metal_mat.clone()),
        NotShadowCaster,
        NotShadowReceiver,
        Transform::from_xyz(0.0, 0.14, 0.0),
        ChildOf(main_rotor_entity),
    ));
    commands.spawn((
        Mesh3d(rotor_disc_mesh),
        MeshMaterial3d(rotor_disc_mat),
        NotShadowCaster,
        NotShadowReceiver,
        Transform::from_xyz(0.0, 0.12, 0.0).with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
        ChildOf(main_rotor_entity),
    ));

    // 4. Tail Rotor Assembly: side-mounted anti-torque rotor blades on the vertical tail fin
    let tail_hub_mesh = meshes.add(Cylinder::new(0.12, 0.24));
    let tail_blade_a_mesh = meshes.add(Cuboid::new(0.06, 2.15, 0.20));
    let tail_blade_b_mesh = meshes.add(Cuboid::new(0.06, 0.20, 2.15));

    let tail_rotor_entity = commands
        .spawn((
            HelicopterTailRotor {
                helicopter: helicopter_entity,
            },
            Mesh3d(tail_hub_mesh),
            MeshMaterial3d(dark_metal_mat.clone()),
            NotShadowCaster,
            NotShadowReceiver,
            Transform::from_xyz(-0.22, 0.78, tail_end_z + 0.08)
                .with_rotation(Quat::from_rotation_z(FRAC_PI_2)),
            ChildOf(helicopter_entity),
        ))
        .id();

    commands.spawn((
        Mesh3d(tail_blade_a_mesh),
        MeshMaterial3d(dark_metal_mat.clone()),
        NotShadowCaster,
        NotShadowReceiver,
        Transform::IDENTITY,
        ChildOf(tail_rotor_entity),
    ));
    commands.spawn((
        Mesh3d(tail_blade_b_mesh),
        MeshMaterial3d(dark_metal_mat),
        NotShadowCaster,
        NotShadowReceiver,
        Transform::IDENTITY,
        ChildOf(tail_rotor_entity),
    ));

    (helicopter_entity, main_rotor_entity, tail_rotor_entity)
}

/// Spawns a stationary M119A3 105mm light towed howitzer field artillery piece for `team` at `pos`
/// facing `facing` (`Vec2` on the XZ plane).
///
/// Modelled after the U.S. Army M119A3 105mm howitzer:
/// - Circular central firing platform / turntable (`Cylinder`) allowing 360° axis rotation.
/// - Lower carriage axle with two pneumatic road wheels (`Cylinder`s on left and right).
/// - Rearward box trail beam (`+Z`) terminating in wide entrenched ground stabilizer spades and a tow lunette.
/// - Sloped left/right gun shields flanking the breech and an M119A3 digital fire control display unit.
/// - Elevating cradle, hydro-pneumatic recoil recuperator cylinder, heavy breech block, and a
///   long 105mm L/30 barrel (`ARTILLERY_BARREL_LENGTH = 5.6 m`) extending forward (`-Z`) with a
///   stepped muzzle collar.
///
/// Returns `(artillery_entity, barrel_entity)`.
pub fn spawn_field_artillery(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    team: Team,
    pos: Vec2,
    facing: Vec2,
) -> (Entity, Entity) {
    let forward = facing.normalize_or(LEVEL_4_FRIENDLY_ARTILLERY_FACING);
    let yaw = f32::atan2(-forward.x, -forward.y);

    let (olive_color, shield_color) = match team {
        Team::Player => (Color::srgb(0.24, 0.34, 0.22), Color::srgb(0.20, 0.29, 0.19)),
        Team::Enemy => (Color::srgb(0.42, 0.26, 0.22), Color::srgb(0.35, 0.21, 0.18)),
    };

    let olive_mat = materials.add(StandardMaterial {
        base_color: olive_color,
        perceptual_roughness: 0.72,
        metallic: 0.22,
        ..default()
    });
    let shield_mat = materials.add(StandardMaterial {
        base_color: shield_color,
        perceptual_roughness: 0.76,
        metallic: 0.26,
        ..default()
    });
    let gunmetal_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.16, 0.18, 0.17),
        perceptual_roughness: 0.48,
        metallic: 0.65,
        ..default()
    });
    let tire_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.11, 0.11, 0.12),
        perceptual_roughness: 0.90,
        metallic: 0.05,
        ..default()
    });
    let dfcs_screen_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.22, 0.62, 0.45),
        unlit: true,
        ..default()
    });

    // 1. Root entity: M119A3 circular firing platform / turntable baseplate on the ground
    let platform_mesh = meshes.add(Cylinder::new(1.05, 0.14));
    let artillery_entity = commands
        .spawn((
            LevelEntity,
            FieldArtillery {
                heading: forward,
                ..default()
            },
            team,
            Mesh3d(platform_mesh),
            MeshMaterial3d(gunmetal_mat.clone()),
            Transform::from_xyz(pos.x, 0.07, pos.y).with_rotation(Quat::from_rotation_y(yaw)),
        ))
        .id();

    // 2. Carriage axle + two pneumatic road wheels + wheel hubs
    let axle_mesh = meshes.add(Cuboid::new(2.35, 0.26, 0.44));
    let wheel_tire_mesh = meshes.add(Cylinder::new(0.56, 0.30));
    let wheel_hub_mesh = meshes.add(Cylinder::new(0.28, 0.34));
    commands.spawn((
        Mesh3d(axle_mesh),
        MeshMaterial3d(olive_mat.clone()),
        Transform::from_xyz(0.0, 0.45, 0.10),
        ChildOf(artillery_entity),
    ));
    for side in [-1.0_f32, 1.0_f32] {
        commands.spawn((
            Mesh3d(wheel_tire_mesh.clone()),
            MeshMaterial3d(tire_mat.clone()),
            Transform::from_xyz(side * 1.28, 0.49, 0.10)
                .with_rotation(Quat::from_rotation_z(FRAC_PI_2)),
            ChildOf(artillery_entity),
        ));
        commands.spawn((
            Mesh3d(wheel_hub_mesh.clone()),
            MeshMaterial3d(olive_mat.clone()),
            Transform::from_xyz(side * 1.28, 0.49, 0.10)
                .with_rotation(Quat::from_rotation_z(FRAC_PI_2)),
            ChildOf(artillery_entity),
        ));
    }

    // 3. M119A3 single box trail extending rearward (`+Z`), rear stabilizer spades, and tow lunette
    let trail_beam_mesh = meshes.add(Cuboid::new(0.46, 0.26, 3.10));
    let rear_spade_mesh = meshes.add(Cuboid::new(1.60, 0.22, 0.56));
    let lunette_ring_mesh = meshes.add(Cylinder::new(0.22, 0.10));
    commands.spawn((
        Mesh3d(trail_beam_mesh),
        MeshMaterial3d(olive_mat.clone()),
        Transform::from_xyz(0.0, 0.26, 1.65).with_rotation(Quat::from_rotation_x(-0.10)),
        ChildOf(artillery_entity),
    ));
    commands.spawn((
        Mesh3d(rear_spade_mesh),
        MeshMaterial3d(gunmetal_mat.clone()),
        Transform::from_xyz(0.0, 0.08, 3.15),
        ChildOf(artillery_entity),
    ));
    commands.spawn((
        Mesh3d(lunette_ring_mesh),
        MeshMaterial3d(gunmetal_mat.clone()),
        Transform::from_xyz(0.0, 0.14, 3.55),
        ChildOf(artillery_entity),
    ));

    // 4. Upper carriage saddle, left/right sloped gun shields, and M119A3 Digital Fire Control unit
    let saddle_mesh = meshes.add(Cuboid::new(0.88, 0.56, 0.95));
    let shield_plate_mesh = meshes.add(Cuboid::new(0.62, 0.78, 0.12));
    let dfcs_box_mesh = meshes.add(Cuboid::new(0.26, 0.24, 0.22));
    commands.spawn((
        Mesh3d(saddle_mesh),
        MeshMaterial3d(olive_mat.clone()),
        Transform::from_xyz(0.0, 0.72, 0.05),
        ChildOf(artillery_entity),
    ));
    for side in [-1.0_f32, 1.0_f32] {
        commands.spawn((
            Mesh3d(shield_plate_mesh.clone()),
            MeshMaterial3d(shield_mat.clone()),
            Transform::from_xyz(side * 0.64, 0.86, -0.18)
                .with_rotation(Quat::from_rotation_x(0.22)),
            ChildOf(artillery_entity),
        ));
    }
    commands.spawn((
        Mesh3d(dfcs_box_mesh),
        MeshMaterial3d(dfcs_screen_mat),
        Transform::from_xyz(-0.56, 1.08, 0.08),
        ChildOf(artillery_entity),
    ));

    // 5. Elevating recoil assembly + long M119A3 105mm L/30 barrel (`-Z` is forward, pitched up ~16°)
    let breech_mesh = meshes.add(Cuboid::new(0.54, 0.44, 1.75));
    let recuperator_mesh = meshes.add(Cylinder::new(0.14, 1.85));
    let barrel_jacket_mesh = meshes.add(Cylinder::new(0.21, 1.45));
    let long_barrel_mesh = meshes.add(Cylinder::new(0.15, ARTILLERY_BARREL_LENGTH));
    let muzzle_collar_mesh = meshes.add(Cylinder::new(0.22, 0.44));

    let barrel_entity = commands
        .spawn((
            ArtilleryBarrel {
                artillery: artillery_entity,
            },
            Mesh3d(breech_mesh),
            MeshMaterial3d(gunmetal_mat.clone()),
            Transform::from_xyz(0.0, 0.98, 0.05).with_rotation(Quat::from_rotation_x(0.28)),
            ChildOf(artillery_entity),
        ))
        .id();

    // Hydro-pneumatic recoil recuperator cylinder above the breech
    commands.spawn((
        Mesh3d(recuperator_mesh),
        MeshMaterial3d(olive_mat.clone()),
        Transform::from_xyz(0.0, 0.28, -0.25).with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
        ChildOf(barrel_entity),
    ));
    // Stepped barrel cradle jacket
    commands.spawn((
        Mesh3d(barrel_jacket_mesh),
        MeshMaterial3d(olive_mat),
        Transform::from_xyz(0.0, 0.04, -1.05).with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
        ChildOf(barrel_entity),
    ));
    // Long 105mm howitzer tube extending forward along `-Z`
    commands.spawn((
        Mesh3d(long_barrel_mesh),
        MeshMaterial3d(gunmetal_mat.clone()),
        Transform::from_xyz(0.0, 0.04, -2.55).with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
        ChildOf(barrel_entity),
    ));
    // M119A3 muzzle brake / collar at the barrel tip
    commands.spawn((
        Mesh3d(muzzle_collar_mesh),
        MeshMaterial3d(gunmetal_mat),
        Transform::from_xyz(0.0, 0.04, -5.20).with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
        ChildOf(barrel_entity),
    ));

    // 6. Spawn 4 operating soldiers standing around behind the howitzer, each armed with a service
    //    rifle and equipped with their own vision cone that reveals the Fog of War.
    let (crew_color, cone_color) = match team {
        Team::Player => (
            Color::srgb(0.18, 0.68, 0.88),
            Color::srgba(0.20, 0.75, 1.0, 0.16),
        ),
        Team::Enemy => (
            Color::srgb(0.90, 0.15, 0.15),
            Color::srgba(0.95, 0.20, 0.20, 0.18),
        ),
    };
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
    let crew_cone_mat = materials.add(StandardMaterial {
        base_color: cone_color,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    let default_artillery = FieldArtillery {
        heading: forward,
        ..default()
    };
    let mut crew_entities = Vec::with_capacity(4);
    for slot in 0..4 {
        let (station_pos, station_facing) =
            default_artillery.crew_station_world_pos_and_facing(pos, slot);
        let station_yaw = f32::atan2(-station_facing.x, -station_facing.y);

        let crew_ent = commands
            .spawn((
                LevelEntity,
                Mesh3d(soldier_mesh.clone()),
                MeshMaterial3d(soldier_mat.clone()),
                Transform::from_xyz(station_pos.x, 0.05, station_pos.y).with_rotation(
                    Quat::from_rotation_y(station_yaw) * Quat::from_rotation_x(-FRAC_PI_2),
                ),
                Visibility::Inherited,
                team,
                Soldier {
                    facing_dir: station_facing,
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::rifle_only(),
                },
                Suppression::default(),
                ArtilleryCrewMember {
                    artillery: artillery_entity,
                    slot,
                },
            ))
            .id();

        if team == Team::Player {
            commands.entity(crew_ent).insert((
                Friendly {
                    fireteam: 4, // Dedicated field artillery crew section
                    slot,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.35, TimerMode::Repeating),
                    speed: 11.0,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                crate::friendly::SquadId(2), // Separate crew ID so infantry squad doctrine ignores howitzer crew
            ));
        }

        // Direction indicator dot on the front edge of the soldier circle
        commands.spawn((
            Mesh3d(pointer_mesh.clone()),
            MeshMaterial3d(pointer_mat.clone()),
            Transform::from_xyz(0.0, 0.42, 0.015),
            ChildOf(crew_ent),
        ));

        // Vision cone mesh child on the ground plane
        commands.spawn((
            Mesh3d(crew_cone_mesh.clone()),
            MeshMaterial3d(crew_cone_mat.clone()),
            NotShadowCaster,
            NotShadowReceiver,
            Transform::from_xyz(0.0, 0.0, -0.015),
            ChildOf(crew_ent),
        ));

        crew_entities.push(crew_ent);
    }

    commands.entity(artillery_entity).insert(FieldArtillery {
        heading: forward,
        crew: crew_entities,
        ..default()
    });

    (artillery_entity, barrel_entity)
}

/// Spawns the friendly APC, friendly Attack Helicopter, and friendly M119A3 Field Artillery in `test_level_4`.
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

    spawn_attack_helicopter(
        &mut commands,
        &mut meshes,
        &mut materials,
        Team::Player,
        LEVEL_4_FRIENDLY_HELICOPTER_SPAWN_POS,
        LEVEL_4_FRIENDLY_HELICOPTER_FACING,
    );

    spawn_field_artillery(
        &mut commands,
        &mut meshes,
        &mut materials,
        Team::Player,
        LEVEL_4_FRIENDLY_ARTILLERY_SPAWN_POS,
        LEVEL_4_FRIENDLY_ARTILLERY_FACING,
    );
}

/// Updates every `AttackHelicopter`:
/// 1. Rapidly spins the top main rotor and rear tail rotor.
/// 2. Executes 2D directional flight movement (forward, backward, and side-to-side strafing) with
///    the realistic speed hierarchy: `forward` (fastest) > `strafe` (intermediate) > `backward` (slowest).
/// 3. Rotates the chin autocannon within its limited forward arc (`±cannon_max_yaw_rad`) to track and
///    fire at enemies in view.
pub fn update_attack_helicopters(
    mut commands: Commands,
    time: Res<Time>,
    current_level: Option<Res<CurrentLevel>>,
    helicopter_order: Option<Res<HelicopterOrder>>,
    bullet_assets: Option<Res<BulletAssets>>,
    mut gunshot_writer: MessageWriter<GunshotEvent>,
    mut helicopter_query: Query<
        (&Team, &mut AttackHelicopter, &mut Transform),
        (
            Without<HelicopterMainRotor>,
            Without<HelicopterTailRotor>,
            Without<HelicopterAutocannon>,
            Without<ApcVehicle>,
            Without<ApcTurret>,
            Without<Soldier>,
            Without<MountedInVehicle>,
            Without<Building>,
            Without<LowWall>,
        ),
    >,
    mut main_rotor_query: Query<
        (&HelicopterMainRotor, &mut Transform),
        (
            Without<AttackHelicopter>,
            Without<HelicopterTailRotor>,
            Without<HelicopterAutocannon>,
            Without<ApcVehicle>,
            Without<ApcTurret>,
            Without<Soldier>,
            Without<MountedInVehicle>,
            Without<Building>,
            Without<LowWall>,
        ),
    >,
    mut tail_rotor_query: Query<
        (&HelicopterTailRotor, &mut Transform),
        (
            Without<AttackHelicopter>,
            Without<HelicopterMainRotor>,
            Without<HelicopterAutocannon>,
            Without<ApcVehicle>,
            Without<ApcTurret>,
            Without<Soldier>,
            Without<MountedInVehicle>,
            Without<Building>,
            Without<LowWall>,
        ),
    >,
    mut cannon_query: Query<
        (&HelicopterAutocannon, &mut Transform),
        (
            Without<AttackHelicopter>,
            Without<HelicopterMainRotor>,
            Without<HelicopterTailRotor>,
            Without<ApcVehicle>,
            Without<ApcTurret>,
            Without<Soldier>,
            Without<MountedInVehicle>,
            Without<Building>,
            Without<LowWall>,
        ),
    >,
    soldiers_q: Query<
        (&Transform, &Team),
        (
            With<Soldier>,
            Without<MountedInVehicle>,
            Without<AttackHelicopter>,
            Without<HelicopterMainRotor>,
            Without<HelicopterTailRotor>,
            Without<HelicopterAutocannon>,
            Without<ApcVehicle>,
            Without<ApcTurret>,
            Without<Building>,
            Without<LowWall>,
        ),
    >,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (
            With<Building>,
            Without<AttackHelicopter>,
            Without<HelicopterMainRotor>,
            Without<HelicopterTailRotor>,
            Without<HelicopterAutocannon>,
            Without<ApcVehicle>,
            Without<ApcTurret>,
            Without<Soldier>,
            Without<MountedInVehicle>,
            Without<LowWall>,
        ),
    >,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }

    let level = current_level.as_deref().copied().unwrap_or_default();
    let world_bound = if level == CurrentLevel::Level4 {
        RTS_SOLDIER_WORLD_BOUND
    } else {
        SOLDIER_WORLD_BOUND
    };
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();

    for (v_team, mut heli, mut heli_tf) in &mut helicopter_query {
        // 1. Spin main and tail rotors
        heli.main_rotor_angle =
            (heli.main_rotor_angle + heli.main_rotor_spin_rate_rad * dt) % std::f32::consts::TAU;
        heli.tail_rotor_angle =
            (heli.tail_rotor_angle + heli.tail_rotor_spin_rate_rad * dt) % std::f32::consts::TAU;

        // Sync player helicopter order from HelicopterOrder resource if present
        if *v_team == Team::Player {
            if let Some(ref h_order) = helicopter_order {
                match h_order.0 {
                    CommanderOrder::TakePosition(target_pos) => {
                        heli.order = VehicleOrder::MoveTo(target_pos);
                    }
                    CommanderOrder::Hold => {
                        if !h_order.is_changed() {
                            // Keep direct programmatic `heli.order` unless HelicopterOrder was explicitly changed to Hold
                        } else {
                            heli.order = VehicleOrder::Hold;
                        }
                    }
                }
            }
        }

        let current_pos = Vec2::new(heli_tf.translation.x, heli_tf.translation.z);
        let current_cannon_pos = heli.cannon_world_pos(current_pos);

        // Pre-scan opposing soldiers so that when the helicopter is holding a position and
        // enemies are detected, it can rotate (and, if blocked by a building's blind spot,
        // nudge laterally/longitudinally) to get into firing position.
        let mut closest_los_enemy: Option<(Vec2, f32)> = None;
        let mut closest_occluded_enemy: Option<(Vec2, f32)> = None;
        for (s_tf, s_team) in &soldiers_q {
            if *s_team == *v_team {
                continue;
            }
            let enemy_pos = Vec2::new(s_tf.translation.x, s_tf.translation.z);
            let to_enemy = enemy_pos - current_cannon_pos;
            let dist_sq = to_enemy.length_squared();
            if dist_sq < 0.25 {
                continue;
            }
            let max_detect_range = heli.cannon_vision_range * 1.25;
            if dist_sq > max_detect_range * max_detect_range {
                continue;
            }
            let dir_to_enemy = to_enemy.normalize_or_zero();
            let in_cannon_range = dist_sq <= heli.cannon_vision_range * heli.cannon_vision_range;
            if in_cannon_range
                && can_see_target_aerial(
                    current_cannon_pos,
                    dir_to_enemy,
                    enemy_pos,
                    heli.cannon_vision_range,
                    std::f32::consts::PI,
                    3.0,
                    &buildings,
                )
            {
                if closest_los_enemy.is_none_or(|(_, best_d2)| dist_sq < best_d2) {
                    closest_los_enemy = Some((enemy_pos, dist_sq));
                }
            } else if closest_occluded_enemy.is_none_or(|(_, best_d2)| dist_sq < best_d2) {
                closest_occluded_enemy = Some((enemy_pos, dist_sq));
            }
        }

        // Helper to rotate `heli.heading` toward a world-space target point
        let rotate_heading_toward = |heli: &mut AttackHelicopter, target_pt: Vec2| {
            let to_pt = target_pt - current_pos;
            if to_pt.length_squared() > 0.25 && !heli.maintain_heading {
                let desired_dir = to_pt.normalize();
                let cur_fwd = heli
                    .heading
                    .normalize_or(LEVEL_4_FRIENDLY_HELICOPTER_FACING);
                let current_yaw = f32::atan2(-cur_fwd.x, -cur_fwd.y);
                let target_yaw = f32::atan2(-desired_dir.x, -desired_dir.y);
                let mut yaw_diff = target_yaw - current_yaw;
                while yaw_diff > std::f32::consts::PI {
                    yaw_diff -= std::f32::consts::TAU;
                }
                while yaw_diff < -std::f32::consts::PI {
                    yaw_diff += std::f32::consts::TAU;
                }
                let max_yaw_step = heli.yaw_rate_rad * dt;
                let new_yaw = if yaw_diff.abs() <= max_yaw_step {
                    target_yaw
                } else {
                    current_yaw + yaw_diff.signum() * max_yaw_step
                };
                heli.heading = Vec2::new(-new_yaw.sin(), -new_yaw.cos())
                    .normalize_or(LEVEL_4_FRIENDLY_HELICOPTER_FACING);
            }
        };

        // 2. Directional flight kinematics: forward (fastest), strafe (intermediate), backward (slowest)
        match heli.order {
            VehicleOrder::MoveTo(target_pos) => {
                let to_target = target_pos - current_pos;
                let dist = to_target.length();
                if dist <= 1.5 {
                    // Arrived at target hover point: brake speeds to zero and rotate toward any detected enemy
                    if let Some((enemy_pos, _)) = closest_los_enemy.or(closest_occluded_enemy) {
                        rotate_heading_toward(&mut heli, enemy_pos);
                    }
                    let decel_step = heli.deceleration * dt;
                    if heli.forward_speed.abs() <= decel_step {
                        heli.forward_speed = 0.0;
                    } else {
                        heli.forward_speed -= heli.forward_speed.signum() * decel_step;
                    }
                    if heli.strafe_speed.abs() <= decel_step {
                        heli.strafe_speed = 0.0;
                    } else {
                        heli.strafe_speed -= heli.strafe_speed.signum() * decel_step;
                    }
                } else {
                    let desired_dir_world = to_target / dist;

                    // When traveling long distance (and heading is not explicitly locked),
                    // turn the helicopter's nose toward the direction of movement and fly forward nose-first
                    // rather than sliding diagonally across the map.
                    let (desired_forward_speed, desired_strafe_speed) =
                        if dist > HELICOPTER_LONG_TRANSIT_DIST && !heli.maintain_heading {
                            rotate_heading_toward(&mut heli, target_pos);
                            let updated_forward = heli.heading;
                            let alignment = desired_dir_world.dot(updated_forward).max(0.0);
                            let arrival_speed = ((2.0 * heli.deceleration * (dist - 0.6).max(0.0))
                                .sqrt())
                            .min(heli.max_forward_speed);
                            (
                                (arrival_speed * alignment * alignment)
                                    .clamp(0.0, heli.max_forward_speed),
                                0.0,
                            )
                        } else {
                            // Short-range tactical adjustment or locked heading: translate via forward/backward/strafe
                            if !heli.maintain_heading {
                                if let Some((enemy_pos, _)) =
                                    closest_los_enemy.or(closest_occluded_enemy)
                                {
                                    rotate_heading_toward(&mut heli, enemy_pos);
                                }
                            }
                            let updated_fwd = heli
                                .heading
                                .normalize_or(LEVEL_4_FRIENDLY_HELICOPTER_FACING);
                            let updated_right = heli.right_dir();
                            let local_forward = desired_dir_world.dot(updated_fwd);
                            let local_right = desired_dir_world.dot(updated_right);
                            let max_dir_speed =
                                heli.max_speed_for_local_dir(local_forward, local_right);
                            let arrival_speed = ((2.0 * heli.deceleration * (dist - 0.6).max(0.0))
                                .sqrt())
                            .min(max_dir_speed);
                            (
                                (local_forward * arrival_speed)
                                    .clamp(-heli.max_backward_speed, heli.max_forward_speed),
                                (local_right * arrival_speed)
                                    .clamp(-heli.max_strafe_speed, heli.max_strafe_speed),
                            )
                        };

                    let forward_diff = desired_forward_speed - heli.forward_speed;
                    let forward_rate = if forward_diff.abs() > 0.01
                        && desired_forward_speed.abs() >= heli.forward_speed.abs()
                    {
                        heli.acceleration
                    } else {
                        heli.deceleration
                    };
                    let max_f_step = forward_rate * dt;
                    if forward_diff.abs() <= max_f_step {
                        heli.forward_speed = desired_forward_speed;
                    } else {
                        heli.forward_speed += forward_diff.signum() * max_f_step;
                    }

                    let strafe_diff = desired_strafe_speed - heli.strafe_speed;
                    let strafe_rate = if strafe_diff.abs() > 0.01
                        && desired_strafe_speed.abs() >= heli.strafe_speed.abs()
                    {
                        heli.acceleration
                    } else {
                        heli.deceleration
                    };
                    let max_s_step = strafe_rate * dt;
                    if strafe_diff.abs() <= max_s_step {
                        heli.strafe_speed = desired_strafe_speed;
                    } else {
                        heli.strafe_speed += strafe_diff.signum() * max_s_step;
                    }
                }
            }
            VehicleOrder::Hold | VehicleOrder::DismountCrew | VehicleOrder::MountCrew => {
                // While holding position, if enemies are detected:
                // 1. Rotate the helicopter's nose toward the detected enemy to bring the limited-arc
                //    chin autocannon into firing position.
                // 2. If no enemy currently has clear LOS because a building's shadow is obstructing
                //    the shot (or the enemy is just outside range), reposition laterally/forward
                //    into a clear firing position.
                let mut reposition_vel_local: Option<(f32, f32)> = None;
                if let Some((enemy_pos, _)) = closest_los_enemy {
                    rotate_heading_toward(&mut heli, enemy_pos);
                } else if let Some((enemy_pos, _)) = closest_occluded_enemy {
                    rotate_heading_toward(&mut heli, enemy_pos);
                    if !heli.maintain_heading {
                        let cur_fwd = heli
                            .heading
                            .normalize_or(LEVEL_4_FRIENDLY_HELICOPTER_FACING);
                        let cur_right = heli.right_dir();
                        let candidates = [cur_right * -1.0, cur_right, cur_fwd, cur_fwd * -1.0];
                        'probe: for step_dist in [4.5_f32, 9.0, 14.0] {
                            for dir_cand in candidates {
                                let probe_heli_pos = current_pos + dir_cand * step_dist;
                                let probe_cannon_pos = heli.cannon_world_pos(probe_heli_pos);
                                let to_e = enemy_pos - probe_cannon_pos;
                                if to_e.length_squared()
                                    <= heli.cannon_vision_range * heli.cannon_vision_range
                                    && can_see_target_aerial(
                                        probe_cannon_pos,
                                        to_e.normalize_or_zero(),
                                        enemy_pos,
                                        heli.cannon_vision_range,
                                        std::f32::consts::PI,
                                        3.0,
                                        &buildings,
                                    )
                                {
                                    let f_comp = dir_cand.dot(cur_fwd) * 6.5;
                                    let s_comp = dir_cand.dot(cur_right) * 6.5;
                                    reposition_vel_local = Some((f_comp, s_comp));
                                    break 'probe;
                                }
                            }
                        }
                    }
                }

                if let Some((desired_f, desired_s)) = reposition_vel_local {
                    let f_diff = desired_f - heli.forward_speed;
                    let max_f = heli.acceleration * dt;
                    if f_diff.abs() <= max_f {
                        heli.forward_speed = desired_f;
                    } else {
                        heli.forward_speed += f_diff.signum() * max_f;
                    }
                    let s_diff = desired_s - heli.strafe_speed;
                    let max_s = heli.acceleration * dt;
                    if s_diff.abs() <= max_s {
                        heli.strafe_speed = desired_s;
                    } else {
                        heli.strafe_speed += s_diff.signum() * max_s;
                    }
                } else {
                    let decel_step = heli.deceleration * dt;
                    if heli.forward_speed.abs() <= decel_step {
                        heli.forward_speed = 0.0;
                    } else {
                        heli.forward_speed -= heli.forward_speed.signum() * decel_step;
                    }
                    if heli.strafe_speed.abs() <= decel_step {
                        heli.strafe_speed = 0.0;
                    } else {
                        heli.strafe_speed -= heli.strafe_speed.signum() * decel_step;
                    }
                }
            }
        }

        // Enforce strict directional speed bounds
        heli.forward_speed = heli
            .forward_speed
            .clamp(-heli.max_backward_speed, heli.max_forward_speed);
        heli.strafe_speed = heli
            .strafe_speed
            .clamp(-heli.max_strafe_speed, heli.max_strafe_speed);

        let vel_2d = heli.velocity_2d();
        let next_pos =
            (current_pos + vel_2d * dt).clamp(Vec2::splat(-world_bound), Vec2::splat(world_bound));
        heli_tf.translation.x = next_pos.x;
        heli_tf.translation.y = heli.hover_altitude;
        heli_tf.translation.z = next_pos.y;

        // Subtle aerodynamic pitch (forward/backward) and roll (strafing)
        let active_forward = heli
            .heading
            .normalize_or(LEVEL_4_FRIENDLY_HELICOPTER_FACING);
        let fuselage_yaw = f32::atan2(-active_forward.x, -active_forward.y);
        let pitch = -(heli.forward_speed / heli.max_forward_speed.max(1.0)) * 0.12;
        let roll = -(heli.strafe_speed / heli.max_strafe_speed.max(1.0)) * 0.14;
        heli_tf.rotation = Quat::from_rotation_y(fuselage_yaw)
            * Quat::from_rotation_x(pitch)
            * Quat::from_rotation_z(roll);

        // 3. Limited-arc chin autocannon target acquisition, slewing, and firing
        //    All target angles and bullet origins are measured from the nose-mounted chin autocannon
        //    (`cannon_mount_pos` / `cannon_muzzle_world_pos`) rather than the center of the helicopter.
        heli.cannon_shoot_timer.tick(time.delta());
        let cannon_mount_pos = heli.cannon_world_pos(next_pos);
        let mut best_in_arc_enemy: Option<(Vec2, f32, f32)> = None;
        let mut best_out_of_arc_enemy: Option<(f32, f32)> = None;

        for (s_tf, s_team) in &soldiers_q {
            if *s_team == *v_team {
                continue;
            }
            let enemy_pos = Vec2::new(s_tf.translation.x, s_tf.translation.z);
            let to_enemy = enemy_pos - cannon_mount_pos;
            let dist_sq = to_enemy.length_squared();
            if dist_sq > heli.cannon_vision_range * heli.cannon_vision_range || dist_sq < 0.25 {
                continue;
            }
            let dir_to_enemy = to_enemy.normalize_or_zero();
            if !can_see_target_aerial(
                cannon_mount_pos,
                dir_to_enemy,
                enemy_pos,
                heli.cannon_vision_range,
                std::f32::consts::PI,
                3.0,
                &buildings,
            ) {
                continue;
            }

            let target_world_yaw = f32::atan2(-dir_to_enemy.x, -dir_to_enemy.y);
            let mut desired_local_yaw = target_world_yaw - fuselage_yaw;
            while desired_local_yaw > std::f32::consts::PI {
                desired_local_yaw -= std::f32::consts::TAU;
            }
            while desired_local_yaw < -std::f32::consts::PI {
                desired_local_yaw += std::f32::consts::TAU;
            }

            if desired_local_yaw.abs() <= heli.cannon_max_yaw_rad {
                if best_in_arc_enemy.is_none_or(|(_, _, best_d2)| dist_sq < best_d2) {
                    best_in_arc_enemy = Some((enemy_pos, desired_local_yaw, dist_sq));
                }
            } else if best_out_of_arc_enemy.is_none_or(|(_, best_d2)| dist_sq < best_d2) {
                best_out_of_arc_enemy = Some((desired_local_yaw, dist_sq));
            }
        }

        let target_local_yaw = if let Some((_, in_arc_yaw, _)) = best_in_arc_enemy {
            in_arc_yaw
        } else if let Some((out_yaw, _)) = best_out_of_arc_enemy {
            // Slew up to the mechanical arc limit toward an out-of-arc enemy, but cannot rotate past it
            out_yaw.clamp(-heli.cannon_max_yaw_rad, heli.cannon_max_yaw_rad)
        } else {
            0.0
        };

        let clamped_target_yaw =
            target_local_yaw.clamp(-heli.cannon_max_yaw_rad, heli.cannon_max_yaw_rad);
        let yaw_diff = clamped_target_yaw - heli.cannon_yaw_local;
        let max_slew_step = heli.cannon_slew_rate_rad * dt;
        if yaw_diff.abs() <= max_slew_step {
            heli.cannon_yaw_local = clamped_target_yaw;
        } else {
            heli.cannon_yaw_local += yaw_diff.signum() * max_slew_step;
        }
        heli.cannon_yaw_local = heli
            .cannon_yaw_local
            .clamp(-heli.cannon_max_yaw_rad, heli.cannon_max_yaw_rad);

        // Fire at visible enemy inside the limited arc once the chin autocannon is aligned
        if let Some((enemy_pos, desired_local_yaw, _)) = best_in_arc_enemy {
            let aim_error = (desired_local_yaw - heli.cannon_yaw_local).abs();
            if aim_error <= 0.18 && heli.cannon_shoot_timer.is_finished() {
                heli.cannon_shoot_timer.reset();
                let cannon_dir = heli.cannon_world_dir();
                let muzzle_pos = heli.cannon_muzzle_world_pos(next_pos);
                let shot_dir = (enemy_pos - muzzle_pos).normalize_or(cannon_dir);
                let bullet_speed = 52.0;
                let target_dist = (enemy_pos - muzzle_pos).length();
                let bullet_lifetime = ((target_dist + 2.2) / bullet_speed).clamp(0.25, 1.5);

                if let Some(ref assets) = bullet_assets {
                    let mat = match *v_team {
                        Team::Player => assets.player_material.clone(),
                        Team::Enemy => assets.enemy_material.clone(),
                    };
                    commands.spawn((
                        LevelEntity,
                        Mesh3d(assets.mesh.clone()),
                        MeshMaterial3d(mat),
                        Transform::from_xyz(muzzle_pos.x, heli.hover_altitude - 0.36, muzzle_pos.y)
                            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                        Bullet {
                            origin: muzzle_pos,
                            direction: shot_dir,
                            speed: bullet_speed,
                            lifetime: bullet_lifetime,
                            team: *v_team,
                            shooter_stance: Stance::Standing,
                            missed_entities: Vec::new(),
                        },
                    ));
                }

                gunshot_writer.write(GunshotEvent {
                    position: muzzle_pos,
                    shooter_team: *v_team,
                });
            }
        }
    }

    for (main_rotor, mut rotor_tf) in &mut main_rotor_query {
        if let Ok((_, heli, _)) = helicopter_query.get(main_rotor.helicopter) {
            rotor_tf.rotation = Quat::from_rotation_y(heli.main_rotor_angle);
        }
    }

    for (tail_rotor, mut tail_tf) in &mut tail_rotor_query {
        if let Ok((_, heli, _)) = helicopter_query.get(tail_rotor.helicopter) {
            tail_tf.rotation =
                Quat::from_rotation_x(heli.tail_rotor_angle) * Quat::from_rotation_z(FRAC_PI_2);
        }
    }

    for (cannon, mut cannon_tf) in &mut cannon_query {
        if let Ok((_, heli, _)) = helicopter_query.get(cannon.helicopter) {
            // Counter-rotate the fuselage's aerodynamic pitch/roll so the gyro-stabilized chin
            // turret and its ground-projected sight cone remain level with the terrain plane.
            let pitch = -(heli.forward_speed / heli.max_forward_speed.max(1.0)) * 0.12;
            let roll = -(heli.strafe_speed / heli.max_strafe_speed.max(1.0)) * 0.14;
            let local_tilt = Quat::from_rotation_x(pitch) * Quat::from_rotation_z(roll);
            cannon_tf.rotation =
                local_tilt.inverse() * Quat::from_rotation_y(heli.cannon_yaw_local);
        }
    }
}

/// Updates every stationary `FieldArtillery` emplacement, its barrel recoil animation, in-flight
/// `ArtilleryShell`s, and medium-circle `ArtilleryExplosion`s on impact:
/// 1. When a friendly Squad Leader calls for support (`SquadLeaderDecision::FixAndSupport { target_center }`),
///    provides `target_center` coordinates to friendly `FieldArtillery` batteries.
/// 2. Rotates the stationary artillery piece very slowly on its central vertical axis (`rotation_speed_rad`)
///    toward `target_coords`.
/// 3. Once aligned within its full-map firing range (`firing_range`) and reloaded (`reload_timer`),
///    fires a 105mm HE `ArtilleryShell` from the muzzle of the long M119A3 barrel.
/// 4. On impact at `target_pos`, the shell explodes in a medium circle (`blast_radius`), eliminating
///    soldiers inside the blast circle, heavily suppressing nearby troops, and spawning a visible
///    medium-circle explosion and crater decal.
pub fn update_field_artillery(
    mut commands: Commands,
    time: Res<Time>,
    squad_doctrine: Option<Res<crate::friendly::SquadDoctrineState>>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    corpse_assets: Option<Res<CorpseAssets>>,
    mut gunshot_writer: MessageWriter<GunshotEvent>,
    mut artillery_query: Query<
        (&Team, &mut FieldArtillery, &mut Transform),
        (
            Without<ArtilleryBarrel>,
            Without<ArtilleryShell>,
            Without<ArtilleryExplosion>,
            Without<AttackHelicopter>,
            Without<HelicopterMainRotor>,
            Without<HelicopterTailRotor>,
            Without<HelicopterAutocannon>,
            Without<ApcVehicle>,
            Without<ApcTurret>,
            Without<Soldier>,
            Without<MountedInVehicle>,
            Without<Building>,
            Without<LowWall>,
        ),
    >,
    mut barrel_query: Query<
        (&ArtilleryBarrel, &mut Transform),
        (
            Without<FieldArtillery>,
            Without<ArtilleryShell>,
            Without<ArtilleryExplosion>,
            Without<AttackHelicopter>,
            Without<HelicopterMainRotor>,
            Without<HelicopterTailRotor>,
            Without<HelicopterAutocannon>,
            Without<ApcVehicle>,
            Without<ApcTurret>,
            Without<Soldier>,
            Without<MountedInVehicle>,
            Without<Building>,
            Without<LowWall>,
        ),
    >,
    mut shell_query: Query<
        (Entity, &mut ArtilleryShell, &mut Transform),
        (
            Without<FieldArtillery>,
            Without<ArtilleryBarrel>,
            Without<ArtilleryExplosion>,
            Without<AttackHelicopter>,
            Without<HelicopterMainRotor>,
            Without<HelicopterTailRotor>,
            Without<HelicopterAutocannon>,
            Without<ApcVehicle>,
            Without<ApcTurret>,
            Without<Soldier>,
            Without<MountedInVehicle>,
            Without<Building>,
            Without<LowWall>,
        ),
    >,
    mut explosion_query: Query<
        (Entity, &mut ArtilleryExplosion, &mut Transform),
        (
            Without<FieldArtillery>,
            Without<ArtilleryBarrel>,
            Without<ArtilleryShell>,
            Without<AttackHelicopter>,
            Without<HelicopterMainRotor>,
            Without<HelicopterTailRotor>,
            Without<HelicopterAutocannon>,
            Without<ApcVehicle>,
            Without<ApcTurret>,
            Without<Soldier>,
            Without<MountedInVehicle>,
            Without<Building>,
            Without<LowWall>,
        ),
    >,
    mut soldiers_q: Query<
        (Entity, &Transform, &Team, &Soldier, &mut Suppression),
        (
            Without<FieldArtillery>,
            Without<ArtilleryBarrel>,
            Without<ArtilleryShell>,
            Without<ArtilleryExplosion>,
            Without<MountedInVehicle>,
        ),
    >,
) {
    let dt = time.delta_secs();
    let sl_support_coords = squad_doctrine
        .as_deref()
        .and_then(|d| d.active_support_coordinates());

    // 1. Update each stationary FieldArtillery piece: receive Squad Leader coordinates, rotate very slowly on axis, and fire
    for (v_team, mut artillery, mut art_tf) in &mut artillery_query {
        artillery.reload_timer.tick(time.delta());
        if artillery.recoil_offset > 0.0 {
            artillery.recoil_offset = (artillery.recoil_offset - 1.2 * dt).max(0.0);
        }

        // Retain living howitzer crew members; require at least 1 living crew soldier to operate the howitzer
        artillery
            .crew
            .retain(|&c_ent| soldiers_q.get(c_ent).is_ok());
        if artillery.crew.is_empty() {
            continue;
        }

        if *v_team == Team::Player {
            if let Some(coords) = sl_support_coords {
                artillery.target_coords = Some(coords);
                artillery.called_by_squad_leader = true;
            } else if artillery.called_by_squad_leader {
                artillery.target_coords = None;
                artillery.called_by_squad_leader = false;
            }
        }

        let artillery_pos = Vec2::new(art_tf.translation.x, art_tf.translation.z);
        let current_fwd = artillery
            .heading
            .normalize_or(LEVEL_4_FRIENDLY_ARTILLERY_FACING);
        let current_yaw = f32::atan2(-current_fwd.x, -current_fwd.y);

        if let Some(target_pos) = artillery.target_coords {
            let to_target = target_pos - artillery_pos;
            let dist = to_target.length();
            if dist >= 1.0 && dist <= artillery.firing_range {
                let desired_dir = to_target / dist;
                let target_yaw = f32::atan2(-desired_dir.x, -desired_dir.y);
                let mut yaw_diff = target_yaw - current_yaw;
                while yaw_diff > std::f32::consts::PI {
                    yaw_diff -= std::f32::consts::TAU;
                }
                while yaw_diff < -std::f32::consts::PI {
                    yaw_diff += std::f32::consts::TAU;
                }

                // Rotate on central axis very slowly (`artillery.rotation_speed_rad`)
                let max_yaw_step = artillery.rotation_speed_rad * dt;
                let new_yaw = if yaw_diff.abs() <= max_yaw_step {
                    target_yaw
                } else {
                    current_yaw + yaw_diff.signum() * max_yaw_step
                };
                artillery.heading = Vec2::new(-new_yaw.sin(), -new_yaw.cos())
                    .normalize_or(LEVEL_4_FRIENDLY_ARTILLERY_FACING);
                art_tf.rotation = Quat::from_rotation_y(new_yaw);

                let remaining_yaw_err = (target_yaw - new_yaw).abs();
                if remaining_yaw_err <= 0.05 && artillery.reload_timer.is_finished() {
                    artillery.reload_timer =
                        Timer::from_seconds(ARTILLERY_RELOAD_SECS, TimerMode::Once);
                    artillery.recoil_offset = 0.65;

                    let muzzle_pos = artillery.muzzle_world_pos(artillery_pos);
                    let flight_dist = (target_pos - muzzle_pos).length().max(0.5);

                    let mut shell_cmd = commands.spawn((
                        LevelEntity,
                        Transform::from_xyz(muzzle_pos.x, 1.45, muzzle_pos.y)
                            .with_rotation(Quat::from_rotation_y(new_yaw)),
                        ArtilleryShell {
                            origin: muzzle_pos,
                            target_pos,
                            speed: ARTILLERY_SHELL_SPEED,
                            total_distance: flight_dist,
                            traveled_distance: 0.0,
                            blast_radius: ARTILLERY_BLAST_RADIUS,
                            team: *v_team,
                        },
                    ));
                    if let (Some(ref mut m_assets), Some(ref mut mat_assets)) =
                        (meshes.as_deref_mut(), materials.as_deref_mut())
                    {
                        let shell_mesh = m_assets.add(Cylinder::new(0.14, 0.72));
                        let shell_mat = mat_assets.add(StandardMaterial {
                            base_color: Color::srgb(0.95, 0.78, 0.25),
                            unlit: true,
                            ..default()
                        });
                        shell_cmd.insert((Mesh3d(shell_mesh), MeshMaterial3d(shell_mat)));
                    }

                    gunshot_writer.write(GunshotEvent {
                        position: muzzle_pos,
                        shooter_team: *v_team,
                    });
                }
            }
        } else {
            art_tf.rotation = Quat::from_rotation_y(current_yaw);
        }
    }

    // 2. Animate hydro-pneumatic recoil on the M119A3 barrel assembly
    for (barrel, mut barrel_tf) in &mut barrel_query {
        if let Ok((_, artillery, _)) = artillery_query.get(barrel.artillery) {
            barrel_tf.translation = Vec3::new(0.0, 0.98, 0.05 + artillery.recoil_offset);
        }
    }

    // 3. Advance in-flight ArtilleryShells along their parabolic trajectory and detonate in a medium circle on impact
    for (shell_entity, mut shell, mut shell_tf) in &mut shell_query {
        shell.traveled_distance += shell.speed * dt;
        let t = (shell.traveled_distance / shell.total_distance.max(0.1)).clamp(0.0, 1.0);
        let ground_pos = shell.origin.lerp(shell.target_pos, t);
        let arc_height = 1.45 + 24.0 * 4.0 * t * (1.0 - t);
        let flight_dir = (shell.target_pos - shell.origin).normalize_or(Vec2::new(0.0, -1.0));
        let flight_yaw = f32::atan2(-flight_dir.x, -flight_dir.y);
        shell_tf.translation = Vec3::new(ground_pos.x, arc_height, ground_pos.y);
        shell_tf.rotation = Quat::from_rotation_y(flight_yaw) * Quat::from_rotation_x(-FRAC_PI_2);

        if shell.traveled_distance >= shell.total_distance {
            let impact_pos = shell.target_pos;
            let blast_r = shell.blast_radius;
            let blast_r_sq = blast_r * blast_r;
            let suppress_r_sq = (blast_r * 2.0) * (blast_r * 2.0);

            // Spawn medium-circle explosion visual effect + scorched crater decal at `impact_pos`
            let mut exp_cmd = commands.spawn((
                LevelEntity,
                NotShadowCaster,
                NotShadowReceiver,
                Transform::from_xyz(impact_pos.x, 0.14, impact_pos.y)
                    .with_rotation(Quat::from_rotation_x(-FRAC_PI_2))
                    .with_scale(Vec3::splat(0.35)),
                ArtilleryExplosion {
                    timer: Timer::from_seconds(0.65, TimerMode::Once),
                    blast_radius: blast_r,
                },
            ));
            if let (Some(ref mut m_assets), Some(ref mut mat_assets)) =
                (meshes.as_deref_mut(), materials.as_deref_mut())
            {
                let blast_circle_mesh = m_assets.add(Circle::new(1.0));
                let blast_mat = mat_assets.add(StandardMaterial {
                    base_color: Color::srgba(1.0, 0.52, 0.12, 0.68),
                    alpha_mode: AlphaMode::Blend,
                    unlit: true,
                    cull_mode: None,
                    ..default()
                });
                exp_cmd.insert((Mesh3d(blast_circle_mesh), MeshMaterial3d(blast_mat)));

                let crater_mesh = m_assets.add(Circle::new(blast_r * 0.52));
                let crater_mat = mat_assets.add(StandardMaterial {
                    base_color: Color::srgba(0.14, 0.13, 0.11, 0.85),
                    alpha_mode: AlphaMode::Blend,
                    perceptual_roughness: 0.98,
                    ..default()
                });
                commands.spawn((
                    LevelEntity,
                    NotShadowCaster,
                    crate::world::ElevationDepression {
                        center: impact_pos,
                        radius: blast_r * 0.55,
                        depth: 1.5,
                    },
                    Mesh3d(crater_mesh),
                    MeshMaterial3d(crater_mat),
                    Transform::from_xyz(impact_pos.x, 0.018, impact_pos.y)
                        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                ));
            }

            // Apply lethal AoE damage within the medium circle (`blast_r`) and heavy suppression nearby
            for (s_entity, s_tf, s_team, _, mut suppression) in &mut soldiers_q {
                let s_pos = Vec2::new(s_tf.translation.x, s_tf.translation.z);
                let dist_sq = (s_pos - impact_pos).length_squared();
                if dist_sq <= blast_r_sq {
                    let mut entity_cmds = commands.entity(s_entity);
                    entity_cmds.despawn_children();
                    entity_cmds
                        .remove::<Soldier>()
                        .remove::<Team>()
                        .remove::<Player>()
                        .remove::<Enemy>()
                        .remove::<Friendly>()
                        .remove::<Zombie>()
                        .remove::<ArtilleryCrewMember>()
                        .insert((
                            Corpse,
                            Visibility::Inherited,
                            Transform::from_xyz(s_pos.x, 0.03, s_pos.y)
                                .with_rotation(s_tf.rotation)
                                .with_scale(Vec3::new(0.65, 0.95, 0.65)),
                        ));
                    if let Some(ref c_assets) = corpse_assets {
                        entity_cmds.insert(MeshMaterial3d(if *s_team == Team::Player {
                            c_assets.player_corpse_material.clone()
                        } else {
                            c_assets.enemy_corpse_material.clone()
                        }));
                        commands.spawn((
                            LevelEntity,
                            Mesh3d(c_assets.blood_pool_mesh.clone()),
                            MeshMaterial3d(c_assets.blood_material.clone()),
                            Transform::from_xyz(s_pos.x, 0.015, s_pos.y)
                                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                            BloodDecal,
                        ));
                    }
                } else if dist_sq <= suppress_r_sq {
                    suppression.add(1.0);
                }
            }

            gunshot_writer.write(GunshotEvent {
                position: impact_pos,
                shooter_team: shell.team,
            });
            commands.entity(shell_entity).despawn();
        }
    }

    // 4. Expand and fade medium-circle ArtilleryExplosions
    for (exp_entity, mut exp, mut exp_tf) in &mut explosion_query {
        exp.timer.tick(time.delta());
        let frac = exp.timer.fraction();
        let radius_scale = exp.blast_radius * (0.35 + 0.65 * frac);
        exp_tf.scale = Vec3::splat(radius_scale);
        if exp.timer.is_finished() {
            commands.entity(exp_entity).despawn();
        }
    }
}

/// Updates the 4 soldiers operating each `FieldArtillery` howitzer (`ArtilleryCrewMember`):
/// - Keeps them stationed behind the howitzer looking along their assigned security sectors so
///   their vision cones reveal the Fog of War around the emplacement.
/// - Allows them to engage any enemies that enter their vision cone using their service rifles.
pub fn update_artillery_crew(
    mut commands: Commands,
    time: Res<Time>,
    bullet_assets: Option<Res<BulletAssets>>,
    mut gunshot_writer: MessageWriter<GunshotEvent>,
    artillery_q: Query<(&Transform, &FieldArtillery), Without<ArtilleryCrewMember>>,
    mut crew_q: Query<
        (
            &ArtilleryCrewMember,
            &mut Transform,
            &Team,
            &mut Soldier,
            Option<&mut Friendly>,
        ),
        (Without<FieldArtillery>, Without<MountedInVehicle>),
    >,
    other_soldiers_q: Query<
        (&Transform, &Team),
        (
            With<Soldier>,
            Without<ArtilleryCrewMember>,
            Without<FieldArtillery>,
            Without<MountedInVehicle>,
        ),
    >,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (
            With<Building>,
            Without<ArtilleryCrewMember>,
            Without<FieldArtillery>,
        ),
    >,
) {
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();

    for (crew, mut c_tf, c_team, mut soldier, mut friendly_opt) in &mut crew_q {
        soldier.shoot_cooldown.tick(time.delta());
        soldier.inventory.tick(time.delta());
        if let Some(ref mut f) = friendly_opt {
            f.shoot_timer.tick(time.delta());
        }

        let c_pos = Vec2::new(c_tf.translation.x, c_tf.translation.z);
        let default_facing = if let Ok((art_tf, art)) = artillery_q.get(crew.artillery) {
            let art_pos = Vec2::new(art_tf.translation.x, art_tf.translation.z);
            let (_, station_facing) = art.crew_station_world_pos_and_facing(art_pos, crew.slot);
            station_facing
        } else {
            soldier.facing_dir
        };

        let vision_range = friendly_opt.as_ref().map_or(24.0, |f| f.vision_range);
        let vision_half_angle = friendly_opt
            .as_ref()
            .map_or(45.0_f32.to_radians(), |f| f.vision_half_angle);
        let inner_radius = friendly_opt.as_ref().map_or(2.0, |f| f.inner_radius);

        let mut nearest_visible_enemy: Option<(Vec2, f32)> = None;
        for (e_tf, e_team) in &other_soldiers_q {
            if *e_team == *c_team {
                continue;
            }
            let e_pos = Vec2::new(e_tf.translation.x, e_tf.translation.z);
            let dist_sq = (e_pos - c_pos).length_squared();
            if dist_sq > vision_range * vision_range {
                continue;
            }
            if can_see_target(
                c_pos,
                soldier.facing_dir,
                e_pos,
                vision_range,
                vision_half_angle,
                inner_radius,
                &buildings,
            ) || can_see_target(
                c_pos,
                default_facing,
                e_pos,
                vision_range,
                vision_half_angle,
                inner_radius,
                &buildings,
            ) {
                if nearest_visible_enemy.is_none_or(|(_, best_d2)| dist_sq < best_d2) {
                    nearest_visible_enemy = Some((e_pos, dist_sq));
                }
            }
        }

        if let Some((enemy_pos, _)) = nearest_visible_enemy {
            let aim_dir = (enemy_pos - c_pos).normalize_or(default_facing);
            soldier.facing_dir = aim_dir;
            let yaw = f32::atan2(-aim_dir.x, -aim_dir.y);
            c_tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2);

            let ready = friendly_opt
                .as_ref()
                .is_none_or(|f| f.shoot_timer.is_finished())
                && soldier.shoot_cooldown.is_finished();
            if ready {
                if soldier.inventory.can_fire() {
                    let fired = soldier
                        .inventory
                        .active_weapon_mut()
                        .is_some_and(|w| w.ammo.try_fire());
                    if fired {
                        soldier.shoot_cooldown.reset();
                        if let Some(ref mut f) = friendly_opt {
                            f.shoot_timer.reset();
                        }
                        if let Some(ref assets) = bullet_assets {
                            let mat = match *c_team {
                                Team::Player => assets.player_material.clone(),
                                Team::Enemy => assets.enemy_material.clone(),
                            };
                            let muzzle = c_pos + aim_dir * (SOLDIER_RADIUS + 0.2);
                            commands.spawn((
                                LevelEntity,
                                Mesh3d(assets.mesh.clone()),
                                MeshMaterial3d(mat),
                                Transform::from_xyz(muzzle.x, 0.08, muzzle.y)
                                    .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                                Bullet {
                                    origin: muzzle,
                                    direction: aim_dir,
                                    speed: 48.0,
                                    lifetime: 1.2,
                                    team: *c_team,
                                    shooter_stance: soldier.stance,
                                    missed_entities: Vec::new(),
                                },
                            ));
                        }
                        gunshot_writer.write(GunshotEvent {
                            position: c_pos,
                            shooter_team: *c_team,
                        });
                    }
                } else if let Some(active) = soldier.inventory.active_weapon_mut() {
                    if active.ammo.current_mag == 0 && active.ammo.reserve_mags > 0 {
                        active.ammo.start_reload();
                    }
                }
            }
        } else {
            soldier.facing_dir = default_facing;
            let yaw = f32::atan2(-default_facing.x, -default_facing.y);
            c_tf.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2);
        }
    }
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
        (
            &mut Transform,
            &mut Soldier,
            &mut Visibility,
            Option<&mut Friendly>,
        ),
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
        (
            &mut Transform,
            &mut Soldier,
            &mut Visibility,
            Option<&mut Friendly>,
        ),
        (With<VehicleCrewMember>, Without<ApcVehicle>),
    >,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (
            With<Building>,
            Without<ApcVehicle>,
            Without<VehicleCrewMember>,
        ),
    >,
    low_walls_q: Query<
        (&Transform, &LowWallCollider),
        (Without<ApcVehicle>, Without<VehicleCrewMember>),
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
        if *v_team == Team::Player
            && !matches!(
                apc.order,
                VehicleOrder::DismountCrew | VehicleOrder::MountCrew
            )
        {
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

/// Resolves full-hull collisions for the `3.4m × 7.6m` APC against buildings, low walls, and trees
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
    for _ in 0..2 {
        for offset in [1.9, 0.0, -1.9] {
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
    }
    pos
}

/// Returns `true` if any of the APC's 3 hull discs still penetrates a building, low wall, or tree
/// trunk collider (which occurs when the APC attempts to wedge between two trees/obstacles closer
/// together than the APC's `3.4m` hull width).
pub fn is_apc_hull_overlapping_obstacles(
    pos: Vec2,
    heading: Vec2,
    buildings: &[(&Transform, &BuildingCollider)],
    low_walls: &[(&Transform, &LowWallCollider)],
) -> bool {
    let fwd = heading.normalize_or(Vec2::new(0.0, -1.0));
    let check_r = APC_HULL_WIDTH * 0.50 - 0.03;
    let check_r_sq = check_r * check_r;
    for offset in [1.9, 0.0, -1.9] {
        let disc_pos = pos + fwd * offset;
        for (b_tf, col) in buildings {
            let c = Vec2::new(b_tf.translation.x, b_tf.translation.z);
            let closest = disc_pos.clamp(c - col.half_size, c + col.half_size);
            if (disc_pos - closest).length_squared() < check_r_sq {
                return true;
            }
        }
        for (w_tf, col) in low_walls {
            let c = Vec2::new(w_tf.translation.x, w_tf.translation.z);
            let closest = disc_pos.clamp(c - col.half_size, c + col.half_size);
            if (disc_pos - closest).length_squared() < check_r_sq {
                return true;
            }
        }
    }
    false
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
    mut turret_q: Query<
        (&ApcTurret, &mut Transform),
        (Without<ApcVehicle>, Without<MountedInVehicle>),
    >,
    mut mounted_crew_q: Query<
        (
            &MountedInVehicle,
            &mut Transform,
            &mut Soldier,
            &mut Visibility,
        ),
        (Without<ApcVehicle>, Without<ApcTurret>),
    >,
    soldiers_q: Query<
        (&Transform, &Team),
        (
            With<Soldier>,
            Without<ApcVehicle>,
            Without<ApcTurret>,
            Without<MountedInVehicle>,
        ),
    >,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (
            With<Building>,
            Without<ApcVehicle>,
            Without<ApcTurret>,
            Without<MountedInVehicle>,
        ),
    >,
    low_walls_q: Query<
        (&Transform, &LowWallCollider),
        (
            Without<ApcVehicle>,
            Without<ApcTurret>,
            Without<MountedInVehicle>,
        ),
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
                            apc.current_speed -=
                                apc.current_speed.signum() * apc.braking_deceleration * dt;
                        }
                    } else {
                        let raw_dir = to_goal / dist;
                        let raw_forward_dot = heading.dot(raw_dir);

                        // Choose reverse gear only when backing straight up into an unobstructed nearby rear point
                        let use_reverse = raw_forward_dot < -0.25
                            && dist <= 24.0
                            && !is_apc_corridor_blocked(pos, raw_dir, dist, &buildings, &low_walls)
                            && !is_apc_corridor_hitting_friendlies(
                                pos,
                                raw_dir,
                                dist,
                                &friendly_troops,
                            );

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
                                apc.current_speed =
                                    (apc.current_speed - decel * dt).max(target_speed);
                            } else {
                                apc.current_speed = (apc.current_speed
                                    + apc.braking_deceleration * dt)
                                    .min(target_speed);
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
                            let sharp_turn_slowdown =
                                if turn_error.abs() > 0.65 { 0.48 } else { 1.0 };
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
                                apc.current_speed =
                                    (apc.current_speed + accel * dt).min(target_speed);
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
            let slope_mult = crate::world::elevation_movement_multiplier(
                pos,
                heading * apc.current_speed.signum(),
                level,
            );
            let next_pos = pos + heading * (apc.current_speed * slope_mult) * dt;
            let resolved = resolve_apc_hull_obstacle_collision(
                next_pos,
                heading,
                &buildings,
                &low_walls,
                world_bound,
            );
            // If `resolved` is still pinched between two closely spaced obstacles (such as forest
            // trees spaced narrower than the 3.4m APC hull), reject the step into the narrow gap.
            if is_apc_hull_overlapping_obstacles(resolved, heading, &buildings, &low_walls)
                && !is_apc_hull_overlapping_obstacles(pos, heading, &buildings, &low_walls)
            {
                apc.current_speed = 0.0;
            } else {
                if (resolved - next_pos).length_squared() > 0.04 {
                    apc.current_speed *= 0.35;
                }
                pos = resolved;
            }
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
                if updated_turret_dir.dot(aim_dir) >= 0.95 && apc.turret_shoot_timer.is_finished() {
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
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (
            With<Building>,
            Without<ApcVehicle>,
            Without<VehicleCrewMember>,
        ),
    >,
    low_walls_q: Query<
        (&Transform, &LowWallCollider),
        (Without<ApcVehicle>, Without<VehicleCrewMember>),
    >,
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
                        if !apc.passengers.contains(&c_ent)
                            && apc.passengers.len() < apc.max_passengers
                        {
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
        (
            With<Soldier>,
            Without<MountedInVehicle>,
            Without<ApcVehicle>,
        ),
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
                        .remove::<ArtilleryCrewMember>()
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
        app.init_resource::<crate::friendly::SquadDoctrineState>();
        app.add_systems(
            Update,
            (
                handle_vehicle_crew_mount_commands,
                update_crewed_apc_vehicles,
                update_dismounted_crew_return,
                resolve_soldier_apc_collisions,
                update_attack_helicopters,
                update_field_artillery,
                update_artillery_crew,
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
            assert_eq!(
                apc.hull_half_size,
                Vec2::new(APC_HULL_WIDTH * 0.5, APC_HULL_LENGTH * 0.5)
            );
            assert!((tf.translation.z - LEVEL_4_FRIENDLY_APC_SPAWN_POS.y).abs() < 0.1);
        }
        assert_eq!(apc_count, 1, "Level 4 should spawn 1 friendly APC");

        let turret_count = app
            .world_mut()
            .query::<&ApcTurret>()
            .iter(app.world())
            .count();
        assert_eq!(
            turret_count, 1,
            "APC must have a circular 360-degree rotating turret"
        );
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

        let mid_z = app
            .world()
            .get::<Transform>(apc_entity)
            .unwrap()
            .translation
            .z;
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

        let rev_z = app
            .world()
            .get::<Transform>(apc_entity)
            .unwrap()
            .translation
            .z;
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
            assert!(
                !apc.has_driver() && !apc.has_gunner(),
                "Driver and Gunner should have exited the APC"
            );
        }
        let dismounted_count = app
            .world_mut()
            .query_filtered::<Entity, (With<VehicleCrewMember>, Without<MountedInVehicle>)>()
            .iter(app.world())
            .count();
        assert_eq!(
            dismounted_count, 2,
            "Both Driver and Gunner should be dismounted on foot"
        );

        // Verify uncrewed APC cannot drive when ordered to MoveTo while Driver is dismounted
        let z_before = app
            .world()
            .get::<Transform>(apc_entity)
            .unwrap()
            .translation
            .z;
        {
            let mut apc = app.world_mut().get_mut::<ApcVehicle>(apc_entity).unwrap();
            apc.order = VehicleOrder::MoveTo(Vec2::new(0.0, 150.0));
        }
        for _ in 0..10 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }
        let z_after = app
            .world()
            .get::<Transform>(apc_entity)
            .unwrap()
            .translation
            .z;
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
        app.world_mut()
            .get_mut::<Transform>(apc_entity)
            .unwrap()
            .translation = Vec3::new(0.0, APC_HULL_HEIGHT * 0.5, 60.0);
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
        app.world_mut()
            .get_mut::<Transform>(apc_entity)
            .unwrap()
            .translation = Vec3::new(0.0, APC_HULL_HEIGHT * 0.5, 58.0);
        {
            let mut apc = app.world_mut().get_mut::<ApcVehicle>(apc_entity).unwrap();
            apc.order = VehicleOrder::MoveTo(Vec2::new(0.0, 20.0));
        }

        let mut max_lateral_detour_x = 0.0_f32;
        for _ in 0..150 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
            let x = app
                .world()
                .get::<Transform>(apc_entity)
                .unwrap()
                .translation
                .x
                .abs();
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
        app.world_mut()
            .get_mut::<Transform>(apc_entity)
            .unwrap()
            .translation = Vec3::new(0.0, APC_HULL_HEIGHT * 0.5, 50.0);

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

        let bullets_fired = app.world_mut().query::<&Bullet>().iter(app.world()).count();
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
        app.world_mut()
            .get_mut::<Transform>(apc_entity)
            .unwrap()
            .translation = Vec3::new(0.0, APC_HULL_HEIGHT * 0.5, 60.0);

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
        app.world_mut()
            .get_mut::<Transform>(apc_entity)
            .unwrap()
            .translation = Vec3::new(0.0, APC_HULL_HEIGHT * 0.5, 50.0);

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

    #[test]
    fn test_level_4_spawns_friendly_attack_helicopter_and_spins_rotors() {
        let mut app = create_vehicle_test_app();
        app.add_systems(Startup, setup_level_vehicles);
        app.update();

        let (heli_entity, heli_tf, heli_team, initial_heli) = {
            let mut q = app
                .world_mut()
                .query::<(Entity, &Transform, &Team, &AttackHelicopter)>();
            let (e, tf, team, h) = q.single(app.world()).unwrap();
            (e, *tf, *team, h.clone())
        };

        assert_eq!(heli_team, Team::Player);
        assert!(
            (initial_heli.heading - LEVEL_4_FRIENDLY_HELICOPTER_FACING).length() < 0.01
                && (initial_heli.hover_altitude - HELICOPTER_HOVER_ALTITUDE).abs() < 0.01
        );
        assert!(
            (heli_tf.translation.x - LEVEL_4_FRIENDLY_HELICOPTER_SPAWN_POS.x).abs() < 0.01
                && (heli_tf.translation.z - LEVEL_4_FRIENDLY_HELICOPTER_SPAWN_POS.y).abs() < 0.01,
            "Attack helicopter should spawn at LEVEL_4_FRIENDLY_HELICOPTER_SPAWN_POS"
        );
        assert!(
            (heli_tf.translation.y - HELICOPTER_HOVER_ALTITUDE).abs() < 0.01,
            "Attack helicopter should hover at HELICOPTER_HOVER_ALTITUDE"
        );

        let initial_main_rotor_rot = app
            .world_mut()
            .query_filtered::<&Transform, With<HelicopterMainRotor>>()
            .single(app.world())
            .unwrap()
            .rotation;
        let initial_tail_rotor_rot = app
            .world_mut()
            .query_filtered::<&Transform, With<HelicopterTailRotor>>()
            .single(app.world())
            .unwrap()
            .rotation;

        // Advance simulation by 50ms and verify both main and tail rotors spin rapidly
        {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
        }
        app.update();

        let updated_heli = app.world().get::<AttackHelicopter>(heli_entity).unwrap();
        assert!(
            updated_heli.main_rotor_angle > initial_heli.main_rotor_angle + 1.0,
            "Main rotor should spin rapidly (expected >1.0 rad in 50ms, got {})",
            updated_heli.main_rotor_angle
        );
        assert!(
            updated_heli.tail_rotor_angle > initial_heli.tail_rotor_angle + 1.5,
            "Tail rotor should spin rapidly (expected >1.5 rad in 50ms, got {})",
            updated_heli.tail_rotor_angle
        );

        let updated_main_rotor_rot = app
            .world_mut()
            .query_filtered::<&Transform, With<HelicopterMainRotor>>()
            .single(app.world())
            .unwrap()
            .rotation;
        let updated_tail_rotor_rot = app
            .world_mut()
            .query_filtered::<&Transform, With<HelicopterTailRotor>>()
            .single(app.world())
            .unwrap()
            .rotation;
        assert!(
            initial_main_rotor_rot.angle_between(updated_main_rotor_rot) > 0.5,
            "Main rotor Transform rotation must update as the blades spin"
        );
        assert!(
            initial_tail_rotor_rot.angle_between(updated_tail_rotor_rot) > 0.5,
            "Tail rotor Transform rotation must update as the blades spin"
        );
    }

    #[test]
    fn test_helicopter_movement_speed_hierarchy_forward_faster_than_strafe_faster_than_backward() {
        let measure_flight = |target_offset: Vec2| -> (f32, f32) {
            let mut app = create_vehicle_test_app();
            app.add_systems(Startup, setup_level_vehicles);
            app.update();

            let heli_entity = app
                .world_mut()
                .query_filtered::<Entity, With<AttackHelicopter>>()
                .single(app.world())
                .unwrap();

            let start_pos = Vec2::new(0.0, 100.0);
            app.world_mut()
                .get_mut::<Transform>(heli_entity)
                .unwrap()
                .translation = Vec3::new(start_pos.x, HELICOPTER_HOVER_ALTITUDE, start_pos.y);
            {
                let mut heli = app
                    .world_mut()
                    .get_mut::<AttackHelicopter>(heli_entity)
                    .unwrap();
                heli.heading = Vec2::new(0.0, -1.0); // Facing North (-Z)
                heli.maintain_heading = true;
                heli.order = VehicleOrder::MoveTo(start_pos + target_offset);
            }

            let mut peak_speed = 0.0_f32;
            for _ in 0..40 {
                {
                    let mut time = app.world_mut().resource_mut::<Time>();
                    time.advance_by(std::time::Duration::from_millis(50));
                }
                app.update();
                let heli = app.world().get::<AttackHelicopter>(heli_entity).unwrap();
                peak_speed = peak_speed.max(heli.velocity_2d().length());
            }

            let end_tf = app.world().get::<Transform>(heli_entity).unwrap();
            let dist_traveled =
                (Vec2::new(end_tf.translation.x, end_tf.translation.z) - start_pos).length();
            (dist_traveled, peak_speed)
        };

        // 1. Forward flight (North along -Z)
        let (forward_dist, forward_peak_speed) = measure_flight(Vec2::new(0.0, -100.0));
        // 2. Side-to-side strafe (East along +X)
        let (strafe_dist, strafe_peak_speed) = measure_flight(Vec2::new(100.0, 0.0));
        // 3. Backward flight (South along +Z)
        let (backward_dist, backward_peak_speed) = measure_flight(Vec2::new(0.0, 100.0));

        assert!(
            backward_dist > 5.0,
            "Helicopter must be able to fly backward, traveled {}",
            backward_dist
        );
        assert!(
            strafe_dist > backward_dist + 3.0 && strafe_peak_speed > backward_peak_speed + 3.0,
            "Strafing (dist={:.2}, peak={:.2}) must be faster than moving backward (dist={:.2}, peak={:.2})",
            strafe_dist,
            strafe_peak_speed,
            backward_dist,
            backward_peak_speed
        );
        assert!(
            forward_dist > strafe_dist + 5.0 && forward_peak_speed > strafe_peak_speed + 5.0,
            "Moving forward (dist={:.2}, peak={:.2}) must be faster than strafing (dist={:.2}, peak={:.2})",
            forward_dist,
            forward_peak_speed,
            strafe_dist,
            strafe_peak_speed
        );
    }

    #[test]
    fn test_helicopter_autocannon_rotates_in_limited_arc_and_fires_at_enemies_in_view() {
        let mut app = create_vehicle_test_app();
        app.add_systems(Startup, setup_level_vehicles);
        app.update();

        let heli_entity = app
            .world_mut()
            .query_filtered::<Entity, With<AttackHelicopter>>()
            .single(app.world())
            .unwrap();
        app.world_mut()
            .get_mut::<Transform>(heli_entity)
            .unwrap()
            .translation = Vec3::new(0.0, HELICOPTER_HOVER_ALTITUDE, 60.0);

        // 1. Spawn an enemy within the chin autocannon's forward arc (~27° off nose at (10.0, 40.0))
        let front_enemy = app
            .world_mut()
            .spawn((
                Transform::from_xyz(10.0, 0.05, 40.0),
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        for _ in 0..15 {
            {
                let mut time = app.world_mut().resource_mut::<Time>();
                time.advance_by(std::time::Duration::from_millis(50));
            }
            app.update();
        }

        let bullets_at_front_enemy: Vec<Vec2> = app
            .world_mut()
            .query::<&Bullet>()
            .iter(app.world())
            .map(|b| b.origin)
            .collect();
        assert!(
            !bullets_at_front_enemy.is_empty(),
            "Helicopter chin autocannon should rotate toward and fire at enemy in its forward arc"
        );
        let heli_center = Vec2::new(0.0, 60.0);
        for origin in &bullets_at_front_enemy {
            let dist_from_center = (*origin - heli_center).length();
            assert!(
                dist_from_center > HELICOPTER_CANNON_MOUNT_FORWARD,
                "Bullets must originate from the nose-mounted autocannon muzzle (>{:.2}m from center), got origin {:?} (dist={:.2}m)",
                HELICOPTER_CANNON_MOUNT_FORWARD,
                origin,
                dist_from_center
            );
        }

        // Despawn the front enemy and existing bullets, then spawn an enemy BEHIND the helicopter
        // at (12.0, 80.0) (~149° off the nose, well outside the ±70° limited arc)
        app.world_mut().despawn(front_enemy);
        let existing_bullets: Vec<Entity> = app
            .world_mut()
            .query_filtered::<Entity, With<Bullet>>()
            .iter(app.world())
            .collect();
        for b in existing_bullets {
            app.world_mut().despawn(b);
        }

        // Lock heading first to verify the autocannon's mechanical ±70° stop in isolation
        {
            let mut heli = app
                .world_mut()
                .get_mut::<AttackHelicopter>(heli_entity)
                .unwrap();
            heli.heading = Vec2::new(0.0, -1.0);
            heli.maintain_heading = true;
        }

        app.world_mut().spawn((
            Transform::from_xyz(12.0, 0.05, 80.0),
            Team::Enemy,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0),
                shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::default(),
            },
        ));

        for _ in 0..20 {
            {
                let mut time = app.world_mut().resource_mut::<Time>();
                time.advance_by(std::time::Duration::from_millis(50));
            }
            app.update();
        }

        let heli = app.world().get::<AttackHelicopter>(heli_entity).unwrap();
        assert!(
            heli.cannon_yaw_local.abs() <= heli.cannon_max_yaw_rad + 1e-4,
            "Autocannon yaw ({:.3} rad) must be strictly clamped to its limited arc (±{:.3} rad)",
            heli.cannon_yaw_local,
            heli.cannon_max_yaw_rad
        );
        let bullets_at_rear_enemy = app.world_mut().query::<&Bullet>().iter(app.world()).count();
        assert_eq!(
            bullets_at_rear_enemy, 0,
            "Helicopter chin autocannon must not fire at enemies behind its limited forward arc while heading is locked"
        );

        // Now unlock heading (`maintain_heading = false`) while holding position: the helicopter
        // should rotate itself toward the detected enemy at (12.0, 80.0) to get in firing position and open fire!
        {
            let mut heli = app
                .world_mut()
                .get_mut::<AttackHelicopter>(heli_entity)
                .unwrap();
            heli.maintain_heading = false;
        }
        for _ in 0..30 {
            {
                let mut time = app.world_mut().resource_mut::<Time>();
                time.advance_by(std::time::Duration::from_millis(50));
            }
            app.update();
        }

        let rotated_heli = app.world().get::<AttackHelicopter>(heli_entity).unwrap();
        let expected_dir = (Vec2::new(12.0, 80.0) - heli_center).normalize();
        assert!(
            rotated_heli.heading.dot(expected_dir) > 0.90,
            "While holding position, helicopter should rotate its heading ({:?}) toward detected enemy ({:?})",
            rotated_heli.heading,
            expected_dir
        );
        let bullets_after_reposition = app.world_mut().query::<&Bullet>().iter(app.world()).count();
        assert!(
            bullets_after_reposition > 0,
            "Helicopter should fire once it has rotated into firing position while holding"
        );
    }

    #[test]
    fn test_helicopter_turns_nose_toward_movement_direction_on_long_distance_order() {
        let mut app = create_vehicle_test_app();
        app.add_systems(Startup, setup_level_vehicles);
        app.update();

        let heli_entity = app
            .world_mut()
            .query_filtered::<Entity, With<AttackHelicopter>>()
            .single(app.world())
            .unwrap();

        let start_pos = Vec2::new(0.0, 120.0);
        let target_pos = Vec2::new(70.0, 50.0); // Northeast at a 45° diagonal, ~99m away
        app.world_mut()
            .get_mut::<Transform>(heli_entity)
            .unwrap()
            .translation = Vec3::new(start_pos.x, HELICOPTER_HOVER_ALTITUDE, start_pos.y);
        {
            let mut heli = app
                .world_mut()
                .get_mut::<AttackHelicopter>(heli_entity)
                .unwrap();
            heli.heading = Vec2::new(0.0, -1.0); // Initially facing due North
            heli.order = VehicleOrder::MoveTo(target_pos);
        }

        for _ in 0..25 {
            {
                let mut time = app.world_mut().resource_mut::<Time>();
                time.advance_by(std::time::Duration::from_millis(50));
            }
            app.update();
        }

        let heli = app.world().get::<AttackHelicopter>(heli_entity).unwrap();
        let heli_tf = app.world().get::<Transform>(heli_entity).unwrap();
        let current_pos = Vec2::new(heli_tf.translation.x, heli_tf.translation.z);
        let desired_dir = (target_pos - current_pos).normalize();

        assert!(
            heli.heading.dot(desired_dir) > 0.98,
            "Helicopter nose heading ({:?}) should rotate to point directly along long-distance travel direction ({:?})",
            heli.heading,
            desired_dir
        );
        assert!(
            heli.strafe_speed.abs() < 0.5 && heli.forward_speed > 10.0,
            "Helicopter should fly forward nose-first (forward_speed={:.2}) without diagonal strafing (strafe_speed={:.2})",
            heli.forward_speed,
            heli.strafe_speed
        );
    }

    #[test]
    fn test_level_4_spawns_friendly_m119a3_field_artillery_with_long_barrel() {
        let mut app = create_vehicle_test_app();
        app.add_systems(Startup, setup_level_vehicles);
        app.update();

        let (art_entity, art_tf, team, artillery) = app
            .world_mut()
            .query::<(Entity, &Transform, &Team, &FieldArtillery)>()
            .single(app.world())
            .expect("Level 4 should spawn 1 friendly M119A3 FieldArtillery piece");

        assert_eq!(*team, Team::Player);
        assert!(
            (art_tf.translation.x - LEVEL_4_FRIENDLY_ARTILLERY_SPAWN_POS.x).abs() < 0.1
                && (art_tf.translation.z - LEVEL_4_FRIENDLY_ARTILLERY_SPAWN_POS.y).abs() < 0.1,
            "FieldArtillery should spawn at LEVEL_4_FRIENDLY_ARTILLERY_SPAWN_POS, got {:?}",
            art_tf.translation
        );
        assert!(
            artillery.firing_range >= crate::common::types::RTS_WORLD_SIZE,
            "FieldArtillery firing range ({}) must cover the entire map ({})",
            artillery.firing_range,
            crate::common::types::RTS_WORLD_SIZE
        );
        assert!(
            artillery.rotation_speed_rad <= 0.8,
            "FieldArtillery must rotate very slowly on its axis (got {} rad/s)",
            artillery.rotation_speed_rad
        );
        assert_eq!(
            artillery.crew.len(),
            4,
            "FieldArtillery should track 4 operating crew soldiers"
        );

        let barrel = app
            .world_mut()
            .query::<&ArtilleryBarrel>()
            .single(app.world())
            .expect("FieldArtillery must have an ArtilleryBarrel child assembly");
        assert_eq!(barrel.artillery, art_entity);

        // Verify the 4 howitzer operating crew soldiers standing behind the howitzer, armed with rifles and vision cones
        let crew_members: Vec<(
            Entity,
            &ArtilleryCrewMember,
            &Transform,
            &Soldier,
            &Friendly,
            &Children,
        )> = app
            .world_mut()
            .query::<(
                Entity,
                &ArtilleryCrewMember,
                &Transform,
                &Soldier,
                &Friendly,
                &Children,
            )>()
            .iter(app.world())
            .collect();
        assert_eq!(
            crew_members.len(),
            4,
            "Expected 4 ArtilleryCrewMember soldiers operating the howitzer"
        );
        for (_, crew, c_tf, soldier, friendly, children) in &crew_members {
            assert_eq!(crew.artillery, art_entity);
            // Since howitzer faces North `(0, -1)`, behind it is `+Z` (`z > LEVEL_4_FRIENDLY_ARTILLERY_SPAWN_POS.y`)
            assert!(
                c_tf.translation.z > LEVEL_4_FRIENDLY_ARTILLERY_SPAWN_POS.y + 1.0,
                "Howitzer crew member (slot {}) should stand behind the howitzer (z > {}), got {:?}",
                crew.slot,
                LEVEL_4_FRIENDLY_ARTILLERY_SPAWN_POS.y,
                c_tf.translation
            );
            assert_eq!(
                soldier.inventory.active_weapon().map(|w| w.weapon_type),
                Some(crate::common::types::WeaponType::Rifle),
                "Howitzer crew member should be armed with a rifle"
            );
            assert!(
                friendly.vision_range >= 20.0,
                "Howitzer crew member should have a Friendly vision cone for revealing Fog of War"
            );
            assert!(
                children.len() >= 2,
                "Howitzer crew member should have direction pointer and vision cone child meshes"
            );
        }
    }

    #[test]
    fn test_field_artillery_rotates_slowly_and_fires_across_entire_map_exploding_in_medium_circle_when_squad_leader_calls_support()
     {
        let mut app = create_vehicle_test_app();
        app.add_systems(Startup, setup_level_vehicles);
        app.update();

        let art_entity = app
            .world_mut()
            .query_filtered::<Entity, With<FieldArtillery>>()
            .single(app.world())
            .unwrap();
        let initial_art_pos = {
            let tf = app.world().get::<Transform>(art_entity).unwrap();
            Vec2::new(tf.translation.x, tf.translation.z)
        };

        // Distant target coordinates far across the map (~550m away to the Northwest)
        let support_coords = Vec2::new(-180.0, -310.0);

        // Spawn two enemies inside the medium explosion circle (`<= ARTILLERY_BLAST_RADIUS = 7.5m`)
        // and one enemy well outside the blast circle (`25m` away)
        let enemy_center = app
            .world_mut()
            .spawn((
                Transform::from_xyz(support_coords.x + 2.0, 0.05, support_coords.y - 1.5),
                Team::Enemy,
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
                    last_pos: support_coords,
                },
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
                Suppression::default(),
            ))
            .id();
        let enemy_edge = app
            .world_mut()
            .spawn((
                Transform::from_xyz(support_coords.x - 5.5, 0.05, support_coords.y + 3.0),
                Team::Enemy,
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
                    last_pos: support_coords,
                },
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
                Suppression::default(),
            ))
            .id();
        let enemy_outside = app
            .world_mut()
            .spawn((
                Transform::from_xyz(support_coords.x + 25.0, 0.05, support_coords.y),
                Team::Enemy,
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
                    last_pos: support_coords,
                },
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
                Suppression::default(),
            ))
            .id();

        // Squad Leader calls for support and provides `support_coords`
        {
            let mut doctrine = app
                .world_mut()
                .resource_mut::<crate::friendly::SquadDoctrineState>();
            doctrine.entry_mut(0).phase = crate::friendly::SquadTacticalPhase::DecisionActive(
                crate::friendly::SquadLeaderDecision::FixAndSupport {
                    target_center: support_coords,
                },
            );
        }

        // 1. After 0.25s (5 ticks), the artillery should still be rotating very slowly on its axis
        //    (has not yet reached alignment and has not fired yet) and must remain stationary
        for _ in 0..5 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }
        let early_shells = app
            .world_mut()
            .query::<&ArtilleryShell>()
            .iter(app.world())
            .count();
        assert_eq!(
            early_shells, 0,
            "FieldArtillery should rotate very slowly on its axis before it can align and fire"
        );

        // 2. Continue advancing until the artillery finishes rotating onto target, fires a shell,
        //    and the shell travels across the map and explodes at `support_coords`
        let mut saw_shell_in_flight = false;
        let mut saw_explosion = false;
        for _ in 0..220 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();

            if app
                .world_mut()
                .query::<&ArtilleryShell>()
                .iter(app.world())
                .next()
                .is_some()
            {
                saw_shell_in_flight = true;
            }
            if app
                .world_mut()
                .query::<&ArtilleryExplosion>()
                .iter(app.world())
                .next()
                .is_some()
            {
                saw_explosion = true;
            }
        }

        let final_art_pos = {
            let tf = app.world().get::<Transform>(art_entity).unwrap();
            Vec2::new(tf.translation.x, tf.translation.z)
        };
        assert!(
            (final_art_pos - initial_art_pos).length() < 1e-4,
            "FieldArtillery must remain stationary on its emplacement"
        );
        assert!(
            saw_shell_in_flight,
            "FieldArtillery should fire an ArtilleryShell once rotated onto the Squad Leader's support coordinates"
        );
        assert!(
            saw_explosion,
            "ArtilleryShell should detonate in a medium-circle ArtilleryExplosion on impact"
        );
        assert!(
            app.world().get::<Corpse>(enemy_center).is_some()
                && app.world().get::<Corpse>(enemy_edge).is_some(),
            "Both enemies inside the medium explosion circle (radius {:.1}m) must be eliminated on impact",
            ARTILLERY_BLAST_RADIUS
        );
        assert!(
            app.world().get::<Soldier>(enemy_outside).is_some(),
            "Enemy well outside the medium blast circle (25m away) should survive"
        );
    }

    #[test]
    fn test_howitzer_crew_vision_cones_reveal_fog_of_war_and_engage_with_rifles() {
        let mut app = create_vehicle_test_app();
        app.init_resource::<Assets<crate::fog::FogMaterial>>();
        app.add_systems(Startup, (crate::fog::setup_fog, setup_level_vehicles));
        app.add_systems(
            Update,
            (
                crate::fog::sync_fog_uniforms,
                crate::enemy::visibility::update_enemy_visibility,
            ),
        );
        app.update();

        // Spawn an enemy directly in front of the howitzer crew's vision cones (`14m` north of the howitzer)
        let enemy_in_cone_pos =
            LEVEL_4_FRIENDLY_ARTILLERY_SPAWN_POS + LEVEL_4_FRIENDLY_ARTILLERY_FACING * 12.0;
        let enemy_ent = app
            .world_mut()
            .spawn((
                Transform::from_xyz(enemy_in_cone_pos.x, 0.05, enemy_in_cone_pos.y),
                Visibility::Hidden,
                Team::Enemy,
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
                    last_pos: enemy_in_cone_pos,
                },
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        for _ in 0..10 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        // Verify the FogMaterial uniform buffer includes the 4 howitzer crew spotters
        let fog_handle = app
            .world_mut()
            .query_filtered::<&MeshMaterial3d<crate::fog::FogMaterial>, With<crate::fog::FogOverlay>>()
            .single(app.world())
            .unwrap()
            .clone();
        let fog_mats = app.world().resource::<Assets<crate::fog::FogMaterial>>();
        let fog_mat = fog_mats.get(&fog_handle).unwrap();
        assert!(
            fog_mat.spotter_count >= 5,
            "FogMaterial spotter_count should include the 4 howitzer crew members, got {}",
            fog_mat.spotter_count
        );

        // Verify the enemy in the howitzer crew's vision cones is revealed through Fog of War
        let enemy_vis = *app.world().get::<Visibility>(enemy_ent).unwrap();
        assert_eq!(
            enemy_vis,
            Visibility::Inherited,
            "Enemy inside the howitzer crew's vision cones should be revealed in the Fog of War"
        );

        // Verify the howitzer crew fires their rifles at the visible enemy
        let rifle_bullets = app.world_mut().query::<&Bullet>().iter(app.world()).count();
        assert!(
            rifle_bullets > 0,
            "Howitzer crew soldiers armed with rifles should fire at an enemy inside their vision cone"
        );
    }

    #[test]
    fn test_level_4_forest_blocks_apc_but_allows_infantry_to_walk_through() {
        let mut app = create_vehicle_test_app();
        app.insert_resource(crate::common::types::SquadCommand::Formation);
        let far_east_goal = Vec2::new(78.0, 2.5);
        app.insert_resource(CommanderOrder::TakePosition(far_east_goal));
        app.add_systems(Startup, crate::world::setup_level_world);
        app.add_systems(Update, crate::friendly::update_friendly);
        app.update();

        // 1. Verify the forest patch spawned on the right (+X) side of the Level 4 map with collision
        let tree_count = app
            .world_mut()
            .query_filtered::<(&Transform, &LowWallCollider), With<crate::world::Tree>>()
            .iter(app.world())
            .map(|(tf, col)| {
                assert!(
                    tf.translation.x > 45.0,
                    "Forest tree should be on the right (+X) side of Level 4 map, got x={}",
                    tf.translation.x
                );
                assert!(
                    col.half_size.x > 0.2 && col.half_size.y > 0.2,
                    "Each tree must have a trunk collider"
                );
            })
            .count();
        assert!(
            tree_count >= 80,
            "Expected dense forest patch on the right side of Level 4, got {} trees",
            tree_count
        );

        // 2. Spawn an APC just west of the forest (`x = 43.0, z = 2.5`) facing East `(1, 0)` and
        //    order it to drive to `far_east_goal = (78.0, 2.5)` (straight through the forest)
        let apc_entity = {
            let mut commands = app.world_mut().commands();
            let mut meshes = Assets::<Mesh>::default();
            let mut materials = Assets::<StandardMaterial>::default();
            let (apc_ent, _, _) = spawn_apc_with_crew(
                &mut commands,
                &mut meshes,
                &mut materials,
                Team::Player,
                Vec2::new(43.0, 2.5),
                Vec2::new(1.0, 0.0),
            );
            apc_ent
        };
        app.update();
        app.world_mut()
            .get_mut::<ApcVehicle>(apc_entity)
            .unwrap()
            .order = VehicleOrder::MoveTo(Vec2::new(60.0, 2.5));

        // 3. Spawn an infantry squad leader just west of the forest (`x = 45.5, z = 2.5`)
        //    ordered to walk straight through the forest to `far_east_goal = (78.0, 2.5)`
        let infantry_ent = app
            .world_mut()
            .spawn((
                Transform::from_xyz(45.5, 0.05, 2.5),
                Friendly {
                    fireteam: 1,
                    slot: 0,
                    state: FriendlyAiState::Follow,
                    shoot_timer: Timer::from_seconds(0.2, TimerMode::Repeating),
                    speed: 10.0,
                    vision_range: 24.0,
                    vision_half_angle: 45.0_f32.to_radians(),
                    inner_radius: 2.0,
                    target_pos: None,
                    target_lost_timer: 0.0,
                },
                Team::Player,
                crate::friendly::SquadId(0),
                Soldier {
                    facing_dir: Vec2::new(1.0, 0.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
                Suppression::default(),
            ))
            .id();

        let mut apc_entered_forest_interior = false;
        for _ in 0..120 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();

            let apc_tf = app.world().get::<Transform>(apc_entity).unwrap();
            // Forest interior spans `X in [50.0, 72.0], Z in [-16.0, 24.0]`
            if apc_tf.translation.x > 50.0
                && apc_tf.translation.x < 72.0
                && apc_tf.translation.z > -16.0
                && apc_tf.translation.z < 24.0
            {
                apc_entered_forest_interior = true;
            }
        }

        assert!(
            !apc_entered_forest_interior,
            "APC must not be able to drive through the dense forest interior"
        );

        let inf_tf = app.world().get::<Transform>(infantry_ent).unwrap();
        assert!(
            inf_tf.translation.x > 73.0,
            "Infantry soldier should be able to walk right through the forest to the far side (x > 73.0), got {:?}",
            inf_tf.translation
        );
    }
}
