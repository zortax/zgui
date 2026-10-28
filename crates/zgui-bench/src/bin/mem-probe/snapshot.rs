//! One reading of everything the probe reports, and how a run is written out.

use std::fmt::Write as _;

use zgui::runtime::Window;
use zgui_profile::Counter;
use zgui_profile::alloc_tag::Tag;

use crate::libc;
use crate::tagging::{self, TagFigures};

/// A named group of figures.
pub(crate) type Group = Vec<(&'static str, u64)>;

/// Everything read at one moment of a run.
pub(crate) struct Snapshot {
    /// The moment's name.
    pub(crate) name: &'static str,
    /// Each tag's figures, by tag index.
    pub(crate) tags: [TagFigures; Tag::COUNT],
    /// What the process holds, from `/proc` and the allocator's own headers.
    pub(crate) process: Group,
    /// The C allocator's statistics.
    pub(crate) mallinfo: Group,
    /// The frame counters that hold a live count.
    pub(crate) live: Group,
    /// How many of each kind of object the window holds.
    pub(crate) objects: Group,
    /// What the renderer and the device report.
    pub(crate) gpu: Group,
}

impl Snapshot {
    /// Reads everything now, with `window` the probe's window and `vram_mib` what a wrapper
    /// reported for the process, if it reported anything.
    pub(crate) fn take(name: &'static str, window: &Window, vram_mib: Option<u64>) -> Self {
        let tags = tagging::figures();
        let mut process = proc_figures();
        process.push(("tag_header_bytes", tagging::header_bytes()));
        process.push((
            "tagged_live_bytes",
            tags.iter().map(|figures| figures.live).sum(),
        ));
        let info = libc::mallinfo();
        let mallinfo = vec![
            ("arena", info.arena as u64),
            ("ordblks", info.ordblks as u64),
            ("hblks", info.hblks as u64),
            ("hblkhd", info.hblkhd as u64),
            ("uordblks", info.uordblks as u64),
            ("fordblks", info.fordblks as u64),
            ("keepcost", info.keepcost as u64),
        ];
        let counters = zgui_profile::counter::snapshot();
        let live = Counter::live()
            .map(|counter| (counter.name(), counters.get(counter)))
            .collect();
        Self {
            name,
            tags,
            process,
            mallinfo,
            live,
            objects: objects(window),
            gpu: gpu(window, vram_mib),
        }
    }

    /// One tag's live bytes.
    pub(crate) fn live_bytes(&self, tag: Tag) -> u64 {
        self.tags[tag.index()].live
    }

    /// One figure of the process group, or zero.
    pub(crate) fn process(&self, key: &str) -> u64 {
        self.process
            .iter()
            .find(|(name, _)| *name == key)
            .map_or(0, |&(_, value)| value)
    }
}

/// The resident set and its parts, from `/proc/self/statm` and `/proc/self/smaps_rollup`, and the
/// thread count from `/proc/self/status`.
fn proc_figures() -> Group {
    let mut group = Group::new();
    let page = libc::page_size();
    if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
        let mut fields = statm.split_whitespace().map(|field| field.parse::<u64>());
        if let (Some(Ok(size)), Some(Ok(resident))) = (fields.next(), fields.next()) {
            group.push(("statm_size_bytes", size * page));
            group.push(("statm_resident_bytes", resident * page));
        }
    }
    const ROLLUP: [(&str, &str); 8] = [
        ("Rss:", "rss_bytes"),
        ("Pss:", "pss_bytes"),
        ("Anonymous:", "anon_bytes"),
        ("Private_Dirty:", "private_dirty_bytes"),
        ("Private_Clean:", "private_clean_bytes"),
        ("Shared_Clean:", "shared_clean_bytes"),
        ("Shared_Dirty:", "shared_dirty_bytes"),
        ("Swap:", "swap_bytes"),
    ];
    if let Some(threads) = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("Threads:"))
                .and_then(|count| count.trim().parse::<u64>().ok())
        })
    {
        group.push(("threads", threads));
    }
    if let Ok(rollup) = std::fs::read_to_string("/proc/self/smaps_rollup") {
        for line in rollup.lines() {
            let mut fields = line.split_whitespace();
            let (Some(field), Some(kib)) = (fields.next(), fields.next()) else {
                continue;
            };
            let Some(&(_, name)) = ROLLUP.iter().find(|(label, _)| *label == field) else {
                continue;
            };
            if let Ok(kib) = kib.parse::<u64>() {
                group.push((name, kib * 1024));
            }
        }
    }
    group
}

