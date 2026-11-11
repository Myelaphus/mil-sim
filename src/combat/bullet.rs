use bevy::prelude::*;
use crate::common::geometry::line_intersects_box;
use crate::common::types::{Soldier, Stance, Team};
use crate::world::buildings::{Building, BuildingCollider, LowWall, LowWallCollider};

#[derive(Component)]
pub struct Bullet {
    pub origin: Vec2,
    pub direction: Vec2,
    pub speed: f32,
    pub lifetime: f32,
    pub team: Team,
    pub shooter_stance: Stance,
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
    low_walls_q: Query<(&Transform, &LowWallCollider), (With<LowWall>, Without<Bullet>)>,
    characters_q: Query<(Entity, &Transform, &Team, &Soldier), Without<Bullet>>,
) {
    let dt = time.delta_secs();
    let soldier_radius = 0.6;
    let bullet_radius = 0.14;
    let hit_dist_sq = (soldier_radius + bullet_radius) * (soldier_radius + bullet_radius);
    let low_walls: Vec<(&Transform, &LowWallCollider)> = low_walls_q.iter().collect();

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

        // 1. Check building collision (full-height buildings block all bullets unconditionally)
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

        // 2. Check low wall collision: bullets fired while crouched or prone cannot clear waist-height walls
        if bullet.shooter_stance != Stance::Standing {
            let mut hit_low_wall = false;
            for (w_tf, collider) in &low_walls {
                let w_center = Vec2::new(w_tf.translation.x, w_tf.translation.z);
                if bullet_pos.x >= w_center.x - collider.half_size.x
                    && bullet_pos.x <= w_center.x + collider.half_size.x
                    && bullet_pos.y >= w_center.y - collider.half_size.y
                    && bullet_pos.y <= w_center.y + collider.half_size.y
                {
                    hit_low_wall = true;
                    break;
                }
            }
            if hit_low_wall {
                commands.entity(b_entity).despawn();
                continue;
            }
        }

        // 3. Check hit on opposing team characters (Player / Enemy)
        let mut bullet_consumed = false;
        for (char_entity, char_tf, char_team, soldier) in &characters_q {
            if bullet.team != *char_team {
                let char_pos = Vec2::new(char_tf.translation.x, char_tf.translation.z);
                if (bullet_pos - char_pos).length_squared() < hit_dist_sq {
                    if soldier.stance != Stance::Standing {
                        // Check if crouched or prone character is protected behind a low wall
                        let mut protected_by_wall = false;
                        for (w_tf, w_collider) in &low_walls {
                            let w_center = Vec2::new(w_tf.translation.x, w_tf.translation.z);
                            let w_min = w_center - w_collider.half_size;
                            let w_max = w_center + w_collider.half_size;

                            // If ray from bullet origin to character intersects the low wall box,
                            // the bullet hits the low wall instead!
                            if line_intersects_box(bullet.origin, char_pos, w_min, w_max) {
                                protected_by_wall = true;
                                break;
                            }
                        }

                        if protected_by_wall {
                            // Shot hits the wall instead of the ducked/prone soldier!
                            bullet_consumed = true;
                            break;
                        }

                        // Stance defense in the open:
                        // Crouched: 50% chance the shot misses the reduced silhouette
                        // Prone: 80% chance the shot misses the very low profile
                        let miss_chance = match soldier.stance {
                            Stance::Crouching => 50,
                            Stance::Prone => 80,
                            Stance::Standing => 0,
                        };

                        let seed = (time.elapsed().as_millis() ^ (char_entity.to_bits() as u128)) as u64;
                        let miss = (seed % 100) < miss_chance;
                        if miss {
                            // Shot missed the ducked/prone soldier!
                            bullet_consumed = true;
                            break;
                        }
                    }

                    // Direct hit on character (standing or failed crouch/prone roll)
                    commands.entity(char_entity).despawn_children();
                    commands.entity(char_entity).despawn();
                    bullet_consumed = true;
                    break;
                }
            }
        }

        if bullet_consumed {
            commands.entity(b_entity).despawn();
        }
    }
}

