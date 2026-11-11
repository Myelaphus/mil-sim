use bevy::prelude::*;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum FriendlyAiState {
    #[default]
    Follow,
    Engage,
}

#[derive(Component)]
pub struct Friendly {
    pub slot: usize, // 0 = Left flank, 1 = Right flank
    pub state: FriendlyAiState,
    pub shoot_timer: Timer,
    pub speed: f32,
    pub vision_range: f32,
    pub vision_half_angle: f32,
    pub inner_radius: f32,
    pub target_pos: Option<Vec2>,
    pub target_lost_timer: f32,
}

