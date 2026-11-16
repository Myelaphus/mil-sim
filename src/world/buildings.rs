use crate::common::types::CurrentLevel;
use bevy::prelude::*;

#[derive(Component)]
pub struct Building;

#[derive(Component, Clone, Copy, Debug)]
pub struct BuildingCollider {
    pub half_size: Vec2,
}

#[derive(Component)]
pub struct LowWall;

#[derive(Component, Clone, Copy, Debug)]
pub struct LowWallCollider {
    pub half_size: Vec2,
}

#[derive(Component)]
pub struct Road;

/// Marker component for individual forest trees in `test_level_4`.
/// Each tree carries a `LowWallCollider` for its trunk so infantry (`1.2m` diameter) can walk
/// between trees while the wider APC (`3.4m` hull width) is blocked from driving through the forest.
#[derive(Component)]
pub struct Tree;

/// Half-size (`m`) of each tree trunk's ground-level collision box (`0.90m` trunk width).
pub const TREE_TRUNK_HALF_SIZE: f32 = 0.45;

/// Marker component for the topographic ridgeline contour layers on the left (West, `-X`) side of the Level 4 village.
#[derive(Component)]
pub struct Ridgeline;

/// Marker component for negative-elevation trench/gully terrain features (`h < 0.0m`).
#[derive(Component)]
pub struct Trench;

/// Circular negative-elevation depression (`h < 0.0m`), such as an artillery shell crater or foxhole.
/// Units inside a depression sit below base elevation (`0.0m`) and appear slightly smaller.
#[derive(Component, Clone, Copy, Debug)]
pub struct ElevationDepression {
    pub center: Vec2,
    pub radius: f32,
    pub depth: f32,
}

/// Western Ridgeline geometry constants on the left (`-X`) side of the `test_level_4` village:
/// - Spans `X in [-96.0, -44.0]` with its military crest at `X = -68.0` and peak height `+7.0m`.
/// - Forward (eastern) slope rises from `X = -44.0` (`24.0m` run to crest), reverse (western) slope
///   descends from `X = -68.0` to `X = -96.0` (`28.0m` run).
/// - Runs North–South centered at `Z = -5.0` with a `84.0m` main crest (`Z in [-47.0, 37.0]`) and
///   smooth `26.0m` north/south shoulders (`Z in [-73.0, 63.0]`).
pub const RIDGE_CREST_X: f32 = -68.0;
pub const RIDGE_EAST_TOE_X: f32 = -44.0;
pub const RIDGE_WEST_TOE_X: f32 = -96.0;
pub const RIDGE_EAST_WIDTH: f32 = 24.0;
pub const RIDGE_WEST_WIDTH: f32 = 28.0;
pub const RIDGE_Z_CENTER: f32 = -5.0;
pub const RIDGE_Z_FLAT_HALF: f32 = 42.0;
pub const RIDGE_Z_SHOULDER: f32 = 26.0;
pub const RIDGE_MAX_HEIGHT: f32 = 7.0;

/// Sunken defensive trench / gully depression (`h < 0.0m`) in `test_level_4` between the ridgeline toe and the village:
pub const LEVEL_4_TRENCH_CENTER: Vec2 = Vec2::new(-38.0, 24.0);
pub const LEVEL_4_TRENCH_HALF_WIDTH_X: f32 = 3.2;
pub const LEVEL_4_TRENCH_FLAT_HALF_Z: f32 = 6.0;
pub const LEVEL_4_TRENCH_SHOULDER_Z: f32 = 3.0;
pub const LEVEL_4_TRENCH_DEPTH: f32 = 1.8;

