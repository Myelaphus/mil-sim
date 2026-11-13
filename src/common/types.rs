use bevy::prelude::*;

// World dimensions and derived boundary constants (Single source of truth)
pub const WORLD_SIZE: f32 = 120.0;
pub const WORLD_HALF_SIZE: f32 = WORLD_SIZE * 0.5;

pub const SOLDIER_RADIUS: f32 = 0.6;
pub const BULLET_RADIUS: f32 = 0.14;

// Soldier movement clamp margin: derived directly from world dimensions and soldier physical radius
pub const SOLDIER_WORLD_BOUND: f32 = WORLD_HALF_SIZE - (SOLDIER_RADIUS * 2.5);

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, States)]
pub enum AppState {
    #[default]
    MainMenu,
    InGame,
}

#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CurrentLevel {
    #[default]
    Level1,
    Level2,
}

#[derive(Component)]
pub struct LevelEntity;

#[derive(Message, Clone, Copy, Debug)]
pub struct GunshotEvent {
    pub position: Vec2,
    pub shooter_team: Team,
}

#[derive(Component, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Team {
    Player,
    Enemy,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum Stance {
    #[default]
    Standing,
    Crouching,
    Prone,
}

// Standard weapon loadout constants
pub const MAG_CAPACITY: u32 = 30;
pub const TOTAL_MAGAZINES: u32 = 7;
pub const RELOAD_DURATION_SECS: f32 = 2.0;

#[derive(Component, Clone, Debug, PartialEq)]
pub struct WeaponAmmo {
    pub current_mag: u32,
    pub reserve_mags: u32,
    pub reload_timer: Option<Timer>,
}

impl Default for WeaponAmmo {
    fn default() -> Self {
        Self {
            current_mag: MAG_CAPACITY,
            reserve_mags: TOTAL_MAGAZINES.saturating_sub(1), // 1 loaded in weapon + 6 reserve = 7 total
            reload_timer: None,
        }
    }
}

#[allow(dead_code)]
impl WeaponAmmo {
    pub fn new(mag_capacity: u32, total_mags: u32) -> Self {
        Self {
            current_mag: mag_capacity,
            reserve_mags: total_mags.saturating_sub(1),
            reload_timer: None,
        }
    }

    pub fn is_reloading(&self) -> bool {
        self.reload_timer.is_some()
    }

    pub fn can_fire(&self) -> bool {
        self.current_mag > 0 && !self.is_reloading()
    }

    pub fn try_fire(&mut self) -> bool {
        if self.can_fire() {
            self.current_mag -= 1;
            true
        } else {
            false
        }
    }

    pub fn start_reload(&mut self) -> bool {
        if self.reserve_mags > 0 && self.current_mag < MAG_CAPACITY && self.reload_timer.is_none() {
            self.reload_timer = Some(Timer::from_seconds(RELOAD_DURATION_SECS, TimerMode::Once));
            true
        } else {
            false
        }
    }

    pub fn tick_reload(&mut self, dt: std::time::Duration) -> bool {
        if let Some(ref mut timer) = self.reload_timer {
            timer.tick(dt);
            if timer.is_finished() {
                self.reload_timer = None;
                if self.reserve_mags > 0 {
                    self.reserve_mags -= 1;
                    self.current_mag = MAG_CAPACITY;
                }
                return true;
            }
        }
        false
    }

    pub fn reload_remaining_secs(&self) -> Option<f32> {
        self.reload_timer.as_ref().map(|t| t.remaining_secs())
    }

    pub fn reload_progress(&self) -> Option<f32> {
        self.reload_timer.as_ref().map(|t| t.fraction())
    }
}

#[derive(Component, Debug, Clone)]
pub struct Soldier {
    pub facing_dir: Vec2,
    pub shoot_cooldown: Timer,
    pub stance: Stance,
    pub ammo: WeaponAmmo,
}

impl Default for Soldier {
    fn default() -> Self {
        Self {
            facing_dir: Vec2::new(0.0, -1.0),
            shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
            stance: Stance::Standing,
            ammo: WeaponAmmo::default(),
        }
    }
}

#[derive(Component)]
pub struct Corpse;

#[derive(Component)]
pub struct BloodDecal;

#[allow(dead_code)]
impl Soldier {
    pub fn new(facing_dir: Vec2, shoot_cooldown: Timer, stance: Stance) -> Self {
        Self {
            facing_dir,
            shoot_cooldown,
            stance,
            ammo: WeaponAmmo::default(),
        }
    }

    pub fn is_crouched(&self) -> bool {
        self.stance == Stance::Crouching
    }

    pub fn is_prone(&self) -> bool {
        self.stance == Stance::Prone
    }

    pub fn is_standing(&self) -> bool {
        self.stance == Stance::Standing
    }

    pub fn directional_speed_multiplier(&self, move_dir: Vec2) -> f32 {
        crate::common::geometry::directional_speed_multiplier(self.facing_dir, move_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soldier_stance_helpers() {
        let mut soldier = Soldier {
            facing_dir: Vec2::Y,
            shoot_cooldown: Timer::from_seconds(1.0, TimerMode::Once),
            stance: Stance::Standing,
            ammo: WeaponAmmo::default(),
        };

        assert!(soldier.is_standing());
        assert!(!soldier.is_crouched());
        assert!(!soldier.is_prone());

        soldier.stance = Stance::Crouching;
        assert!(!soldier.is_standing());
        assert!(soldier.is_crouched());
        assert!(!soldier.is_prone());

        soldier.stance = Stance::Prone;
        assert!(!soldier.is_standing());
        assert!(!soldier.is_crouched());
        assert!(soldier.is_prone());
    }

    #[test]
    fn test_weapon_ammo_initialization_and_firing() {
        let mut ammo = WeaponAmmo::default();
        assert_eq!(ammo.current_mag, 30);
        assert_eq!(ammo.reserve_mags, 6);
        assert!(!ammo.is_reloading());
        assert!(ammo.can_fire());

        // Fire 10 rounds
        for _ in 0..10 {
            assert!(ammo.try_fire());
        }
        assert_eq!(ammo.current_mag, 20);
        assert_eq!(ammo.reserve_mags, 6);

        // Fire remaining 20 rounds
        for _ in 0..20 {
            assert!(ammo.try_fire());
        }
        assert_eq!(ammo.current_mag, 0);
        assert!(!ammo.can_fire());
        assert!(!ammo.try_fire()); // Cannot fire when mag is empty
    }

    #[test]
    fn test_weapon_ammo_reloading_and_timing() {
        let mut ammo = WeaponAmmo::default();
        ammo.current_mag = 0;
        assert_eq!(ammo.reserve_mags, 6);

        // Start reload
        assert!(ammo.start_reload());
        assert!(ammo.is_reloading());
        assert!(!ammo.can_fire());

        // Advance 1 second (halfway through 2.0s reload)
        let finished = ammo.tick_reload(std::time::Duration::from_millis(1000));
        assert!(!finished);
        assert!(ammo.is_reloading());
        assert!(!ammo.can_fire());
        let remaining = ammo.reload_remaining_secs().unwrap();
        assert!((remaining - 1.0).abs() < 0.05);

        // Advance another 1.1 seconds (completes reload)
        let finished = ammo.tick_reload(std::time::Duration::from_millis(1100));
        assert!(finished);
        assert!(!ammo.is_reloading());
        assert!(ammo.can_fire());
        assert_eq!(ammo.current_mag, 30);
        assert_eq!(ammo.reserve_mags, 5); // Consumed 1 reserve magazine
    }
}


