pub mod ai;
pub mod systems;
pub mod visibility;

use bevy::prelude::*;
use crate::common::types::AppState;
pub use systems::*;
pub use visibility::*;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_enemy)
            .add_systems(
                Update,
                (update_enemy, update_zombies, update_enemy_visibility)
                    .run_if(in_state(AppState::InGame)),
            );
    }
}
