use std::f32::consts::FRAC_PI_2;
use bevy::prelude::*;
use crate::common::geometry::line_intersects_box;
use crate::common::types::{
    BloodDecal, Corpse, Soldier, Stance, Team, BULLET_RADIUS, SOLDIER_RADIUS, WORLD_HALF_SIZE,
};
use crate::enemy::ai::Enemy;
use crate::player::Player;
use crate::world::buildings::{Building, BuildingCollider, LowWall, LowWallCollider};

#[derive(Component)]
pub struct Bullet {
    pub origin: Vec2,
    pub direction: Vec2,
    pub speed: f32,
    pub lifetime: f32,
    pub team: Team,
    pub shooter_stance: Stance,
    pub missed_entities: Vec<Entity>,
}

#[derive(Resource)]
pub struct BulletAssets {
    pub mesh: Handle<Mesh>,
    pub player_material: Handle<StandardMaterial>,
    pub enemy_material: Handle<StandardMaterial>,
}

#[derive(Resource)]
pub struct CorpseAssets {
    pub blood_pool_mesh: Handle<Mesh>,
    pub blood_splatter_mesh: Handle<Mesh>,
    pub blood_material: Handle<StandardMaterial>,
    pub player_corpse_material: Handle<StandardMaterial>,
    pub enemy_corpse_material: Handle<StandardMaterial>,
}

