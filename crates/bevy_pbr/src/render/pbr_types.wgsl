#define_import_path bevy_pbr::pbr_types

struct StandardMaterial {
    base_color: vec4<f32>,
    emissive: vec4<f32>,
    perceptual_roughness: f32,
    metallic: f32,
    reflectance: f32,
    diffuse_transmission: f32,
    specular_transmission: f32,
    thickness: f32,
    ior: f32,
    attenuation_distance: f32,
    attenuation_color: vec4<f32>,
    // 'flags' is a bit field indicating various options.
    // u32 is 32 bits so in theory we'd have up to 32 options, however some lower-end mobile GPUs
    // only support 16-bit integers, so we have to use the lower half of two u32s to store the flags
    flags: vec2<u32>,
    alpha_cutoff: f32,
    parallax_depth_scale: f32,
    max_parallax_layer_count: f32,
    lightmap_exposure: f32,
    max_relief_mapping_search_steps: u32,
    /// ID for specifying which deferred lighting pass should be used for rendering this material, if any.
    deferred_lighting_pass_id: u32,
};

// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
// NOTE: if these flags are updated or changed. Be sure to also update
// deferred_flags_from_mesh_material_flags and mesh_material_flags_from_deferred_flags
// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
const STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT: vec2<u32>                   = vec2(     0u,    1u);
const STANDARD_MATERIAL_FLAGS_EMISSIVE_TEXTURE_BIT: vec2<u32>                     = vec2(     0u,    2u);
const STANDARD_MATERIAL_FLAGS_METALLIC_ROUGHNESS_TEXTURE_BIT: vec2<u32>           = vec2(     0u,    4u);
const STANDARD_MATERIAL_FLAGS_OCCLUSION_TEXTURE_BIT: vec2<u32>                    = vec2(     0u,    8u);
const STANDARD_MATERIAL_FLAGS_DOUBLE_SIDED_BIT: vec2<u32>                         = vec2(     0u,   16u);
const STANDARD_MATERIAL_FLAGS_UNLIT_BIT: vec2<u32>                                = vec2(     0u,   32u);
const STANDARD_MATERIAL_FLAGS_TWO_COMPONENT_NORMAL_MAP: vec2<u32>                 = vec2(     0u,   64u);
const STANDARD_MATERIAL_FLAGS_FLIP_NORMAL_MAP_Y: vec2<u32>                        = vec2(     0u,  128u);
const STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT: vec2<u32>                          = vec2(     0u,  256u);
const STANDARD_MATERIAL_FLAGS_DEPTH_MAP_BIT: vec2<u32>                            = vec2(     0u,  512u);
const STANDARD_MATERIAL_FLAGS_SPECULAR_TRANSMISSION_TEXTURE_BIT: vec2<u32>        = vec2(     0u, 1024u);
const STANDARD_MATERIAL_FLAGS_THICKNESS_TEXTURE_BIT: vec2<u32>                    = vec2(     0u, 2048u);
const STANDARD_MATERIAL_FLAGS_DIFFUSE_TRANSMISSION_TEXTURE_BIT: vec2<u32>         = vec2(     0u, 4096u);
const STANDARD_MATERIAL_FLAGS_ATTENUATION_ENABLED_BIT: vec2<u32>                  = vec2(     0u, 8192u);
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS: vec2<u32>                 = vec2(57344u,     0u); // (0b111u32 << 29)
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE: vec2<u32>                        = vec2(    0u,     0u); // (0u32 << 29)
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_MASK: vec2<u32>                          = vec2( 8192u,     0u); // (1u32 << 29)
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND: vec2<u32>                         = vec2(16384u,     0u); // (2u32 << 29)
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_PREMULTIPLIED: vec2<u32>                 = vec2(24576u,     0u); // (3u32 << 29)
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD: vec2<u32>                           = vec2(32768u,     0u); // (4u32 << 29)
const STANDARD_MATERIAL_FLAGS_ALPHA_MODE_MULTIPLY: vec2<u32>                      = vec2(40960u,     0u); // (5u32 << 29)
// ↑ To calculate/verify the values above, use the following playground:
// https://play.rust-lang.org/?version=stable&mode=debug&edition=2021&gist=7792f8dd6fc6a8d4d0b6b1776898a7f4


// Creates a StandardMaterial with default values
fn standard_material_new() -> StandardMaterial {
    var material: StandardMaterial;

    // NOTE: Keep in-sync with src/pbr_material.rs!
    material.base_color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    material.emissive = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    material.perceptual_roughness = 0.5;
    material.metallic = 0.00;
    material.reflectance = 0.5;
    material.diffuse_transmission = 0.0;
    material.specular_transmission = 0.0;
    material.thickness = 0.0;
    material.ior = 1.5;
    material.attenuation_distance = 1.0;
    material.attenuation_color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    material.flags = STANDARD_MATERIAL_FLAGS_ALPHA_MODE_OPAQUE;
    material.alpha_cutoff = 0.5;
    material.parallax_depth_scale = 0.1;
    material.max_parallax_layer_count = 16.0;
    material.max_relief_mapping_search_steps = 5u;
    material.deferred_lighting_pass_id = 1u;

    return material;
}

struct PbrInput {
    material: StandardMaterial,
    diffuse_occlusion: vec3<f32>,
    specular_occlusion: f32,
    frag_coord: vec4<f32>,
    world_position: vec4<f32>,
    // Normalized world normal used for shadow mapping as normal-mapping is not used for shadow
    // mapping
    world_normal: vec3<f32>,
    // Normalized normal-mapped world normal used for lighting
    N: vec3<f32>,
    // Normalized view vector in world space, pointing from the fragment world position toward the
    // view world position
    V: vec3<f32>,
    lightmap_light: vec3<f32>,
    is_orthographic: bool,
    flags: u32,
};

// Creates a PbrInput with default values
fn pbr_input_new() -> PbrInput {
    var pbr_input: PbrInput;

    pbr_input.material = standard_material_new();
    pbr_input.diffuse_occlusion = vec3<f32>(1.0);
    pbr_input.specular_occlusion = 1.0;

    pbr_input.frag_coord = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    pbr_input.world_position = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    pbr_input.world_normal = vec3<f32>(0.0, 0.0, 1.0);

    pbr_input.is_orthographic = false;

    pbr_input.N = vec3<f32>(0.0, 0.0, 1.0);
    pbr_input.V = vec3<f32>(1.0, 0.0, 0.0);

    pbr_input.lightmap_light = vec3<f32>(0.0);

    pbr_input.flags = 0u;

    return pbr_input;
}
