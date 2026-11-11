pub mod ai;
pub mod systems;
pub mod visibility;

use bevy::prelude::*;
pub use systems::*;
pub use visibility::*;

pub struct EnemyPlugin;

impl Plugin for EnemyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_enemy)
            .add_systems(Update, (update_enemy, update_enemy_visibility));
    }
}
