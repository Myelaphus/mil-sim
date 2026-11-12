mod combat;
mod common;
mod enemy;
mod fog;
mod friendly;
mod player;
mod ui;
mod world;

use bevy::prelude::*;
use combat::CombatPlugin;
use common::geometry::resolve_soldier_pairwise_collisions;
use common::types::AppState;
use enemy::EnemyPlugin;
use fog::FogPlugin;
use friendly::FriendlyPlugin;
use player::PlayerPlugin;
use ui::UiPlugin;
use world::WorldPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Mil-Sim - Tactical Operations".to_string(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .init_state::<AppState>()
        .add_plugins((
            WorldPlugin,
            FogPlugin,
            CombatPlugin,
            PlayerPlugin,
            FriendlyPlugin,
            EnemyPlugin,
            UiPlugin,
        ))
        .add_systems(
            Update,
            resolve_soldier_pairwise_collisions
                .after(player::update_player)
                .after(friendly::update_friendly)
                .after(enemy::update_enemy)
                .run_if(in_state(AppState::InGame)),
        )
        .run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use combat::bullet::{BulletAssets, CorpseAssets};
    use fog::material::FogMaterial;

    #[test]
    fn test_system_query_conflicts() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Assets<FogMaterial>>();
        app.init_resource::<Time>();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ui::LevelEndManager>();
        app.add_plugins(bevy::state::app::StatesPlugin);
        app.init_state::<AppState>();
        app.add_message::<common::types::GunshotEvent>();
        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let bullet_mesh = meshes.add(Circle::new(common::types::BULLET_RADIUS));
        let blood_pool_mesh = meshes.add(Circle::new(0.85));
        let blood_splatter_mesh = meshes.add(Circle::new(0.22));
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let player_mat = materials.add(Color::srgb(1.0, 1.0, 1.0));
        let enemy_mat = materials.add(Color::srgb(1.0, 0.0, 0.0));
        let blood_mat = materials.add(Color::srgb(0.5, 0.0, 0.0));
        let corpse_mat = materials.add(Color::srgb(0.3, 0.3, 0.3));
        app.insert_resource(BulletAssets {
            mesh: bullet_mesh,
            player_material: player_mat,
            enemy_material: enemy_mat,
        });
        app.insert_resource(CorpseAssets {
            blood_pool_mesh,
            blood_splatter_mesh,
            blood_material: blood_mat,
            player_corpse_material: corpse_mat.clone(),
            enemy_corpse_material: corpse_mat,
        });
        app.add_systems(
            Update,
            (
                player::update_player,
                friendly::update_friendly,
                enemy::update_enemy,
                combat::update_bullets,
                enemy::update_enemy_visibility,
                resolve_soldier_pairwise_collisions,
                world::update_camera,
                ui::update_fps,
                ui::update_main_menu_interactions,
                ui::check_level_end_conditions,
            ),
        );
        app.update();
    }
}
