use bevy::prelude::*;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum EnemyAiState {
    Search,
    Engage,
    Investigate,
}

pub const ENEMY_WAYPOINTS: [Vec2; 8] = [
    Vec2::new(-35.0, -25.0), // 0: Northwest Avenue (unobstructed road west of North HQ)
    Vec2::new(0.0, -25.0),   // 1: North Central Plaza intersection
    Vec2::new(35.0, -25.0),  // 2: Northeast Avenue (unobstructed road east of Radar Bunker)
    Vec2::new(35.0, 5.0),    // 3: East Logistics Road (unobstructed road east of Warehouse)
    Vec2::new(35.0, 32.0),   // 4: Southeast Armory Road
    Vec2::new(0.0, 32.0),    // 5: South Checkpoint Gate
    Vec2::new(-35.0, 32.0),  // 6: Southwest Motor Pool Road
    Vec2::new(-35.0, 5.0),   // 7: West Barracks Road (unobstructed road west of Barracks)
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
    pub stuck_timer: f32,
    pub last_pos: Vec2,
}

