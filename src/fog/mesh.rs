use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};

// Generates a 2D circular sector mesh (vision cone fan)
pub fn create_vision_cone_mesh(radius: f32, fov_radians: f32, segments: u32) -> Mesh {
    let mut positions = vec![Vec3::ZERO];
    let mut normals = vec![Vec3::Z];
    let half_fov = fov_radians * 0.5;
    let angle_step = fov_radians / segments as f32;

    for i in 0..=segments {
        let angle = -half_fov + i as f32 * angle_step;
        positions.push(Vec3::new(radius * angle.sin(), radius * angle.cos(), 0.005));
        normals.push(Vec3::Z);
    }

    let mut indices = Vec::new();
    for i in 1..=segments {
        indices.push(0);
        indices.push(i);
        indices.push(i + 1);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

