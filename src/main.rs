mod combat;
mod common;
mod enemy;
mod fog;
mod player;
mod ui;
mod world;

use bevy::prelude::*;
use combat::CombatPlugin;
use common::geometry::resolve_soldier_pairwise_collisions;
use enemy::EnemyPlugin;
use fog::FogPlugin;
use player::PlayerPlugin;
use ui::UiPlugin;
use world::WorldPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Mil-Sim - Vision Cone & Fog of War".to_string(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            WorldPlugin,
            FogPlugin,
            CombatPlugin,
            PlayerPlugin,
            EnemyPlugin,
            UiPlugin,
        ))
        .add_systems(
            Update,
            resolve_soldier_pairwise_collisions
                .after(player::update_player)
                .after(enemy::update_enemy),
        )
        .run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use combat::bullet::BulletAssets;
    use fog::material::FogMaterial;

    #[test]
    fn test_system_query_conflicts() {
        let mut app = App::new();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<Assets<FogMaterial>>();
        app.init_resource::<Time>();
        app.init_resource::<ButtonInput<KeyCode>>();
        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let bullet_mesh = meshes.add(Circle::new(0.14));
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let player_mat = materials.add(Color::srgb(1.0, 1.0, 1.0));
        let enemy_mat = materials.add(Color::srgb(1.0, 0.0, 0.0));
        app.insert_resource(BulletAssets {
            mesh: bullet_mesh,
            player_material: player_mat,
            enemy_material: enemy_mat,
        });
        app.add_systems(
            Update,
            (
                player::update_player,
                enemy::update_enemy,
                combat::update_bullets,
                enemy::update_enemy_visibility,
                resolve_soldier_pairwise_collisions,
                ui::update_fps,
            ),
        );
        app.update();
    }
}
