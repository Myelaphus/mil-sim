#import bevy_pbr::forward_io::VertexOutput

struct FogMaterial {
    soldier_pos: vec2<f32>,
    soldier_dir: vec2<f32>,
    vision_range: f32,
    vision_angle: f32,
    inner_radius: f32,
    fog_alpha: f32,
    building_count: u32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    buildings: array<vec4<f32>, 16>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> material: FogMaterial;

// 2D Line-segment vs AABB intersection test
// Returns true if the segment from origin to p_target intersects the box before reaching p_target
fn ray_intersects_box(origin: vec2<f32>, p_target: vec2<f32>, b: vec4<f32>) -> bool {
    // If target point is inside this building, it does not occlude itself
    if (p_target.x >= b.x && p_target.x <= b.z && p_target.y >= b.y && p_target.y <= b.w) {
        return false;
    }

    let delta = p_target - origin;
    var tmin = 0.0;
    var tmax = 1.0;

    // X-axis slab
    if (abs(delta.x) > 0.0001) {
        let inv_dx = 1.0 / delta.x;
        var t1 = (b.x - origin.x) * inv_dx;
        var t2 = (b.z - origin.x) * inv_dx;
        if (t1 > t2) {
            let tmp = t1;
            t1 = t2;
            t2 = tmp;
        }
        tmin = max(tmin, t1);
        tmax = min(tmax, t2);
        if (tmin > tmax) {
            return false;
        }
    } else {
        if (origin.x < b.x || origin.x > b.z) {
            return false;
        }
    }

    // Y-axis (world Z) slab
    if (abs(delta.y) > 0.0001) {
        let inv_dy = 1.0 / delta.y;
        var t1 = (b.y - origin.y) * inv_dy;
        var t2 = (b.w - origin.y) * inv_dy;
        if (t1 > t2) {
            let tmp = t1;
            t1 = t2;
            t2 = tmp;
        }
        tmin = max(tmin, t1);
        tmax = min(tmax, t2);
        if (tmin > tmax) {
            return false;
        }
    } else {
        if (origin.y < b.y || origin.y > b.w) {
            return false;
        }
    }

    // Intersects strictly between soldier and target
    return tmin < 0.999 && tmax > 0.001;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let world_xz = vec2<f32>(in.world_position.x, in.world_position.z);
    let to_frag = world_xz - material.soldier_pos;
    let dist = length(to_frag);

    // Fog color: dark slate tactical grey
    let fog_base_color = vec3<f32>(0.13, 0.14, 0.17);

    // 1. Close-range 360° awareness circle around the soldier
    let inner_visibility = 1.0 - smoothstep(material.inner_radius - 0.6, material.inner_radius, dist);

    // 2. Max vision range cutoff
    let range_fade = smoothstep(material.vision_range - 2.0, material.vision_range, dist);

    // 3. Directional vision cone calculation
    let safe_dist = max(dist, 0.001);
    let dir = to_frag / safe_dist;
    let dot_prod = dot(dir, normalize(material.soldier_dir));
    let cos_half_angle = cos(material.vision_angle);

    // Anti-aliased smooth edge along the vision cone flanks
    let angle_visibility = smoothstep(cos_half_angle - 0.025, cos_half_angle + 0.025, dot_prod);

    // Combined cone visibility with distance falloff
    let cone_visibility = (1.0 - range_fade) * angle_visibility;

    // Base visibility without obstacles
    var visibility = clamp(max(cone_visibility, inner_visibility), 0.0, 1.0);

    // 4. Line-of-sight obstruction check against buildings
    // If a building is between the soldier and this point, cast fog shadow
    if (visibility > 0.0) {
        for (var i = 0u; i < material.building_count; i = i + 1u) {
            if (ray_intersects_box(material.soldier_pos, world_xz, material.buildings[i])) {
                visibility = 0.0;
                break;
            }
        }
    }

    // Outside cone or behind buildings: greyed out with fog_alpha (0.82)
    // Inside clear line of sight: crystal clear (alpha = 0.0)
    let alpha = mix(material.fog_alpha, 0.0, visibility);

    // Subtle luminous edge highlight along the visible perimeter
    let cone_boundary = smoothstep(0.0, 0.08, visibility) * smoothstep(0.3, 0.08, visibility);
    let final_color = mix(fog_base_color, vec3<f32>(0.75, 0.88, 1.0), cone_boundary * 0.35);

    return vec4<f32>(final_color, alpha);
}
