use bevy::prelude::*;
use crate::common::types::{SOLDIER_RADIUS, SOLDIER_WORLD_BOUND};
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
    // World boundary collision check derived from unified world dimensions
    pos.x = pos.x.clamp(-SOLDIER_WORLD_BOUND, SOLDIER_WORLD_BOUND);
    pos.y = pos.y.clamp(-SOLDIER_WORLD_BOUND, SOLDIER_WORLD_BOUND);
    pos
}

// Resolves collision of a circle against another circle (pushes pos out to radius + other_radius)
pub fn resolve_circle_circle_collision(
    mut pos: Vec2,
    radius: f32,
    other_pos: Vec2,
    other_radius: f32,
) -> Vec2 {
    let diff = pos - other_pos;
    let dist_sq = diff.length_squared();
    let min_dist = radius + other_radius;
    if dist_sq < min_dist * min_dist {
        if dist_sq > 0.00001 {
            let dist = dist_sq.sqrt();
            let overlap = min_dist - dist;
            pos += (diff / dist) * overlap;
        } else {
            pos.x += min_dist;
        }
    }
    pos
}

// Directional movement speed multiplier:
// Moving forward (in the direction currently facing) is 100% normal speed.
// Moving laterally (perpendicular / strafing) or backwards is 50% normal speed.
// Smoothly interpolates between 1.0 (forward) and 0.5 (lateral/backwards).
pub fn directional_speed_multiplier(facing_dir: Vec2, move_dir: Vec2) -> f32 {
    let facing_sq = facing_dir.length_squared();
    let move_sq = move_dir.length_squared();
    if facing_sq < 0.0001 || move_sq < 0.0001 {
        return 1.0;
    }

    let dot = (facing_dir.dot(move_dir) / (facing_sq.sqrt() * move_sq.sqrt())).clamp(-1.0, 1.0);
    if dot > 0.0 {
        0.5 + 0.5 * dot
    } else {
        0.5
    }
}

