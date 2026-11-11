use std::f32::consts::FRAC_PI_2;
use bevy::prelude::*;
use crate::combat::bullet::{Bullet, BulletAssets};
use crate::common::geometry::{
    line_intersects_box, resolve_circle_circle_collision, resolve_circle_obstacle_collision,
};
use crate::common::types::{Soldier, Team};
use crate::world::buildings::{BuildingCollider, LowWallCollider};

#[derive(Component)]
pub struct Player;

pub fn setup_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let pointer_mesh = meshes.add(Circle::new(0.15));
    let player_soldier_mesh = meshes.add(Circle::new(0.6));
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
            Mesh3d(player_soldier_mesh),
            MeshMaterial3d(player_soldier_material),
            Transform::from_xyz(0.0, 0.05, 20.0)
                .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
            Player,
            Team::Player,
            Soldier {
                facing_dir: Vec2::new(0.0, -1.0), // Facing North (-Z)
                shoot_cooldown: Timer::from_seconds(0.12, TimerMode::Once),
                is_crouched: false,
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
) {
    let Ok((mut soldier_tf, mut soldier)) = player_q.single_mut() else {
        return;
    };

    // 1. Crouch toggle (C button) & visual scale feedback
    if keyboard_input.just_pressed(KeyCode::KeyC) {
        soldier.is_crouched = !soldier.is_crouched;
    }

    soldier_tf.scale = if soldier.is_crouched {
        Vec3::splat(0.72) // Compact profile indicating crouching posture
    } else {
        Vec3::ONE
    };

    // 2. WASD / Arrow keys movement with obstacle & soldier collision
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
        let speed = if soldier.is_crouched {
            base_speed * 0.55 // Slower tactical crouch-walking
        } else {
            base_speed
        };
        let delta = move_delta.normalize() * speed * time.delta_secs();
        let soldier_radius = 0.6;

        let mut new_pos = Vec2::new(
            soldier_tf.translation.x + delta.x,
            soldier_tf.translation.z + delta.z,
        );

        // Resolve collision against other soldiers (cannot occupy same space)
        for other_tf in &other_soldiers_q {
            let other_pos = Vec2::new(other_tf.translation.x, other_tf.translation.z);
            new_pos = resolve_circle_circle_collision(new_pos, soldier_radius, other_pos, soldier_radius);
        }

        let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();
        let low_walls: Vec<(&Transform, &LowWallCollider)> = low_walls_q.iter().collect();
        let resolved =
            resolve_circle_obstacle_collision(new_pos, soldier_radius, &buildings, &low_walls);
        soldier_tf.translation.x = resolved.x;
        soldier_tf.translation.z = resolved.y;
    }

    // 3. Mouse aim direction for vision cone & soldier orientation
    let Ok(window) = windows.single() else {
        return;
    };
    let Ok((camera, camera_gt)) = camera_q.single() else {
        return;
    };

    if let Some(cursor_pos) = window.cursor_position() {
        if let Ok(ray) = camera.viewport_to_world(camera_gt, cursor_pos) {
            if let Some(dist) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) {
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

    // 4. Spacebar shooting (Player team)
    soldier.shoot_cooldown.tick(time.delta());
    let should_shoot = keyboard_input.just_pressed(KeyCode::Space)
        || (keyboard_input.pressed(KeyCode::Space) && soldier.shoot_cooldown.is_finished());

    if should_shoot {
        soldier.shoot_cooldown.reset();
        let soldier_pos = Vec2::new(soldier_tf.translation.x, soldier_tf.translation.z);
        let spawn_pos = soldier_pos + soldier.facing_dir * 0.85;

        // When crouched, shots aimed directly into/across an adjacent low wall cannot pass through
        let mut muzzle_blocked = false;
        if soldier.is_crouched {
            for (w_tf, w_collider) in &low_walls_q {
                let w_center = Vec2::new(w_tf.translation.x, w_tf.translation.z);
                let w_min = w_center - w_collider.half_size;
                let w_max = w_center + w_collider.half_size;

                let aim_probe = spawn_pos + soldier.facing_dir * 0.5;
                if line_intersects_box(soldier_pos, aim_probe, w_min, w_max)
                    || (spawn_pos.x >= w_min.x && spawn_pos.x <= w_max.x && spawn_pos.y >= w_min.y && spawn_pos.y <= w_max.y)
                {
                    muzzle_blocked = true;
                    break;
                }
            }
        }

        if !muzzle_blocked {
            commands.spawn((
                Mesh3d(bullet_assets.mesh.clone()),
                MeshMaterial3d(bullet_assets.player_material.clone()),
                Transform::from_xyz(spawn_pos.x, 0.08, spawn_pos.y)
                    .with_rotation(Quat::from_rotation_x(-FRAC_PI_2)),
                Bullet {
                    origin: soldier_pos,
                    direction: soldier.facing_dir,
                    speed: 45.0,
                    lifetime: 1.5,
                    team: Team::Player,
                    shooter_crouched: soldier.is_crouched,
                },
            ));
        }
    }
}

