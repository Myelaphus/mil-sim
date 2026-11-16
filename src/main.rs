mod combat;
mod common;
mod enemy;
mod fog;
mod friendly;
mod player;
mod ui;
mod vehicle;
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
use vehicle::VehiclePlugin;
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
        .init_resource::<common::types::CurrentLevel>()
        .init_resource::<common::types::SquadCommand>()
        .init_resource::<common::types::SelectedOrderTool>()
        .init_resource::<common::types::CommanderOrder>()
        .init_resource::<common::types::CommandGroup>()
        .init_resource::<common::types::ApcOrder>()
        .init_resource::<common::types::HelicopterOrder>()
        .init_resource::<common::types::FogOfWarEnabled>()
        .init_resource::<friendly::SquadDoctrineState>()
        .add_plugins((
            WorldPlugin,
            FogPlugin,
            CombatPlugin,
            PlayerPlugin,
            FriendlyPlugin,
            EnemyPlugin,
            VehiclePlugin,
            UiPlugin,
        ))
        .add_systems(
            Update,
            resolve_soldier_pairwise_collisions
                .after(player::update_player)
                .after(friendly::update_friendly)
                .after(enemy::update_enemy)
                .after(enemy::update_zombies)
                .after(vehicle::update_dismounted_crew_return)
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
        app.init_resource::<common::types::CurrentLevel>();
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
                enemy::update_zombies,
                vehicle::handle_vehicle_crew_mount_commands,
                vehicle::update_crewed_apc_vehicles,
                vehicle::update_dismounted_crew_return,
                vehicle::resolve_soldier_apc_collisions,
                vehicle::update_attack_helicopters,
                vehicle::update_field_artillery,
                combat::update_bullets,
                enemy::update_enemy_visibility,
                world::sync_elevation_visual_scale,
                resolve_soldier_pairwise_collisions,
                world::update_camera,
                ui::update_fps,
                ui::update_ammo_ui,
                ui::update_orders_ui,
                ui::update_main_menu_interactions,
                ui::check_level_end_conditions,
            ),
        );
        app.update();
    }
}
