//! CPU atlas storage and cached browser images of its tiles.

use std::collections::BTreeMap;

use wasm_bindgen::{Clamped, JsValue};
use web_sys::{HtmlCanvasElement, ImageData};
use zgui_atlas::{MemorySink, SinkError, TextureFormat, TextureId, TextureKind, TextureSink};
use zgui_geom::{Device, Rect, Size};
use zgui_scene::SpriteTile;

use crate::Surface;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ImageKey {
    texture: u32,
    bounds: [i32; 4],
    tint: Option<[u32; 4]>,
}

#[derive(Default)]
pub(crate) struct CanvasAtlas {
    memory: MemorySink,
    textures: BTreeMap<TextureId, u64>,
    images: BTreeMap<ImageKey, HtmlCanvasElement>,
}

impl CanvasAtlas {
    pub(crate) fn bytes_written(&self) -> u64 {
        self.memory.bytes_written()
    }

    pub(crate) fn bytes_held(&self) -> u64 {
        self.textures.values().sum::<u64>() + self.image_bytes()
    }

    fn image_bytes(&self) -> u64 {
        self.images
            .values()
            .map(|image| u64::from(image.width()) * u64::from(image.height()) * 4)
            .sum()
    }

    pub(crate) fn clear_images(&mut self) -> u64 {
        let freed = self.image_bytes();
        self.images.clear();
        freed
    }

    pub(crate) fn image(
        &mut self,
        tile: SpriteTile,
        tint: Option<[f32; 4]>,
    ) -> Result<HtmlCanvasElement, JsValue> {
        let key = ImageKey {
            texture: tile.texture,
            bounds: tile.bounds,
            tint: tint.map(|color| color.map(f32::to_bits)),
        };
        if let Some(image) = self.images.get(&key) {
            return Ok(image.clone());
        }
        let kind = TextureKind::ALL
            .get((tile.texture >> 16) as usize)
            .copied()
            .ok_or_else(|| JsValue::from_str("unresolved atlas texture"))?;
        let id = TextureId::new(kind, tile.texture & 0xffff);
        let [x, y, width, height] = tile.bounds;
        if width <= 0 || height <= 0 {
            return Err(JsValue::from_str("empty atlas tile"));
        }
        let mut pixels = vec![0; width as usize * height as usize * 4];
        for row in 0..height {
            for column in 0..width {
                let texel = self
                    .memory
                    .texel(id, x + column, y + row)
                    .ok_or_else(|| JsValue::from_str("atlas tile lies outside its texture"))?;
                let rgba = if let Some(color) = tint {
                    let coverage = match kind {
                        TextureKind::Mono => f32::from(texel[0]) / 255.0,
                        TextureKind::Subpixel => {
                            (f32::from(texel[0]) + f32::from(texel[1]) + f32::from(texel[2]))
                                / 765.0
                        }
                        _ => f32::from(texel[3]) / 255.0,
                    };
                    mask_pixel(color, coverage)
                } else {
                    color_pixel(texel)
                };
                let offset = (row as usize * width as usize + column as usize) * 4;
                pixels[offset..offset + 4].copy_from_slice(&rgba);
            }
        }
        let surface = Surface::new(width as u32, height as u32)?;
        let data = ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(&pixels),
            width as u32,
            height as u32,
        )?;
        surface.context.put_image_data(&data, 0.0, 0.0)?;
        self.images.insert(key, surface.canvas.clone());
        Ok(surface.canvas)
    }
}

fn byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// ImageData takes straight sRGB; zgui stores premultiplied sRGB.
fn mask_pixel(color: [f32; 4], coverage: f32) -> [u8; 4] {
    if color[3] <= 0.0 {
        return [0; 4];
    }
    [
        byte(color[0] / color[3]),
        byte(color[1] / color[3]),
        byte(color[2] / color[3]),
        byte(color[3] * coverage),
    ]
}

fn color_pixel(texel: &[u8]) -> [u8; 4] {
    let alpha = texel[3];
    if alpha == 0 {
        return [0; 4];
    }
    [
        byte(f32::from(texel[0]) / f32::from(alpha)),
        byte(f32::from(texel[1]) / f32::from(alpha)),
        byte(f32::from(texel[2]) / f32::from(alpha)),
        alpha,
    ]
}

impl TextureSink for CanvasAtlas {
    fn create_texture(
        &mut self,
        texture: TextureId,
        size: Size<i32, Device>,
        format: TextureFormat,
    ) -> Result<(), SinkError> {
        self.memory.create_texture(texture, size, format)?;
        self.textures.insert(
            texture,
            format.bytes_for(size.width as u32, size.height as u32),
        );
        self.clear_images();
        Ok(())
    }

    fn write_texture(
        &mut self,
        texture: TextureId,
        bounds: Rect<i32, Device>,
        format: TextureFormat,
        bytes: &[u8],
    ) -> Result<(), SinkError> {
        self.memory.write_texture(texture, bounds, format, bytes)?;
        // Atlas allocations can reuse a rectangle. Invalidate converted tiles on every upload.
        self.clear_images();
        Ok(())
    }

    fn destroy_texture(&mut self, texture: TextureId) {
        self.memory.destroy_texture(texture);
        self.textures.remove(&texture);
        self.clear_images();
    }
}
