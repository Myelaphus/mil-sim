use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use crate::common::types::Soldier;

#[derive(Component)]
pub struct FogOverlay;

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct FogMaterial {
    #[uniform(0)]
    pub soldier_pos: Vec2,
    #[uniform(0)]
    pub soldier_dir: Vec2,
    #[uniform(0)]
    pub vision_range: f32,
    #[uniform(0)]
    pub vision_angle: f32,
    #[uniform(0)]
    pub inner_radius: f32,
    #[uniform(0)]
    pub fog_alpha: f32,
    #[uniform(0)]
    pub building_count: u32,
    #[uniform(0)]
    pub _pad0: f32,
    #[uniform(0)]
    pub _pad1: f32,
    #[uniform(0)]
    pub _pad2: f32,
    #[uniform(0)]
    pub buildings: [Vec4; 16],
}

impl Material for FogMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/fog.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

// Synchronizes Fog of War shader uniforms with current player soldier position and facing direction
pub fn sync_fog_uniforms(
    player_q: Query<(&Transform, &Soldier), With<crate::player::Player>>,
    fog_q: Query<&MeshMaterial3d<FogMaterial>, With<FogOverlay>>,
    mut fog_materials: ResMut<Assets<FogMaterial>>,
) {
    let Ok((soldier_tf, soldier)) = player_q.single() else {
        return;
    };
    let soldier_xz = Vec2::new(soldier_tf.translation.x, soldier_tf.translation.z);
    for fog_handle in &fog_q {
        if let Some(mut fog_mat) = fog_materials.get_mut(fog_handle) {
            fog_mat.soldier_pos = soldier_xz;
            fog_mat.soldier_dir = soldier.facing_dir;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_wgsl_validation() {
        let mut source = include_str!("../../assets/shaders/fog.wgsl").to_string();
        source = source.replace("#{MATERIAL_BIND_GROUP}", "3");
        source = source.replace(
            "#import bevy_pbr::forward_io::VertexOutput",
            "struct VertexOutput { @builtin(position) position: vec4<f32>, @location(0) world_position: vec4<f32>, @location(1) world_normal: vec3<f32>, };",
        );

        let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| {
            panic!("WGSL parse error: {}", e.emit_to_string(&source));
        });

        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        );
        if let Err(e) = validator.validate(&module) {
            panic!("Naga validation error: {:?}", e);
        }
        println!("Naga validation SUCCESSFUL!");
    }
}

