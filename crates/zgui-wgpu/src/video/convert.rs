//! The pass that turns a frame's planes into the colour texture a surface shows.

use std::sync::Arc;

use crate::wgpu;
use crate::wgpu::util::DeviceExt as _;

use super::{Planes, VideoFrame};

/// The format of the converted picture: unencoded, so the gamma-encoded values pass unchanged.
const OUTPUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The pipeline, layout and sampler of the conversion, built once per device.
pub(crate) struct Converter {
    /// The full-target draw that converts.
    pipeline: wgpu::RenderPipeline,
    /// The bindings it reads.
    layout: wgpu::BindGroupLayout,
    /// Bilinear and clamped, so subsampled chroma is interpolated at the luma grid.
    sampler: wgpu::Sampler,
}

impl Converter {
    /// Builds the conversion for `device`.
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("zgui.video.convert"),
            source: wgpu::ShaderSource::Wgsl(include_str!("convert.wgsl").into()),
        });
        let plane = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("zgui.video.convert"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                plane(2),
                plane(3),
                plane(4),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("zgui.video.convert"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("zgui.video.convert"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: OUTPUT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("zgui.video.convert"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            layout,
            sampler,
        }
    }

    /// Converts `frame` into `target` and returns the texture that now holds it.
    ///
    /// `target` is reused while its size matches the frame, and replaced when it does not. The
    /// frame's planes and guard are released once the device finished the pass.
    pub(crate) fn convert(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: VideoFrame,
        target: &mut Option<Arc<wgpu::Texture>>,
    ) -> Arc<wgpu::Texture> {
        let (width, height) = frame.size();
        let output = match target {
            Some(held) if held.width() == width && held.height() == height => Arc::clone(held),
            _ => {
                let fresh = Arc::new(device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("zgui.video.picture"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: OUTPUT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                }));
                *target = Some(Arc::clone(&fresh));
                fresh
            }
        };

        let params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("zgui.video.params"),
            contents: &params_bytes(&frame),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let view =
            |texture: &wgpu::Texture| texture.create_view(&wgpu::TextureViewDescriptor::default());
        let (luma, cb, cr) = match &frame.planes {
            Planes::Biplanar { luma, chroma } => (view(luma), view(chroma), view(chroma)),
            Planes::Triplanar { luma, cb, cr } => (view(luma), view(cb), view(cr)),
        };
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("zgui.video.convert"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&luma),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&cb),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&cr),
                },
            ],
        });

        let target_view = view(&output);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("zgui.video.convert"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("zgui.video.convert"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([encoder.finish()]);
        // wgpu keeps the planes alive until the pass completes; the guard needs the same promise.
        if let Some(guard) = frame.guard {
            queue.on_submitted_work_done(move || drop(guard));
        }
        output
    }
}

