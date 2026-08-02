//! The whole of a finished scene, as one block of text.

use core::fmt;

use zgui_bits::DamageSet;
use zgui_scene::{Batch, PlannedPass, Scene};

use crate::text::Writer;
use crate::text::number::rect;
use crate::transcript::{clip, primitive};

/// One finished scene, rendered as stable, diffable text.
///
/// Compare two of these and the diff is the frame's difference. Hold one in a file and it is a
/// golden. It carries no scene and borrows nothing, so it outlives the frame it was taken from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transcript {
    /// The rendered text, ending in a newline.
    text: String,
}

impl Transcript {
    /// The text.
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The text, taken.
    pub fn into_string(self) -> String {
        self.text
    }

    /// The lines, without their terminators.
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.text.lines()
    }

    /// How many lines there are.
    pub fn line_count(&self) -> usize {
        self.text.lines().count()
    }
}

impl fmt::Display for Transcript {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.text)
    }
}

/// Renders `scene`, drawn against `damage`, as a transcript.
///
/// The primitives are walked through the scene's own batching, so they appear in the order a
/// renderer would draw them and a batch that stopped merging is visible as a count. Vector content
/// appears where its composite falls in that order, with the pass's items nested underneath, which
/// is where it is actually drawn — a path is rasterised elsewhere and composited back in at one
/// point in the order, and listing the paths in array order would describe a frame nobody draws.
///
/// # Panics
///
/// Panics unless the scene has been finished: the arrays are not in draw order before then, and a
/// transcript taken from them would be a stable rendering of a sequence no renderer will ever use.
pub fn of(scene: &Scene, damage: &DamageSet) -> Transcript {
    assert!(
        scene.is_finished(),
        "a transcript needs a finished scene; call Scene::finish() first"
    );
    let mut writer = Writer::new();
    let viewport = scene.viewport();
    writer.line(&format!(
        "scene viewport={}x{}",
        viewport.width, viewport.height
    ));
    write_damage(&mut writer, damage);
    write_passes(&mut writer, scene);
    write_primitives(&mut writer, scene);
    Transcript {
        text: writer.finish(),
    }
}

/// The damage the frame was drawn against.
fn write_damage(writer: &mut Writer, damage: &DamageSet) {
    if damage.is_full() {
        writer.line("damage full");
        return;
    }
    writer.nested(&format!("damage rects={}", damage.len()), |writer| {
        for held in damage.rects() {
            writer.line(&format!(
                "rect({}, {}, {}, {})",
                held.origin.x, held.origin.y, held.size.width, held.size.height
            ));
        }
    });
}

/// The vector work the scene planned.
fn write_passes(writer: &mut Writer, scene: &Scene) {
    let plan = scene.pass_plan();
    let header = format!(
        "passes {} clip_layers={} culled={}",
        plan.passes.len(),
        plan.clip_layers,
        plan.culled
    );
    if plan.passes.is_empty() {
        writer.line(&header);
        return;
    }
    writer.nested(&header, |writer| {
        for (index, pass) in plan.passes.iter().enumerate() {
            writer.line(&pass_line(scene, index, pass));
        }
    });
}

/// One planned pass.
fn pass_line(scene: &Scene, index: usize, pass: &PlannedPass) -> String {
    format!(
        "pass {index} region={} clip={} instanced={} composite_order={} items={}",
        rect([
            pass.region.origin.x as f32,
            pass.region.origin.y as f32,
            pass.region.size.width as f32,
            pass.region.size.height as f32,
        ]),
        clip::chain(&scene.clips, pass.clip),
        pass.instanced,
        pass.composite_order,
        pass.items.len()
    )
}

/// Every primitive, in the order a renderer draws it.
fn write_primitives(writer: &mut Writer, scene: &Scene) {
    let batches: Vec<Batch> = scene.batches().collect();
    writer.nested(
        &format!(
            "primitives {} batches={}",
            scene.primitives.len(),
            batches.len()
        ),
        |writer| {
            for batch in &batches {
                write_batch(writer, scene, batch);
            }
        },
    );
}

/// One draw call's worth of primitives.
fn write_batch(writer: &mut Writer, scene: &Scene, batch: &Batch) {
    let primitives = &scene.primitives;
    match batch {
        Batch::Quads(range) => {
            for quad in &primitives.quads[range.clone()] {
                writer.line(&primitive::quad(scene, quad));
            }
        }
        Batch::Shadows(range) => {
            for shadow in &primitives.shadows[range.clone()] {
                writer.line(&primitive::shadow(scene, shadow));
            }
        }
        Batch::Decorations(range) => {
            for decoration in &primitives.decorations[range.clone()] {
                writer.line(&primitive::decoration(scene, decoration));
            }
        }
        Batch::MonoSprites { range, .. } => {
            for sprite in &primitives.mono_sprites[range.clone()] {
                writer.line(&primitive::mono_sprite(scene, sprite));
            }
        }
        Batch::SubpixelSprites { range, .. } => {
            for sprite in &primitives.subpixel_sprites[range.clone()] {
                writer.line(&primitive::subpixel_sprite(scene, sprite));
            }
        }
        Batch::ColorSprites { range, .. } => {
            for sprite in &primitives.color_sprites[range.clone()] {
                writer.line(&primitive::color_sprite(scene, sprite));
            }
        }
        Batch::External(index) => {
            writer.line(&primitive::external(scene, &primitives.externals[*index]));
        }
        Batch::Backdrop(index) => {
            writer.line(&primitive::backdrop(scene, &primitives.backdrops[*index]));
        }
        Batch::Group(index) => {
            writer.line(&primitive::group(scene, &primitives.groups[*index]));
        }
        Batch::Vector(index) => write_composite(writer, scene, *index),
    }
}

/// One vector composite, with the items its pass rasterised.
fn write_composite(writer: &mut Writer, scene: &Scene, index: usize) {
    let plan = scene.pass_plan();
    let Some(pass) = plan.passes.get(index) else {
        writer.line(&format!("vector_composite pass={index} <missing>"));
        return;
    };
    writer.nested(
        &format!("vector_composite {}", pass_line(scene, index, pass)),
        |writer| {
            for item in plan.items_of(pass) {
                let Some(vector) = scene.primitives.vectors.get(item.item) else {
                    writer.line(&format!("<missing vector item {}>", item.item));
                    continue;
                };
                let mut line = primitive::vector(scene, vector);
                if !item.residual.is_root() {
                    line.push_str(&format!(
                        " residual={}",
                        clip::chain(&scene.clips, item.residual)
                    ));
                }
                writer.line(&line);
            }
        },
    );
}