/// How many of each kind of object the window holds.
fn objects(window: &Window) -> Group {
    let layout = window.layout().borrow();
    let paint = window.painter().cache();
    vec![
        ("dom_nodes", window.document().borrow().len() as u64),
        ("boxes", u64::from(layout.len())),
        ("box_slots", u64::from(layout.box_capacity())),
        ("fragments", u64::from(layout.fragment_count())),
        ("fragments_spilled", layout.spilled_fragment_count() as u64),
        ("box_layout_heap_bytes", layout.layout_heap_bytes() as u64),
        ("paragraphs_held", window.shaped_held().paragraphs as u64),
        ("paint_records", paint.len() as u64),
        ("paint_record_bytes", paint.bytes() as u64),
    ]
}

/// What the renderer reports, and the device's own allocator when the renderer has a device.
fn gpu(window: &Window, vram_mib: Option<u64>) -> Group {
    let renderer = window.renderer();
    let memory = renderer.memory();
    let mut group = vec![
        ("renderer_fixed_bytes", memory.fixed),
        ("renderer_targets_bytes", memory.targets),
        ("renderer_scratch_bytes", memory.scratch),
        ("renderer_atlases_bytes", memory.atlases),
        ("renderer_buffers_bytes", memory.buffers),
        ("renderer_total_bytes", memory.total()),
    ];
    let wgpu = renderer
        .as_any()
        .and_then(|any| any.downcast_ref::<zgui_render_wgpu::WgpuRenderer>());
    group.push(("renderer_is_wgpu", u64::from(wgpu.is_some())));
    if let Some(report) = wgpu.and_then(|wgpu| wgpu.gpu().device().generate_allocator_report()) {
        group.push(("allocator_allocated_bytes", report.total_allocated_bytes));
        group.push(("allocator_reserved_bytes", report.total_reserved_bytes));
        group.push(("allocator_blocks", report.blocks.len() as u64));
        group.push(("allocator_allocations", report.allocations.len() as u64));
    }
    if let Some(mib) = vram_mib {
        group.push(("nvidia_used_bytes", mib * 1024 * 1024));
    }
    group
}

/// The sizes of the records a row's memory is made of.
pub(crate) fn sizes() -> Group {
    use core::mem::size_of;
    vec![
        ("NodeInner", size_of::<zgui_dom::NodeInner>() as u64),
        ("BoxNode", size_of::<zgui_layout::BoxNode>() as u64),
        (
            "BoxLayout",
            zgui_layout::LayoutStore::BOX_LAYOUT_SIZE as u64,
        ),
        (
            "InlineResolution",
            size_of::<zgui_layout::inline::resolved::InlineResolution>() as u64,
        ),
        ("Fragment", size_of::<zgui_layout::Fragment>() as u64),
        ("HitEntry", size_of::<zgui_layout::HitEntry>() as u64),
        (
            "paint::Record",
            size_of::<zgui_paint::walk::replay::Record>() as u64,
        ),
        ("ChunkPrims", size_of::<zgui::scene::ChunkPrims>() as u64),
        (
            "ShapedParagraph<ShapedLayout>",
            size_of::<zgui::text::ShapedParagraph<zgui_text_parley::ShapedLayout>>() as u64,
        ),
        (
            "ShapedLayout",
            size_of::<zgui_text_parley::ShapedLayout>() as u64,
        ),
        (
            "parley::Layout",
            zgui_text_parley::shape::engine::LAYOUT_SIZE as u64,
        ),
        ("accesskit::Node", zgui_a11y::NODE_SIZE as u64),
    ]
}

