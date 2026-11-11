pub mod material;
pub mod mesh;

use bevy::prelude::*;
pub use material::*;
use crate::common::types::WORLD_SIZE;
use crate::world::buildings::COMPOUND_BUILDINGS;

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<FogMaterial>::default())
            .add_systems(Startup, setup_fog)
            .add_systems(Update, sync_fog_uniforms);
    }
}

fn setup_fog(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut fog_materials: ResMut<Assets<FogMaterial>>,
) {
    let mut building_boxes = [Vec4::ZERO; 16];
    for (i, (size, position)) in COMPOUND_BUILDINGS.iter().enumerate() {
        let half_x = size.x * 0.5;
        let half_z = size.z * 0.5;
        building_boxes[i] = Vec4::new(
            position.x - half_x,
            position.z - half_z,
            position.x + half_x,
            position.z + half_z,
        );
    }

    let fog_material = fog_materials.add(FogMaterial {
        soldier_pos: Vec2::new(0.0, 48.0),
        soldier_dir: Vec2::new(0.0, -1.0),
        vision_range: 36.0,
        vision_angle: 45.0_f32.to_radians(), // 90° vision cone (+/- 45°)
        inner_radius: 3.0,                   // Close-range personal awareness circle
        fog_alpha: 0.82,                     // Dark/greyed-out opacity outside vision cone
        building_count: COMPOUND_BUILDINGS.len() as u32,
        _pad0: 0.0,
        _pad1: 0.0,
        _pad2: 0.0,
        buildings: building_boxes,
    });

    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(WORLD_SIZE * 2.0, WORLD_SIZE * 2.0))),
        MeshMaterial3d(fog_material),
        Transform::from_xyz(0.0, 4.0, 0.0),
        FogOverlay,
    ));
}
