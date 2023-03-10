#define_import_path bevy_pbr::lighting

// From the Filament design doc
// https://google.github.io/filament/Filament.html#table_symbols
// Symbol Definition
// v    View unit vector
// l    Incident light unit vector
// n    Surface normal unit vector
// h    Half unit vector between l and v
// f    BRDF
// f_d    Diffuse component of a BRDF
// f_r    Specular component of a BRDF
// α    Roughness, remapped from using input perceptualRoughness
// σ    Diffuse reflectance
// Ω    Spherical domain
// f0    Reflectance at normal incidence
// f90    Reflectance at grazing angle
// χ+(a)    Heaviside function (1 if a>0 and 0 otherwise)
// nior    Index of refraction (IOR) of an interface
// ⟨n⋅l⟩    Dot product clamped to [0..1]
// ⟨a⟩    Saturated value (clamped to [0..1])

// The Bidirectional Reflectance Distribution Function (BRDF) describes the surface response of a standard material
// and consists of two components, the diffuse component (f_d) and the specular component (f_r):
// f(v,l) = f_d(v,l) + f_r(v,l)
//
// The form of the microfacet model is the same for diffuse and specular
// f_r(v,l) = f_d(v,l) = 1 / { |n⋅v||n⋅l| } ∫_Ω D(m,α) G(v,l,m) f_m(v,l,m) (v⋅m) (l⋅m) dm
//
// In which:
// D, also called the Normal Distribution Function (NDF) models the distribution of the microfacets
// G models the visibility (or occlusion or shadow-masking) of the microfacets
// f_m is the microfacet BRDF and differs between specular and diffuse components
//
// The above integration needs to be approximated.

// distanceAttenuation is simply the square falloff of light intensity
// combined with a smooth attenuation at the edge of the light radius
//
// light radius is a non-physical construct for efficiency purposes,
// because otherwise every light affects every fragment in the scene
fn getDistanceAttenuation(distanceSquare: f32, inverseRangeSquared: f32) -> f32 {
    let factor = distanceSquare * inverseRangeSquared;
    let smoothFactor = saturate(1.0 - factor * factor);
    let attenuation = smoothFactor * smoothFactor;
    return attenuation * 1.0 / max(distanceSquare, 0.0001);
}

// Normal distribution function (specular D)
// Based on https://google.github.io/filament/Filament.html#citation-walter07

// D_GGX(h,α) = α^2 / { π ((n⋅h)^2 (α2−1) + 1)^2 }

// Simple implementation, has precision problems when using fp16 instead of fp32
// see https://google.github.io/filament/Filament.html#listing_speculardfp16
fn D_GGX(roughness: f32, NoH: f32, h: vec3<f32>) -> f32 {
    let oneMinusNoHSquared = 1.0 - NoH * NoH;
    let a = NoH * roughness;
    let k = roughness / (oneMinusNoHSquared + a * a);
    let d = k * k * (1.0 / PI);
    return d;
}

// Visibility function (Specular G)
// V(v,l,a) = G(v,l,α) / { 4 (n⋅v) (n⋅l) }
// such that f_r becomes
// f_r(v,l) = D(h,α) V(v,l,α) F(v,h,f0)
// where
// V(v,l,α) = 0.5 / { n⋅l sqrt((n⋅v)^2 (1−α2) + α2) + n⋅v sqrt((n⋅l)^2 (1−α2) + α2) }
// Note the two sqrt's, that may be slow on mobile, see https://google.github.io/filament/Filament.html#listing_approximatedspecularv
fn V_SmithGGXCorrelated(roughness: f32, NoV: f32, NoL: f32) -> f32 {
    let a2 = roughness * roughness;
    let lambdaV = NoL * sqrt((NoV - a2 * NoV) * NoV + a2);
    let lambdaL = NoV * sqrt((NoL - a2 * NoL) * NoL + a2);
    let v = 0.5 / (lambdaV + lambdaL);
    return v;
}

