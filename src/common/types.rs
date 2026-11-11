use bevy::prelude::*;

#[derive(Component, PartialEq, Eq, Clone, Copy, Debug)]
pub enum Team {
    Player,
    Enemy,
}

#[derive(Component)]
pub struct Soldier {
    pub facing_dir: Vec2,
    pub shoot_cooldown: Timer,
    pub is_crouched: bool,
}

