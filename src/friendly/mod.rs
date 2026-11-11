pub mod ai;
pub mod systems;

use bevy::prelude::*;
pub use ai::*;
pub use systems::*;

pub struct FriendlyPlugin;

impl Plugin for FriendlyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_friendly)
            .add_systems(Update, update_friendly);
    }
}

