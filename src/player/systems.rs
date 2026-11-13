use crate::combat::bullet::{Bullet, BulletAssets};
use crate::common::geometry::{
    apply_shoot_spread, directional_speed_multiplier, line_intersects_box,
    resolve_circle_circle_collision, resolve_circle_obstacle_collision,
};
use crate::common::types::{GunshotEvent, LevelEntity, SOLDIER_RADIUS, Soldier, Stance, Team};
use crate::world::buildings::{BuildingCollider, LowWallCollider};
use bevy::prelude::*;
use std::f32::consts::FRAC_PI_2;

#[derive(Component)]
pub struct Player;

pub fn setup_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let pointer_mesh = meshes.add(Circle::new(0.15));
    let player_soldier_mesh = meshes.add(Circle::new(SOLDIER_RADIUS));
    let player_soldier_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.15, 0.50, 0.95),
        perceptual_roughness: 0.5,
        ..default()
    });
    let player_pointer_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.95, 1.0),
        unlit: true,
        ..default()
    });

    let player = commands
        .spawn((
            LevelEntity,
            Mesh3d(player_soldier_mesh),
            MeshMaterial3d(player_soldier_material),
            Transform::from_xyz(0.0, 0.05, 48.0).with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0), // Facing North (-Z)
                shoot_cooldown: Timer::from_seconds(0.12, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::player_default(),
            },
        ))
        .id();

    commands.spawn((
        Mesh3d(pointer_mesh),
        MeshMaterial3d(player_pointer_material),
        Transform::from_xyz(0.0, 0.42, 0.01),
        ChildOf(player),
    ));
}

