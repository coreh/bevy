// Lens flare exploration: two feature-generation strategies sharing one
// prefiltered (thresholded, half-res) buffer, so only the ghost math differs.
//
// Mode 0 follows John Chapman's "Pseudo Lens Flare" (2013):
// https://john-chapman-graphics.blogspot.com/2013/02/pseudo-lens-flare.html
// Mode 1 is the classic light-driven billboard chain, evaluated analytically.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct LensFlareSettings {
    light_uv: vec2<f32>,
    intensity: f32,
    threshold: f32,
    ghost_dispersal: f32,
    halo_width: f32,
    chromatic_offset: f32,
    aspect: f32,
    ghost_count: u32,
    mode: u32,
    _padding: vec2<f32>,
}

@group(0) @binding(0) var screen_texture: texture_2d<f32>;
@group(0) @binding(1) var texture_sampler: sampler;
@group(0) @binding(2) var<uniform> settings: LensFlareSettings;
// Only bound by the composite pass.
@group(0) @binding(3) var flare_texture: texture_2d<f32>;

const CENTER: vec2<f32> = vec2<f32>(0.5, 0.5);

// Prefilter: 4-tap box downsample of the main texture, then a soft threshold.
// The blur matters as much as the threshold here — sharp texels produce
// visibly aliased ghosts once they get scaled and scattered.
@fragment
fn prefilter(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(screen_texture));
    let o = texel * 0.5;

    var sum = textureSample(screen_texture, texture_sampler, in.uv + vec2(-o.x, -o.y)).rgb;
    sum += textureSample(screen_texture, texture_sampler, in.uv + vec2(o.x, -o.y)).rgb;
    sum += textureSample(screen_texture, texture_sampler, in.uv + vec2(-o.x, o.y)).rgb;
    sum += textureSample(screen_texture, texture_sampler, in.uv + vec2(o.x, o.y)).rgb;
    let color = sum * 0.25;

    let bright = max(color - vec3(settings.threshold), vec3(0.0));
    return vec4(bright, 1.0);
}

// Stand-in for the 1D "lens color" LUT a real implementation would sample,
// tinting ghosts by their radial distance to fake per-element coatings.
fn lens_tint(t: f32) -> vec3<f32> {
    let warm = vec3(1.0, 0.72, 0.38);
    let cool = vec3(0.35, 0.65, 1.0);
    let mid = vec3(0.85, 1.0, 0.75);
    if t < 0.5 {
        return mix(warm, mid, t * 2.0);
    }
    return mix(mid, cool, (t - 0.5) * 2.0);
}

fn aspect_length(v: vec2<f32>) -> f32 {
    return length(vec2(v.x * settings.aspect, v.y));
}

fn sample_flare(uv: vec2<f32>) -> vec3<f32> {
    return textureSample(flare_texture, texture_sampler, uv).rgb;
}

// Radial chromatic dispersion: each channel is fetched at a slightly
// different radius, which is what gives ghosts their coloured fringes.
fn sample_dispersed(uv: vec2<f32>, direction: vec2<f32>) -> vec3<f32> {
    let d = direction * settings.chromatic_offset;
    return vec3(
        sample_flare(uv + d).r,
        sample_flare(uv).g,
        sample_flare(uv - d).b,
    );
}

// Falloff that keeps ghosts concentrated near the centre of the frame.
fn radial_weight(uv: vec2<f32>) -> f32 {
    let w = aspect_length(CENTER - uv) / aspect_length(CENTER);
    return pow(saturate(1.0 - w), 10.0);
}

fn screen_space_flare(uv: vec2<f32>) -> vec3<f32> {
    // Mirroring about the centre is the whole trick: bright regions on one
    // side of the frame become ghost sources on the opposite side.
    let flipped = vec2(1.0) - uv;
    let ghost_vec = (CENTER - flipped) * settings.ghost_dispersal;
    let dispersal_dir = normalize(ghost_vec + vec2(1e-6));

    var result = vec3(0.0);

    for (var i = 0u; i < settings.ghost_count; i += 1u) {
        let offset = fract(flipped + ghost_vec * f32(i));
        let t = f32(i) / f32(max(settings.ghost_count, 1u));
        result += sample_dispersed(offset, dispersal_dir) * radial_weight(offset) * lens_tint(t);
    }

    // Halo: a single tap at a fixed radius, producing the ring that appears
    // when the source is near the centre.
    let halo_uv = fract(flipped + dispersal_dir * settings.halo_width);
    result += sample_dispersed(halo_uv, dispersal_dir) * radial_weight(halo_uv);

    return result;
}

fn analytic_flare(uv: vec2<f32>) -> vec3<f32> {
    // Cheap occlusion stand-in: if the source is hidden, the prefiltered
    // buffer is dark where the source projects, so the whole chain fades.
    // A real implementation would use a depth/HZB visibility test instead.
    let source = sample_flare(settings.light_uv);
    let luminance = dot(source, vec3(0.2126, 0.7152, 0.0722));
    if luminance <= 0.0 {
        return vec3(0.0);
    }
    let occlusion = saturate(luminance * 0.25);
    // Keep the source's hue but drop its HDR magnitude, which would otherwise
    // blow the whole chain out.
    let tint = source / luminance;

    // Fade as the source leaves the frame, since it has no on-screen pixels
    // to justify it any more.
    let off_screen = saturate(1.0 - aspect_length(max(
        abs(settings.light_uv - CENTER) - vec2(0.5),
        vec2(0.0),
    )) * 8.0);

    let axis = CENTER - settings.light_uv;
    var result = vec3(0.0);

    for (var i = 0u; i < settings.ghost_count; i += 1u) {
        let t = f32(i + 1u) / f32(max(settings.ghost_count, 1u));
        let center = settings.light_uv + axis * t * 2.0 * settings.ghost_dispersal;
        // Vary sizes so the chain does not read as a rubber stamp.
        let radius = mix(0.015, 0.07, fract(t * 2.7));
        let d = aspect_length(uv - center) / radius;
        let disc = saturate(smoothstep(1.0, 0.55, d) - smoothstep(0.85, 0.45, d) * 0.55);
        result += lens_tint(t) * disc * radial_weight(center) * 3.0;
    }

    // Halo ring centred on the frame, at a radius set by halo_width.
    let ring = aspect_length(uv - CENTER);
    let halo = smoothstep(0.03, 0.0, abs(ring - settings.halo_width * 0.6));
    result += lens_tint(0.5) * halo * 0.06;

    return result * tint * occlusion * off_screen;
}

@fragment
fn composite(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let scene = textureSample(screen_texture, texture_sampler, in.uv).rgb;

    var flare: vec3<f32>;
    if settings.mode == 0u {
        flare = screen_space_flare(in.uv);
    } else {
        flare = analytic_flare(in.uv);
    }

    return vec4(scene + flare * settings.intensity, 1.0);
}
