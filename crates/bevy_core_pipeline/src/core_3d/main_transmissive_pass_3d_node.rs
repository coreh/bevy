use super::{Camera3d, ViewTransmissionTexture};
use crate::core_3d::Transmissive3d;
use bevy_ecs::{prelude::*, query::QueryItem};
use bevy_render::{
    camera::ExtractedCamera,
    render_graph::{NodeRunError, RenderGraphContext, ViewNode},
    render_phase::RenderPhase,
    render_resource::{
        Extent3d, LoadOp, Operations, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    },
    renderer::RenderContext,
    view::{ViewDepthTexture, ViewTarget},
};
#[cfg(feature = "trace")]
use bevy_utils::tracing::info_span;
use std::ops::Range;

/// A [`bevy_render::render_graph::Node`] that runs the [`Transmissive3d`] [`RenderPhase`].
#[derive(Default)]
pub struct MainTransmissivePass3dNode;

impl ViewNode for MainTransmissivePass3dNode {
    type ViewQuery = (
        &'static ExtractedCamera,
        &'static Camera3d,
        &'static RenderPhase<Transmissive3d>,
        &'static ViewTarget,
        Option<&'static ViewTransmissionTexture>,
        &'static ViewDepthTexture,
    );

    fn run(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        (camera, camera_3d, transmissive_phase, target, transmission, depth): QueryItem<
            Self::ViewQuery,
        >,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let view_entity = graph.view_entity();

        let physical_target_size = camera.physical_target_size.unwrap();

        let render_pass_descriptor = RenderPassDescriptor {
            label: Some("main_transmissive_pass_3d"),
            // NOTE: The transmissive pass loads the color buffer as well as overwriting it where appropriate.
            color_attachments: &[Some(target.get_color_attachment(Operations {
                load: LoadOp::Load,
                store: true,
            }))],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                view: &depth.view,
                // NOTE: The transmissive main pass loads the depth buffer and possibly overwrites it
                depth_ops: Some(Operations {
                    load: LoadOp::Load,
                    store: true,
                }),
                stencil_ops: None,
            }),
        };

        // Run the transmissive pass, sorted back-to-front
        // NOTE: Scoped to drop the mutable borrow of render_context
        #[cfg(feature = "trace")]
        let _main_transmissive_pass_3d_span = info_span!("main_transmissive_pass_3d").entered();

        if !transmissive_phase.items.is_empty() {
            let transmissive_steps = camera_3d.transmissive_steps;
            if transmissive_steps > 0 {
                let transmission =
                    transmission.expect("`ViewTransmissionTexture` should exist at this point");
                for range in split_range(0..transmissive_phase.items.len(), transmissive_steps) {
                    render_context.command_encoder().copy_texture_to_texture(
                        target.main_texture().as_image_copy(),
                        transmission.texture.as_image_copy(),
                        Extent3d {
                            width: physical_target_size.x,
                            height: physical_target_size.y,
                            depth_or_array_layers: 1,
                        },
                    );

                    let mut render_pass =
                        render_context.begin_tracked_render_pass(render_pass_descriptor.clone());

                    if let Some(viewport) = camera.viewport.as_ref() {
                        render_pass.set_camera_viewport(viewport);
                    }

                    transmissive_phase.render_range(&mut render_pass, world, view_entity, range);
                }
            } else {
                let mut render_pass =
                    render_context.begin_tracked_render_pass(render_pass_descriptor);

                if let Some(viewport) = camera.viewport.as_ref() {
                    render_pass.set_camera_viewport(viewport);
                }

                transmissive_phase.render(&mut render_pass, world, view_entity);
            }
        }

        Ok(())
    }
}

/// Splits a [`Range`] into at most `max_num_splits` sub-ranges without overlaps
///
/// Properly takes into account remainders of inexact divisions (by adding extra
/// elements to the initial sub-ranges as needed)
fn split_range(range: Range<usize>, max_num_splits: usize) -> impl Iterator<Item = Range<usize>> {
    let len = range.end - range.start;
    assert!(len > 0, "to be split, a range must not be empty");
    assert!(max_num_splits > 0, "max_num_splits must be at least 1");
    let num_splits = max_num_splits.min(len);
    let step = len / num_splits;
    let mut rem = len % num_splits;
    let mut start = range.start;

    (0..num_splits).map(move |_| {
        let extra = if rem > 0 {
            rem -= 1;
            1
        } else {
            0
        };
        let end = (start + step + extra).min(range.end);
        let result = start..end;
        start = end;
        result
    })
}