/// The uniform block `convert.wgsl` reads for `frame`.
fn params_bytes(frame: &VideoFrame) -> Vec<u8> {
    let (width, height) = frame.size();
    let luma = frame.planes.luma();
    let biplanar = matches!(frame.planes, Planes::Biplanar { .. });
    let [red, green, blue] = frame.color.transform();
    let shape = [
        width as f32 / luma.width() as f32,
        height as f32 / luma.height() as f32,
        f32::from(u8::from(biplanar)),
        0.0,
    ];
    [red, green, blue, shape]
        .into_iter()
        .flatten()
        .flat_map(f32::to_ne_bytes)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use zgui_geom::Size;
    use zgui_render_wgpu::Gpu;
    use zgui_render_wgpu::renderer::readback;

    use super::*;
    use crate::video::testing::{BLUE, RED, device, i420};
    use crate::video::{ColorMatrix, ColorRange, ColorSpace};

    /// Converts `frame` and reads the picture back as RGBA rows.
    fn convert(gpu: &Gpu, frame: VideoFrame) -> readback::Pixels {
        let mut target = None;
        let picture =
            Converter::new(gpu.device()).convert(gpu.device(), gpu.queue(), frame, &mut target);
        let size = Size::new(picture.width() as i32, picture.height() as i32);
        readback::read(gpu, &picture, picture.format(), size)
    }

    fn near(actual: [u8; 4], expected: [u8; 3]) -> bool {
        actual[..3]
            .iter()
            .zip(expected)
            .all(|(&a, e)| a.abs_diff(e) <= 2)
            && actual[3] == 255
    }

    #[test]
    fn three_planes_convert_to_the_colours_their_samples_mean() {
        let Some((gpu, _held)) = device() else { return };
        let pixels = convert(&gpu, VideoFrame::new(i420(&gpu), ColorSpace::BT709));
        assert!(
            near(pixels.rgba(0, 0), [255, 0, 0]),
            "{:?}",
            pixels.rgba(0, 0)
        );
        assert!(
            near(pixels.rgba(7, 1), [0, 0, 255]),
            "{:?}",
            pixels.rgba(7, 1)
        );
    }

    #[test]
    fn interleaved_chroma_reads_cr_from_the_second_channel() {
        let Some((gpu, _held)) = device() else { return };
        let luma: Vec<u8> = (0..16)
            .map(|i| if i % 8 < 4 { RED[0] } else { BLUE[0] })
            .collect();
        let chroma = [
            RED[1], RED[2], RED[1], RED[2], BLUE[1], BLUE[2], BLUE[1], BLUE[2],
        ];
        let planes = Planes::upload_nv12(gpu.device(), gpu.queue(), 8, 2, (&luma, 8), (&chroma, 8));
        let pixels = convert(&gpu, VideoFrame::new(planes, ColorSpace::BT709));
        assert!(
            near(pixels.rgba(0, 0), [255, 0, 0]),
            "{:?}",
            pixels.rgba(0, 0)
        );
        assert!(
            near(pixels.rgba(7, 1), [0, 0, 255]),
            "{:?}",
            pixels.rgba(7, 1)
        );
    }

    #[test]
    fn the_stated_colour_space_decides_the_colour() {
        let Some((gpu, _held)) = device() else { return };
        let full = ColorSpace::new(ColorMatrix::Bt709, ColorRange::Full);
        let limited = convert(&gpu, VideoFrame::new(i420(&gpu), ColorSpace::BT709));
        let full = convert(&gpu, VideoFrame::new(i420(&gpu), full));
        assert_ne!(limited.rgba(0, 0), full.rgba(0, 0));
    }

    #[test]
    fn a_visible_size_crops_the_padding_away() {
        let Some((gpu, _held)) = device() else { return };
        let pixels = convert(
            &gpu,
            VideoFrame::new(i420(&gpu), ColorSpace::BT709).with_visible_size(4, 2),
        );
        assert_eq!(pixels.size(), Size::new(4, 2));
        assert!(
            near(pixels.rgba(0, 0), [255, 0, 0]),
            "{:?}",
            pixels.rgba(0, 0)
        );
    }

    #[test]
    fn the_target_is_reused_while_the_size_holds() {
        let Some((gpu, _held)) = device() else { return };
        let converter = Converter::new(gpu.device());
        let mut target = None;
        let frame = || VideoFrame::new(i420(&gpu), ColorSpace::BT709);
        let first = converter.convert(gpu.device(), gpu.queue(), frame(), &mut target);
        let second = converter.convert(gpu.device(), gpu.queue(), frame(), &mut target);
        assert!(Arc::ptr_eq(&first, &second));
        let cropped = converter.convert(
            gpu.device(),
            gpu.queue(),
            frame().with_visible_size(2, 2),
            &mut target,
        );
        assert!(!Arc::ptr_eq(&first, &cropped));
    }

    #[test]
    fn the_guard_is_released_once_the_device_is_done() {
        let Some((gpu, _held)) = device() else { return };
        struct Flag(Arc<AtomicBool>);
        impl Drop for Flag {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let released = Arc::new(AtomicBool::new(false));
        let frame =
            VideoFrame::new(i420(&gpu), ColorSpace::BT709).with_guard(Flag(Arc::clone(&released)));
        Converter::new(gpu.device()).convert(gpu.device(), gpu.queue(), frame, &mut None);
        gpu.wait();
        assert!(released.load(Ordering::SeqCst));
    }
}