// Fresnel function
// see https://google.github.io/filament/Filament.html#citation-schlick94
// F_Schlick(v,h,f_0,f_90) = f_0 + (f_90 − f_0) (1 − v⋅h)^5
fn F_Schlick_vec(f0: vec3<f32>, f90: f32, VoH: f32) -> vec3<f32> {
    // not using mix to keep the vec3 and float versions identical
    return f0 + (f90 - f0) * pow(1.0 - VoH, 5.0);
}

fn F_Schlick(f0: f32, f90: f32, VoH: f32) -> f32 {
    // not using mix to keep the vec3 and float versions identical
    return f0 + (f90 - f0) * pow(1.0 - VoH, 5.0);
}

fn fresnel(f0: vec3<f32>, LoH: f32) -> vec3<f32> {
    // f_90 suitable for ambient occlusion
    // see https://google.github.io/filament/Filament.html#lighting/occlusion
    let f90 = saturate(dot(f0, vec3<f32>(50.0 * 0.33)));
    return F_Schlick_vec(f0, f90, LoH);
}

// Specular BRDF
// https://google.github.io/filament/Filament.html#materialsystem/specularbrdf

// Cook-Torrance approximation of the microfacet model integration using Fresnel law F to model f_m
// f_r(v,l) = { D(h,α) G(v,l,α) F(v,h,f0) } / { 4 (n⋅v) (n⋅l) }
fn specular(
    f0: vec3<f32>,
    roughness: f32,
    h: vec3<f32>,
    NoV: f32,
    NoL: f32,
    NoH: f32,
    LoH: f32,
    specularIntensity: f32,
    f_ab: vec2<f32>
) -> vec3<f32> {
    let D = D_GGX(roughness, NoH, h);
    let V = V_SmithGGXCorrelated(roughness, NoV, NoL);
    let F = fresnel(f0, LoH);

    var Fr = (specularIntensity * D * V) * F;

    // Multiscattering approximation: https://google.github.io/filament/Filament.html#listing_energycompensationimpl
    Fr *= 1.0 + f0 * (1.0 / f_ab.x - 1.0);

    return Fr;
}

// Diffuse BRDF
// https://google.github.io/filament/Filament.html#materialsystem/diffusebrdf
// fd(v,l) = σ/π * 1 / { |n⋅v||n⋅l| } ∫Ω D(m,α) G(v,l,m) (v⋅m) (l⋅m) dm
//
// simplest approximation
// float Fd_Lambert() {
//     return 1.0 / PI;
// }
//
// vec3 Fd = diffuseColor * Fd_Lambert();
//
// Disney approximation
// See https://google.github.io/filament/Filament.html#citation-burley12
// minimal quality difference
fn Fd_Burley(roughness: f32, NoV: f32, NoL: f32, LoH: f32) -> f32 {
    let f90 = 0.5 + 2.0 * roughness * LoH * LoH;
    let lightScatter = F_Schlick(1.0, f90, NoL);
    let viewScatter = F_Schlick(1.0, f90, NoV);
    return lightScatter * viewScatter * (1.0 / PI);
}

// Scale/bias approximation
// https://www.unrealengine.com/en-US/blog/physically-based-shading-on-mobile
// TODO: Use a LUT (more accurate)
fn F_AB(perceptual_roughness: f32, NoV: f32) -> vec2<f32> {
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = perceptual_roughness * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * NoV)) * r.x + r.y;
    return vec2<f32>(-1.04, 1.04) * a004 + r.zw;
}

fn EnvBRDFApprox(f0: vec3<f32>, f_ab: vec2<f32>) -> vec3<f32> {
    return f0 * f_ab.x + f_ab.y;
}

fn perceptualRoughnessToRoughness(perceptualRoughness: f32) -> f32 {
    // clamp perceptual roughness to prevent precision problems
    // According to Filament design 0.089 is recommended for mobile
    // Filament uses 0.045 for non-mobile
    let clampedPerceptualRoughness = clamp(perceptualRoughness, 0.089, 1.0);
    return clampedPerceptualRoughness * clampedPerceptualRoughness;
}