/// Computes the analytical static terrain elevation (`m`) at `pos` (`(x, z)`) for `test_level_4`,
/// combining the positive-elevation Western Ridgeline (`0.0m .. +7.0m`) and the negative-elevation
/// sunken trench (`0.0m .. -1.8m`).
///
/// This exact polynomial formula is mirrored in `assets/shaders/fog.wgsl` for GPU Fog-of-War
/// ridgeline occlusion and high-ground range calculation.
pub fn base_terrain_elevation(pos: Vec2) -> f32 {
    let mut h = 0.0_f32;

    // 1. Positive elevation: Western Ridgeline (`X in [-96.0, -44.0], Z in [-73.0, 63.0]`)
    if pos.x > RIDGE_WEST_TOE_X && pos.x < RIDGE_EAST_TOE_X {
        let u = if pos.x >= RIDGE_CREST_X {
            (pos.x - RIDGE_CREST_X) / RIDGE_EAST_WIDTH
        } else {
            (RIDGE_CREST_X - pos.x) / RIDGE_WEST_WIDTH
        };
        let vz = ((pos.y - RIDGE_Z_CENTER).abs() - RIDGE_Z_FLAT_HALF).max(0.0) / RIDGE_Z_SHOULDER;
        if u < 1.0 && vz < 1.0 {
            let ux = 1.0 - u * u;
            let uz = 1.0 - vz * vz;
            h += RIDGE_MAX_HEIGHT * (ux * ux) * (uz * uz);
        }
    }

    // 2. Negative elevation: Sunken defensive trench (`X in [-41.2, -34.8], Z in [15.0, 33.0]`)
    let tx = (pos.x - LEVEL_4_TRENCH_CENTER.x).abs() / LEVEL_4_TRENCH_HALF_WIDTH_X;
    let tz = ((pos.y - LEVEL_4_TRENCH_CENTER.y).abs() - LEVEL_4_TRENCH_FLAT_HALF_Z).max(0.0)
        / LEVEL_4_TRENCH_SHOULDER_Z;
    if tx < 1.0 && tz < 1.0 {
        let wx = 1.0 - tx * tx;
        let wz = 1.0 - tz * tz;
        h -= LEVEL_4_TRENCH_DEPTH * (wx * wx) * (wz * wz);
    }

    h
}

/// Returns the static terrain elevation (`m`) at `pos` for `level` (`0.0m` in Levels 1–3; ridgeline & trench in Level 4).
pub fn terrain_elevation(pos: Vec2, level: CurrentLevel) -> f32 {
    if level == CurrentLevel::Level4 {
        base_terrain_elevation(pos)
    } else {
        0.0
    }
}

/// Returns the terrain elevation (`m`) at `pos` including any dynamic `ElevationDepression` craters/trenches.
pub fn terrain_elevation_with_depressions(
    pos: Vec2,
    level: CurrentLevel,
    depressions: &[ElevationDepression],
) -> f32 {
    let mut h = terrain_elevation(pos, level);
    let mut max_crater_drop = 0.0_f32;
    for dep in depressions {
        if dep.radius > 0.01 {
            let r_norm = (pos - dep.center).length() / dep.radius;
            if r_norm < 1.0 {
                let w = 1.0 - r_norm * r_norm;
                let drop = dep.depth * (w * w);
                if drop > max_crater_drop {
                    max_crater_drop = drop;
                }
            }
        }
    }
    h -= max_crater_drop;
    h
}

/// Returns the 2D terrain slope gradient `(dh/dx, dh/dz)` at `pos` via central finite differences.
pub fn terrain_gradient(pos: Vec2, level: CurrentLevel) -> Vec2 {
    if level != CurrentLevel::Level4 {
        return Vec2::ZERO;
    }
    let eps = 0.25_f32;
    let dh_dx = (base_terrain_elevation(pos + Vec2::new(eps, 0.0))
        - base_terrain_elevation(pos - Vec2::new(eps, 0.0)))
        / (2.0 * eps);
    let dh_dz = (base_terrain_elevation(pos + Vec2::new(0.0, eps))
        - base_terrain_elevation(pos - Vec2::new(0.0, eps)))
        / (2.0 * eps);
    Vec2::new(dh_dx, dh_dz)
}

/// Computes the visual scale multiplier for infantry and vehicles at `elevation_m` (`m` relative to `0.0m` base):
/// - Higher elevations (`elevation_m > 0.0`, e.g. on the `+7.0m` ridgeline) appear slightly larger (`up to 1.26x`).
/// - Base elevation (`elevation_m == 0.0`) has scale `1.0x`.
/// - Below base elevation (`elevation_m < 0.0`, e.g. in a trench or artillery crater) appears slightly smaller (`down to 0.82x`).
pub fn elevation_visual_scale(elevation_m: f32) -> f32 {
    (1.0 + 0.032 * elevation_m).clamp(0.82, 1.26)
}

