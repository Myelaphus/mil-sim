pub mod bullet;

use bevy::prelude::*;
pub use bullet::*;
use crate::common::types::BULLET_RADIUS;

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
    let bullet_mesh = meshes.add(Circle::new(BULLET_RADIUS));
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

    let blood_pool_mesh = meshes.add(Circle::new(0.85));
    let blood_splatter_mesh = meshes.add(Circle::new(0.22));
    let blood_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.50, 0.02, 0.02, 0.92),
        perceptual_roughness: 0.25,
        cull_mode: None,
        ..default()
    });
    let player_corpse_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.20, 0.28, 0.38),
        perceptual_roughness: 0.6,
        ..default()
    });
    let enemy_corpse_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.38, 0.22, 0.22),
        perceptual_roughness: 0.6,
        ..default()
    });

    commands.insert_resource(CorpseAssets {
        blood_pool_mesh,
        blood_splatter_mesh,
        blood_material,
        player_corpse_material,
        enemy_corpse_material,
    });
}

