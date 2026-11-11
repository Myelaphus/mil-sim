use bevy::prelude::*;
use crate::world::buildings::{BuildingCollider, LowWallCollider};

// Multi-pass circle vs obstacle (buildings + low walls) collision resolver with smooth sliding
pub fn resolve_circle_obstacle_collision(
    mut pos: Vec2,
    radius: f32,
    buildings: &[(&Transform, &BuildingCollider)],
    low_walls: &[(&Transform, &LowWallCollider)],
) -> Vec2 {
    for _ in 0..4 {
        // 1. Resolve collision against buildings
        for (b_tf, collider) in buildings {
            let b_center = Vec2::new(b_tf.translation.x, b_tf.translation.z);
            let closest = Vec2::new(
                pos.x.clamp(
                    b_center.x - collider.half_size.x,
                    b_center.x + collider.half_size.x,
                ),
                pos.y.clamp(
                    b_center.y - collider.half_size.y,
                    b_center.y + collider.half_size.y,
                ),
            );

            let diff = pos - closest;
            let dist_sq = diff.length_squared();

            if dist_sq < radius * radius {
                if dist_sq > 0.00001 {
                    let dist = dist_sq.sqrt();
                    let overlap = radius - dist;
                    pos += (diff / dist) * overlap;
                } else {
                    let d_left = (pos.x - (b_center.x - collider.half_size.x)).abs();
                    let d_right = ((b_center.x + collider.half_size.x) - pos.x).abs();
                    let d_top = (pos.y - (b_center.y - collider.half_size.y)).abs();
                    let d_bottom = ((b_center.y + collider.half_size.y) - pos.y).abs();
                    let min_d = d_left.min(d_right).min(d_top).min(d_bottom);
                    if min_d == d_left {
                        pos.x = b_center.x - collider.half_size.x - radius;
                    } else if min_d == d_right {
                        pos.x = b_center.x + collider.half_size.x + radius;
                    } else if min_d == d_top {
                        pos.y = b_center.y - collider.half_size.y - radius;
                    } else {
                        pos.y = b_center.y + collider.half_size.y + radius;
                    }
                }
            }
        }

        // 2. Resolve collision against low walls
        for (w_tf, collider) in low_walls {
            let w_center = Vec2::new(w_tf.translation.x, w_tf.translation.z);
            let closest = Vec2::new(
                pos.x.clamp(
                    w_center.x - collider.half_size.x,
                    w_center.x + collider.half_size.x,
                ),
                pos.y.clamp(
                    w_center.y - collider.half_size.y,
                    w_center.y + collider.half_size.y,
                ),
            );

            let diff = pos - closest;
            let dist_sq = diff.length_squared();

            if dist_sq < radius * radius {
                if dist_sq > 0.00001 {
                    let dist = dist_sq.sqrt();
                    let overlap = radius - dist;
                    pos += (diff / dist) * overlap;
                } else {
                    let d_left = (pos.x - (w_center.x - collider.half_size.x)).abs();
                    let d_right = ((w_center.x + collider.half_size.x) - pos.x).abs();
                    let d_top = (pos.y - (w_center.y - collider.half_size.y)).abs();
                    let d_bottom = ((w_center.y + collider.half_size.y) - pos.y).abs();
                    let min_d = d_left.min(d_right).min(d_top).min(d_bottom);
                    if min_d == d_left {
                        pos.x = w_center.x - collider.half_size.x - radius;
                    } else if min_d == d_right {
                        pos.x = w_center.x + collider.half_size.x + radius;
                    } else if min_d == d_top {
                        pos.y = w_center.y - collider.half_size.y - radius;
                    } else {
                        pos.y = w_center.y + collider.half_size.y + radius;
                    }
                }
            }
        }
    }
    pos.x = pos.x.clamp(-28.0, 28.0);
    pos.y = pos.y.clamp(-28.0, 28.0);
    pos
}

// Multi-pass circle vs building collision resolver (backward-compatible)
#[allow(dead_code)]
pub fn resolve_circle_building_collision(
    pos: Vec2,
    radius: f32,
    buildings: &[(&Transform, &BuildingCollider)],
) -> Vec2 {
    resolve_circle_obstacle_collision(pos, radius, buildings, &[])
}

