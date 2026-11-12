pub mod systems;

use bevy::prelude::*;
use crate::common::types::AppState;
pub use systems::*;

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_player)
            .add_systems(Update, update_player.run_if(in_state(AppState::InGame)));
    }
}

