//! Pixel formats and layout modifiers.
//!
//! These are the one part of the interface bindgen cannot produce: `drm_fourcc.h` states them as
//! function-like macros, which bindgen skips. They are written out here and checked against the
//! values the header's own macros expand to.
//!
//! Which pairs of the two a plane can actually scan out is the plane's own answer, published as a
//! blob. [`FormatModifiers`] is that blob read back.

use std::mem::offset_of;

use crate::sys;

/// A pixel format, as the four-character code the kernel names it by.
///
/// The code holds the four characters in order, least significant byte first.
///
/// ```
/// use zgui_drm::format::Format;
///
/// assert_eq!(Format::XRGB8888.0.to_le_bytes(), *b"XR24");
/// assert_eq!(Format::ARGB8888.0.to_le_bytes(), *b"AR24");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Format(pub u32);

/// Returns the code these four characters spell.
///
/// This is `fourcc_code` from `drm_fourcc.h`.
const fn fourcc(a: u8, b: u8, c: u8, d: u8) -> Format {
    Format((a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24))
}

impl Format {
    /// 32-bit ARGB, eight bits each.
    pub const ARGB8888: Self = fourcc(b'A', b'R', b'2', b'4');
    /// 32-bit RGB with the top eight bits unused.
    pub const XRGB8888: Self = fourcc(b'X', b'R', b'2', b'4');
    /// 32-bit ABGR, eight bits each.
    pub const ABGR8888: Self = fourcc(b'A', b'B', b'2', b'4');
    /// 32-bit BGR with the top eight bits unused.
    pub const XBGR8888: Self = fourcc(b'X', b'B', b'2', b'4');

    /// Returns how many bytes one pixel of this format takes.
    ///
    /// Only the formats above are answered, because only they are ones this crate hands to a
    /// scanout. Anything else answers `None`, because a guess here would size a buffer wrong.
    pub fn bytes_per_pixel(self) -> Option<u32> {
        match self {
            Self::ARGB8888 | Self::XRGB8888 | Self::ABGR8888 | Self::XBGR8888 => Some(4),
            _ => None,
        }
    }
}

/// A layout modifier: how the pixels of a format are actually arranged in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Modifier(pub u64);

impl Modifier {
    /// Row-major, no tiling, no compression.
    pub const LINEAR: Self = Self(0);
    /// The layout is unknown, and the driver should pick.
    ///
    /// This is `DRM_FORMAT_MOD_INVALID`, and a framebuffer created with it is created without
    /// naming a modifier at all rather than naming this one.
    pub const INVALID: Self = Self(0x00ff_ffff_ffff_ffff);
}

/// The version of the `IN_FORMATS` blob this reads.
///
/// `FORMAT_BLOB_CURRENT` in `drm_mode.h`. The macro sits inside the struct body, so it falls
/// outside the names bindgen was asked for and is written out here.
const CURRENT_VERSION: u32 = 1;

/// Which layouts a plane can scan each format out in.
///
/// This is a plane's `IN_FORMATS` property read back. The plane's own format list says which
/// formats the hardware takes and says nothing about how their pixels are arranged, so a caller
/// that wants a buffer another driver wrote — a Vulkan image handed straight to a scanout — asks
/// here: these are the pairs the display hardware can read, and the graphics driver has to produce
/// one of them.
///
/// # What the blob says
///
/// A list of formats, and a list of modifiers each carrying a bitmask over that format list. The
/// mask is 64 bits wide and the list can be longer, so every modifier also carries the place its
/// mask starts at. Bit 0 names the format at that place. A driver with 130 formats states a
/// modifier for formats 98 to 102 as a start of 64 with bits 34 to 38 set, which is the example
/// `drm_mode.h` gives.
#[derive(Debug, Clone)]
pub struct FormatModifiers {
    /// The formats, in the order the blob lists them, which the bitmasks index.
    formats: Vec<Format>,
    /// The layouts for each format, at the same place as the format they belong to.
    modifiers: Vec<Vec<Modifier>>,
}