// Pairwise collision resolution system ensuring soldiers cannot occupy the same point in space
pub fn resolve_soldier_pairwise_collisions(
    mut soldiers_q: Query<(Entity, &mut Transform), With<crate::common::types::Soldier>>,
    buildings_q: Query<(&Transform, &BuildingCollider), (With<crate::world::buildings::Building>, Without<crate::common::types::Soldier>)>,
    low_walls_q: Query<(&Transform, &LowWallCollider), (With<crate::world::buildings::LowWall>, Without<crate::common::types::Soldier>)>,
) {
    let buildings: Vec<(&Transform, &BuildingCollider)> = buildings_q.iter().collect();
    let low_walls: Vec<(&Transform, &LowWallCollider)> = low_walls_q.iter().collect();
    let soldier_radius = SOLDIER_RADIUS;
    let min_dist = soldier_radius * 2.0;
    let min_dist_sq = min_dist * min_dist;

    for _ in 0..2 {
        let mut combinations = soldiers_q.iter_combinations_mut();
        while let Some([(_e1, mut tf1), (_e2, mut tf2)]) = combinations.fetch_next() {
            let p1 = Vec2::new(tf1.translation.x, tf1.translation.z);
            let p2 = Vec2::new(tf2.translation.x, tf2.translation.z);
            let diff = p1 - p2;
            let dist_sq = diff.length_squared();

            if dist_sq < min_dist_sq {
                let (push1, push2) = if dist_sq > 0.00001 {
                    let dist = dist_sq.sqrt();
                    let overlap = min_dist - dist;
                    let normal = diff / dist;
                    (normal * (overlap * 0.5), -normal * (overlap * 0.5))
                } else {
                    (Vec2::new(soldier_radius, 0.0), Vec2::new(-soldier_radius, 0.0))
                };

                let new_p1 = resolve_circle_obstacle_collision(p1 + push1, soldier_radius, &buildings, &low_walls);
                let new_p2 = resolve_circle_obstacle_collision(p2 + push2, soldier_radius, &buildings, &low_walls);

                tf1.translation.x = new_p1.x;
                tf1.translation.z = new_p1.y;
                tf2.translation.x = new_p2.x;
                tf2.translation.z = new_p2.y;
            }
        }
    }
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

    // 1. If direct direction is clear, proceed directly
    if !is_path_blocked(origin, desired_norm, 1.8, buildings, low_walls, soldier_radius) {
        return desired_norm;
    }

    // 2. Test alternate steering angles with standard probe distance (1.8m)
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

    // 3. Shorter probe fallback (0.9m) to allow navigating around tight corners and alleys
    for angle in ANGLES {
        let cos = angle.cos();
        let sin = angle.sin();
        let cand = Vec2::new(
            desired_norm.x * cos - desired_norm.y * sin,
            desired_norm.x * sin + desired_norm.y * cos,
        );
        if !is_path_blocked(origin, cand, 0.9, buildings, low_walls, soldier_radius) {
            return cand;
        }
    }

    // 4. Wall-sliding projection: calculate surface tangent from the nearest obstacle
    let mut closest_normal = Vec2::ZERO;
    let mut min_dist_sq = f32::MAX;
    for (b_tf, collider) in buildings {
        let b_center = Vec2::new(b_tf.translation.x, b_tf.translation.z);
        let closest = Vec2::new(
            origin.x.clamp(b_center.x - collider.half_size.x, b_center.x + collider.half_size.x),
            origin.y.clamp(b_center.y - collider.half_size.y, b_center.y + collider.half_size.y),
        );
        let diff = origin - closest;
        let d2 = diff.length_squared();
        if d2 < min_dist_sq && d2 < (soldier_radius * 2.2) * (soldier_radius * 2.2) {
            min_dist_sq = d2;
            if d2 > 0.0001 {
                closest_normal = diff.normalize();
            }
        }
    }
    for (w_tf, collider) in low_walls {
        let w_center = Vec2::new(w_tf.translation.x, w_tf.translation.z);
        let closest = Vec2::new(
            origin.x.clamp(w_center.x - collider.half_size.x, w_center.x + collider.half_size.x),
            origin.y.clamp(w_center.y - collider.half_size.y, w_center.y + collider.half_size.y),
        );
        let diff = origin - closest;
        let d2 = diff.length_squared();
        if d2 < min_dist_sq && d2 < (soldier_radius * 2.2) * (soldier_radius * 2.2) {
            min_dist_sq = d2;
            if d2 > 0.0001 {
                closest_normal = diff.normalize();
            }
        }
    }

    if closest_normal.length_squared() > 0.5 {
        let t1 = Vec2::new(-closest_normal.y, closest_normal.x);
        let t2 = Vec2::new(closest_normal.y, -closest_normal.x);
        let slide = if desired_norm.dot(t1) >= desired_norm.dot(t2) { t1 } else { t2 };
        if !is_path_blocked(origin, slide, 0.6, buildings, low_walls, soldier_radius) {
            return slide;
        }
    }

    desired_norm
}

/// Maximum angular spread (in radians, +/- ~8 degrees) applied to shots fired while moving.
pub const MOVING_SHOOT_MAX_SPREAD_RAD: f32 = 0.14;