/// Computes the high-ground vision range bonus (`m`) for a unit at `elevation_m`:
/// `+1.0m` of additional sight range per meter of positive elevation.
pub fn elevation_vision_range_bonus(elevation_m: f32) -> f32 {
    elevation_m.max(0.0) * 1.0
}

/// Computes the movement speed multiplier for moving along `move_dir` on sloped terrain at `pos`:
/// - Moving uphill (`grade > 0`) slows movement down to `0.60x`.
/// - Moving downhill (`grade < 0`) speeds movement up to `1.12x`.
/// - Moving on flat ground or along a contour line (`grade == 0`) is `1.0x`.
pub fn elevation_movement_multiplier(pos: Vec2, move_dir: Vec2, level: CurrentLevel) -> f32 {
    if level != CurrentLevel::Level4 {
        return 1.0;
    }
    let dir = move_dir.normalize_or_zero();
    if dir.length_squared() < 1e-4 {
        return 1.0;
    }
    let grad = terrain_gradient(pos, level);
    let grade = dir.dot(grad);
    if grade >= 0.0 {
        // Uphill penalty
        (1.0 - grade * 0.75).clamp(0.60, 1.0)
    } else {
        // Downhill bonus
        (1.0 + (-grade) * 0.25).clamp(1.0, 1.12)
    }
}

/// Deterministic positions `(x, z)` for the dense forest patch on the right (East, `+X`) side
/// of the `test_level_4` map (`X in [48.0, 74.0], Z in [-20.0, 28.0]`).
///
/// Trees are arranged in a staggered hexagonal woodland grid (`step_x = 3.25m, step_z = 3.15m`)
/// with slight natural jitter (`±0.18m`). This guarantees:
/// - Clear gaps between adjacent tree trunks are `~2.0m – 3.0m`, which is wide enough for infantry
///   (`2 * SOLDIER_RADIUS = 1.20m`) to walk through freely,
/// - Every gap is strictly narrower than the APC (`APC_HULL_WIDTH = 3.40m`, `2 * APC_CLEARANCE_RADIUS = 4.10m`),
///   preventing the APC from driving through the forest.
pub fn level_4_forest_trees() -> Vec<Vec2> {
    let cols = 8usize;
    let rows = 15usize;
    let base_x = 48.0_f32;
    let base_z = -20.0_f32;
    let step_x = 3.25_f32;
    let step_z = 3.15_f32;

    let mut trees = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        for col in 0..cols {
            // Trim extreme corners for a rounded natural forest patch silhouette
            if (row == 0 || row == rows - 1) && (col == 0 || col == cols - 1) {
                continue;
            }
            let stagger_x = if row % 2 == 1 { step_x * 0.5 } else { 0.0 };
            // Deterministic pseudo-random organic offset in [-0.18, +0.18]
            let hash_a = ((row as u32 * 73 + col as u32 * 151 + 19) % 101) as f32 / 50.0 - 1.0;
            let hash_b = ((row as u32 * 137 + col as u32 * 53 + 47) % 101) as f32 / 50.0 - 1.0;
            let jx = hash_a * 0.18;
            let jz = hash_b * 0.18;

            let x = base_x + col as f32 * step_x + stagger_x + jx;
            let z = base_z + row as f32 * step_z + jz;
            trees.push(Vec2::new(x, z));
        }
    }
    trees
}

