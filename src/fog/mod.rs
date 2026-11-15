pub mod material;
pub mod mesh;

use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
pub use material::*;
use crate::common::types::{AppState, CurrentLevel, LevelEntity, RTS_WORLD_SIZE, WORLD_SIZE};
use crate::world::buildings::{COMPOUND_BUILDINGS, VILLAGE_BUILDINGS};

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<FogMaterial>::default())
            .add_systems(OnEnter(AppState::InGame), setup_fog)
            .add_systems(Update, sync_fog_uniforms.run_if(in_state(AppState::InGame)));
    }
}

pub fn setup_fog(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut fog_materials: ResMut<Assets<FogMaterial>>,
    current_level: Option<Res<CurrentLevel>>,
) {
    let level = current_level.as_deref().copied().unwrap_or_default();
    let mut building_boxes = [Vec4::ZERO; 16];
    let level_buildings: &[(Vec3, Vec3)] = match level {
        CurrentLevel::Level1 | CurrentLevel::Level2 => &COMPOUND_BUILDINGS,
        CurrentLevel::Level3 => &[],
        CurrentLevel::Level4 => &VILLAGE_BUILDINGS,
    };
    for (i, (size, position)) in level_buildings.iter().enumerate() {
        let half_x = size.x * 0.5;
        let half_z = size.z * 0.5;
        building_boxes[i] = Vec4::new(
            position.x - half_x,
            position.z - half_z,
            position.x + half_x,
            position.z + half_z,
        );
    }
    let building_count = level_buildings.len() as u32;

    let vision_range = if level == CurrentLevel::Level4 {
        24.0
    } else {
        36.0
    };
    let plane_size = if level == CurrentLevel::Level4 {
        RTS_WORLD_SIZE * 2.0
    } else {
        WORLD_SIZE * 2.0
    };

    let fog_material = fog_materials.add(FogMaterial {
        soldier_pos: Vec2::new(0.0, 48.0),
        soldier_dir: Vec2::new(0.0, -1.0),
        vision_range,
        vision_angle: 45.0_f32.to_radians(), // 90° vision cone (+/- 45°)
        inner_radius: 3.0,                   // Close-range personal awareness circle
        fog_alpha: 0.82,                     // Dark/greyed-out opacity outside vision cone
        building_count,
        spotter_count: 0,
        _pad1: 0.0,
        _pad2: 0.0,
        buildings: building_boxes,
        spotters: [Vec4::ZERO; 16],
    });

    commands.spawn((
        LevelEntity,
        Mesh3d(meshes.add(Plane3d::default().mesh().size(plane_size, plane_size))),
        MeshMaterial3d(fog_material),
        Transform::from_xyz(0.0, 4.0, 0.0),
        NotShadowCaster,
        NotShadowReceiver,
        FogOverlay,
    ));
}
