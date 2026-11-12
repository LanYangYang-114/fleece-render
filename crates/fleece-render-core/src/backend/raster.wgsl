// One invocation per pixel avoids races between overlapping triangles. The
// formulas mirror the CPU backend, including its depth tolerance, nearest
// texel addressing and per-fragment RGBA8 quantization.
struct Triangle {
    vertices: array<vec4<f32>, 3>,
    v_area: vec4<f32>,
    texture_shade: vec4<u32>,
    bounds: vec4<u32>,
}

struct Params {
    width: u32,
    height: u32,
    tiles_x: u32,
    tile_size: u32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> triangles: array<Triangle>;
@group(0) @binding(2) var<storage, read> bins: array<u32>;
@group(0) @binding(3) var<storage, read> texels: array<u32>;
@group(0) @binding(4) var<storage, read_write> output: array<u32>;

fn edge(a: vec2<f32>, b: vec2<f32>, c: vec2<f32>) -> f32 {
    return (c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x);
}

fn rgba(pixel: u32) -> vec4<f32> {
    return vec4<f32>(vec4<u32>(pixel, pixel >> 8u, pixel >> 16u, pixel >> 24u) & vec4<u32>(255u));
}

fn blend(dst: vec4<f32>, src: vec4<f32>) -> vec4<f32> {
    if src.a == 255.0 {
        return src;
    }
    let sa = src.a / 255.0;
    let da = dst.a / 255.0;
    let alpha = sa + da * (1.0 - sa);
    let rgb = (src.rgb * sa + dst.rgb * da * (1.0 - sa)) / alpha;
    return floor(clamp(vec4<f32>(rgb, alpha * 255.0), vec4<f32>(0.0), vec4<f32>(255.0)) + 0.5);
}

@compute @workgroup_size(8, 8)
fn raster(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.width || id.y >= params.height {
        return;
    }
    let sample_pos = vec2<f32>(id.xy) + 0.5;
    let tile = id.xy / params.tile_size;
    let bin = (tile.y * params.tiles_x + tile.x) * 2u;
    var depth = 3.402823e38;
    var color = vec4<f32>(0.0);
    for (var i = bins[bin]; i < bins[bin + 1u]; i += 1u) {
        let t = triangles[bins[i]];
        if any(id.xy < t.bounds.xy) || any(id.xy > t.bounds.zw) {
            continue;
        }
        let p0 = t.vertices[0];
        let p1 = t.vertices[1];
        let p2 = t.vertices[2];
        var w = vec3<f32>(edge(p1.xy, p2.xy, sample_pos), edge(p2.xy, p0.xy, sample_pos),
                          edge(p0.xy, p1.xy, sample_pos)) / t.v_area.w;
        if any(w < vec3<f32>(-0.00001)) {
            continue;
        }
        w = clamp(w, vec3<f32>(0.0), vec3<f32>(1.0));
        let sum = w.x + w.y + w.z;
        if sum <= 0.0 {
            continue;
        }
        w /= sum;
        let inv_z = w.x * p0.z + w.y * p1.z + w.z * p2.z;
        if inv_z <= 0.0 {
            continue;
        }
        let z = 1.0 / inv_z;
        if z > depth + 0.0001 {
            continue;
        }
        let u = (w.x * p0.w * p0.z + w.y * p1.w * p1.z + w.z * p2.w * p2.z) / inv_z;
        let v = (w.x * t.v_area.x * p0.z + w.y * t.v_area.y * p1.z + w.z * t.v_area.z * p2.z) / inv_z;
        let size = t.texture_shade.yz;
        let xy = min(vec2<u32>(clamp(vec2<f32>(u, v), vec2<f32>(0.0), vec2<f32>(0.999999)) * vec2<f32>(size)), size - 1u);
        let texel = rgba(texels[t.texture_shade.x + xy.y * size.x + xy.x]);
        if texel.a == 0.0 {
            continue;
        }
        depth = z;
        let lit = floor(texel.rgb * bitcast<f32>(t.texture_shade.w) + 0.5);
        color = blend(color, vec4<f32>(lit, texel.a));
    }
    let c = vec4<u32>(color);
    output[id.y * params.width + id.x] = c.r | (c.g << 8u) | (c.b << 16u) | (c.a << 24u);
}