// 11 Buildings in an expanded realistic military base layout (120x120 world)
pub const COMPOUND_BUILDINGS: [(Vec3, Vec3); 11] = [
    // --- North Sector: Command & Communications ---
    // 1. North HQ Command Center (Northwest)
    (Vec3::new(14.0, 3.5, 10.0), Vec3::new(-16.0, 1.75, -38.0)),
    // 2. Communications & Radar Bunker (Northeast)
    (Vec3::new(10.0, 3.0, 8.0), Vec3::new(16.0, 1.5, -38.0)),
    // 3. North Guard Watchpost (North center)
    (Vec3::new(6.0, 2.5, 6.0), Vec3::new(0.0, 1.25, -48.0)),

    // --- Central Sector: Barracks, Logistics & Plaza ---
    // 4. West Barracks / Living Quarters
    (Vec3::new(12.0, 3.0, 8.0), Vec3::new(-26.0, 1.5, -16.0)),
    // 5. Medical Station / Research Lab (East)
    (Vec3::new(9.0, 2.8, 9.0), Vec3::new(24.0, 1.4, -18.0)),
    // 6. Central Logistics HQ (West of Plaza)
    (Vec3::new(12.0, 3.2, 10.0), Vec3::new(-14.0, 1.6, -2.0)),
    // 7. East Supply Warehouse / Depot
    (Vec3::new(15.0, 3.2, 10.0), Vec3::new(20.0, 1.6, 2.0)),

    // --- South Sector: Motor Pool, Armory & Checkpoint ---
    // 8. Motor Pool Repair Garage (Southwest)
    (Vec3::new(14.0, 3.0, 10.0), Vec3::new(-24.0, 1.5, 20.0)),
    // 9. Armory & Munitions Depot (Southeast)
    (Vec3::new(10.0, 2.8, 10.0), Vec3::new(22.0, 1.4, 22.0)),
    // 10. South Gatehouse / Checkpoint (West side)
    (Vec3::new(6.0, 2.2, 5.0), Vec3::new(-12.0, 1.1, 38.0)),
    // 11. South Inspection Post (East side)
    (Vec3::new(6.0, 2.2, 5.0), Vec3::new(12.0, 1.1, 38.0)),
];

// Waist-height low walls (Y = 0.75) positioned tactically around corridors, checkpoints, and courtyards
pub const COMPOUND_LOW_WALLS: [(Vec3, Vec3); 14] = [
    // --- South Entrance & Gate Checkpoint ---
    (Vec3::new(5.0, 0.75, 1.0), Vec3::new(-4.5, 0.375, 44.0)),
    (Vec3::new(5.0, 0.75, 1.0), Vec3::new(4.5, 0.375, 44.0)),
    (Vec3::new(6.0, 0.75, 1.0), Vec3::new(-5.0, 0.375, 30.0)),
    (Vec3::new(6.0, 0.75, 1.0), Vec3::new(5.0, 0.375, 30.0)),

    // --- Motor Pool & Armory Defensive Barriers ---
    (Vec3::new(1.0, 0.75, 8.0), Vec3::new(-14.0, 0.375, 18.0)),
    (Vec3::new(1.0, 0.75, 8.0), Vec3::new(14.0, 0.375, 20.0)),

    // --- Central Plaza & Cross-fire Emplacements ---
    (Vec3::new(7.0, 0.75, 1.0), Vec3::new(-4.0, 0.375, 8.0)),
    (Vec3::new(7.0, 0.75, 1.0), Vec3::new(4.0, 0.375, -8.0)),
    (Vec3::new(1.0, 0.75, 6.0), Vec3::new(-5.0, 0.375, -2.0)),
    (Vec3::new(1.0, 0.75, 6.0), Vec3::new(5.0, 0.375, -2.0)),

    // --- North HQ & Comms Perimeter (tactical emplacements leaving central avenues wide open) ---
    (Vec3::new(6.0, 0.75, 1.0), Vec3::new(-16.0, 0.375, -31.5)),
    (Vec3::new(6.0, 0.75, 1.0), Vec3::new(16.0, 0.375, -31.5)),
    (Vec3::new(5.0, 0.75, 1.0), Vec3::new(-9.0, 0.375, -27.0)),
    (Vec3::new(5.0, 0.75, 1.0), Vec3::new(9.0, 0.375, -27.0)),
];

// 4 waist-high walls arranged in a 16m x 16m square forming the Courtyard for test_level_3.
// Sized so soldiers can see (24m vision) and fire (67.5m rifle range) from one side to the other.
pub const COURTYARD_LOW_WALLS: [(Vec3, Vec3); 4] = [
    // North courtyard wall
    (Vec3::new(16.0, 0.75, 1.0), Vec3::new(0.0, 0.375, -8.0)),
    // South courtyard wall
    (Vec3::new(16.0, 0.75, 1.0), Vec3::new(0.0, 0.375, 8.0)),
    // West courtyard wall
    (Vec3::new(1.0, 0.75, 15.0), Vec3::new(-8.0, 0.375, 0.0)),
    // East courtyard wall
    (Vec3::new(1.0, 0.75, 15.0), Vec3::new(8.0, 0.375, 0.0)),
];