/// Apply an accuracy penalty (angular spread) when firing while moving.
/// Stationary shots return `dir` unchanged.
pub fn apply_shoot_spread(dir: Vec2, is_moving: bool, seed: u64) -> Vec2 {
    if !is_moving {
        return dir;
    }
    // SplitMix64 mixing for deterministic, well-distributed pseudo-random angle
    let mut z = seed.wrapping_add(0x9e3779b97f4a7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z = z ^ (z >> 31);
    // Map to [-1.0, 1.0]
    let unit = ((z >> 11) as f64 / ((1u64 << 53) as f64)) * 2.0 - 1.0;
    let angle = (unit as f32) * MOVING_SHOOT_MAX_SPREAD_RAD;
    let (s, c) = angle.sin_cos();
    let norm = dir.normalize_or_zero();
    Vec2::new(norm.x * c - norm.y * s, norm.x * s + norm.y * c).normalize_or_zero()
}

/// Shortest Euclidean distance from `pos` to an axis-aligned box `(center, half_size)`.
pub fn distance_to_box(pos: Vec2, center: Vec2, half_size: Vec2) -> f32 {
    let min = center - half_size;
    let max = center + half_size;
    let clamped = pos.clamp(min, max);
    (pos - clamped).length()
}

/// Returns true if `pos` is within `max_dist` of any low wall collider box.
pub fn is_near_low_wall(
    pos: Vec2,
    low_walls: &[(&Transform, &LowWallCollider)],
    max_dist: f32,
) -> bool {
    low_walls.iter().any(|(w_tf, w_col)| {
        let center = Vec2::new(w_tf.translation.x, w_tf.translation.z);
        distance_to_box(pos, center, w_col.half_size) <= max_dist
    })
}

/// Returns true if the line of fire from `shooter_pos` to `target_pos` intersects any low wall.
pub fn has_low_wall_in_line_of_fire(
    shooter_pos: Vec2,
    target_pos: Vec2,
    low_walls: &[(&Transform, &LowWallCollider)],
) -> bool {
    low_walls.iter().any(|(w_tf, w_col)| {
        let center = Vec2::new(w_tf.translation.x, w_tf.translation.z);
        let min = center - w_col.half_size;
        let max = center + w_col.half_size;
        line_intersects_box(shooter_pos, target_pos, min, max)
    })
}

/// Finds the nearest valid cover position directly behind an obstruction (`LowWall` or `Building`)
/// that places the obstruction between the cover spot and `threat_pos`.
pub fn find_cover_position(
    soldier_pos: Vec2,
    threat_pos: Vec2,
    buildings: &[(&Transform, &BuildingCollider)],
    low_walls: &[(&Transform, &LowWallCollider)],
    soldier_radius: f32,
    search_radius: f32,
) -> Option<Vec2> {
    find_cover_position_with_offset(
        soldier_pos,
        threat_pos,
        buildings,
        low_walls,
        soldier_radius,
        search_radius,
        0.0,
    )
}

/// Like `find_cover_position`, with an optional `lateral_offset` along wide cover walls so
/// multiple squadmates can spread out side-by-side behind the same wall without stacking.
pub fn find_cover_position_with_offset(
    soldier_pos: Vec2,
    threat_pos: Vec2,
    buildings: &[(&Transform, &BuildingCollider)],
    low_walls: &[(&Transform, &LowWallCollider)],
    soldier_radius: f32,
    search_radius: f32,
    lateral_offset: f32,
) -> Option<Vec2> {
    let stand_off = soldier_radius + 0.25;
    let mut best_spot: Option<(Vec2, f32)> = None;

    let mut evaluate_box = |center: Vec2, half_size: Vec2, is_low_wall: bool| {
        let soldier_dist_to_box = distance_to_box(soldier_pos, center, half_size);
        if soldier_dist_to_box > search_radius {
            return;
        }
        // Only use obstructions that are on our side of the engagement (not closer to the threat)
        let threat_dist_to_box = distance_to_box(threat_pos, center, half_size);
        if threat_dist_to_box + 1.0 < soldier_dist_to_box {
            return;
        }

        let min = center - half_size;
        let max = center + half_size;
        let min_x = (min.x + soldier_radius).min(center.x);
        let max_x = (max.x - soldier_radius).max(center.x);
        let min_y = (min.y + soldier_radius).min(center.y);
        let max_y = (max.y - soldier_radius).max(center.y);

        let target_x = if max_x - min_x > 6.0 {
            (soldier_pos.x + lateral_offset).clamp(min_x, max_x)
        } else {
            soldier_pos.x.clamp(min_x, max_x)
        };
        let target_y = if max_y - min_y > 6.0 {
            (soldier_pos.y + lateral_offset).clamp(min_y, max_y)
        } else {
            soldier_pos.y.clamp(min_y, max_y)
        };

        let candidates = [
            // North face (-Z)
            Vec2::new(target_x, min.y - stand_off),
            // South face (+Z)
            Vec2::new(target_x, max.y + stand_off),
            // West face (-X)
            Vec2::new(min.x - stand_off, target_y),
            // East face (+X)
            Vec2::new(max.x + stand_off, target_y),
        ];

        for cand in candidates {
            // The obstruction must genuinely lie between `threat_pos` and `cand`
            if !line_intersects_box(threat_pos, cand, min, max) {
                continue;
            }
            let dist = (cand - soldier_pos).length();
            if dist > search_radius + 4.0 {
                continue;
            }
            let resolved =
                resolve_circle_obstacle_collision(cand, soldier_radius, buildings, low_walls);
            if (resolved - cand).length_squared() > 0.04 {
                continue;
            }
            // Prefer low walls slightly because soldiers can see and shoot over them
            let score = if is_low_wall {
                (dist - 1.5).max(0.0)
            } else {
                dist
            };
            if best_spot.map_or(true, |(_, best_score)| score < best_score) {
                best_spot = Some((resolved, score));
            }
        }
    };

    for (w_tf, w_col) in low_walls {
        let center = Vec2::new(w_tf.translation.x, w_tf.translation.z);
        evaluate_box(center, w_col.half_size, true);
    }
    for (b_tf, b_col) in buildings {
        let center = Vec2::new(b_tf.translation.x, b_tf.translation.z);
        evaluate_box(center, b_col.half_size, false);
    }

    best_spot.map(|(spot, _)| spot)
}

/// Returns true if the shooter at `shooter_pos` has a clear, unobstructed line of fire to `target_pos`
/// that is not blocked by any building OR any waist-high low wall.
pub fn has_clear_shot_past_cover(
    shooter_pos: Vec2,
    target_pos: Vec2,
    buildings: &[(&Transform, &BuildingCollider)],
    low_walls: &[(&Transform, &LowWallCollider)],
) -> bool {
    if has_low_wall_in_line_of_fire(shooter_pos, target_pos, low_walls) {
        return false;
    }
    for (b_tf, b_col) in buildings {
        let center = Vec2::new(b_tf.translation.x, b_tf.translation.z);
        let min = center - b_col.half_size;
        let max = center + b_col.half_size;
        if line_intersects_box(shooter_pos, target_pos, min, max) {
            return false;
        }
    }
    true
}

/// Computes the next flanking waypoint along cover towards an enfilade position (perpendicular to
/// enemy cover) where the flanking soldier has a clear, unobstructed shot on `enemy_pos`.
pub fn compute_flank_waypoint(
    soldier_pos: Vec2,
    enemy_pos: Vec2,
    flank_right: bool,
    slot: usize,
    buildings: &[(&Transform, &BuildingCollider)],
    low_walls: &[(&Transform, &LowWallCollider)],
    soldier_radius: f32,
) -> Vec2 {
    // Determine outer lateral bounds of cover structures in the engagement area
    let mut min_x = enemy_pos.x - 8.0;
    let mut max_x = enemy_pos.x + 8.0;

    for (w_tf, w_col) in low_walls {
        let center = Vec2::new(w_tf.translation.x, w_tf.translation.z);
        if (center - enemy_pos).length() < 30.0 || (center - soldier_pos).length() < 30.0 {
            min_x = min_x.min(center.x - w_col.half_size.x);
            max_x = max_x.max(center.x + w_col.half_size.x);
        }
    }
    for (b_tf, b_col) in buildings {
        let center = Vec2::new(b_tf.translation.x, b_tf.translation.z);
        if (center - enemy_pos).length() < 30.0 || (center - soldier_pos).length() < 30.0 {
            min_x = min_x.min(center.x - b_col.half_size.x);
            max_x = max_x.max(center.x + b_col.half_size.x);
        }
    }

    let stand_off = soldier_radius + 0.55;
    let flank_x = if flank_right {
        max_x + stand_off
    } else {
        min_x - stand_off
    };

    // Stagger slots slightly near the enemy's depth so flanking fire team members don't stack
    let slot_offset = (slot as f32 - 1.5) * 1.3;
    let target_y = enemy_pos.y + slot_offset * 0.4;
    let final_spot = Vec2::new(flank_x, target_y);

    // Stage 1: Move laterally behind current cover out to the flank corridor (`flank_x`) first
    // before advancing towards the enemy depth (`target_y`), so flankers don't cut across the open center.
    if (soldier_pos.x - flank_x).abs() > 2.0 && (soldier_pos.y - enemy_pos.y).abs() > 4.0 {
        let corner = Vec2::new(flank_x, soldier_pos.y);
        resolve_circle_obstacle_collision(corner, soldier_radius, buildings, low_walls)
    } else {
        resolve_circle_obstacle_collision(final_spot, soldier_radius, buildings, low_walls)
    }
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
    fn test_wall_sliding_tangent_avoids_getting_stuck() {
        // Soldier flush against the west face of a building (at x = -4.6, half_size = 4.0)
        let dummy_tf = Transform::from_xyz(0.0, 0.0, 0.0);
        let dummy_collider = BuildingCollider {
            half_size: Vec2::new(4.0, 10.0),
        };
        let buildings = [(&dummy_tf, &dummy_collider)];

        let origin = Vec2::new(-4.55, 0.0);
        // Desired direction points straight east into the solid building
        let desired = Vec2::new(1.0, 0.0);
        let steered = find_avoidance_direction(origin, desired, &buildings, &[], 0.6);

        // Steering must find a non-zero sliding direction parallel to the wall
        assert!(steered.y.abs() > 0.5);
        assert!(!is_path_blocked(origin, steered, 0.6, &buildings, &[], 0.6));
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

    #[test]
    fn test_circle_circle_collision() {
        let pos_a = Vec2::new(0.0, 0.0);
        let pos_b = Vec2::new(0.8, 0.0);
        let radius = 0.6; // min separation is 1.2

        let resolved_a = resolve_circle_circle_collision(pos_a, radius, pos_b, radius);
        let dist = (resolved_a - pos_b).length();
        assert!(dist >= 1.199);
    }

    #[test]
    fn test_world_border_collision() {
        // Position at South gate (y = 48.0) in the expanded 120x120 world should NOT be clamped to 28.0
        let pos = Vec2::new(0.0, 48.0);
        let resolved = resolve_circle_obstacle_collision(pos, SOLDIER_RADIUS, &[], &[]);
        assert_eq!(resolved.y, 48.0);

        // Position exceeding world bounds should be clamped to derived SOLDIER_WORLD_BOUND
        let out_of_bounds = Vec2::new(75.0, -80.0);
        let resolved_oob = resolve_circle_obstacle_collision(out_of_bounds, SOLDIER_RADIUS, &[], &[]);
        assert_eq!(resolved_oob.x, SOLDIER_WORLD_BOUND);
        assert_eq!(resolved_oob.y, -SOLDIER_WORLD_BOUND);
    }

    #[test]
    fn test_directional_speed_multiplier() {
        let facing = Vec2::new(0.0, -1.0); // Facing North

        // 1. Moving forward (same direction) -> 100% normal speed
        let forward = Vec2::new(0.0, -1.0);
        let mult = directional_speed_multiplier(facing, forward);
        assert!((mult - 1.0).abs() < 0.001, "Forward multiplier should be 1.0, got {}", mult);

        // 2. Moving laterally right (perpendicular East) -> 50% normal speed
        let right = Vec2::new(1.0, 0.0);
        let mult = directional_speed_multiplier(facing, right);
        assert!((mult - 0.5).abs() < 0.001, "Lateral multiplier should be 0.5, got {}", mult);

        // 3. Moving laterally left (perpendicular West) -> 50% normal speed
        let left = Vec2::new(-1.0, 0.0);
        let mult = directional_speed_multiplier(facing, left);
        assert!((mult - 0.5).abs() < 0.001, "Lateral multiplier should be 0.5, got {}", mult);

        // 4. Moving backwards (opposite direction South) -> 50% normal speed
        let backwards = Vec2::new(0.0, 1.0);
        let mult = directional_speed_multiplier(facing, backwards);
        assert!((mult - 0.5).abs() < 0.001, "Backwards multiplier should be 0.5, got {}", mult);

        // 5. Moving diagonal backwards-left (South-West) -> 50% normal speed
        let back_diag = Vec2::new(-1.0, 1.0);
        let mult = directional_speed_multiplier(facing, back_diag);
        assert!((mult - 0.5).abs() < 0.001, "Backwards-diagonal multiplier should be 0.5, got {}", mult);

        // 6. Moving diagonal forward-right (North-East, 45 deg) -> smooth transition (~0.854)
        let fwd_diag = Vec2::new(1.0, -1.0);
        let mult = directional_speed_multiplier(facing, fwd_diag);
        let expected = 0.5 + 0.5 * std::f32::consts::FRAC_1_SQRT_2;
        assert!((mult - expected).abs() < 0.01, "Diagonal forward multiplier should be ~{:.3}, got {}", expected, mult);
    }

    #[test]
    fn test_apply_shoot_spread_stationary_vs_moving() {
        let dir = Vec2::new(0.0, -1.0);

        // Stationary shots have zero spread penalty
        for seed in 0..10u64 {
            let out = apply_shoot_spread(dir, false, seed);
            assert!((out - dir).length() < 1e-6, "Stationary shot should have zero spread");
        }

        // Moving shots apply bounded angular spread within MOVING_SHOOT_MAX_SPREAD_RAD
        let mut any_deviated = false;
        for seed in 0..20u64 {
            let out = apply_shoot_spread(dir, true, seed);
            let angle = dir.angle_to(out).abs();
            assert!(
                angle <= MOVING_SHOOT_MAX_SPREAD_RAD + 1e-4,
                "Moving spread angle {} exceeded max {}",
                angle,
                MOVING_SHOOT_MAX_SPREAD_RAD
            );
            if angle > 1e-3 {
                any_deviated = true;
            }
        }
        assert!(any_deviated, "At least some moving shots should deviate from exact aim");
    }

    #[test]
    fn test_find_cover_position_places_wall_between_soldier_and_threat() {
        let north_wall_tf = Transform::from_xyz(0.0, 0.375, -8.0);
        let south_wall_tf = Transform::from_xyz(0.0, 0.375, 8.0);
        let wall_col = LowWallCollider {
            half_size: Vec2::new(8.0, 0.5),
        };
        let low_walls = [(&north_wall_tf, &wall_col), (&south_wall_tf, &wall_col)];

        // Friendly on south side (0, 12.0) under fire from enemy on north side (0, -10.5)
        let friendly_cover = find_cover_position(
            Vec2::new(0.0, 12.0),
            Vec2::new(0.0, -10.5),
            &[],
            &low_walls,
            SOLDIER_RADIUS,
            16.0,
        )
        .expect("Should find cover behind south courtyard wall");
        assert!(
            (friendly_cover.y - 9.35).abs() < 0.1,
            "Friendly cover spot should be directly behind south wall (z ≈ 9.35), got {:?}",
            friendly_cover
        );

        // Enemy on north side (-3.0, -11.5) under fire from friendly on south side (0, 12.0)
        let enemy_cover = find_cover_position(
            Vec2::new(-3.0, -11.5),
            Vec2::new(0.0, 12.0),
            &[],
            &low_walls,
            SOLDIER_RADIUS,
            16.0,
        )
        .expect("Should find cover behind north courtyard wall");
        assert!(
            (enemy_cover.y - (-9.35)).abs() < 0.1 && (enemy_cover.x - (-3.0)).abs() < 0.1,
            "Enemy cover spot should be directly behind north wall (-3.0, -9.35), got {:?}",
            enemy_cover
        );
    }
}

