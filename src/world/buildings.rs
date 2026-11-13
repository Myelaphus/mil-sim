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