impl FormatModifiers {
    /// Parses an `IN_FORMATS` blob.
    ///
    /// Answers `None` for a blob this cannot read: one of a version it does not know, or one whose
    /// header describes more than the bytes hold.
    ///
    /// # Untrusted bytes
    ///
    /// The bytes arrive from a driver, and every field below is read out of a slice instead of
    /// through a struct pointer. A `Vec<u8>` is aligned for one byte, `struct drm_format_modifier`
    /// wants eight, and casting the one to the other is undefined on every target — including the
    /// ones where it happens to work. So each field is a `from_ne_bytes` over four or eight bytes,
    /// native-endian because this is the kernel's own memory.
    ///
    /// Every offset, count and index is checked. Nothing here can panic, index past the end or
    /// overflow in release.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        // The offsets are taken from the generated struct, so a header that changed shape moves
        // them here as well.
        type Header = sys::drm_format_modifier_blob;
        type Entry = sys::drm_format_modifier;

        if read_u32(bytes, offset_of!(Header, version))? != CURRENT_VERSION {
            return None;
        }
        // `flags` has no flag defined, so nothing reads it.
        let count_formats = read_u32(bytes, offset_of!(Header, count_formats))?;
        let formats_offset = read_u32(bytes, offset_of!(Header, formats_offset))?;
        let count_modifiers = read_u32(bytes, offset_of!(Header, count_modifiers))?;
        let modifiers_offset = read_u32(bytes, offset_of!(Header, modifiers_offset))?;

        // Both tables are bounded against the blob before either is read, so a count the header
        // invented allocates nothing.
        let listed = table(bytes, formats_offset, count_formats, size_of::<u32>())?;
        let entries = table(bytes, modifiers_offset, count_modifiers, size_of::<Entry>())?;

        let formats: Vec<Format> = listed
            .chunks_exact(size_of::<u32>())
            .map(|field| read_u32(field, 0).map(Format))
            .collect::<Option<_>>()?;
        let mut modifiers = vec![Vec::new(); formats.len()];

        for entry in entries.chunks_exact(size_of::<Entry>()) {
            let covered = read_u64(entry, offset_of!(Entry, formats))?;
            let start = read_u32(entry, offset_of!(Entry, offset))?;
            let modifier = Modifier(read_u64(entry, offset_of!(Entry, modifier))?);

            for bit in 0..u64::BITS {
                if covered & (1_u64 << bit) == 0 {
                    continue;
                }
                // A mask may name places the list does not have: a list whose length is not a
                // multiple of 64 leaves bits over at the end of its last window. Those bits
                // describe no format.
                let Some(layouts) = start
                    .checked_add(bit)
                    .and_then(|index| usize::try_from(index).ok())
                    .and_then(|index| modifiers.get_mut(index))
                else {
                    continue;
                };
                // A driver may name one layout for one format twice, over two windows that
                // overlap. The answer is a set either way.
                if !layouts.contains(&modifier) {
                    layouts.push(modifier);
                }
            }
        }

        Some(Self { formats, modifiers })
    }

    /// Returns every format this plane can scan out, in the order the blob lists them.
    ///
    /// The same formats the plane's own list holds. A caller picking a format walks these and asks
    /// [`FormatModifiers::modifiers`] about each.
    pub fn formats(&self) -> &[Format] {
        &self.formats
    }

    /// Returns the layouts this plane accepts for `format`.
    ///
    /// Empty where the plane does not take the format at all, and where it takes the format and
    /// the driver named no layout for it. A driver that publishes no layout for a format it lists
    /// is stating nothing about that format, so a caller with no other source picks the format's
    /// layout itself.
    ///
    /// A blob that listed one format twice is answered from the first of the two places.
    pub fn modifiers(&self, format: Format) -> &[Modifier] {
        self.formats
            .iter()
            .position(|listed| *listed == format)
            .and_then(|index| self.modifiers.get(index))
            .map_or(&[], Vec::as_slice)
    }

    /// Returns `true` if this plane can scan `format` out in `modifier`.
    pub fn supports(&self, format: Format, modifier: Modifier) -> bool {
        self.modifiers(format).contains(&modifier)
    }
}

