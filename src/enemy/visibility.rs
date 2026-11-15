use bevy::prelude::*;
use crate::common::geometry::{can_see_target, can_see_target_aerial};
use crate::common::types::{FogOfWarEnabled, Soldier, Team};
use crate::enemy::ai::Enemy;
use crate::friendly::Friendly;
use crate::player::Player;
use crate::vehicle::AttackHelicopter;
use crate::world::buildings::{Building, BuildingCollider};

// Dynamically sets enemy visibility based on fire team vision cones (player, living friendlies, and friendly helicopters)
// and the FogOfWarEnabled debug toggle.
pub fn update_enemy_visibility(
    fog_enabled: Option<Res<FogOfWarEnabled>>,
    player_q: Query<(&Transform, &Soldier), (With<Player>, Without<Enemy>, Without<Friendly>, Without<AttackHelicopter>)>,
    friendly_q: Query<(&Transform, &Soldier), (With<Friendly>, Without<Enemy>, Without<Player>, Without<AttackHelicopter>)>,
    heli_q: Query<(&Transform, &Team, &AttackHelicopter), (Without<Enemy>, Without<Player>, Without<Friendly>)>,
    mut enemy_q: Query<
        (&Transform, &mut Visibility),
        (With<Enemy>, Without<Player>, Without<Friendly>, Without<AttackHelicopter>),
    >,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (
            With<Building>,
            Without<Player>,
            Without<Enemy>,
            Without<Friendly>,
            Without<AttackHelicopter>,
        ),
    >,
) {
    // When Fog of War is toggled OFF for debugging, reveal all enemy units on the map
    if !fog_enabled.as_deref().map_or(true, |f| f.0) {
        for (_, mut vis) in &mut enemy_q {
            *vis = Visibility::Inherited;
        }
        return;
    }

    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();

    // Collect all fire team spotters (player, friendlies, and friendly attack helicopters): (pos, dir, range, half_angle, is_aerial)
    let mut spotters: Vec<(Vec2, Vec2, f32, f32, bool)> = Vec::new();

    if let Ok((p_tf, p_soldier)) = player_q.single() {
        spotters.push((
            Vec2::new(p_tf.translation.x, p_tf.translation.z),
            p_soldier.facing_dir,
            36.0, // Player vision range
            45.0_f32.to_radians(),
            false,
        ));
    }

    for (f_tf, f_soldier) in &friendly_q {
        spotters.push((
            Vec2::new(f_tf.translation.x, f_tf.translation.z),
            f_soldier.facing_dir,
            24.0, // Friendly vision range
            45.0_f32.to_radians(),
            false,
        ));
    }

    for (h_tf, team, heli) in &heli_q {
        if *team == Team::Player {
            let heli_pos = Vec2::new(h_tf.translation.x, h_tf.translation.z);
            spotters.push((
                heli.cannon_world_pos(heli_pos),
                heli.cannon_world_dir(),
                heli.cannon_vision_range,
                35.0_f32.to_radians(),
                true,
            ));
        }
    }

    if spotters.is_empty() {
        // Fire team eliminated: enemies remain hidden
        for (_, mut vis) in &mut enemy_q {
            *vis = Visibility::Hidden;
        }
        return;
    }

    for (enemy_tf, mut vis) in &mut enemy_q {
        let enemy_pos = Vec2::new(enemy_tf.translation.x, enemy_tf.translation.z);

        // Visible if ANY fire team member or friendly helicopter has line-of-sight in their vision cone
        let spotted = spotters.iter().any(|(pos, dir, range, half_angle, is_aerial)| {
            if *is_aerial {
                can_see_target_aerial(
                    *pos,
                    *dir,
                    enemy_pos,
                    *range,
                    *half_angle,
                    3.0,
                    &buildings,
                )
            } else {
                can_see_target(
                    *pos,
                    *dir,
                    enemy_pos,
                    *range,
                    *half_angle,
                    3.0,
                    &buildings,
                )
            }
        });

        *vis = if spotted {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

