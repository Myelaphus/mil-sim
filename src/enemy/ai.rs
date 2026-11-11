use bevy::prelude::*;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum EnemyAiState {
    Search,
    Engage,
    Investigate,
}

pub const ENEMY_WAYPOINTS: [Vec2; 8] = [
    Vec2::new(-20.0, -18.0), // Northwest outer corner
    Vec2::new(-20.0, 0.0),   // West outer road
    Vec2::new(0.0, 0.0),     // Central courtyard
    Vec2::new(0.0, 16.0),    // South courtyard approach
    Vec2::new(17.0, 16.0),   // Southeast outer road
    Vec2::new(17.0, 0.0),    // East outer road
    Vec2::new(0.0, 0.0),     // Central courtyard
    Vec2::new(0.0, -18.0),   // North post
];

#[derive(Component)]
pub struct Enemy {
    pub state: EnemyAiState,
    pub current_waypoint_idx: usize,
    pub last_known_player_pos: Option<Vec2>,
    pub investigate_timer: Timer,
    pub shoot_timer: Timer,
    pub speed: f32,
    pub vision_range: f32,
    pub vision_half_angle: f32,
    pub inner_radius: f32,
}

