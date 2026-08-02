//! What a renderer or a rasteriser is holding.

/// Video memory currently held, broken down far enough to budget against.
///
/// Every field is bytes, and every field is something that grows with content rather than being
/// fixed at startup — except [`MemoryReport::fixed`], which is exactly the part that does not, and
/// is separated for that reason: a rasteriser whose fixed cost is large and whose per-frame cost is
/// small has a completely different budget from one where it is the other way round.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryReport {
    /// Held from construction and independent of what is drawn.
    pub fixed: u64,
    /// The persistent target a frame is composed into.
    pub targets: u64,
    /// Scratch textures, which scale with surface size and with how much is drawn at once.
    pub scratch: u64,
    /// Textures holding cached raster content.
    pub atlases: u64,
    /// Vertex, instance and uniform buffers.
    pub buffers: u64,
}

impl MemoryReport {
    /// A report of nothing held.
    pub const ZERO: Self = Self {
        fixed: 0,
        targets: 0,
        scratch: 0,
        atlases: 0,
        buffers: 0,
    };

    /// Everything, added up.
    pub const fn total(&self) -> u64 {
        self.fixed + self.targets + self.scratch + self.atlases + self.buffers
    }

    /// The two reports added field by field.
    ///
    /// A renderer aggregates its own report with its rasteriser's, so a budget is written against
    /// one number rather than against a list of components a caller has to remember to sum.
    pub const fn plus(self, other: Self) -> Self {
        Self {
            fixed: self.fixed + other.fixed,
            targets: self.targets + other.targets,
            scratch: self.scratch + other.scratch,
            atlases: self.atlases + other.atlases,
            buffers: self.buffers + other.buffers,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MemoryReport;

    #[test]
    fn aggregating_two_reports_adds_every_component() {
        let renderer = MemoryReport {
            targets: 8,
            atlases: 4,
            ..MemoryReport::ZERO
        };
        let raster = MemoryReport {
            fixed: 16,
            scratch: 2,
            ..MemoryReport::ZERO
        };
        let whole = renderer.plus(raster);

        assert_eq!(whole.fixed, 16);
        assert_eq!(whole.scratch, 2);
        assert_eq!(whole.total(), 30);
        assert_eq!(whole.total(), renderer.total() + raster.total());
    }
}
