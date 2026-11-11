use bevy::prelude::*;
use crate::common::geometry::can_see_target;
use crate::common::types::Soldier;
use crate::enemy::ai::Enemy;
use crate::player::Player;
use crate::world::buildings::{Building, BuildingCollider};

// Dynamically sets enemy visibility based on player's vision cone and building line-of-sight
pub fn update_enemy_visibility(
    player_q: Query<(&Transform, &Soldier), (With<Player>, Without<Enemy>)>,
    mut enemy_q: Query<(&Transform, &mut Visibility), (With<Enemy>, Without<Player>)>,
    buildings_q: Query<
        (&Transform, &BuildingCollider),
        (With<Building>, Without<Player>, Without<Enemy>),
    >,
) {
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();

    let Ok((player_tf, player_soldier)) = player_q.single() else {
        // Player is dead: enemies remain hidden
        for (_, mut vis) in &mut enemy_q {
            *vis = Visibility::Hidden;
        }
        return;
    };

    let player_pos = Vec2::new(player_tf.translation.x, player_tf.translation.z);

    for (enemy_tf, mut vis) in &mut enemy_q {
        let enemy_pos = Vec2::new(enemy_tf.translation.x, enemy_tf.translation.z);
        let visible = can_see_target(
            player_pos,
            player_soldier.facing_dir,
            enemy_pos,
            26.0,
            45.0_f32.to_radians(),
            2.5,
            &buildings,
        );

        *vis = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

