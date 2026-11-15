#import bevy_pbr::forward_io::VertexOutput

struct FogMaterial {
    soldier_pos: vec2<f32>,
    soldier_dir: vec2<f32>,
    vision_range: f32,
    vision_angle: f32,
    inner_radius: f32,
    fog_alpha: f32,
    building_count: u32,
    spotter_count: u32,
    _pad1: f32,
    _pad2: f32,
    buildings: array<vec4<f32>, 16>,
    spotters: array<vec4<f32>, 32>,
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

// Aerial line-of-sight vs building AABB (b = vec4(min_x, min_z, max_x, max_z)).
// Because the helicopter hovers well above building roofs:
// 1. A building directly underneath the helicopter (`origin` inside `b`) does not obstruct its sight cone.
// 2. Other buildings only cast a finite shadow (`shadow_depth`) directly behind their far wall,
//    leaving the area beyond that shadow visible.
fn ray_intersects_box_aerial(origin: vec2<f32>, p_target: vec2<f32>, b: vec4<f32>) -> bool {
    // If target point is on/inside this building footprint, it does not occlude itself
    if (p_target.x >= b.x && p_target.x <= b.z && p_target.y >= b.y && p_target.y <= b.w) {
        return false;
    }

    // If the aerial spotter is directly over this building's roof, this building does not block its cone
    if (origin.x >= b.x && origin.x <= b.z && origin.y >= b.y && origin.y <= b.w) {
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

    if (tmin < 0.999 && tmax > 0.001) {
        let total_dist = length(delta);
        let exit_dist = tmax * total_dist;
        let dist_behind_building = (1.0 - tmax) * total_dist;
        let shadow_depth = clamp(exit_dist * 0.45, 4.0, 9.5);
        return dist_behind_building <= shadow_depth;
    }

    return false;
}

fn compute_spotter_visibility(world_xz: vec2<f32>, s_pos: vec2<f32>, s_dir: vec2<f32>) -> f32 {
    let is_aerial = length(s_dir) > 1.5;
    let spotter_range = select(material.vision_range, 38.0, is_aerial);
    let spotter_angle = select(material.vision_angle, 0.6108652, is_aerial);

    let to_frag = world_xz - s_pos;
    let dist = length(to_frag);

    // 1. Close-range 360° awareness circle around the unit
    let inner_visibility = 1.0 - smoothstep(material.inner_radius - 0.6, material.inner_radius, dist);

    // 2. Max vision range cutoff
    let range_fade = smoothstep(spotter_range - 2.0, spotter_range, dist);

    // 3. Directional vision cone calculation
    let safe_dist = max(dist, 0.001);
    let dir = to_frag / safe_dist;
    let dot_prod = dot(dir, normalize(s_dir));
    let cos_half_angle = cos(spotter_angle);

    // Anti-aliased smooth edge along the vision cone flanks
    let angle_visibility = smoothstep(cos_half_angle - 0.025, cos_half_angle + 0.025, dot_prod);

    // Combined cone visibility with distance falloff
    let cone_visibility = (1.0 - range_fade) * angle_visibility;

    // Base visibility without obstacles
    var vis = clamp(max(cone_visibility, inner_visibility), 0.0, 1.0);

    // 4. Line-of-sight obstruction check against buildings
    if (vis > 0.0) {
        for (var i = 0u; i < material.building_count; i = i + 1u) {
            let blocked = select(
                ray_intersects_box(s_pos, world_xz, material.buildings[i]),
                ray_intersects_box_aerial(s_pos, world_xz, material.buildings[i]),
                is_aerial
            );
            if (blocked) {
                vis = 0.0;
                break;
            }
        }
    }

    return vis;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let world_xz = vec2<f32>(in.world_position.x, in.world_position.z);

    // Fog color: dark slate tactical grey
    let fog_base_color = vec3<f32>(0.13, 0.14, 0.17);

    var visibility = 0.0;
    if (material.spotter_count == 0u) {
        visibility = compute_spotter_visibility(world_xz, material.soldier_pos, material.soldier_dir);
    } else {
        for (var s = 0u; s < material.spotter_count; s = s + 1u) {
            let spotter = material.spotters[s];
            let s_vis = compute_spotter_visibility(world_xz, spotter.xy, spotter.zw);
            visibility = max(visibility, s_vis);
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
