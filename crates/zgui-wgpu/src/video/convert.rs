//! The pass that turns a frame's planes into the colour texture a surface shows.

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::wgpu;
use crate::wgpu::util::DeviceExt as _;

use super::params::{self, Variant};
use super::{Planes, VideoFrame};

/// The shader, layouts and sampler of the conversion, built once per device, and a pipeline per
/// variant, built when a first frame needs it.
pub(crate) struct Converter {
    /// `convert.wgsl`.
    module: wgpu::ShaderModule,
    /// The bindings the pass reads.
    layout: wgpu::BindGroupLayout,
    /// The layout every variant shares.
    pipeline_layout: wgpu::PipelineLayout,
    /// Bilinear and clamped, so subsampled chroma is interpolated at the luma grid.
    sampler: wgpu::Sampler,
    /// One full-target draw per variant.
    pipelines: FxHashMap<Variant, wgpu::RenderPipeline>,
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("zgui.video.convert"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            module,
            layout,
            pipeline_layout,
            sampler,
            pipelines: FxHashMap::default(),
        }
    }

    /// The pipeline for `variant`, built on first use.
    fn pipeline(&mut self, device: &wgpu::Device, variant: Variant) -> &wgpu::RenderPipeline {
        let Self {
            module,
            pipeline_layout,
            pipelines,
            ..
        } = self;
        pipelines.entry(variant).or_insert_with(|| {
            let constants = [("MAPPED", f64::from(u8::from(variant.mapped)))];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("zgui.video.convert"),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &constants,
                        ..Default::default()
                    },
                    targets: &[Some(wgpu::ColorTargetState {
                        format: variant.output,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })
    }

    /// Converts `frame` into `target` and returns the texture that now holds it.
    ///
    /// `target` is reused while its size and format match the frame, and replaced when they do
    /// not. The frame's planes and guard are released once the device finished the pass.
    pub(crate) fn convert(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: VideoFrame,
        target: &mut Option<Arc<wgpu::Texture>>,
    ) -> Arc<wgpu::Texture> {
        let (width, height) = frame.size();
        let variant = Variant::of(&frame);
        let output = match target {
            Some(held)
                if held.width() == width
                    && held.height() == height
                    && held.format() == variant.output =>
            {
                Arc::clone(held)
            }
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
                    format: variant.output,
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
            contents: &params::uniform(&frame),
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
            pass.set_pipeline(self.pipeline(device, variant));
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use zgui_render_wgpu::Gpu;

    use super::*;
    use crate::video::testing::{BLUE, RED, device, halves, i420, plane, read, solid, upload};
    use crate::video::{
        ChromaSiting, ColorRange, ColorSpace, SampleDepth, SampleSize, TransferFunction,
    };

    /// Converts `frame` and reads the picture back as RGBA in [0, 1].
    fn convert(gpu: &Gpu, frame: VideoFrame) -> Vec<[f32; 4]> {
        let picture =
            Converter::new(gpu.device()).convert(gpu.device(), gpu.queue(), frame, &mut None);
        read(gpu, &picture)
    }

    /// Whether `actual` is within two 8-bit steps of `expected`, and opaque.
    fn near(actual: [f32; 4], expected: [u8; 3]) -> bool {
        actual[..3]
            .iter()
            .zip(expected)
            .all(|(a, e)| (a * 255.0 - f32::from(e)).abs() <= 2.0)
            && actual[3] == 1.0
    }

    /// A 16-bit 4:2:0 picture of one colour's codes, packed as `depth` says.
    fn deep(gpu: &Gpu, codes: [u16; 3], biplanar: bool, shift: u32) -> Option<Planes> {
        let bytes = |values: Vec<u16>| -> Vec<u8> {
            values
                .into_iter()
                .flat_map(|v| (v << shift).to_ne_bytes())
                .collect()
        };
        let luma = bytes(vec![codes[0]; 16]);
        let planes = if biplanar {
            let chroma = bytes([codes[1], codes[2]].repeat(4));
            Planes::upload_biplanar(
                gpu.device(),
                gpu.queue(),
                SampleSize::U16,
                plane(&luma, 8, 2),
                plane(&chroma, 4, 1),
            )
        } else {
            let (cb, cr) = (bytes(vec![codes[1]; 4]), bytes(vec![codes[2]; 4]));
            Planes::upload_triplanar(
                gpu.device(),
                gpu.queue(),
                SampleSize::U16,
                [plane(&luma, 8, 2), plane(&cb, 4, 1), plane(&cr, 4, 1)],
            )
        };
        match planes {
            Ok(planes) => Some(planes),
            Err(unsupported) => {
                eprintln!("skipped: {unsupported}");
                None
            }
        }
    }

    /// The luma code of a grey whose encoded value is `signal`, at 8-bit limited range.
    fn grey(signal: f32) -> [u8; 3] {
        [(16.0 + 219.0 * signal).round() as u8, 128, 128]
    }

    #[test]
    fn three_planes_convert_to_the_colours_their_codes_mean() {
        let Some((gpu, _held)) = device() else { return };
        let pixels = convert(&gpu, VideoFrame::new(i420(&gpu), ColorSpace::BT709));
        assert!(near(pixels[0], [255, 0, 0]), "{:?}", pixels[0]);
        assert!(near(pixels[15], [0, 0, 255]), "{:?}", pixels[15]);
    }

    #[test]
    fn interleaved_chroma_reads_cr_from_the_second_channel() {
        let Some((gpu, _held)) = device() else { return };
        let [luma, cb, cr] = halves(RED, BLUE);
        let chroma: Vec<u8> = cb.iter().zip(&cr).flat_map(|(b, r)| [*b, *r]).collect();
        let planes = Planes::upload_biplanar(
            gpu.device(),
            gpu.queue(),
            SampleSize::U8,
            plane(&luma, 8, 2),
            plane(&chroma, 4, 1),
        )
        .unwrap();
        let pixels = convert(&gpu, VideoFrame::new(planes, ColorSpace::BT709));
        assert!(near(pixels[0], [255, 0, 0]), "{:?}", pixels[0]);
        assert!(near(pixels[15], [0, 0, 255]), "{:?}", pixels[15]);
    }

    #[test]
    fn the_stated_range_decides_the_colour() {
        let Some((gpu, _held)) = device() else { return };
        let limited = convert(&gpu, VideoFrame::new(i420(&gpu), ColorSpace::BT709));
        let full = convert(
            &gpu,
            VideoFrame::new(i420(&gpu), ColorSpace::BT709.with_range(ColorRange::Full)),
        );
        assert_ne!(limited[0], full[0]);
    }

    #[test]
    fn a_visible_size_crops_the_padding_away() {
        let Some((gpu, _held)) = device() else { return };
        let picture = Converter::new(gpu.device()).convert(
            gpu.device(),
            gpu.queue(),
            VideoFrame::new(i420(&gpu), ColorSpace::BT709).with_visible_size(4, 2),
            &mut None,
        );
        assert_eq!((picture.width(), picture.height()), (4, 2));
        assert!(near(read(&gpu, &picture)[0], [255, 0, 0]));
    }

    #[test]
    fn the_target_is_reused_while_its_size_and_format_hold() {
        let Some((gpu, _held)) = device() else { return };
        let mut converter = Converter::new(gpu.device());
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
        assert!(!Arc::ptr_eq(&second, &cropped));
        let hdr = converter.convert(
            gpu.device(),
            gpu.queue(),
            VideoFrame::new(i420(&gpu), ColorSpace::BT2100_PQ).with_visible_size(2, 2),
            &mut target,
        );
        assert!(!Arc::ptr_eq(&cropped, &hdr));
        assert_eq!(hdr.format(), wgpu::TextureFormat::Rgba16Float);
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

    #[test]
    fn low_packed_ten_bit_planes_read_their_own_range() {
        let Some((gpu, _held)) = device() else { return };
        // BT.709 red at 10 bits.
        let Some(planes) = deep(&gpu, [250, 409, 960], false, 0) else {
            return;
        };
        let frame = VideoFrame::new(planes, ColorSpace::BT709).with_depth(SampleDepth::TEN);
        let pixels = convert(&gpu, frame);
        assert!(near(pixels[0], [255, 0, 0]), "{:?}", pixels[0]);
    }

    #[test]
    fn high_packed_p010_planes_read_their_own_range() {
        let Some((gpu, _held)) = device() else { return };
        let Some(planes) = deep(&gpu, [250, 409, 960], true, 6) else {
            return;
        };
        let frame = VideoFrame::new(planes, ColorSpace::BT709).with_depth(SampleDepth::P010);
        let pixels = convert(&gpu, frame);
        assert!(near(pixels[0], [255, 0, 0]), "{:?}", pixels[0]);
    }

    #[test]
    fn bt2020_greys_stay_grey_and_its_green_clips_into_bt709() {
        let Some((gpu, _held)) = device() else { return };
        let wide = convert(
            &gpu,
            VideoFrame::new(solid(&gpu, grey(0.5)), ColorSpace::BT2020),
        );
        let native = convert(
            &gpu,
            VideoFrame::new(solid(&gpu, grey(0.5)), ColorSpace::BT709),
        );
        assert!(
            wide[0][..3]
                .iter()
                .zip(&native[0][..3])
                .all(|(a, b)| (a - b).abs() < 2.0 / 255.0),
            "{:?} against {:?}",
            wide[0],
            native[0]
        );
        // Full BT.2020 green: Y′ = Kg, Cb = −Kg / 2(1 − Kb), Cr = −Kg / 2(1 − Kr).
        let (kr, kb) = (0.2627f32, 0.0593f32);
        let kg = 1.0 - kr - kb;
        let code = |v: f32, scale: f32, offset: f32| (offset + scale * v).round() as u8;
        let green = [
            code(kg, 219.0, 16.0),
            code(-kg / (2.0 * (1.0 - kb)), 224.0, 128.0),
            code(-kg / (2.0 * (1.0 - kr)), 224.0, 128.0),
        ];
        let pixels = convert(
            &gpu,
            VideoFrame::new(solid(&gpu, green), ColorSpace::BT2020),
        );
        assert!(near(pixels[0], [0, 255, 0]), "{:?}", pixels[0]);
    }

    #[test]
    fn wide_gamut_colours_match_a_reference_conversion() {
        let Some((gpu, _held)) = device() else { return };
        let decode = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let encode = |v: f32| {
            if v <= 0.0031308 {
                v * 12.92
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            }
        };
        let (kr, kb) = (0.2627f32, 0.0593f32);
        let kg = 1.0 - kr - kb;
        let crate::video::color::Gamut(gamut) =
            crate::video::color::Gamut::to_bt709(crate::video::ColorPrimaries::Bt2020);
        for [r, g, b] in [[0.6f32, 0.4, 0.3], [0.3, 0.5, 0.45], [0.5, 0.45, 0.6]] {
            let y = kr * r + kg * g + kb * b;
            let codes = [
                (16.0 + 219.0 * y).round() as u8,
                (128.0 + 224.0 * (b - y) / (2.0 * (1.0 - kb))).round() as u8,
                (128.0 + 224.0 * (r - y) / (2.0 * (1.0 - kr))).round() as u8,
            ];
            let linear = [r, g, b].map(decode);
            let expected = gamut.map(|row| {
                let v = row[0] * linear[0] + row[1] * linear[1] + row[2] * linear[2];
                (encode(v.clamp(0.0, 1.0)) * 255.0).round() as u8
            });
            let shown = convert(
                &gpu,
                VideoFrame::new(solid(&gpu, codes), ColorSpace::BT2020),
            )[0];
            assert!(near(shown, expected), "{shown:?} against {expected:?}");
        }
    }

    #[test]
    fn pq_black_stays_black_and_the_source_peak_shows_white() {
        let Some((gpu, _held)) = device() else { return };
        let peak = crate::video::color::transfer::pq_encode(1000.0);
        let frame = |signal| VideoFrame::new(solid(&gpu, grey(signal)), ColorSpace::BT2100_PQ);
        assert!(near(convert(&gpu, frame(0.0))[0], [0, 0, 0]));
        let white = convert(&gpu, frame(peak))[0];
        assert!(near(white, [255, 255, 255]), "{white:?}");
    }

    #[test]
    fn pq_brightness_rises_with_luminance_and_rolls_off_toward_the_peak() {
        let Some((gpu, _held)) = device() else { return };
        let pq = crate::video::color::transfer::pq_encode;
        let shown = |nits: f32| {
            let frame = VideoFrame::new(solid(&gpu, grey(pq(nits))), ColorSpace::BT2100_PQ)
                .with_peak_luminance(1000.0);
            convert(&gpu, frame)[0][1]
        };
        let (dim, white, bright, peak) = (shown(50.0), shown(203.0), shown(500.0), shown(1000.0));
        assert!(
            dim < white && white < bright && bright < peak,
            "{dim} {white} {bright} {peak}"
        );
        assert!(white > 0.85, "reference white stays bright: {white}");
    }

    #[test]
    fn hlg_at_full_signal_shows_white() {
        let Some((gpu, _held)) = device() else { return };
        let frame = VideoFrame::new(solid(&gpu, grey(1.0)), ColorSpace::BT2100_HLG);
        let white = convert(&gpu, frame)[0];
        assert!(near(white, [255, 255, 255]), "{white:?}");
        let dark = convert(
            &gpu,
            VideoFrame::new(solid(&gpu, grey(0.25)), ColorSpace::BT2100_HLG),
        )[0];
        assert!(dark[1] < white[1]);
    }

    #[test]
    fn pure_power_curves_decode_their_own_exponent() {
        let Some((gpu, _held)) = device() else { return };
        let mut space = ColorSpace::BT709;
        space.transfer = TransferFunction::Gamma28;
        let darker = convert(&gpu, VideoFrame::new(solid(&gpu, grey(0.5)), space))[0][1];
        let native = convert(
            &gpu,
            VideoFrame::new(solid(&gpu, grey(0.5)), ColorSpace::BT709),
        )[0][1];
        assert!(darker < native, "{darker} {native}");
    }

    #[test]
    fn chroma_siting_moves_the_colour_edge() {
        let Some((gpu, _held)) = device() else { return };
        let frame = |siting| {
            VideoFrame::new(upload(&gpu, halves(RED, BLUE)), ColorSpace::BT709)
                .with_chroma_siting(siting)
        };
        let center = convert(&gpu, frame(ChromaSiting::Center));
        let left = convert(&gpu, frame(ChromaSiting::Left));
        // The last red column reads more of the blue chroma when chroma sits on the left.
        assert!(left[3][2] > center[3][2], "{:?} {:?}", left[3], center[3]);
        assert!(near(left[0], [255, 0, 0]) && near(center[0], [255, 0, 0]));
    }
}
