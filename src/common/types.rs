use bevy::prelude::*;

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


