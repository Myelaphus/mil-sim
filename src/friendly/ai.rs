use bevy::prelude::*;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum FriendlyAiState {
    #[default]
    Follow,
    Engage,
}

#[derive(Component, Debug, Clone)]
pub struct Friendly {
    pub fireteam: usize, // 1 = First Fire Team (Alpha), 2 = Second Fire Team (Bravo)
    pub slot: usize,     // Slot within the fire team
    pub state: FriendlyAiState,
    pub shoot_timer: Timer,
    pub speed: f32,
    pub vision_range: f32,
    pub vision_half_angle: f32,
    pub inner_radius: f32,
    pub target_pos: Option<Vec2>,
    pub target_lost_timer: f32,
}

