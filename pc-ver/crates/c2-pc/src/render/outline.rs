//! The advanced binoculars' outlines (gear, not in the original): an
//! outlined model is first drawn in one flat colour, shifted a few pixels
//! in each of eight directions, depth-tested against the world but not
//! written; the model itself then covers the middle, leaving a rim
//! (`model.wgsl`: `vs_outline`).

use c2_core::game::Outline;

use super::{
    model_buffers, pipeline, pipeline_layout, shader, Blend, Globals, GpuModel, Instance,
    ModelRenderer, PipelineDesc,
};
use crate::gpu::Gpu;

/// Copies of an outlined model, one per direction.
pub(super) const DIRECTIONS: usize = 8;

pub(super) struct OutlinePipelines {
    /// Solid faces: every texel counts.
    solid: wgpu::RenderPipeline,
    /// Cutout faces: transparent texels leave no rim.
    cutout: wgpu::RenderPipeline,
}

impl ModelRenderer {
    /// This renderer, able to draw outlines ([`ModelRenderer::draw_outlined`]).
    pub fn with_outlines(mut self, gpu: &Gpu, globals: &Globals) -> Self {
        let layout = pipeline_layout(gpu, "model outline", &[&globals.layout, &self.tex_layout]);
        let module = shader(gpu, "model", include_str!("../shaders/model.wgsl"));
        let buffers = model_buffers();
        let desc = |label, fs| PipelineDesc {
            label,
            module: &module,
            layout: &layout,
            vs: "vs_outline",
            fs,
            buffers: &buffers,
            blend: Blend::Overlay,
        };
        self.outline = Some(OutlinePipelines {
            solid: pipeline(gpu, desc("model outline", "fs_outline")),
            cutout: pipeline(gpu, desc("model outline cutout", "fs_outline_cutout")),
        });
        self
    }
}

/// The eight shifted copies of `base` (an outlined model's instance):
/// `dither` carries the direction (0..8), `light` the colour (0 calm,
/// 1 hostile), as `vs_outline` reads them.
pub(super) fn copies(base: Instance, tint: Outline) -> [Instance; DIRECTIONS] {
    let light = match tint {
        Outline::Calm => 0.0,
        Outline::Hostile => 1.0,
    };
    std::array::from_fn(|d| Instance {
        light,
        dither: d as f32,
        ..base
    })
}

impl OutlinePipelines {
    /// Draws the outlines: `outlined[k]` names a batch, whose copies start
    /// at instance `first + k * DIRECTIONS`. Blended faces leave no rim.
    pub(super) fn draw(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        batches: &[(&GpuModel, &[Instance])],
        outlined: &[usize],
        first: u32,
    ) {
        for (pipe, cutout) in [(&self.solid, false), (&self.cutout, true)] {
            pass.set_pipeline(pipe);
            for (k, &b) in outlined.iter().enumerate() {
                let Some((m, _)) = batches.get(b) else {
                    continue;
                };
                let range = if cutout {
                    m.opaque..m.cutout
                } else {
                    0..m.opaque
                };
                if range.is_empty() {
                    continue;
                }
                let at = first + (k * DIRECTIONS) as u32;
                pass.set_vertex_buffer(0, m.vbuf.slice(..));
                pass.draw(range, at..at + DIRECTIONS as u32);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_carry_direction_and_colour_and_keep_the_pose() {
        let base = Instance {
            offset: [1.0, 2.0, 3.0],
            turns: 2.0,
            light: 0.8,
            layer: 5,
            dither: 0.0,
        };
        for (tint, light) in [(Outline::Calm, 0.0), (Outline::Hostile, 1.0)] {
            let c = copies(base, tint);
            for (d, i) in c.iter().enumerate() {
                assert_eq!(i.dither, d as f32);
                assert_eq!(i.light, light);
                assert_eq!(
                    (i.offset, i.turns, i.layer),
                    (base.offset, base.turns, base.layer)
                );
            }
        }
    }
}