fn point_light(
    world_position: vec3<f32>,
    light_id: u32,
    roughness: f32,
    NdotV: f32,
    N: vec3<f32>,
    V: vec3<f32>,
    R: vec3<f32>,
    F0: vec3<f32>,
    f_ab: vec2<f32>,
    diffuseColor: vec3<f32>
) -> vec3<f32> {
    let light = &point_lights.data[light_id];
    let light_to_frag = (*light).position_radius.xyz - world_position.xyz;
    let distance_square = dot(light_to_frag, light_to_frag);
    let rangeAttenuation = getDistanceAttenuation(distance_square, (*light).color_inverse_square_range.w);

    // Specular.
    // Representative Point Area Lights.
    // see http://blog.selfshadow.com/publications/s2013-shading-course/karis/s2013_pbs_epic_notes_v2.pdf p14-16
    let a = roughness;
    let centerToRay = dot(light_to_frag, R) * R - light_to_frag;
    let closestPoint = light_to_frag + centerToRay * saturate((*light).position_radius.w * inverseSqrt(dot(centerToRay, centerToRay)));
    let LspecLengthInverse = inverseSqrt(dot(closestPoint, closestPoint));
    let normalizationFactor = a / saturate(a + ((*light).position_radius.w * 0.5 * LspecLengthInverse));
    let specularIntensity = normalizationFactor * normalizationFactor;

    var L: vec3<f32> = closestPoint * LspecLengthInverse; // normalize() equivalent?
    var H: vec3<f32> = normalize(L + V);
    var NoL: f32 = saturate(dot(N, L));
    var NoH: f32 = saturate(dot(N, H));
    var LoH: f32 = saturate(dot(L, H));

    let specular_light = specular(F0, roughness, H, NdotV, NoL, NoH, LoH, specularIntensity, f_ab);

    // Diffuse.
    // Comes after specular since its NoL is used in the lighting equation.
    L = normalize(light_to_frag);
    H = normalize(L + V);
    NoL = saturate(dot(N, L));
    NoH = saturate(dot(N, H));
    LoH = saturate(dot(L, H));

    let diffuse = diffuseColor * Fd_Burley(roughness, NdotV, NoL, LoH);

    // See https://google.github.io/filament/Filament.html#mjx-eqn-pointLightLuminanceEquation
    // Lout = f(v,l) Φ / { 4 π d^2 }⟨n⋅l⟩
    // where
    // f(v,l) = (f_d(v,l) + f_r(v,l)) * light_color
    // Φ is luminous power in lumens
    // our rangeAttentuation = 1 / d^2 multiplied with an attenuation factor for smoothing at the edge of the non-physical maximum light radius

    // For a point light, luminous intensity, I, in lumens per steradian is given by:
    // I = Φ / 4 π
    // The derivation of this can be seen here: https://google.github.io/filament/Filament.html#mjx-eqn-pointLightLuminousPower

    // NOTE: (*light).color.rgb is premultiplied with (*light).intensity / 4 π (which would be the luminous intensity) on the CPU

    return ((diffuse + specular_light) * (*light).color_inverse_square_range.rgb) * (rangeAttenuation * NoL);
}

fn spot_light(
    world_position: vec3<f32>,
    light_id: u32,
    roughness: f32,
    NdotV: f32,
    N: vec3<f32>,
    V: vec3<f32>,
    R: vec3<f32>,
    F0: vec3<f32>,
    f_ab: vec2<f32>,
    diffuseColor: vec3<f32>
) -> vec3<f32> {
    // reuse the point light calculations
    let point_light = point_light(world_position, light_id, roughness, NdotV, N, V, R, F0, f_ab, diffuseColor);

    let light = &point_lights.data[light_id];

    // reconstruct spot dir from x/z and y-direction flag
    var spot_dir = vec3<f32>((*light).light_custom_data.x, 0.0, (*light).light_custom_data.y);
    spot_dir.y = sqrt(max(0.0, 1.0 - spot_dir.x * spot_dir.x - spot_dir.z * spot_dir.z));
    if ((*light).flags & POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE) != 0u {
        spot_dir.y = -spot_dir.y;
    }
    let light_to_frag = (*light).position_radius.xyz - world_position.xyz;

    // calculate attenuation based on filament formula https://google.github.io/filament/Filament.html#listing_glslpunctuallight
    // spot_scale and spot_offset have been precomputed
    // note we normalize here to get "l" from the filament listing. spot_dir is already normalized
    let cd = dot(-spot_dir, normalize(light_to_frag));
    let attenuation = saturate(cd * (*light).light_custom_data.z + (*light).light_custom_data.w);
    let spot_attenuation = attenuation * attenuation;

    return point_light * spot_attenuation;
}