pub fn can_see_target(
    seer_pos: Vec2,
    seer_dir: Vec2,
    target_pos: Vec2,
    vision_range: f32,
    vision_half_angle: f32,
    inner_radius: f32,
    buildings: &[(&Transform, &BuildingCollider)],
) -> bool {
    let to_point = target_pos - seer_pos;
    let dist = to_point.length();

    if dist > vision_range {
        return false;
    }

    let in_inner = dist <= inner_radius;
    let in_cone = if dist > 0.001 {
        let dir = to_point / dist;
        let dot = dir.dot(seer_dir.normalize_or_zero());
        dot >= vision_half_angle.cos()
    } else {
        true
    };

    if !in_inner && !in_cone {
        return false;
    }

    // Line of sight check against buildings
    for (b_tf, collider) in buildings {
        let b_center = Vec2::new(b_tf.translation.x, b_tf.translation.z);
        let b_min = b_center - collider.half_size;
        let b_max = b_center + collider.half_size;

        if line_intersects_box(seer_pos, target_pos, b_min, b_max) {
            return false;
        }
    }

    true
}

pub fn line_intersects_box(origin: Vec2, target: Vec2, b_min: Vec2, b_max: Vec2) -> bool {
    // If target point is inside this building, it does not occlude itself
    if target.x >= b_min.x && target.x <= b_max.x && target.y >= b_min.y && target.y <= b_max.y {
        return false;
    }

    let delta = target - origin;
    let mut tmin = 0.0_f32;
    let mut tmax = 1.0_f32;

    if delta.x.abs() > 0.0001 {
        let inv_dx = 1.0 / delta.x;
        let mut t1 = (b_min.x - origin.x) * inv_dx;
        let mut t2 = (b_max.x - origin.x) * inv_dx;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        tmin = tmin.max(t1);
        tmax = tmax.min(t2);
        if tmin > tmax {
            return false;
        }
    } else if origin.x < b_min.x || origin.x > b_max.x {
        return false;
    }

    if delta.y.abs() > 0.0001 {
        let inv_dy = 1.0 / delta.y;
        let mut t1 = (b_min.y - origin.y) * inv_dy;
        let mut t2 = (b_max.y - origin.y) * inv_dy;
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        tmin = tmin.max(t1);
        tmax = tmax.min(t2);
        if tmin > tmax {
            return false;
        }
    } else if origin.y < b_min.y || origin.y > b_max.y {
        return false;
    }

    tmin < 0.999 && tmax > 0.001
}

// Checks if moving from `origin` in `dir` by `probe_dist` penetrates any building or low wall obstacle
pub fn is_path_blocked(
    origin: Vec2,
    dir: Vec2,
    probe_dist: f32,
    buildings: &[(&Transform, &BuildingCollider)],
    low_walls: &[(&Transform, &LowWallCollider)],
    soldier_radius: f32,
) -> bool {
    let check_radius = soldier_radius * 0.92;
    let steps = [probe_dist * 0.35, probe_dist * 0.7, probe_dist];
    for step in steps {
        let probe = origin + dir * step;
        for (b_tf, collider) in buildings {
            let b_center = Vec2::new(b_tf.translation.x, b_tf.translation.z);
            let closest = Vec2::new(
                probe.x.clamp(
                    b_center.x - collider.half_size.x,
                    b_center.x + collider.half_size.x,
                ),
                probe.y.clamp(
                    b_center.y - collider.half_size.y,
                    b_center.y + collider.half_size.y,
                ),
            );
            if (probe - closest).length_squared() < check_radius * check_radius {
                return true;
            }
        }
        for (w_tf, collider) in low_walls {
            let w_center = Vec2::new(w_tf.translation.x, w_tf.translation.z);
            let closest = Vec2::new(
                probe.x.clamp(
                    w_center.x - collider.half_size.x,
                    w_center.x + collider.half_size.x,
                ),
                probe.y.clamp(
                    w_center.y - collider.half_size.y,
                    w_center.y + collider.half_size.y,
                ),
            );
            if (probe - closest).length_squared() < check_radius * check_radius {
                return true;
            }
        }
    }
    false
}

