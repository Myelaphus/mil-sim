pub mod fps;

use bevy::prelude::*;
pub use fps::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_fps_ui)
            .add_systems(Update, update_fps);
    }
}