/// Returns the `count` entries of `stride` bytes that start at `offset`, where `bytes` holds them
/// all.
///
/// Answers `None` for a table that runs past the blob, and for a count that overflows the
/// multiply. A 32-bit target overflows on a count the header invented.
fn table(bytes: &[u8], offset: u32, count: u32, stride: usize) -> Option<&[u8]> {
    let start = usize::try_from(offset).ok()?;
    let length = usize::try_from(count).ok()?.checked_mul(stride)?;
    bytes.get(start..start.checked_add(length)?)
}

/// Returns the `u32` that `bytes` holds at `at`, where it holds four bytes there.
fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let field = bytes.get(at..at.checked_add(size_of::<u32>())?)?;
    Some(u32::from_ne_bytes(field.try_into().ok()?))
}

/// Returns the `u64` that `bytes` holds at `at`, where it holds eight bytes there.
fn read_u64(bytes: &[u8], at: usize) -> Option<u64> {
    let field = bytes.get(at..at.checked_add(size_of::<u64>())?)?;
    Some(u64::from_ne_bytes(field.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fourcc_is_its_four_characters_little_endian() {
        // `XR24` is the format almost every scanout in this crate uses, and 0x34325258 is what
        // `fourcc_code('X', 'R', '2', '4')` expands to in the kernel's own header.
        assert_eq!(Format::XRGB8888.0, 0x3432_5258);
        assert_eq!(Format::ARGB8888.0, 0x3432_5241);
        assert_eq!(Format::ABGR8888.0, 0x3432_4241);
        assert_eq!(Format::XBGR8888.0, 0x3432_4258);
    }

    #[test]
    fn the_two_modifiers_that_are_not_vendor_specific_have_their_stated_values() {
        assert_eq!(Modifier::LINEAR.0, 0);
        assert_eq!(Modifier::INVALID.0, (1 << 56) - 1);
    }

    #[test]
    fn only_the_formats_a_scanout_takes_report_a_pixel_size() {
        assert_eq!(Format::XRGB8888.bytes_per_pixel(), Some(4));
        assert_eq!(Format(0).bytes_per_pixel(), None);
    }

    // What follows is the `IN_FORMATS` parser, over blobs built here byte by byte. A driver is not
    // needed to state one, and the bitmask window is the part that decides which layout is asked
    // of a graphics API for which format — read wrong, it names layouts the hardware cannot scan
    // and the screen goes black with nothing reported.

    /// The layout NVIDIA scans a tiled buffer out in, used as a value that is not `LINEAR`.
    const TILED: Modifier = Modifier(0x0300_0000_0060_6014);

    /// One `struct drm_format_modifier`, as the tests state one.
    struct Entry {
        /// The bitmask over the format list, with bit 0 naming the format at `offset`.
        formats: u64,
        /// Where the mask's window starts in the format list.
        offset: u32,
        /// The layout the mask applies to.
        modifier: u64,
    }

    /// Returns a blob of `formats` and `entries`, of the version the parser reads.
    ///
    /// The format table follows the header and the modifier table follows that. The kernel pads
    /// the second to eight bytes and this does not, so every blob built here holds `u64` fields at
    /// an odd place. The parser reads the whole blob as bytes either way.
    fn blob(formats: &[Format], entries: &[Entry]) -> Vec<u8> {
        let formats_offset = u32::try_from(size_of::<sys::drm_format_modifier_blob>())
            .expect("a header of six words fits in a word");
        let listed = u32::try_from(formats.len()).expect("the tests state few formats");
        let modifiers_offset = formats_offset + listed * 4;

        let mut bytes = header(
            CURRENT_VERSION,
            listed,
            formats_offset,
            u32::try_from(entries.len()).expect("the tests state few modifiers"),
            modifiers_offset,
        );
        for format in formats {
            bytes.extend(format.0.to_ne_bytes());
        }
        for entry in entries {
            bytes.extend(entry.formats.to_ne_bytes());
            bytes.extend(entry.offset.to_ne_bytes());
            bytes.extend(0_u32.to_ne_bytes());
            bytes.extend(entry.modifier.to_ne_bytes());
        }
        bytes
    }

    /// Returns a blob header, and nothing after it.
    fn header(
        version: u32,
        count_formats: u32,
        formats_offset: u32,
        count_modifiers: u32,
        modifiers_offset: u32,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(version.to_ne_bytes());
        // `flags`, of which the header defines none.
        bytes.extend(0_u32.to_ne_bytes());
        bytes.extend(count_formats.to_ne_bytes());
        bytes.extend(formats_offset.to_ne_bytes());
        bytes.extend(count_modifiers.to_ne_bytes());
        bytes.extend(modifiers_offset.to_ne_bytes());
        bytes
    }

    /// Returns `count` formats, all different, none of which is a real fourcc.
    fn formats(count: u32) -> Vec<Format> {
        (0..count).map(Format).collect()
    }

    #[test]
    fn a_modifier_covers_the_formats_its_window_names_and_no_others() {
        // The example `drm_mode.h` gives: a list longer than one mask, and a modifier for formats
        // 98 to 102 stated as a window starting at 64 with bits 34 to 38 set. The window is what
        // the format exists for — a parser that read the mask against the start of the list would
        // answer formats 34 to 38 here, with nothing to say it had.
        let formats = formats(130);
        let parsed = FormatModifiers::parse(&blob(
            &formats,
            &[Entry {
                formats: 0b1_1111 << 34,
                offset: 64,
                modifier: TILED.0,
            }],
        ))
        .expect("a blob the kernel's own example describes parses");

        assert_eq!(
            parsed.modifiers(formats[97]),
            &[],
            "the format below the window is outside it"
        );
        assert_eq!(
            parsed.modifiers(formats[103]),
            &[],
            "the format above the window is outside it"
        );
        // And the whole list, which is where formats 34 to 38 would show up.
        for (index, format) in formats.iter().enumerate() {
            let covered: &[Modifier] = if (98..=102).contains(&index) {
                &[TILED]
            } else {
                &[]
            };
            assert_eq!(
                parsed.modifiers(*format),
                covered,
                "format {index}, against a window covering 98 to 102"
            );
        }
    }

    #[test]
    fn a_window_that_starts_at_the_head_of_the_list_reads_the_low_bits() {
        // The ordinary case, and the header's other example: under 65 formats, formats 0 and 2.
        let formats = formats(8);
        let parsed = FormatModifiers::parse(&blob(
            &formats,
            &[Entry {
                formats: 0x0000_0000_0000_0005,
                offset: 0,
                modifier: Modifier::LINEAR.0,
            }],
        ))
        .expect("a blob of one window parses");

        assert_eq!(parsed.modifiers(formats[0]), &[Modifier::LINEAR]);
        assert_eq!(parsed.modifiers(formats[1]), &[]);
        assert_eq!(parsed.modifiers(formats[2]), &[Modifier::LINEAR]);
        assert_eq!(parsed.modifiers(formats[3]), &[]);
    }

    #[test]
    fn one_format_carries_every_layout_named_for_it() {
        // What a plane that can scan tiled and linear buffers of one format publishes, which is
        // what the caller of this intersects with what a graphics API can render into.
        let formats = [Format::XRGB8888, Format::ARGB8888];
        let parsed = FormatModifiers::parse(&blob(
            &formats,
            &[
                Entry {
                    formats: 0b01,
                    offset: 0,
                    modifier: TILED.0,
                },
                Entry {
                    formats: 0b01,
                    offset: 0,
                    modifier: Modifier::LINEAR.0,
                },
            ],
        ))
        .expect("a blob of two modifiers parses");

        assert_eq!(
            parsed.modifiers(Format::XRGB8888),
            &[TILED, Modifier::LINEAR],
            "both layouts are answered, in the order the blob states them"
        );
        assert!(parsed.supports(Format::XRGB8888, TILED));
        assert!(parsed.supports(Format::XRGB8888, Modifier::LINEAR));
        assert!(!parsed.supports(Format::ARGB8888, TILED));
    }

    #[test]
    fn one_layout_covers_every_format_its_mask_names() {
        let formats = [Format::XRGB8888, Format::XBGR8888, Format::ARGB8888];
        let parsed = FormatModifiers::parse(&blob(
            &formats,
            &[Entry {
                formats: 0b101,
                offset: 0,
                modifier: TILED.0,
            }],
        ))
        .expect("a blob of one modifier over two formats parses");

        assert_eq!(parsed.modifiers(Format::XRGB8888), &[TILED]);
        assert_eq!(parsed.modifiers(Format::XBGR8888), &[]);
        assert_eq!(parsed.modifiers(Format::ARGB8888), &[TILED]);
    }

    #[test]
    fn a_format_no_modifier_names_is_listed_and_carries_none() {
        let formats = [Format::XRGB8888, Format::ARGB8888];
        let parsed = FormatModifiers::parse(&blob(
            &formats,
            &[Entry {
                formats: 0b01,
                offset: 0,
                modifier: Modifier::LINEAR.0,
            }],
        ))
        .expect("a blob whose modifiers cover part of the list parses");

        assert_eq!(
            parsed.formats(),
            &formats,
            "a format with no layout is still a format the plane takes"
        );
        assert_eq!(
            parsed.modifiers(Format::ARGB8888),
            &[],
            "and it carries no layout"
        );
        assert!(!parsed.supports(Format::ARGB8888, Modifier::LINEAR));
    }

    #[test]
    fn a_format_the_plane_does_not_list_carries_no_layout() {
        let parsed = FormatModifiers::parse(&blob(
            &[Format::ARGB8888],
            &[Entry {
                formats: 0b1,
                offset: 0,
                modifier: Modifier::LINEAR.0,
            }],
        ))
        .expect("a blob of one format parses");

        assert_eq!(parsed.modifiers(Format::XRGB8888), &[]);
        assert!(!parsed.supports(Format::XRGB8888, Modifier::LINEAR));
    }

    #[test]
    fn a_bit_naming_a_place_the_format_list_does_not_have_describes_nothing() {
        // Every list that is not a multiple of 64 long has a last window with bits over the end,
        // and a driver may set them. They name no format, and reading one must not index past the
        // list.
        let formats = formats(3);
        let parsed = FormatModifiers::parse(&blob(
            &formats,
            &[
                Entry {
                    formats: u64::MAX,
                    offset: 0,
                    modifier: Modifier::LINEAR.0,
                },
                Entry {
                    formats: u64::MAX,
                    offset: u32::MAX,
                    modifier: TILED.0,
                },
            ],
        ))
        .expect("a blob whose masks run past the list parses");

        assert_eq!(
            parsed.formats().len(),
            3,
            "the list is the length it states"
        );
        for format in parsed.formats() {
            assert_eq!(
                parsed.modifiers(*format),
                &[Modifier::LINEAR],
                "the window that starts past every format adds nothing"
            );
        }
    }

    #[test]
    fn a_layout_named_twice_for_one_format_is_answered_once() {
        // Two windows can overlap, and a driver may state the same pair in both.
        let parsed = FormatModifiers::parse(&blob(
            &[Format::XRGB8888],
            &[
                Entry {
                    formats: 0b1,
                    offset: 0,
                    modifier: Modifier::LINEAR.0,
                },
                Entry {
                    formats: 0b1,
                    offset: 0,
                    modifier: Modifier::LINEAR.0,
                },
            ],
        ))
        .expect("a blob stating one pair twice parses");

        assert_eq!(parsed.modifiers(Format::XRGB8888), &[Modifier::LINEAR]);
    }

    #[test]
    fn a_modifier_table_padded_the_way_the_kernel_pads_it_reads_the_same() {
        // The kernel rounds the modifier table's offset up to eight bytes. The parser follows the
        // offset the header states, so both layouts describe the same thing.
        let formats = [Format::XRGB8888, Format::ARGB8888, Format::XBGR8888];
        let header_bytes = u32::try_from(size_of::<sys::drm_format_modifier_blob>())
            .expect("a header of six words fits in a word");
        // Three formats end the table at 36, and the kernel would start the modifiers at 40.
        let mut bytes = header(CURRENT_VERSION, 3, header_bytes, 1, 40);
        for format in formats {
            bytes.extend(format.0.to_ne_bytes());
        }
        bytes.extend(0_u32.to_ne_bytes());
        bytes.extend(0b101_u64.to_ne_bytes());
        bytes.extend(0_u32.to_ne_bytes());
        bytes.extend(0_u32.to_ne_bytes());
        bytes.extend(TILED.0.to_ne_bytes());

        let parsed = FormatModifiers::parse(&bytes).expect("a padded blob parses");

        assert_eq!(parsed.modifiers(Format::XRGB8888), &[TILED]);
        assert_eq!(parsed.modifiers(Format::ARGB8888), &[]);
        assert_eq!(parsed.modifiers(Format::XBGR8888), &[TILED]);
    }

    // What follows is malformed input. The bytes come from a driver, so every one of these has to
    // be answered rather than trusted: none may panic, index past the end or overflow.

    #[test]
    fn a_blob_of_no_bytes_is_refused() {
        assert!(FormatModifiers::parse(&[]).is_none());
    }

    #[test]
    fn a_blob_that_stops_inside_its_header_is_refused() {
        let whole = header(CURRENT_VERSION, 0, 24, 0, 24);
        for length in 0..whole.len() {
            assert!(
                FormatModifiers::parse(&whole[..length]).is_none(),
                "a header cut to {length} bytes describes nothing"
            );
        }
        assert!(
            FormatModifiers::parse(&whole).is_some(),
            "and the whole header, of a blob that lists nothing, is read"
        );
    }

    #[test]
    fn a_version_this_does_not_know_is_refused() {
        // The one version is 1. A later one may lay the tables out differently, and reading it as
        // if it were this one would report layouts that were never stated.
        for version in [0, 2, u32::MAX] {
            assert!(
                FormatModifiers::parse(&header(version, 0, 24, 0, 24)).is_none(),
                "version {version} is not the version this reads"
            );
        }
    }

    #[test]
    fn a_count_of_formats_the_blob_does_not_hold_is_refused() {
        // Two formats stated, one written.
        let mut bytes = header(CURRENT_VERSION, 2, 24, 0, 32);
        bytes.extend(Format::XRGB8888.0.to_ne_bytes());

        assert!(FormatModifiers::parse(&bytes).is_none());
    }

    #[test]
    fn a_count_of_modifiers_the_blob_does_not_hold_is_refused() {
        // One modifier stated, half of one written.
        let mut bytes = header(CURRENT_VERSION, 0, 24, 1, 24);
        bytes.extend([0_u8; 12]);

        assert!(FormatModifiers::parse(&bytes).is_none());
    }

    #[test]
    fn a_format_table_that_starts_past_the_end_is_refused() {
        assert!(FormatModifiers::parse(&header(CURRENT_VERSION, 1, 4096, 0, 24)).is_none());
    }

    #[test]
    fn a_modifier_table_that_starts_past_the_end_is_refused() {
        assert!(FormatModifiers::parse(&header(CURRENT_VERSION, 0, 24, 1, 4096)).is_none());
    }

    #[test]
    fn a_count_that_would_overflow_the_size_of_its_table_is_refused() {
        // On a 64-bit target the multiply is merely enormous and the table runs past the blob. On
        // a 32-bit one it wraps, and a count of 0xaaaaaaab modifiers of 24 bytes comes to 8 — a
        // table that fits, holding entries that were never written.
        assert!(FormatModifiers::parse(&header(CURRENT_VERSION, 0, 24, 0xaaaa_aaab, 24)).is_none());
        assert!(
            FormatModifiers::parse(&header(CURRENT_VERSION, u32::MAX, 24, 0, 24)).is_none(),
            "and the same count of formats"
        );
    }

    #[test]
    fn an_offset_that_would_overflow_when_the_table_is_added_is_refused() {
        assert!(FormatModifiers::parse(&header(CURRENT_VERSION, 1, u32::MAX, 0, 24)).is_none());
        assert!(FormatModifiers::parse(&header(CURRENT_VERSION, 0, 24, 1, u32::MAX)).is_none());
    }

    #[test]
    fn a_blob_that_lists_nothing_answers_nothing() {
        // A driver may publish the property and state no format at all. That is an answer, and it
        // says the plane takes nothing this way.
        let parsed = FormatModifiers::parse(&blob(&[], &[])).expect("an empty blob parses");

        assert_eq!(parsed.formats(), &[]);
        assert_eq!(parsed.modifiers(Format::XRGB8888), &[]);
        assert!(!parsed.supports(Format::XRGB8888, Modifier::LINEAR));
    }
}