/// A whole run, as the probe writes it.
pub(crate) struct Run<'a> {
    /// The process id.
    pub(crate) pid: u32,
    /// How many records the run loaded.
    pub(crate) rows: usize,
    /// The mode's name.
    pub(crate) mode: &'a str,
    /// Whether the run drew on the device.
    pub(crate) gpu: bool,
    /// The record sizes.
    pub(crate) sizes: &'a Group,
    /// The snapshots, in the order they were taken.
    pub(crate) snapshots: &'a [Snapshot],
    /// How many edits one round of edits made.
    pub(crate) edits: usize,
}

impl Run<'_> {
    /// The run as one JSON object.
    pub(crate) fn json(&self) -> String {
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"pid\":{},\"rows\":{},\"mode\":\"{}\",\"gpu\":{},\"sizes\":{},\"snapshots\":[",
            self.pid,
            self.rows,
            self.mode,
            self.gpu,
            object(self.sizes)
        );
        for (index, snapshot) in self.snapshots.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            let tags: Vec<String> = Tag::ALL
                .iter()
                .map(|tag| {
                    let figures = snapshot.tags[tag.index()];
                    format!(
                        "\"{}\":{{\"live\":{},\"count\":{},\"peak\":{},\"allocs\":{},\"total\":{}}}",
                        tag.label(),
                        figures.live,
                        figures.count,
                        figures.peak,
                        figures.allocs,
                        figures.total
                    )
                })
                .collect();
            let _ = write!(
                out,
                "{{\"name\":\"{}\",\"tags\":{{{}}},\"process\":{},\"mallinfo\":{},\"live\":{},\
                 \"objects\":{},\"gpu\":{}}}",
                snapshot.name,
                tags.join(","),
                object(&snapshot.process),
                object(&snapshot.mallinfo),
                object(&snapshot.live),
                object(&snapshot.objects),
                object(&snapshot.gpu)
            );
        }
        out.push_str("]}\n");
        out
    }

    /// The run as `snapshot`, `key` and `value` columns, one figure per line, for `--compare`.
    pub(crate) fn tsv(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "meta\trows\t{}", self.rows);
        let _ = writeln!(out, "meta\tpid\t{}", self.pid);
        for (key, value) in self.sizes {
            let _ = writeln!(out, "meta\tsize.{key}\t{value}");
        }
        for snapshot in self.snapshots {
            let name = snapshot.name;
            for tag in Tag::ALL {
                let figures = snapshot.tags[tag.index()];
                let label = tag.label();
                let _ = writeln!(out, "{name}\ttag.{label}.live\t{}", figures.live);
                let _ = writeln!(out, "{name}\ttag.{label}.count\t{}", figures.count);
                let _ = writeln!(out, "{name}\ttag.{label}.peak\t{}", figures.peak);
            }
            for (group, figures) in [
                ("process", &snapshot.process),
                ("mallinfo", &snapshot.mallinfo),
                ("live", &snapshot.live),
                ("objects", &snapshot.objects),
                ("gpu", &snapshot.gpu),
            ] {
                for (key, value) in figures {
                    let _ = writeln!(out, "{name}\t{group}.{key}\t{value}");
                }
            }
        }
        out
    }

    /// A table of live bytes per tag, one column per snapshot, in KiB.
    pub(crate) fn table(&self) -> String {
        let mut out = String::new();
        let _ = write!(out, "{:<16}", "KiB live");
        for snapshot in self.snapshots {
            let _ = write!(out, "{:>11}", snapshot.name);
        }
        out.push('\n');
        for tag in Tag::ALL {
            let _ = write!(out, "{:<16}", tag.label());
            for snapshot in self.snapshots {
                let _ = write!(out, "{:>11}", snapshot.live_bytes(tag) / 1024);
            }
            out.push('\n');
        }
        for key in [
            "tagged_live_bytes",
            "tag_header_bytes",
            "rss_bytes",
            "anon_bytes",
            "private_dirty_bytes",
        ] {
            let _ = write!(out, "{key:<16.16}");
            for snapshot in self.snapshots {
                let _ = write!(out, "{:>11}", snapshot.process(key) / 1024);
            }
            out.push('\n');
        }
        out
    }
}