// 10 Buildings forming a small central village along the North-South road (X = 0) for test_level_4.
// Clustered near the center of the 1200m x 1200m combined-arms map with clear road passage along X in [-4, 4].
pub const VILLAGE_BUILDINGS: [(Vec3, Vec3); 10] = [
    // 1. North-West Farmhouse
    (Vec3::new(10.0, 3.0, 8.0), Vec3::new(-16.0, 1.5, -55.0)),
    // 2. North-East General Store
    (Vec3::new(12.0, 3.2, 9.0), Vec3::new(17.0, 1.6, -50.0)),
    // 3. West Roadside Cottage
    (Vec3::new(8.0, 2.8, 8.0), Vec3::new(-14.0, 1.4, -26.0)),
    // 4. East Village Hall / Clinic
    (Vec3::new(14.0, 3.5, 10.0), Vec3::new(18.0, 1.75, -20.0)),
    // 5. West Barn / Outbuilding
    (Vec3::new(14.0, 3.4, 10.0), Vec3::new(-34.0, 1.7, -8.0)),
    // 6. Central-West Bakery / Shop
    (Vec3::new(9.0, 3.0, 11.0), Vec3::new(-14.0, 1.5, 2.0)),
    // 7. Central-East Tavern / Inn
    (Vec3::new(11.0, 3.2, 12.0), Vec3::new(16.0, 1.6, 8.0)),
    // 8. East Workshop / Garage
    (Vec3::new(12.0, 3.0, 9.0), Vec3::new(36.0, 1.5, 22.0)),
    // 9. South-West Residence
    (Vec3::new(10.0, 2.8, 8.0), Vec3::new(-15.0, 1.4, 34.0)),
    // 10. South-East Mill / Warehouse
    (Vec3::new(13.0, 3.2, 10.0), Vec3::new(18.0, 1.6, 42.0)),
];

// 16 free-standing waist-high walls (Y = 0.75) around and within the central village for test_level_4.
pub const VILLAGE_LOW_WALLS: [(Vec3, Vec3); 16] = [
    // --- Southern Village Approach & Outskirts ---
    (Vec3::new(7.0, 0.75, 1.0), Vec3::new(-9.0, 0.375, 54.0)),
    (Vec3::new(7.0, 0.75, 1.0), Vec3::new(9.0, 0.375, 54.0)),
    (Vec3::new(5.5, 0.75, 1.0), Vec3::new(-6.0, 0.375, 44.0)),
    (Vec3::new(5.5, 0.75, 1.0), Vec3::new(6.5, 0.375, 44.0)),

    // --- Southern & Central Village Defensive Emplacements ---
    (Vec3::new(5.0, 0.75, 1.0), Vec3::new(-5.5, 0.375, 32.0)),
    (Vec3::new(5.0, 0.75, 1.0), Vec3::new(5.5, 0.375, 32.0)),
    (Vec3::new(1.0, 0.75, 6.0), Vec3::new(-8.0, 0.375, 22.0)),
    (Vec3::new(1.0, 0.75, 6.0), Vec3::new(8.0, 0.375, 22.0)),
    (Vec3::new(5.0, 0.75, 1.0), Vec3::new(-5.5, 0.375, 16.5)),
    (Vec3::new(5.0, 0.75, 1.0), Vec3::new(5.5, 0.375, 16.5)),

    // --- Northern Village & Flank Garden Walls ---
    (Vec3::new(6.0, 0.75, 1.0), Vec3::new(-6.5, 0.375, -8.0)),
    (Vec3::new(6.0, 0.75, 1.0), Vec3::new(6.5, 0.375, -8.0)),
    (Vec3::new(1.0, 0.75, 7.0), Vec3::new(-24.0, 0.375, 14.0)),
    (Vec3::new(1.0, 0.75, 7.0), Vec3::new(26.0, 0.375, -4.0)),
    (Vec3::new(6.0, 0.75, 1.0), Vec3::new(-7.0, 0.375, -36.0)),
    (Vec3::new(6.0, 0.75, 1.0), Vec3::new(7.0, 0.375, -36.0)),
];



