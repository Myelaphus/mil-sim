pub mod ammo;
pub mod fps;
pub mod menu;

use bevy::prelude::*;
use crate::common::types::AppState;
pub use ammo::*;
pub use fps::*;
pub use menu::*;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LevelEndManager>()
            .add_systems(Startup, setup_fps_ui)
            .add_systems(Update, update_fps)
            .add_systems(OnEnter(AppState::MainMenu), setup_main_menu)
            .add_systems(
                Update,
                update_main_menu_interactions.run_if(in_state(AppState::MainMenu)),
            )
            .add_systems(OnExit(AppState::MainMenu), cleanup_main_menu)
            .add_systems(OnEnter(AppState::InGame), setup_ammo_ui)
            .add_systems(
                Update,
                (
                    check_level_end_conditions,
                    update_ammo_ui,
                )
                    .run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), cleanup_level);
    }
}

