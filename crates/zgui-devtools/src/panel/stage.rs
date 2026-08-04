//! What a stage of a frame is called in words, and which part of the pipeline it belongs to.
//!
//! The timeline is read out of the latency marks the frame loop writes, and those are named for the
//! person who put them there: `f.restyle`, `p.glyphs`, `acq.out`. That is the right name to write in
//! the source and the wrong one to show somebody trying to find out why their window is slow, so
//! this maps one to the other.
//!
//! **A mark is written *before* the work it names.** `mark("f.restyle"); self.restyle();` — so the
//! gap between `f.restyle` and the mark after it is the restyle, and a stage is labelled by the work
//! its *opening* mark introduces. Getting this backwards would put every duration against the wrong
//! name, which is worse than showing the raw marks.
//!
//! Anything unrecognised keeps its raw name and lands in [`Category::Other`]. A build that adds a
//! mark and forgets this table therefore shows the new stage rather than hiding it — the table is an
//! improvement on the raw name, never a filter over it.

/// Which part of the pipeline a stage belongs to, which is what colours it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Category {
    /// Input, timers, the reactive flush: everything that produces changes.
    Events,
    /// The cascade and what it feeds.
    Style,
    /// Boxes, layout and the geometry published from it.
    Layout,
    /// Turning laid-out boxes into a scene.
    Paint,
    /// The renderer, up to the point the work leaves the CPU.
    Render,
    /// Waiting on the graphics device or the compositor.
    Gpu,
    /// Everything this build has no name for.
    #[default]
    Other,
}

impl Category {
    /// Every category, in pipeline order, which is the order the legend lists them.
    pub(crate) const ALL: [Self; 7] = [
        Self::Events,
        Self::Style,
        Self::Layout,
        Self::Paint,
        Self::Render,
        Self::Gpu,
        Self::Other,
    ];

    /// What the legend calls it.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Events => "events",
            Self::Style => "style",
            Self::Layout => "layout",
            Self::Paint => "paint",
            Self::Render => "render",
            Self::Gpu => "gpu",
            Self::Other => "other",
        }
    }

    /// The suffix its class names end in, which is what the sheet colours on.
    pub(crate) const fn suffix(self) -> &'static str {
        self.label()
    }
}

/// What a stage opened by `name` did, and which part of the pipeline did it.
///
/// The label is `None` for a mark this build has no words for, and the caller shows the raw name
/// instead of inventing one.
pub(crate) fn describe(name: &str) -> (Option<&'static str>, Category) {
    if let Some(known) = exact(name) {
        return (Some(known.0), known.1);
    }
    (None, family(name))
}

