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

// 4 Buildings in a realistic compound layout (flanking a central access road/plaza)
pub const COMPOUND_BUILDINGS: [(Vec3, Vec3); 4] = [
    // Medium building 1: Command / HQ (Northwest)
    (Vec3::new(10.0, 3.0, 8.0), Vec3::new(-12.0, 1.5, -8.0)),
    // Medium building 2: Barracks / Depot (Northeast)
    (Vec3::new(8.0, 2.5, 8.0), Vec3::new(10.0, 1.25, -7.0)),
    // Small building 3: Armory / Workshop (Southeast)
    (Vec3::new(5.0, 2.2, 5.0), Vec3::new(11.0, 1.1, 7.0)),
    // Small building 4: Guard post / Checkpoint (Southwest)
    (Vec3::new(4.0, 2.0, 4.0), Vec3::new(-8.0, 1.0, 8.0)),
];

// Waist-height low walls (Y = 0.75) positioned tactically around compound corridors and courtyards
pub const COMPOUND_LOW_WALLS: [(Vec3, Vec3); 4] = [
    // Low wall 1: South entrance checkpoint barrier (West side)
    (Vec3::new(4.5, 0.75, 0.8), Vec3::new(-4.0, 0.375, 13.0)),
    // Low wall 2: South entrance checkpoint barrier (East side)
    (Vec3::new(4.5, 0.75, 0.8), Vec3::new(4.0, 0.375, 13.0)),
    // Low wall 3: Central plaza barricade (North-central courtyard)
    (Vec3::new(5.0, 0.75, 0.8), Vec3::new(-3.5, 0.375, -2.5)),
    // Low wall 4: East depot perimeter barricade
    (Vec3::new(0.8, 0.75, 5.0), Vec3::new(5.5, 0.375, 3.5)),
];