// Determines the best unobstructed direction toward the desired goal, steering around obstacles
pub fn find_avoidance_direction(
    origin: Vec2,
    desired_dir: Vec2,
    buildings: &[(&Transform, &BuildingCollider)],
    low_walls: &[(&Transform, &LowWallCollider)],
    soldier_radius: f32,
) -> Vec2 {
    if desired_dir.length_squared() < 0.001 {
        return desired_dir;
    }
    let desired_norm = desired_dir.normalize();

    // If direct direction is clear, proceed directly
    if !is_path_blocked(origin, desired_norm, 1.8, buildings, low_walls, soldier_radius) {
        return desired_norm;
    }

    // Branch alternate steering angles (left/right up to ~150 degrees)
    const ANGLES: [f32; 10] = [
        0.52, -0.52, // ~30 deg
        1.05, -1.05, // ~60 deg
        1.57, -1.57, // ~90 deg
        2.09, -2.09, // ~120 deg
        2.62, -2.62, // ~150 deg
    ];

    for angle in ANGLES {
        let cos = angle.cos();
        let sin = angle.sin();
        let cand = Vec2::new(
            desired_norm.x * cos - desired_norm.y * sin,
            desired_norm.x * sin + desired_norm.y * cos,
        );
        if !is_path_blocked(origin, cand, 1.8, buildings, low_walls, soldier_radius) {
            return cand;
        }
    }

    desired_norm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_circle_aabb_collision() {
        let b_center = Vec2::new(10.0, 0.0);
        let half_size = Vec2::new(4.0, 4.0);
        let soldier_radius = 0.6;

        let pos = Vec2::new(6.2, 0.0);
        let closest = Vec2::new(
            pos.x.clamp(b_center.x - half_size.x, b_center.x + half_size.x),
            pos.y.clamp(b_center.y - half_size.y, b_center.y + half_size.y),
        );
        let diff = pos - closest;
        let dist_sq = diff.length_squared();
        assert!(dist_sq < soldier_radius * soldier_radius);
    }

    #[test]
    fn test_enemy_visibility_in_fog() {
        let player_pos = Vec2::ZERO;
        let player_dir = Vec2::new(0.0, -1.0);
        let vision_range = 26.0;
        let vision_half_angle = 45.0_f32.to_radians();
        let inner_radius = 2.5;

        let north_point = Vec2::new(0.0, -10.0);
        assert!(can_see_target(
            player_pos,
            player_dir,
            north_point,
            vision_range,
            vision_half_angle,
            inner_radius,
            &[],
        ));

        let south_point = Vec2::new(0.0, 10.0);
        assert!(!can_see_target(
            player_pos,
            player_dir,
            south_point,
            vision_range,
            vision_half_angle,
            inner_radius,
            &[],
        ));
    }

    #[test]
    fn test_can_see_target_occlusion() {
        let seer_pos = Vec2::new(0.0, -15.0);
        let seer_dir = Vec2::new(0.0, 1.0);
        let target_pos = Vec2::new(0.0, 15.0);

        let dummy_tf = Transform::from_xyz(0.0, 0.0, 0.0);
        let dummy_collider = BuildingCollider {
            half_size: Vec2::new(4.0, 4.0),
        };
        let buildings = [(&dummy_tf, &dummy_collider)];

        assert!(!can_see_target(
            seer_pos,
            seer_dir,
            target_pos,
            40.0,
            45.0_f32.to_radians(),
            2.0,
            &buildings,
        ));
    }

    #[test]
    fn test_resolve_circle_collision() {
        let b_center = Vec2::new(0.0, 0.0);
        let b_half_size = Vec2::new(5.0, 5.0);
        let soldier_radius = 0.6;

        let pos = Vec2::new(0.0, -5.2);
        let dummy_tf = Transform::from_xyz(b_center.x, 0.0, b_center.y);
        let dummy_collider = BuildingCollider { half_size: b_half_size };
        let buildings = [(&dummy_tf, &dummy_collider)];

        let resolved = resolve_circle_building_collision(pos, soldier_radius, &buildings);
        assert!(resolved.y <= -5.6);
    }

    #[test]
    fn test_obstacle_avoidance_steering() {
        let dummy_tf = Transform::from_xyz(0.0, 0.0, 0.0);
        let dummy_collider = BuildingCollider {
            half_size: Vec2::new(4.0, 4.0),
        };
        let buildings = [(&dummy_tf, &dummy_collider)];

        let origin = Vec2::new(0.0, -4.6);
        let desired = Vec2::new(0.0, 1.0);
        let steered = find_avoidance_direction(origin, desired, &buildings, &[], 0.6);

        assert!(steered.x.abs() > 0.4);
        assert!(!is_path_blocked(origin, steered, 1.5, &buildings, &[], 0.6));
    }

    #[test]
    fn test_low_wall_collision() {
        let wall_tf = Transform::from_xyz(0.0, 0.0, 0.0);
        let wall_collider = LowWallCollider {
            half_size: Vec2::new(2.0, 0.4),
        };
        let low_walls = [(&wall_tf, &wall_collider)];

        // Soldier trying to walk into south face of low wall
        let pos = Vec2::new(0.0, 0.5);
        let resolved = resolve_circle_obstacle_collision(pos, 0.6, &[], &low_walls);
        assert!(resolved.y >= 1.0);
    }
}