/// The stages this build knows by name.
///
/// Written out rather than derived from the prefix, because the prefix says which subsystem wrote
/// the mark and the label has to say what the machine was doing — and those differ often enough
/// that a rule would be wrong in both directions.
fn exact(name: &str) -> Option<(&'static str, Category)> {
    use Category::{Events, Gpu, Layout, Paint, Render, Style};
    Some(match name {
        // The frame loop, in the order it runs.
        "f.begin" => ("Start the frame", Events),
        "f.drain" => ("Dispatch input events", Events),
        "f.timers" => ("Fire timers", Events),
        "f.scroll" => ("Advance scrolling", Events),
        "f.gestures" => ("Advance gestures", Events),
        "f.reconfigure" => ("Reconfigure the surface", Render),
        "f.device" => ("Advance the device epoch", Render),
        "f.animate" => ("Advance animations", Style),
        "f.flush" => ("Flush reactive updates", Events),
        "f.commands" => ("Carry out commands", Events),
        "f.restyle" => ("Recompute styles", Style),
        "f.restyled" => ("Publish running animations", Style),
        "f.brushes" => ("Update text brushes", Style),
        "f.boxes" => ("Build the box tree", Layout),
        "f.layout" => ("Lay out", Layout),
        "f.fragments" => ("Publish fragments", Layout),
        "f.enter" => ("Enter owed focus traps", Events),
        "f.dispatch_scroll" => ("Dispatch scroll events", Events),
        "f.observe" => ("Deliver geometry observations", Layout),
        "f.rehit" => ("Rebuild hit testing", Layout),
        "f.publish_brushes" => ("Publish text brushes", Style),
        "f.caret" => ("Plan carets", Paint),
        "f.paint" => ("Paint and draw", Paint),
        "f.painted" => ("Record what was presented", Gpu),
        "f.a11y" => ("Publish accessibility", Events),
        "f.recycle" => ("Recycle the frame's arenas", Events),
        "f.end" => ("End the frame", Events),
        "f.declined" => ("Decline the frame", Events),
        "f.held" => ("Hold the frame", Events),
        // Damage bookkeeping: instants either side of the stages that move it.
        "d.boxes" => ("Damage before the box tree", Layout),
        "d.prelayout" => ("Damage before layout", Layout),
        "d.postlayout" => ("Damage after layout", Layout),
        "d.postexpand" => ("Damage after expansion", Paint),
        // Paint.
        "p.expand" => ("Expand the damage", Paint),
        "p.emit" => ("Emit the display list", Paint),
        "p.finish" => ("Finish the scene", Paint),
        "p.upload" => ("Upload textures", Paint),
        "p.glyphs" => ("Rasterise glyphs", Paint),
        "p.draw" => ("Draw", Paint),
        // The renderer.
        "draw.in" => ("Hand the scene to the renderer", Render),
        "draw.undamaged" => ("Skip an undamaged frame", Render),
        "r.buffers" => ("Update GPU buffers", Render),
        "r.vectors" => ("Encode vector paths", Render),
        "r.plan" => ("Plan the passes", Render),
        "r.blocks" => ("Build the draw blocks", Render),
        "r.record" => ("Record GPU commands", Render),
        // The device and the compositor.
        "acq.in" | "acq.out" => ("Acquire the surface", Gpu),
        "sub.out" => ("Submit to the queue", Gpu),
        "pres.out" => ("Present", Gpu),
        // Text, and the window itself.
        "t.reshaped" => ("Reshape text", Style),
        "w.cfg" => ("Configure the window", Render),
        "w.resize" => ("Resize the window", Layout),
        "w.resize.same" => ("Resize to the size it had", Layout),
        "w.resize.deferred" => ("Defer a resize", Layout),
        "w.rescale" => ("Rescale the window", Layout),
        "w.laidout" => ("Lay out for the new size", Layout),
        "w.reclamp" => ("Re-clamp the scroll", Layout),
        "evt.in" | "evt.out" => ("Take platform events", Events),
        "wait.in" => ("Wait for events", Events),
        "cfg.in" | "cfg.same" => ("Configure the surface", Render),
        "req.redraw" => ("Ask for a redraw", Events),
        _ => return None,
    })
}

/// Which part of the pipeline an unrecognised mark most likely came from.
///
/// By the prefix the subsystems already use, so a mark added tomorrow is at least the right colour
/// even before anybody writes it a label.
fn family(name: &str) -> Category {
    let prefix = name.split('.').next().unwrap_or(name);
    match prefix {
        "f" | "evt" | "wait" | "req" => Category::Events,
        "why" | "b" | "t" => Category::Style,
        "d" | "w" => Category::Layout,
        "p" => Category::Paint,
        "r" | "draw" | "cfg" => Category::Render,
        "acq" | "sub" | "pres" => Category::Gpu,
        _ => Category::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::{Category, describe};

    #[test]
    fn a_stage_this_build_knows_is_shown_in_words() {
        assert_eq!(
            describe("f.restyle"),
            (Some("Recompute styles"), Category::Style)
        );
        assert_eq!(describe("f.layout"), (Some("Lay out"), Category::Layout));
        assert_eq!(
            describe("p.glyphs"),
            (Some("Rasterise glyphs"), Category::Paint)
        );
        assert_eq!(describe("pres.out"), (Some("Present"), Category::Gpu));
    }

    #[test]
    fn a_mark_this_build_has_no_words_for_keeps_its_own_name() {
        // The property that matters: a build that adds a mark and forgets this table shows the new
        // stage under its raw name rather than dropping it, so the table can never hide a stage.
        assert_eq!(describe("t.stage").0, None);
        assert_eq!(describe("zzz.unheard-of"), (None, Category::Other));
    }

    #[test]
    fn an_unlabelled_mark_still_takes_the_colour_of_the_subsystem_that_wrote_it() {
        assert_eq!(describe("why.initial").1, Category::Style);
        assert_eq!(describe("p.something-new").1, Category::Paint);
        assert_eq!(describe("acq.something-new").1, Category::Gpu);
    }
}
