use bevy::prelude::*;
use crate::common::geometry::can_see_target;
use crate::common::types::Soldier;
use crate::enemy::ai::Enemy;
use crate::friendly::Friendly;
use crate::player::Player;
use crate::world::buildings::{Building, BuildingCollider};

// Dynamically sets enemy visibility based on squad vision cones (player & living friendlies)
pub fn update_enemy_visibility(
    player_q: Query<(&Transform, &Soldier), (With<Player>, Without<Enemy>, Without<Friendly>)>,
    friendly_q: Query<(&Transform, &Soldier), (With<Friendly>, Without<Enemy>, Without<Player>)>,
    mut enemy_q: Query<
        (&Transform, &mut Visibility),
        (With<Enemy>, Without<Player>, Without<Friendly>),
    >,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (
            With<Building>,
            Without<Player>,
            Without<Enemy>,
            Without<Friendly>,
        ),
    >,
) {
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();

    // Collect all squad spotters (player and friendlies)
    let mut spotters: Vec<(Vec2, Vec2, f32)> = Vec::new();

    if let Ok((p_tf, p_soldier)) = player_q.single() {
        spotters.push((
            Vec2::new(p_tf.translation.x, p_tf.translation.z),
            p_soldier.facing_dir,
            36.0, // Player vision range
        ));
    }

    for (f_tf, f_soldier) in &friendly_q {
        spotters.push((
            Vec2::new(f_tf.translation.x, f_tf.translation.z),
            f_soldier.facing_dir,
            24.0, // Friendly vision range
        ));
    }

    if spotters.is_empty() {
        // Squad eliminated: enemies remain hidden
        for (_, mut vis) in &mut enemy_q {
            *vis = Visibility::Hidden;
        }
        return;
    }

    for (enemy_tf, mut vis) in &mut enemy_q {
        let enemy_pos = Vec2::new(enemy_tf.translation.x, enemy_tf.translation.z);

        // Visible if ANY squad member has line-of-sight in their vision cone
        let spotted = spotters.iter().any(|(pos, dir, range)| {
            can_see_target(
                *pos,
                *dir,
                enemy_pos,
                *range,
                45.0_f32.to_radians(),
                3.0,
                &buildings,
            )
        });

        *vis = if spotted {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

