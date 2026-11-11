pub mod bullet;

use bevy::prelude::*;
pub use bullet::*;

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_bullet_assets)
            .add_systems(Update, update_bullets);
    }
}

fn setup_bullet_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let bullet_mesh = meshes.add(Circle::new(0.14));
    let player_bullet_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.85, 0.2),
        unlit: true,
        ..default()
    });
    let enemy_bullet_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.25, 0.25),
        unlit: true,
        ..default()
    });

    commands.insert_resource(BulletAssets {
        mesh: bullet_mesh,
        player_material: player_bullet_material,
        enemy_material: enemy_bullet_material,
    });
}

