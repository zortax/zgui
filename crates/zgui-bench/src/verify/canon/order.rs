//! The draw order of a drawing, reduced to what the picture depends on.
//!
//! The compositor batches instances that do not overlap, and the batches it forms depend on which
//! atlas page each glyph landed on, so two windows can draw the same picture in different
//! sequences. What a picture depends on is the order of instances that overlap, and the nesting of
//! groups. A scope is the drawing at the top level or inside one group.

/// One instance of a scope, or a group nested in it.
pub(super) enum Item {
    /// An instance or an object drawn one at a time, as its canonical line.
    Line {
        /// The line without its atlas tile.
        text: String,
        /// The address of the atlas tile it is drawn from, if any.
        tile: Option<String>,
    },
    /// A group, with the line that opens it and the scope inside it.
    Group {
        /// The line that opens the group.
        text: String,
        /// What the group draws.
        inner: Scope,
    },
}

/// What one scope draws, in draw order.
#[derive(Default)]
pub(super) struct Scope {
    pub(super) items: Vec<Item>,
}

impl Item {
    /// The line that names the item.
    fn text(&self) -> &str {
        match self {
            Self::Line { text, .. } | Self::Group { text, .. } => text,
        }
    }

    /// Where the item draws on the output, when its line says.
    fn reach(&self) -> Option<[f32; 4]> {
        let text = self.text();
        let rect = rect(text, "bounds=rect(").or_else(|| rect(text, "ink=rect("))?;
        let [dx, dy] = translation(text);
        Some([rect[0] + dx, rect[1] + dy, rect[2], rect[3]])
    }
}

impl Scope {
    /// Writes the scope: its items sorted, each group followed by its own scope, and then the
    /// order of every pair of items that overlap.
    ///
    /// Tiles are numbered in the order they are written, so a window that draws two glyphs from
    /// one tile still differs from one that draws them from two.
    pub(super) fn write(&self, out: &mut String, tiles: &mut Vec<String>) {
        let mut sorted: Vec<usize> = (0..self.items.len()).collect();
        sorted.sort_by(|one, two| self.items[*one].text().cmp(self.items[*two].text()));
        let mut rank = vec![0; self.items.len()];
        for (at, index) in sorted.iter().enumerate() {
            rank[*index] = at;
        }
        for index in &sorted {
            match &self.items[*index] {
                Item::Line { text, tile } => {
                    out.push_str(text);
                    if let Some(tile) = tile {
                        out.push_str(&format!(" tile=#{}", number(tiles, tile)));
                    }
                    out.push('\n');
                }
                Item::Group { text, inner } => {
                    out.push_str(text);
                    out.push('\n');
                    inner.write(out, tiles);
                    out.push_str("  group_end\n");
                }
            }
        }
        let reaches: Vec<Option<[f32; 4]>> = self.items.iter().map(Item::reach).collect();
        let mut pairs = Vec::new();
        for first in 0..self.items.len() {
            for second in first + 1..self.items.len() {
                if let (Some(one), Some(two)) = (reaches[first], reaches[second])
                    && overlap(one, two)
                {
                    pairs.push((rank[first], rank[second]));
                }
            }
        }
        pairs.sort_unstable();
        for (first, second) in pairs {
            out.push_str(&format!("  order {first} before {second}\n"));
        }
    }
}

/// Returns the number of `tile` among the distinct tiles in `tiles`, numbering it if it is new.
fn number(tiles: &mut Vec<String>, tile: &str) -> usize {
    tiles
        .iter()
        .position(|seen| seen == tile)
        .unwrap_or_else(|| {
            tiles.push(tile.to_owned());
            tiles.len() - 1
        })
}

/// Reports whether two rectangles share some area.
fn overlap(one: [f32; 4], two: [f32; 4]) -> bool {
    one[0] < two[0] + two[2]
        && two[0] < one[0] + one[2]
        && one[1] < two[1] + two[3]
        && two[1] < one[1] + one[3]
}

/// Reads the rectangle after `field` in `line`.
fn rect(line: &str, field: &str) -> Option<[f32; 4]> {
    let rest = &line[line.find(field)? + field.len()..];
    let numbers: Vec<f32> = rest[..rest.find(')')?]
        .split(',')
        .filter_map(|number| number.trim().parse().ok())
        .collect();
    numbers.try_into().ok()
}

/// Reads the translation of the instance's `space=translate[x, y]`, or none.
fn translation(line: &str) -> [f32; 2] {
    let Some(at) = line.find("space=translate[") else {
        return [0.0, 0.0];
    };
    let rest = &line[at + "space=translate[".len()..];
    let numbers: Vec<f32> = rest[..rest.find(']').unwrap_or(rest.len())]
        .split(',')
        .filter_map(|number| number.trim().parse().ok())
        .collect();
    match numbers[..] {
        [x, y] => [x, y],
        _ => [0.0, 0.0],
    }
}
