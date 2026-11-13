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
    Level3,
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
pub const WEAPON_SWAP_DURATION_SECS: f32 = 2.0;
pub const PISTOL_MAG_CAPACITY: u32 = 17;
pub const PISTOL_TOTAL_MAGAZINES: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WeaponType {
    Rifle,
    Pistol,
}

impl WeaponType {
    pub fn name(&self) -> &'static str {
        match self {
            WeaponType::Rifle => "SERVICE RIFLE",
            WeaponType::Pistol => "SIDEARM PISTOL",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            WeaponType::Rifle => "SERVICE RIFLE  •  5.56mm",
            WeaponType::Pistol => "SIDEARM PISTOL  •  9mm",
        }
    }

    pub fn mag_capacity(&self) -> u32 {
        match self {
            WeaponType::Rifle => MAG_CAPACITY,
            WeaponType::Pistol => PISTOL_MAG_CAPACITY,
        }
    }

    pub fn bullet_speed(&self) -> f32 {
        match self {
            WeaponType::Rifle => 45.0,
            WeaponType::Pistol => 40.0,
        }
    }

    pub fn bullet_lifetime(&self) -> f32 {
        match self {
            WeaponType::Rifle => 1.5, // 67.5m range
            WeaponType::Pistol => 0.65, // 26.0m range (reduced range compared to rifle)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeaponAmmo {
    pub current_mag: u32,
    pub reserve_mags: u32,
    pub mag_capacity: u32,
    pub reload_timer: Option<Timer>,
}

impl Default for WeaponAmmo {
    fn default() -> Self {
        Self::new(MAG_CAPACITY, TOTAL_MAGAZINES)
    }
}

#[allow(dead_code)]
impl WeaponAmmo {
    pub fn new(mag_capacity: u32, total_mags: u32) -> Self {
        Self {
            current_mag: mag_capacity,
            reserve_mags: total_mags.saturating_sub(1),
            mag_capacity,
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
        if self.reserve_mags > 0 && self.current_mag < self.mag_capacity && self.reload_timer.is_none() {
            self.reload_timer = Some(Timer::from_seconds(RELOAD_DURATION_SECS, TimerMode::Once));
            true
        } else {
            false
        }
    }

    pub fn cancel_reload(&mut self) {
        self.reload_timer = None;
    }

    pub fn tick_reload(&mut self, dt: std::time::Duration) -> bool {
        if let Some(ref mut timer) = self.reload_timer {
            timer.tick(dt);
            if timer.is_finished() {
                self.reload_timer = None;
                if self.reserve_mags > 0 {
                    self.reserve_mags -= 1;
                    self.current_mag = self.mag_capacity;
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

    pub fn is_completely_empty(&self) -> bool {
        self.current_mag == 0 && self.reserve_mags == 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Weapon {
    pub weapon_type: WeaponType,
    pub ammo: WeaponAmmo,
}

impl Weapon {
    pub fn new(weapon_type: WeaponType, total_mags: u32) -> Self {
        Self {
            weapon_type,
            ammo: WeaponAmmo::new(weapon_type.mag_capacity(), total_mags),
        }
    }

    pub fn rifle() -> Self {
        Self::new(WeaponType::Rifle, TOTAL_MAGAZINES)
    }

    pub fn pistol() -> Self {
        Self::new(WeaponType::Pistol, PISTOL_TOTAL_MAGAZINES)
    }

    pub fn bullet_speed(&self) -> f32 {
        self.weapon_type.bullet_speed()
    }

    pub fn bullet_lifetime(&self) -> f32 {
        self.weapon_type.bullet_lifetime()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Inventory {
    pub weapons: Vec<Weapon>,
    pub active_index: usize,
    pub swap_timer: Option<Timer>,
    pub pending_swap_index: Option<usize>,
}

impl Default for Inventory {
    fn default() -> Self {
        Self::rifle_only()
    }
}

#[allow(dead_code)]
impl Inventory {
    pub fn new(weapons: Vec<Weapon>) -> Self {
        Self {
            weapons,
            active_index: 0,
            swap_timer: None,
            pending_swap_index: None,
        }
    }

    pub fn player_default() -> Self {
        Self::new(vec![Weapon::rifle(), Weapon::pistol()])
    }

    pub fn rifle_only() -> Self {
        Self::new(vec![Weapon::rifle()])
    }

    pub fn active_weapon(&self) -> Option<&Weapon> {
        self.weapons.get(self.active_index)
    }

    pub fn active_weapon_mut(&mut self) -> Option<&mut Weapon> {
        self.weapons.get_mut(self.active_index)
    }

    pub fn is_swapping(&self) -> bool {
        self.swap_timer.is_some()
    }

    pub fn swap_progress(&self) -> Option<f32> {
        self.swap_timer.as_ref().map(|t| t.fraction())
    }

    pub fn swap_remaining_secs(&self) -> Option<f32> {
        self.swap_timer.as_ref().map(|t| t.remaining_secs())
    }

    pub fn pending_weapon(&self) -> Option<&Weapon> {
        if let Some(idx) = self.pending_swap_index {
            self.weapons.get(idx)
        } else {
            self.active_weapon()
        }
    }

    pub fn start_swap(&mut self) -> bool {
        if self.weapons.len() > 1 && !self.is_swapping() {
            let next_index = (self.active_index + 1) % self.weapons.len();
            if let Some(active) = self.active_weapon_mut() {
                active.ammo.cancel_reload();
            }
            self.pending_swap_index = Some(next_index);
            self.swap_timer = Some(Timer::from_seconds(WEAPON_SWAP_DURATION_SECS, TimerMode::Once));
            true
        } else {
            false
        }
    }

    pub fn tick(&mut self, dt: std::time::Duration) {
        if let Some(ref mut timer) = self.swap_timer {
            timer.tick(dt);
            if timer.is_finished() {
                if let Some(target) = self.pending_swap_index.take() {
                    self.active_index = target;
                }
                self.swap_timer = None;
            }
        } else if let Some(active) = self.active_weapon_mut() {
            active.ammo.tick_reload(dt);
        }
    }

    pub fn has_sidearm(&self) -> bool {
        self.weapons.iter().any(|w| w.weapon_type == WeaponType::Pistol)
    }

    pub fn can_fire(&self) -> bool {
        !self.is_swapping() && self.active_weapon().map(|w| w.ammo.can_fire()).unwrap_or(false)
    }
}

#[derive(Component, Debug, Clone)]
pub struct Soldier {
    pub facing_dir: Vec2,
    pub shoot_cooldown: Timer,
    pub stance: Stance,
    pub inventory: Inventory,
}

impl Default for Soldier {
    fn default() -> Self {
        Self {
            facing_dir: Vec2::new(0.0, -1.0),
            shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
            stance: Stance::Standing,
            inventory: Inventory::default(),
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
            inventory: Inventory::default(),
        }
    }

    pub fn with_inventory(mut self, inventory: Inventory) -> Self {
        self.inventory = inventory;
        self
    }

    pub fn active_weapon(&self) -> Option<&Weapon> {
        self.inventory.active_weapon()
    }

    pub fn active_weapon_mut(&mut self) -> Option<&mut Weapon> {
        self.inventory.active_weapon_mut()
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
            inventory: Inventory::default(),
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

    #[test]
    fn test_player_inventory_rifle_and_pistol_swap() {
        let mut inv = Inventory::player_default();
        assert_eq!(inv.weapons.len(), 2);
        assert_eq!(inv.active_weapon().unwrap().weapon_type, WeaponType::Rifle);
        assert_eq!(inv.active_weapon().unwrap().ammo.current_mag, 30);
        assert_eq!(inv.active_weapon().unwrap().ammo.reserve_mags, 6);
        assert!(inv.has_sidearm());

        // Check pistol properties: 1 magazine of 17 rounds, reduced range
        let pistol = &inv.weapons[1];
        assert_eq!(pistol.weapon_type, WeaponType::Pistol);
        assert_eq!(pistol.ammo.current_mag, 17);
        assert_eq!(pistol.ammo.reserve_mags, 0);
        assert!(pistol.bullet_speed() * pistol.bullet_lifetime() < 30.0);
        assert!(inv.weapons[0].bullet_speed() * inv.weapons[0].bullet_lifetime() > 60.0);

        // Initiate swap to pistol
        assert!(inv.start_swap());
        assert!(inv.is_swapping());
        assert!(!inv.can_fire()); // Cannot fire while swapping weapons
        assert_eq!(inv.pending_weapon().unwrap().weapon_type, WeaponType::Pistol);

        // Advance 1.0s (halfway through 2.0s swap)
        inv.tick(std::time::Duration::from_millis(1000));
        assert!(inv.is_swapping());
        assert_eq!(inv.active_weapon().unwrap().weapon_type, WeaponType::Rifle); // Still rifle until swap finishes

        // Advance another 1.1s (completes 2.0s swap)
        inv.tick(std::time::Duration::from_millis(1100));
        assert!(!inv.is_swapping());
        assert_eq!(inv.active_weapon().unwrap().weapon_type, WeaponType::Pistol);
        assert!(inv.can_fire());
        assert_eq!(inv.active_weapon().unwrap().ammo.current_mag, 17);

        // Fire 1 pistol shot
        inv.active_weapon_mut().unwrap().ammo.try_fire();
        assert_eq!(inv.active_weapon().unwrap().ammo.current_mag, 16);

        // Swap back to rifle
        assert!(inv.start_swap());
        inv.tick(std::time::Duration::from_millis(2100));
        assert_eq!(inv.active_weapon().unwrap().weapon_type, WeaponType::Rifle);
        assert_eq!(inv.active_weapon().unwrap().ammo.current_mag, 30); // Rifle ammo preserved!
    }
}