impl Run<'_> {
    /// The snapshot named `name`, if the run took it.
    fn snapshot(&self, name: &str) -> Option<&Snapshot> {
        self.snapshots.iter().find(|snapshot| snapshot.name == name)
    }

    /// The last snapshot whose name is `stem` or `stem` with a round number, and the one before it.
    fn last_two(&self, stem: &str) -> Option<(&Snapshot, &Snapshot)> {
        let rounds: Vec<usize> = self
            .snapshots
            .iter()
            .enumerate()
            .filter(|(_, snapshot)| {
                snapshot
                    .name
                    .strip_prefix(stem)
                    .is_some_and(|rest| rest.chars().all(|c| c.is_ascii_digit()))
            })
            .map(|(index, _)| index)
            .collect();
        let &last = rounds.last()?;
        let before = last.checked_sub(1)?;
        Some((&self.snapshots[before], &self.snapshots[last]))
    }

    /// What each tag and each count kept: per edit over the last round of edits, at the unmount
    /// and the first remount against the level after the edits, and per cycle over the last
    /// cycle.
    ///
    /// The last round of edits is compared with the snapshot before it, which is the round before
    /// or the first frame. A run with one round therefore shows the warm-up too.
    pub(crate) fn growth(&self) -> String {
        let mut out = String::new();
        let Some((before_edits, edits)) = self.last_two("edits") else {
            return out;
        };
        let unmount = self.snapshot("unmount");
        let remount = self.snapshot("remount");
        let cycle = self
            .last_two("remount")
            .filter(|(before, _)| before.name.starts_with("remount"));
        let per_edit = self.edits.max(1) as f64;
        let _ = writeln!(
            out,
            "GROWTH {:<28}{:>12}{:>12}{:>12}{:>12}",
            "figure", "per edit", "unmount", "remount", "per cycle"
        );
        let mut line = |key: &str, figure: &dyn Fn(&Snapshot) -> u64| {
            let delta = |from: &Snapshot, to: &Snapshot| figure(to) as f64 - figure(from) as f64;
            let edit = delta(before_edits, edits) / per_edit;
            let unmount = unmount.map(|unmount| delta(edits, unmount));
            let remount = remount.map(|remount| delta(edits, remount));
            let cycle = cycle.map(|(before, after)| delta(before, after));
            if edit == 0.0
                && [unmount, remount, cycle]
                    .iter()
                    .all(|value| value.is_none_or(|value| value == 0.0))
            {
                return;
            }
            let shown = |value: Option<f64>| value.map_or("-".to_owned(), |v| format!("{v:.0}"));
            let _ = writeln!(
                out,
                "GROWTH {key:<28}{edit:>12.1}{:>12}{:>12}{:>12}",
                shown(unmount),
                shown(remount),
                shown(cycle)
            );
        };
        for tag in Tag::ALL {
            line(&format!("tag.{}", tag.label()), &|snapshot| {
                snapshot.live_bytes(tag)
            });
        }
        for key in ["tagged_live_bytes", "rss_bytes"] {
            line(key, &|snapshot| snapshot.process(key));
        }
        for (group, figures) in [("objects", &edits.objects), ("live", &edits.live)] {
            for &(key, _) in figures {
                line(&format!("{group}.{key}"), &|snapshot| {
                    let figures = if group == "objects" {
                        &snapshot.objects
                    } else {
                        &snapshot.live
                    };
                    figures
                        .iter()
                        .find(|(name, _)| *name == key)
                        .map_or(0, |&(_, value)| value)
                });
            }
        }
        out
    }
}

/// A group as a JSON object of numbers.
fn object(group: &Group) -> String {
    let fields: Vec<String> = group
        .iter()
        .map(|(key, value)| format!("\"{key}\":{value}"))
        .collect();
    format!("{{{}}}", fields.join(","))
}
