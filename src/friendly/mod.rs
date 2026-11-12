pub mod ai;
pub mod systems;

use bevy::prelude::*;
use crate::common::types::AppState;
pub use ai::*;
pub use systems::*;

pub struct FriendlyPlugin;

impl Plugin for FriendlyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), setup_friendly)
            .add_systems(Update, update_friendly.run_if(in_state(AppState::InGame)));
    }
}