fn directional_light(light_id: u32, roughness: f32, NdotV: f32, normal: vec3<f32>, view: vec3<f32>, R: vec3<f32>, F0: vec3<f32>, f_ab: vec2<f32>, diffuseColor: vec3<f32>) -> vec3<f32> {
    let light = &lights.directional_lights[light_id];

    let incident_light = (*light).direction_to_light.xyz;

    let half_vector = normalize(incident_light + view);
    let NoL = saturate(dot(normal, incident_light));
    let NoH = saturate(dot(normal, half_vector));
    let LoH = saturate(dot(incident_light, half_vector));

    let diffuse = diffuseColor * Fd_Burley(roughness, NdotV, NoL, LoH);
    let specularIntensity = 1.0;
    let specular_light = specular(F0, roughness, half_vector, NdotV, NoL, NoH, LoH, specularIntensity, f_ab);

    return (specular_light + diffuse) * (*light).color.rgb * NoL;
}

fn transmissive_light(frag_coord: vec3<f32>, N: vec3<f32>, V: vec3<f32>, ior: f32, thickness: f32, perceptual_roughness: f32, transmissive_color: vec3<f32>) -> vec3<f32> {
    // Calculate view aspect ratio, used later to scale offset so that it's proportionate
    let aspect = view.viewport.z / view.viewport.w;

    // Calculate the ratio between refaction indexes. Assume air/vacuum for the space outside the mesh
    let eta = 1.0 / ior;

    // Calculate incidence vector (opposite to view vector) and its dot product with the mesh normal
    let I = -V;
    let NdotI = dot(N, I);

    // Calculate refracted direction using Snell's law
    let k = 1.0 - eta * eta * (1.0 - NdotI * NdotI);
    let T = eta * I - (eta * NdotI + sqrt(k)) * N;

    // Transform refracted direction into view space
    let view_T = (view.inverse_view * vec4<f32>(T.xyz, 0.0)).xyz;

    // Calculate an offset position, by multiplying the refracted direction by the thickness and adding it to
    // the fragment position scaled by viewport size. Use the aspect ratio calculated earlier stay proportionate
    let offset_position = frag_coord.xy / view.viewport.zw + view_T.xy * thickness * vec2<f32>(1.0, -aspect) ;

    // Fetch background color
    let background_color = fetch_transmissive_background(offset_position, frag_coord, aspect, perceptual_roughness);

    // Calculate final color by applying transmissive color to background
    // TODO: Add support for attenuationColor and attenuationDistance
    return transmissive_color * background_color;
}

fn fetch_transmissive_background(offset_position: vec2<f32>, frag_coord: vec3<f32>, aspect: f32, perceptual_roughness: f32) -> vec3<f32> {
    // Number of taps scale with perceptual roughness
    // Minimum: 1, Maximum: 9
    let num_taps = i32(max(perceptual_roughness * 40.0, 8.0)) + 1;
    var result = vec3<f32>(0.0, 0.0, 0.0);
    for (var i: i32 = 0; i < num_taps; i = i + 1) {
        // Magic numbers have been empirically chosen to produce blurry results that look “smooth”
        let dither = screen_space_dither(frag_coord.xy + vec2<f32>(f32(i) * 4773.0, f32(i) * 1472.0));
        let dither_offset = perceptual_roughness * (30.0 * dither.yz + 0.03 * normalize(dither).xy) * vec2<f32>(1.0, -aspect);

        // Sample the view transmission texture at the offset position + dither offset, to get the background color
        // TODO: Use depth prepass data to reject values that are in front of the current fragment
        result += textureSample(
            view_transmission_texture,
            view_transmission_sampler,
            offset_position + dither_offset,
        ).rgb;
    }

    result /= f32(num_taps);

    return result;
}
