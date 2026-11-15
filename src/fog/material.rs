use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use crate::common::types::{FogOfWarEnabled, Soldier};
use crate::friendly::Friendly;
use crate::player::Player;

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
    pub spotter_count: u32,
    #[uniform(0)]
    pub _pad1: f32,
    #[uniform(0)]
    pub _pad2: f32,
    #[uniform(0)]
    pub buildings: [Vec4; 16],
    #[uniform(0)]
    pub spotters: [Vec4; 16],
}

impl Material for FogMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/fog.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

// Synchronizes Fog of War shader uniforms and visibility with player / friendly spotters and debug toggle
pub fn sync_fog_uniforms(
    fog_enabled: Option<Res<FogOfWarEnabled>>,
    player_q: Query<(&Transform, &Soldier), (With<Player>, Without<Friendly>)>,
    friendly_q: Query<(&Transform, &Soldier), (With<Friendly>, Without<Player>)>,
    mut fog_q: Query<(&MeshMaterial3d<FogMaterial>, &mut Visibility), With<FogOverlay>>,
    mut fog_materials: ResMut<Assets<FogMaterial>>,
) {
    let enabled = fog_enabled.map_or(true, |f| f.0);
    for (_, mut vis) in &mut fog_q {
        *vis = if enabled {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if !enabled {
        return;
    }

    if let Ok((soldier_tf, soldier)) = player_q.single() {
        let soldier_xz = Vec2::new(soldier_tf.translation.x, soldier_tf.translation.z);
        for (fog_handle, _) in &fog_q {
            if let Some(mut fog_mat) = fog_materials.get_mut(fog_handle) {
                fog_mat.soldier_pos = soldier_xz;
                fog_mat.soldier_dir = soldier.facing_dir;
                fog_mat.spotter_count = 0;
            }
        }
    } else {
        // RTS mode (Level 4): populate spotters from all living friendly squad members
        let mut spotter_buf = [Vec4::ZERO; 16];
        let mut count = 0usize;
        for (f_tf, f_soldier) in &friendly_q {
            if count < 16 {
                spotter_buf[count] = Vec4::new(
                    f_tf.translation.x,
                    f_tf.translation.z,
                    f_soldier.facing_dir.x,
                    f_soldier.facing_dir.y,
                );
                count += 1;
            }
        }
        for (fog_handle, _) in &fog_q {
            if let Some(mut fog_mat) = fog_materials.get_mut(fog_handle) {
                fog_mat.spotters = spotter_buf;
                fog_mat.spotter_count = count as u32;
            }
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