pub fn update_player(
    mut commands: Commands,
    windows: Query<&Window>,
    camera_q: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    bullet_assets: Res<BulletAssets>,
    mut player_q: Query<(&mut Transform, &mut Soldier), With<Player>>,
    buildings_q: Query<(&Transform, &BuildingCollider), Without<Player>>,
    low_walls_q: Query<(&Transform, &LowWallCollider), Without<Player>>,
    other_soldiers_q: Query<&Transform, (With<Soldier>, Without<Player>)>,
    mut gunshot_writer: MessageWriter<GunshotEvent>,
) {
    let Ok((mut soldier_tf, mut soldier)) = player_q.single_mut() else {
        return;
    };

    // 1. Stance toggles (C for crouch, V for prone) & visual scale feedback
    if keyboard_input.just_pressed(KeyCode::KeyC) {
        soldier.stance = if soldier.stance == Stance::Crouching {
            Stance::Standing
        } else {
            Stance::Crouching
        };
    }
    if keyboard_input.just_pressed(KeyCode::KeyV) {
        soldier.stance = if soldier.stance == Stance::Prone {
            Stance::Standing
        } else {
            Stance::Prone
        };
    }

    soldier_tf.scale = match soldier.stance {
        Stance::Standing => Vec3::ONE,
        Stance::Crouching => Vec3::splat(0.72), // Compact profile indicating crouching posture
        Stance::Prone => Vec3::new(0.55, 1.15, 0.55), // Elongated narrow crawler profile
    };

    // 2. Mouse aim direction for vision cone & soldier orientation
    if let Ok(window) = windows.single() {
        if let Ok((camera, camera_gt)) = camera_q.single() {
            if let Some(cursor_pos) = window.cursor_position() {
                if let Ok(ray) = camera.viewport_to_world(camera_gt, cursor_pos) {
                    if let Some(dist) =
                        ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))
                    {
                        let ground_pos = ray.get_point(dist);
                        let diff_x = ground_pos.x - soldier_tf.translation.x;
                        let diff_z = ground_pos.z - soldier_tf.translation.z;
                        let aim_2d = Vec2::new(diff_x, diff_z);

                        if aim_2d.length_squared() > 0.01 {
                            let dir = aim_2d.normalize();
                            soldier.facing_dir = dir;

                            // Rotate soldier token to face mouse cursor
                            let yaw = f32::atan2(-dir.x, -dir.y);
                            soldier_tf.rotation =
                                Quat::from_rotation_y(yaw) * Quat::from_rotation_x(-FRAC_PI_2);
                        }
                    }
                }
            }
        }
    }

    // 3. WASD / Arrow keys movement with directional speed multiplier
    let mut move_delta = Vec3::ZERO;
    if keyboard_input.pressed(KeyCode::KeyW) || keyboard_input.pressed(KeyCode::ArrowUp) {
        move_delta.z -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyS) || keyboard_input.pressed(KeyCode::ArrowDown) {
        move_delta.z += 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyA) || keyboard_input.pressed(KeyCode::ArrowLeft) {
        move_delta.x -= 1.0;
    }
    if keyboard_input.pressed(KeyCode::KeyD) || keyboard_input.pressed(KeyCode::ArrowRight) {
        move_delta.x += 1.0;
    }

    if move_delta.length_squared() > 0.0 {
        let base_speed = 12.0;
        let stance_speed = match soldier.stance {
            Stance::Standing => base_speed,
            Stance::Crouching => base_speed * 0.55, // Slower tactical crouch-walking
            Stance::Prone => base_speed * 0.23,     // Very slow crawling speed
        };
        let move_dir = Vec2::new(move_delta.x, move_delta.z).normalize();
        // Moving forward (facing direction) is 100% normal speed; lateral or backwards is 50% normal speed
        let dir_mult = directional_speed_multiplier(soldier.facing_dir, move_dir);
        let speed = stance_speed * dir_mult;
        let delta = move_delta.normalize() * speed * time.delta_secs();
        let soldier_radius = SOLDIER_RADIUS;

        let mut new_pos = Vec2::new(
            soldier_tf.translation.x + delta.x,
            soldier_tf.translation.z + delta.z,
        );

        // Resolve collision against other soldiers (cannot occupy same space)
        for other_tf in &other_soldiers_q {
            let other_pos = Vec2::new(other_tf.translation.x, other_tf.translation.z);
            new_pos =
                resolve_circle_circle_collision(new_pos, soldier_radius, other_pos, soldier_radius);
        }

        let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();
        let low_walls: Vec<(&Transform, &LowWallCollider)> = low_walls_q.iter().collect();
        let resolved =
            resolve_circle_obstacle_collision(new_pos, soldier_radius, &buildings, &low_walls);
        soldier_tf.translation.x = resolved.x;
        soldier_tf.translation.z = resolved.y;
    }

    // 4. Weapon inventory, swapping, reloading & shooting (Player team)
    soldier.inventory.tick(time.delta());

    // 'F' key weapon swap (takes 2 seconds)
    if keyboard_input.just_pressed(KeyCode::KeyF) {
        soldier.inventory.start_swap();
    }

    // 'R' key manual reload (if not swapping)
    if keyboard_input.just_pressed(KeyCode::KeyR) && !soldier.inventory.is_swapping() {
        if let Some(weapon) = soldier.inventory.active_weapon_mut() {
            weapon.ammo.start_reload();
        }
    }

    soldier.shoot_cooldown.tick(time.delta());
    let wants_to_shoot = keyboard_input.just_pressed(KeyCode::Space)
        || (keyboard_input.pressed(KeyCode::Space) && soldier.shoot_cooldown.is_finished());

    if wants_to_shoot && !soldier.inventory.is_swapping() {
        let mut should_fire = false;
        let mut bullet_speed = 45.0;
        let mut bullet_lifetime = 1.5;
        if let Some(weapon) = soldier.inventory.active_weapon_mut() {
            if weapon.ammo.current_mag == 0 {
                // Out of ammo in current magazine: automatically trigger reload if reserves available
                weapon.ammo.start_reload();
            } else if weapon.ammo.can_fire() {
                weapon.ammo.current_mag -= 1;
                bullet_speed = weapon.bullet_speed();
                bullet_lifetime = weapon.bullet_lifetime();
                should_fire = true;
            }
        }

        if should_fire {
            soldier.shoot_cooldown.reset();
            let soldier_pos = Vec2::new(soldier_tf.translation.x, soldier_tf.translation.z);
            let spawn_pos = soldier_pos + soldier.facing_dir * 0.85;

            // When crouched or prone, shots aimed directly into/across an adjacent low wall cannot pass through
            let mut muzzle_blocked = false;
            if soldier.stance != Stance::Standing {
                for (w_tf, w_collider) in &low_walls_q {
                    let w_center = Vec2::new(w_tf.translation.x, w_tf.translation.z);
                    let w_min = w_center - w_collider.half_size;
                    let w_max = w_center + w_collider.half_size;

                    let aim_probe = spawn_pos + soldier.facing_dir * 0.5;
                    if line_intersects_box(soldier_pos, aim_probe, w_min, w_max)
                        || (spawn_pos.x >= w_min.x
                            && spawn_pos.x <= w_max.x
                            && spawn_pos.y >= w_min.y
                            && spawn_pos.y <= w_max.y)
                    {
                        muzzle_blocked = true;
                        break;
                    }
                }
            }

            if !muzzle_blocked {
                let is_moving = move_delta.length_squared() > 0.0;
                let spread_seed = time.elapsed_secs().to_bits() as u64
                    ^ (soldier_pos.x.to_bits() as u64)
                    ^ ((soldier_pos.y.to_bits() as u64) << 16);
                let shot_dir = apply_shoot_spread(soldier.facing_dir, is_moving, spread_seed);
                commands.spawn((
                    LevelEntity,
                    Mesh3d(bullet_assets.mesh.clone()),
                    MeshMaterial3d(bullet_assets.player_material.clone()),
                    Transform::from_xyz(spawn_pos.x, 0.08, spawn_pos.y)
                        .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                    Bullet {
                        origin: soldier_pos,
                        direction: shot_dir,
                        speed: bullet_speed,
                        lifetime: bullet_lifetime,
                        team: Team::Player,
                        shooter_stance: soldier.stance,
                        missed_entities: Vec::new(),
                    },
                ));
                gunshot_writer.write(GunshotEvent {
                    position: soldier_pos,
                    shooter_team: Team::Player,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_player_test_app() -> App {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<Time>();
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.add_message::<GunshotEvent>();

        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        let dummy_mesh = meshes.add(Circle::new(0.1));
        let mut mats = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let dummy_mat = mats.add(Color::WHITE);

        app.insert_resource(BulletAssets {
            mesh: dummy_mesh,
            player_material: dummy_mat.clone(),
            enemy_material: dummy_mat,
        });

        app.add_systems(Update, update_player);
        app
    }

    #[test]
    fn test_player_forward_movement_is_full_speed() {
        let mut app = create_player_test_app();

        // Player facing North (0.0, -1.0) at (0, 0, 0)
        let player_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 0.0),
                Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::default(),
                },
            ))
            .id();

        // Press 'W' to move North (forward, matching facing_dir)
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::KeyW);

        // Advance 0.5s: base standing speed = 12.0 m/s * 1.0 (forward) = 12.0 m/s
        // Distance traveled should be 6.0m North (Z = -6.0)
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(500));

        app.update();

        let tf = app.world().get::<Transform>(player_entity).unwrap();
        assert!(
            (tf.translation.z - (-6.0)).abs() < 0.01,
            "Player moving forward should move at 100% normal speed (-6.0m), got Z = {}",
            tf.translation.z
        );
        assert_eq!(tf.translation.x, 0.0);
    }

    #[test]
    fn test_player_lateral_and_backwards_movement_is_half_speed() {
        // 1. Backwards test (moving South while facing North)
        {
            let mut app = create_player_test_app();
            let player_entity = app
                .world_mut()
                .spawn((
                    Transform::from_xyz(0.0, 0.05, 0.0),
                    Player,
                    Soldier {
                        facing_dir: Vec2::new(0.0, -1.0), // Facing North
                        shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                        stance: Stance::Standing,
                        inventory: crate::common::types::Inventory::default(),
                    },
                ))
                .id();

            // Press 'S' to backpedal South (opposite to facing_dir)
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyS);

            // Advance 0.5s: 12.0 m/s * 0.5 (backwards) * 0.5s = 3.0m South (Z = 3.0)
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(500));

            app.update();

            let tf = app.world().get::<Transform>(player_entity).unwrap();
            assert!(
                (tf.translation.z - 3.0).abs() < 0.01,
                "Player moving backwards should move at 50% normal speed (3.0m), got Z = {}",
                tf.translation.z
            );
        }

        // 2. Lateral strafe test (moving East while facing North)
        {
            let mut app = create_player_test_app();
            let player_entity = app
                .world_mut()
                .spawn((
                    Transform::from_xyz(0.0, 0.05, 0.0),
                    Player,
                    Soldier {
                        facing_dir: Vec2::new(0.0, -1.0), // Facing North
                        shoot_cooldown: Timer::from_seconds(0.2, TimerMode::Once),
                        stance: Stance::Standing,
                        inventory: crate::common::types::Inventory::default(),
                    },
                ))
                .id();

            // Press 'D' to strafe East (perpendicular to facing_dir)
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyD);

            // Advance 0.5s: 12.0 m/s * 0.5 (lateral) * 0.5s = 3.0m East (X = 3.0)
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(500));

            app.update();

            let tf = app.world().get::<Transform>(player_entity).unwrap();
            assert!(
                (tf.translation.x - 3.0).abs() < 0.01,
                "Player moving laterally should move at 50% normal speed (3.0m), got X = {}",
                tf.translation.x
            );
        }
    }

    #[test]
    fn test_player_weapon_swap_with_f_key() {
        let mut app = create_player_test_app();

        let player_entity = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 0.05, 0.0),
                Player,
                Soldier {
                    facing_dir: Vec2::new(0.0, -1.0),
                    shoot_cooldown: Timer::from_seconds(0.12, TimerMode::Once),
                    stance: Stance::Standing,
                    inventory: crate::common::types::Inventory::player_default(),
                },
            ))
            .id();

        // Initially equipped with Rifle
        let soldier = app.world().get::<Soldier>(player_entity).unwrap();
        assert_eq!(
            soldier.inventory.active_weapon().unwrap().weapon_type,
            crate::common::types::WeaponType::Rifle
        );

        // Press 'F' to initiate swap
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::KeyF);

        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(50));
        app.update();

        // Clear inputs so KeyF is no longer just_pressed
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.clear();

        // Swapping is in progress (takes 2.0s)
        let soldier = app.world().get::<Soldier>(player_entity).unwrap();
        assert!(soldier.inventory.is_swapping());
        assert_eq!(
            soldier.inventory.pending_weapon().unwrap().weapon_type,
            crate::common::types::WeaponType::Pistol
        );

        // Advance 2.1 seconds to finish swap
        let mut time = app.world_mut().resource_mut::<Time>();
        time.advance_by(std::time::Duration::from_millis(2100));
        app.update();

        // Now equipped with Pistol (17 rounds, reduced range)
        let soldier = app.world().get::<Soldier>(player_entity).unwrap();
        assert!(!soldier.inventory.is_swapping());
        let active = soldier.inventory.active_weapon().unwrap();
        assert_eq!(active.weapon_type, crate::common::types::WeaponType::Pistol);
        assert_eq!(active.ammo.current_mag, 17);
        assert_eq!(active.ammo.reserve_mags, 0);
        assert!(active.bullet_speed() * active.bullet_lifetime() < 30.0);
    }

    #[test]
    fn test_player_moving_shoot_has_spread_stationary_does_not() {
        let mut app = create_player_test_app();

        app.world_mut().spawn((
            Transform::from_xyz(0.0, 0.05, 0.0),
            Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0),
                shoot_cooldown: Timer::from_seconds(0.01, TimerMode::Once),
                stance: Stance::Standing,
                inventory: crate::common::types::Inventory::player_default(),
            },
        ));

        // 1. Fire while stationary
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Space);
        {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(20));
        }
        app.update();

        let stationary_bullets: Vec<Vec2> = app
            .world_mut()
            .query::<&Bullet>()
            .iter(app.world())
            .map(|b| b.direction)
            .collect();
        assert_eq!(stationary_bullets.len(), 1);
        assert!(
            (stationary_bullets[0] - Vec2::new(0.0, -1.0)).length() < 1e-5,
            "Stationary shot must have zero spread, got {:?}",
            stationary_bullets[0]
        );

        // 2. Fire multiple shots while moving laterally (KeyD)
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyD);
        for _ in 0..8 {
            let mut time = app.world_mut().resource_mut::<Time>();
            time.advance_by(std::time::Duration::from_millis(50));
            app.update();
        }

        let all_bullets: Vec<Vec2> = app
            .world_mut()
            .query::<&Bullet>()
            .iter(app.world())
            .map(|b| b.direction)
            .collect();
        assert!(all_bullets.len() > 2);
        let any_spread = all_bullets.iter().skip(1).any(|&dir| {
            let angle = Vec2::new(0.0, -1.0).angle_to(dir).abs();
            angle > 1e-3 && angle <= crate::common::geometry::MOVING_SHOOT_MAX_SPREAD_RAD + 1e-4
        });
        assert!(
            any_spread,
            "Moving shots should apply angular spread penalty"
        );
    }
}