// Bullet flight, obstacle collision, and player/enemy death handling
pub fn update_bullets(
    mut commands: Commands,
    time: Res<Time>,
    corpse_assets: Res<CorpseAssets>,
    mut bullet_q: Query<(Entity, &mut Transform, &mut Bullet)>,
    buildings_q: Query<(&Transform, &BuildingCollider), (With<Building>, Without<Bullet>)>,
    low_walls_q: Query<(&Transform, &LowWallCollider), (With<LowWall>, Without<Bullet>)>,
    characters_q: Query<(Entity, &Transform, &Team, &Soldier), Without<Bullet>>,
) {
    let dt = time.delta_secs();
    let soldier_radius = SOLDIER_RADIUS;
    let bullet_radius = BULLET_RADIUS;
    let hit_dist_sq = (soldier_radius + bullet_radius) * (soldier_radius + bullet_radius);
    let low_walls: Vec<(&Transform, &LowWallCollider)> = low_walls_q.iter().collect();

    for (b_entity, mut tf, mut bullet) in &mut bullet_q {
        // Lifetime check
        if bullet.lifetime <= 0.0 {
            commands.entity(b_entity).despawn();
            continue;
        }

        tf.translation.x += bullet.direction.x * bullet.speed * dt;
        tf.translation.z += bullet.direction.y * bullet.speed * dt;

        // Map boundaries check derived directly from unified world dimensions
        if tf.translation.x.abs() > WORLD_HALF_SIZE || tf.translation.z.abs() > WORLD_HALF_SIZE {
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
                // If this bullet has already rolled a miss against this character, ignore them so it keeps moving past
                if bullet.missed_entities.contains(&char_entity) {
                    continue;
                }

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
                            // Shot missed the ducked/prone soldier: mark character as missed
                            // so the bullet continues moving through/past them!
                            bullet.missed_entities.push(char_entity);
                            continue;
                        }
                    }

                    // Direct hit on character: stops at the soldier and converts them to a static corpse
                    let mut entity_cmds = commands.entity(char_entity);
                    entity_cmds.despawn_children();
                    entity_cmds
                        .remove::<Soldier>()
                        .remove::<Team>()
                        .remove::<Player>()
                        .remove::<Enemy>()
                        .insert((
                            Corpse,
                            Visibility::Inherited,
                            Transform::from_xyz(char_pos.x, 0.03, char_pos.y)
                                .with_rotation(char_tf.rotation)
                                .with_scale(Vec3::new(0.65, 0.95, 0.65)),
                            MeshMaterial3d(if *char_team == Team::Player {
                                corpse_assets.player_corpse_material.clone()
                            } else {
                                corpse_assets.enemy_corpse_material.clone()
                            }),
                        ));

                    // Spawn primary blood pool decal underneath
                    commands.spawn((
                        Mesh3d(corpse_assets.blood_pool_mesh.clone()),
                        MeshMaterial3d(corpse_assets.blood_material.clone()),
                        Transform::from_xyz(char_pos.x, 0.015, char_pos.y)
                            .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                        BloodDecal,
                    ));

                    // Spawn small splatter droplets nearby for realistic ground pool
                    let splatters = [
                        Vec2::new(0.50, 0.40),
                        Vec2::new(-0.45, -0.35),
                    ];
                    for offset in splatters {
                        commands.spawn((
                            Mesh3d(corpse_assets.blood_splatter_mesh.clone()),
                            MeshMaterial3d(corpse_assets.blood_material.clone()),
                            Transform::from_xyz(char_pos.x + offset.x, 0.015, char_pos.y + offset.y)
                                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                            BloodDecal,
                        ));
                    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bullet_kills_soldier_creates_corpse_and_blood() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let bullet_mesh = meshes.add(Circle::new(0.14));
        let blood_pool_mesh = meshes.add(Circle::new(0.85));
        let blood_splatter_mesh = meshes.add(Circle::new(0.22));
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let dummy_mat = materials.add(Color::srgb(1.0, 1.0, 1.0));
        let blood_mat = materials.add(Color::srgb(0.5, 0.0, 0.0));

        app.insert_resource(BulletAssets {
            mesh: bullet_mesh,
            player_material: dummy_mat.clone(),
            enemy_material: dummy_mat.clone(),
        });
        app.insert_resource(CorpseAssets {
            blood_pool_mesh,
            blood_splatter_mesh,
            blood_material: blood_mat,
            player_corpse_material: dummy_mat.clone(),
            enemy_corpse_material: dummy_mat,
        });

        // Spawn a standing enemy soldier at (0, 0, 5)
        let enemy_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 5.0),
                crate::enemy::ai::Enemy {
                    state: crate::enemy::ai::EnemyAiState::Search,
                    current_waypoint_idx: 0,
                    last_known_player_pos: None,
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 5.5,
                    vision_range: 16.0,
                    vision_half_angle: 0.78,
                    inner_radius: 2.0,
                    stuck_timer: 0.0,
                    last_pos: Vec2::new(0.0, 5.0),
                },
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                },
            ))
            .id();

        // Spawn a player bullet right at the enemy's position
        let bullet_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.08, 5.0),
                Bullet {
                    origin: Vec2::new(0.0, 0.0),
                    direction: Vec2::new(0.0, 1.0),
                    speed: 40.0,
                    lifetime: 1.0,
                    team: Team::Player,
                    shooter_stance: Stance::Standing,
                    missed_entities: Vec::new(),
                },
            ))
            .id();

        app.add_systems(Update, update_bullets);
        app.update();

        // Verify the bullet hit and was despawned
        assert!(app.world().get_entity(bullet_entity).is_err());

        // Verify the enemy entity still exists as a corpse (NOT despawned)
        let Ok(corpse) = app.world().get_entity(enemy_entity) else {
            panic!("Corpse entity should still exist");
        };
        assert!(corpse.contains::<Corpse>());
        assert!(!corpse.contains::<Soldier>());
        assert!(!corpse.contains::<Team>());
        assert!(!corpse.contains::<crate::enemy::ai::Enemy>());

        // Verify blood decals were spawned
        let blood_decal_count = app
            .world_mut()
            .query::<&BloodDecal>()
            .iter(app.world())
            .count();
        assert!(blood_decal_count > 0, "Blood decal should be spawned under the fallen soldier");
    }

    #[test]
    fn test_bullet_miss_continues_moving_past_soldier() {
        let mut app = App::new();
        app.init_resource::<Time>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let bullet_mesh = meshes.add(Circle::new(0.14));
        let blood_pool_mesh = meshes.add(Circle::new(0.85));
        let blood_splatter_mesh = meshes.add(Circle::new(0.22));
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let dummy_mat = materials.add(Color::srgb(1.0, 1.0, 1.0));
        let blood_mat = materials.add(Color::srgb(0.5, 0.0, 0.0));

        app.insert_resource(BulletAssets {
            mesh: bullet_mesh,
            player_material: dummy_mat.clone(),
            enemy_material: dummy_mat.clone(),
        });
        app.insert_resource(CorpseAssets {
            blood_pool_mesh,
            blood_splatter_mesh,
            blood_material: blood_mat,
            player_corpse_material: dummy_mat.clone(),
            enemy_corpse_material: dummy_mat,
        });

        // Spawn a prone soldier at (0, 0, 5)
        let enemy_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 5.0),
                crate::enemy::ai::Enemy {
                    state: crate::enemy::ai::EnemyAiState::Search,
                    current_waypoint_idx: 0,
                    last_known_player_pos: None,
                    investigate_timer: Timer::from_seconds(3.0, TimerMode::Once),
                    shoot_timer: Timer::from_seconds(0.5, TimerMode::Repeating),
                    speed: 5.5,
                    vision_range: 16.0,
                    vision_half_angle: 0.78,
                    inner_radius: 2.0,
                    stuck_timer: 0.0,
                    last_pos: Vec2::new(0.0, 5.0),
                },
                Team::Enemy,
                Soldier {
                    facing_dir: Vec2::new(0.0, 1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Prone,
                },
            ))
            .id();

        // Spawn a bullet moving towards +Z that has already rolled a miss against enemy_entity
        let bullet_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.08, 5.0),
                Bullet {
                    origin: Vec2::new(0.0, 0.0),
                    direction: Vec2::new(0.0, 1.0),
                    speed: 40.0,
                    lifetime: 1.0,
                    team: Team::Player,
                    shooter_stance: Stance::Standing,
                    missed_entities: vec![enemy_entity],
                },
            ))
            .id();

        app.add_systems(Update, update_bullets);
        app.update();

        // Verify the bullet still exists and has continued moving past the soldier
        let Ok(bullet) = app.world().get_entity(bullet_entity) else {
            panic!("Bullet should still exist and continue moving past the soldier");
        };
        let bullet_tf = bullet.get::<Transform>().unwrap();
        // Since speed = 40.0 and dt default, bullet has traveled forward
        assert!(bullet_tf.translation.z >= 5.0);

        // Verify enemy soldier is still alive and unharmed
        let Ok(enemy) = app.world().get_entity(enemy_entity) else {
            panic!("Enemy soldier should still exist");
        };
        assert!(enemy.contains::<Soldier>());
        assert!(!enemy.contains::<Corpse>());
    }
}

