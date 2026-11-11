use bevy::prelude::*;

// World dimensions and derived boundary constants (Single source of truth)
pub const WORLD_SIZE: f32 = 120.0;
pub const WORLD_HALF_SIZE: f32 = WORLD_SIZE * 0.5;

pub const SOLDIER_RADIUS: f32 = 0.6;
pub const BULLET_RADIUS: f32 = 0.14;

// Soldier movement clamp margin: derived directly from world dimensions and soldier physical radius
pub const SOLDIER_WORLD_BOUND: f32 = WORLD_HALF_SIZE - (SOLDIER_RADIUS * 2.5);

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

#[derive(Component)]
pub struct Soldier {
    pub facing_dir: Vec2,
    pub shoot_cooldown: Timer,
    pub stance: Stance,
}

#[derive(Component)]
pub struct Corpse;

#[derive(Component)]
pub struct BloodDecal;

#[allow(dead_code)]
impl Soldier {
    pub fn is_crouched(&self) -> bool {
        self.stance == Stance::Crouching
    }

    pub fn is_prone(&self) -> bool {
        self.stance == Stance::Prone
    }

    pub fn is_standing(&self) -> bool {
        self.stance == Stance::Standing
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
}


