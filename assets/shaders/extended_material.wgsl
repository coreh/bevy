#import bevy_pbr::pbr_fragment          pbr_input_from_standard_material
#import bevy_pbr::pbr_functions         alpha_discard
#import bevy_pbr::mesh_view_bindings as view_bindings

#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io            VertexOutput, FragmentOutput
#import bevy_pbr::pbr_deferred_functions  deferred_output
#else
#import bevy_pbr::forward_io            VertexOutput, FragmentOutput
#import bevy_pbr::pbr_functions         apply_pbr_lighting, main_pass_post_lighting_processing
#endif

struct MyExtendedMaterial {
    quantize_steps: u32,
}

@group(1) @binding(100)
var<uniform> my_extended_material: MyExtendedMaterial;

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    // generate a PbrInput struct from the StandardMaterial bindings
    var pbr_input = pbr_input_from_standard_material(in, is_front);

    let scale = 12.0;
    pbr_input.material.thickness += cos(pbr_input.world_position.x * scale + view_bindings::globals.time * 10.0) * 0.1 + sin(pbr_input.world_position.y * scale + view_bindings::globals.time * 10.0) * 0.1 + sin(pbr_input.world_position.z * scale + view_bindings::globals.time * 5.0) * 0.1;
    pbr_input.material.reflectance = 0.0;
    pbr_input.N = normalize(pbr_input.N + vec3<f32>(cos(pbr_input.world_position.x * scale + view_bindings::globals.time * 10.0) * 0.1 + sin(pbr_input.world_position.y * scale + view_bindings::globals.time * 10.0) * 0.1 + sin(pbr_input.world_position.z * scale + view_bindings::globals.time * 5.0) * 0.1, 0.0, 0.0));
    // pbr_input.material.perceptual_roughness += cos(pbr_input.world_position.x * 10.0 + view_bindings::globals.time * 10.0) * 0.2 + sin(pbr_input.world_position.y * 30.0 + view_bindings::globals.time * 10.0) * 0.2 + sin(pbr_input.world_position.z * 20.0 + view_bindings::globals.time * 5.0) * 0.2;
    pbr_input.material.emissive.r += (
        pow(max(0.0, 1.0-dot(pbr_input.N, pbr_input.V)), 8.0) +
        max(0.0, cos(pbr_input.N.x * 5.0 + pbr_input.world_position.y * 200.0 + view_bindings::globals.time * 10.0) * 0.004) * max(0.0, 1.0-dot(pbr_input.N, pbr_input.V))
    ) * 100.0 * pbr_input.material.thickness;

    // alpha discard
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

#ifdef PREPASS_PIPELINE
    // in deferred mode we can't modify anything after that, as lighting is run in a separate fullscreen shader.
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);

    out.color = main_pass_post_lighting_processing(pbr_input, out.color);

#endif

    return out;
}
