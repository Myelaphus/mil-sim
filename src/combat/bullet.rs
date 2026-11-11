use bevy::prelude::*;
use crate::common::types::Team;
use crate::world::buildings::{Building, BuildingCollider};

#[derive(Component)]
pub struct Bullet {
    pub direction: Vec2,
    pub speed: f32,
    pub lifetime: f32,
    pub team: Team,
}

#[derive(Resource)]
pub struct BulletAssets {
    pub mesh: Handle<Mesh>,
    pub player_material: Handle<StandardMaterial>,
    pub enemy_material: Handle<StandardMaterial>,
}

// Bullet flight, obstacle collision, and player/enemy death handling
pub fn update_bullets(
    mut commands: Commands,
    time: Res<Time>,
    mut bullet_q: Query<(Entity, &mut Transform, &Bullet)>,
    buildings_q: Query<(&Transform, &BuildingCollider), (With<Building>, Without<Bullet>)>,
    characters_q: Query<(Entity, &Transform, &Team), Without<Bullet>>,
) {
    let dt = time.delta_secs();
    let soldier_radius = 0.6;
    let bullet_radius = 0.14;
    let hit_dist_sq = (soldier_radius + bullet_radius) * (soldier_radius + bullet_radius);

    for (b_entity, mut tf, bullet) in &mut bullet_q {
        // Lifetime check
        if bullet.lifetime <= 0.0 {
            commands.entity(b_entity).despawn();
            continue;
        }

        tf.translation.x += bullet.direction.x * bullet.speed * dt;
        tf.translation.z += bullet.direction.y * bullet.speed * dt;

        // Map boundaries check
        if tf.translation.x.abs() > 30.0 || tf.translation.z.abs() > 30.0 {
            commands.entity(b_entity).despawn();
            continue;
        }

        let bullet_pos = Vec2::new(tf.translation.x, tf.translation.z);

        // 1. Check building collision
        let mut hit_building = false;
        for (b_tf, collider) in &buildings_q {
            let b_center = Vec2::new(b_tf.translation.x, b_tf.translation.z);
            if bullet_pos.x >= b_center.x - collider.half_size.x
                && bullet_pos.x <= b_center.x + collider.half_size.x
                && bullet_pos.y >= b_center.y - collider.half_size.y
                && bullet_pos.y <= b_center.y + collider.half_size.y
            {
                hit_building = true;
                break;
            }
        }
        if hit_building {
            commands.entity(b_entity).despawn();
            continue;
        }

        // 2. Check hit on opposing team characters (Player / Enemy)
        let mut hit_character = false;
        for (char_entity, char_tf, char_team) in &characters_q {
            if bullet.team != *char_team {
                let char_pos = Vec2::new(char_tf.translation.x, char_tf.translation.z);
                if (bullet_pos - char_pos).length_squared() < hit_dist_sq {
                    // Character dies when hit by opposing bullet!
                    commands.entity(char_entity).despawn_children();
                    commands.entity(char_entity).despawn();
                    hit_character = true;
                    break;
                }
            }
        }

        if hit_character {
            commands.entity(b_entity).despawn();
        }
    }
}

