// Copyright 2026 the zgui Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// ZGUI-PATCH: this whole file is local to zgui.

//! An on-disk copy of what the fontconfig backend finds.
//!
//! A cold start spends most of its font time in fontconfig: it loads the configuration, sorts every
//! installed face once for each generic family, and sorts them again for each family a document
//! names. The file keeps the results of that work: the family names, the generic family lists, and
//! each family and fallback that a process resolved. A later process that finds the file valid
//! reads the results and loads fontconfig only for a question the file cannot answer.
//!
//! The file is valid while each font directory and configuration file that fontconfig reported
//! has the same identity, size and modification time, and while the environment that fontconfig
//! reads is the same. Installing or removing a font changes the directory that holds it, so the
//! next process builds the file again.
//!
//! `ZGUI_FONT_CACHE=off` disables the file. `ZGUI_FONT_CACHE=<absolute dir>` puts it in that
//! directory. Otherwise it is in `$XDG_CACHE_HOME/zgui`, or `$HOME/.cache/zgui`.

use std::{
    borrow::ToOwned,
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    format, fs,
    io::Write as _,
    os::unix::ffi::{OsStrExt, OsStringExt},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    string::String,
    time::{Duration, SystemTime},
    vec::Vec,
};

use hashbrown::HashMap;

use crate::system_cache::{HITS, MISSES, STORES};

/// Raised when the layout of the file changes.
const FORMAT: u32 = 1;

/// The first bytes of the file.
const MAGIC: &[u8; 4] = b"ZGFC";

/// The clock tick margin of a stamp. A stamp that changed less than this before the scan started
/// can change again in the same tick, so the file is not written.
const SETTLE: Duration = Duration::from_secs(2);

/// Record tags.
const FAMILY: u8 = 1;
const FALLBACK: u8 = 2;

/// The environment variables that change what fontconfig loads or how it matches.
const ENV: &[&str] = &[
    "FONTCONFIG_FILE",
    "FONTCONFIG_PATH",
    "FONTCONFIG_SYSROOT",
    "FC_LANG",
    "LC_ALL",
    "LC_CTYPE",
    "LANG",
    "LANGUAGE",
    "HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_DATA_DIRS",
];

/// One face of a family, as fontconfig reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RawFace {
    pub(super) path: OsString,
    pub(super) index: u32,
    pub(super) weight: Option<i32>,
    pub(super) width: Option<i32>,
    pub(super) slant: Option<i32>,
}

/// What fontconfig reported when the file was built.
pub(super) struct Base {
    /// The family names of each face, without repeats, in the order fontconfig listed them.
    pub(super) names: Vec<Vec<String>>,
    /// For each generic family, the names that resolved to a family, in order.
    pub(super) generics: Vec<Vec<String>>,
}

/// The paths fontconfig reported, for the stamps.
pub(super) struct Reported {
    pub(super) font_dirs: Vec<Vec<u8>>,
    pub(super) config_files: Vec<Vec<u8>>,
}

/// A fallback question: a script and an optional locale.
type FallbackSlot = ([u8; 4], Option<String>);

/// Where the file is, and the environment it is valid for.
pub(super) struct Place {
    file: PathBuf,
    key: Vec<u8>,
}

/// The file of this process, after a load or a store.
pub(super) struct Live {
    file: PathBuf,
    identity: u64,
    families: HashMap<String, Vec<RawFace>>,
    fallbacks: HashMap<FallbackSlot, Option<String>>,
}

impl Place {
    /// The file this process uses, or `None` when the cache is off or has no directory.
    pub(super) fn from_env(version: i32) -> Option<Self> {
        let dir = match std::env::var_os("ZGUI_FONT_CACHE") {
            Some(value) if value == "off" || value == "0" => return None,
            Some(value) if Path::new(&value).is_absolute() => PathBuf::from(value),
            _ => default_dir()?,
        };
        let mut key = Writer::default();
        key.u32(FORMAT);
        key.u32(version as u32);
        for name in ENV {
            key.bytes(name.as_bytes());
            match std::env::var_os(name) {
                Some(value) => {
                    key.u8(1);
                    key.bytes(value.as_bytes());
                }
                None => key.u8(0),
            }
        }
        let key = key.buf;
        let file = dir.join(format!("fonts-{:016x}.bin", fnv(&key)));
        Some(Self { file, key })
    }

    /// The file's contents, when the file exists and is valid.
    pub(super) fn load(&self) -> Option<(Base, Live)> {
        let loaded = fs::read(&self.file).ok().and_then(|data| self.parse(&data));
        if loaded.is_some() {
            HITS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        } else {
            MISSES.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        loaded
    }

    fn parse(&self, data: &[u8]) -> Option<(Base, Live)> {
        let mut r = Reader { data, at: 0 };
        if r.take(4)? != MAGIC || r.u32()? != FORMAT || r.bytes()? != self.key.as_slice() {
            return None;
        }
        if r.u8()? == 1 && r.bytes()? != prgname().as_bytes() {
            return None;
        }
        for _ in 0..r.u32()? {
            let path = Path::new(OsStr::from_bytes(r.bytes()?));
            let present = r.u8()? == 1;
            let now = fs::metadata(path).ok();
            match (present, now) {
                (false, None) => {}
                (true, Some(now)) => {
                    let then = [r.u64()?, r.u64()?, r.u64()?, r.u64()?, r.u64()?, r.u64()?];
                    if then != stamp_of(&now) {
                        return None;
                    }
                }
                _ => return None,
            }
        }
        let mut names = Vec::new();
        for _ in 0..r.u32()? {
            names.push(r.strings()?);
        }
        let mut generics = Vec::new();
        for _ in 0..r.u32()? {
            generics.push(r.strings()?);
        }
        let identity = fnv(&data[..r.at]);
        if r.u64()? != identity {
            return None;
        }
        let mut live = Live {
            file: self.file.clone(),
            identity,
            families: HashMap::default(),
            fallbacks: HashMap::default(),
        };
        // A record cut short or damaged ends the read, and the records before it stay valid. A
        // record that a process wrote for an older base is skipped.
        while r.at < data.len() {
            let Some((owner, tag, payload)) = r.record() else {
                break;
            };
            if owner == identity && live.apply(tag, payload).is_none() {
                break;
            }
        }
        Some((Base { names, generics }, live))
    }

    /// Writes a new file for `base`, and returns it for the records that follow.
    ///
    /// `started` is the time before fontconfig loaded its configuration and scanned the fonts. A
    /// path that changed after that time (or less than [`SETTLE`] before it) can hold a change the
    /// scan did not see, so its stamp is not trusted. Returns `None` and writes nothing when a
    /// stamp is not trusted, or when the write fails.
    pub(super) fn store(
        &self,
        base: &Base,
        reported: &Reported,
        started: SystemTime,
    ) -> Option<Live> {
        let mut w = Writer::default();
        w.buf.extend_from_slice(MAGIC);
        w.u32(FORMAT);
        w.bytes(&self.key);
        if reported
            .config_files
            .iter()
            .any(|file| mentions_prgname(Path::new(OsStr::from_bytes(file))))
        {
            w.u8(1);
            w.bytes(prgname().as_bytes());
        } else {
            w.u8(0);
        }
        let paths = stamped_paths(reported);
        let settled = started.checked_sub(SETTLE)?;
        w.u32(paths.len() as u32);
        for path in &paths {
            w.bytes(path);
            match fs::metadata(Path::new(OsStr::from_bytes(path))) {
                Ok(meta) => {
                    if meta.modified().map_or(true, |modified| modified > settled) {
                        return None;
                    }
                    w.u8(1);
                    for value in stamp_of(&meta) {
                        w.u64(value);
                    }
                }
                Err(_) => w.u8(0),
            }
        }
        w.u32(base.names.len() as u32);
        for names in &base.names {
            w.strings(names);
        }
        w.u32(base.generics.len() as u32);
        for names in &base.generics {
            w.strings(names);
        }
        let identity = fnv(&w.buf);
        w.u64(identity);

        fs::create_dir_all(self.file.parent()?).ok()?;
        let temp = self
            .file
            .with_extension(format!("{}.tmp", std::process::id()));
        if fs::write(&temp, &w.buf)
            .and_then(|()| fs::rename(&temp, &self.file))
            .is_err()
        {
            let _ = fs::remove_file(&temp);
            return None;
        }
        STORES.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        Some(Live {
            file: self.file.clone(),
            identity,
            families: HashMap::default(),
            fallbacks: HashMap::default(),
        })
    }
}

impl Live {
    /// The faces of the family `name`, when a process resolved it before.
    pub(super) fn family(&self, name: &str) -> Option<&[RawFace]> {
        self.families.get(name).map(Vec::as_slice)
    }

    /// Keeps the faces of the family `name`, here and in the file.
    pub(super) fn add_family(&mut self, name: &str, faces: &[RawFace]) {
        let mut w = Writer::default();
        w.string(name);
        w.u32(faces.len() as u32);
        for face in faces {
            w.bytes(face.path.as_bytes());
            w.u32(face.index);
            for value in [face.weight, face.width, face.slant] {
                w.option_i32(value);
            }
        }
        self.append(FAMILY, &w.buf);
        self.families.insert(name.to_owned(), faces.to_vec());
    }

    /// The fallback family name for a script and locale, when a process resolved it before.
    pub(super) fn fallback(&self, script: [u8; 4], locale: Option<&str>) -> Option<Option<&str>> {
        self.fallbacks
            .get(&(script, locale.map(ToOwned::to_owned)))
            .map(Option::as_deref)
    }

    /// Keeps the fallback family name for a script and locale, here and in the file.
    pub(super) fn add_fallback(
        &mut self,
        script: [u8; 4],
        locale: Option<&str>,
        name: Option<&str>,
    ) {
        let mut w = Writer::default();
        w.buf.extend_from_slice(&script);
        w.option_string(locale);
        w.option_string(name);
        self.append(FALLBACK, &w.buf);
        self.fallbacks.insert(
            (script, locale.map(ToOwned::to_owned)),
            name.map(ToOwned::to_owned),
        );
    }

    fn apply(&mut self, tag: u8, payload: &[u8]) -> Option<()> {
        let mut r = Reader {
            data: payload,
            at: 0,
        };
        match tag {
            FAMILY => {
                let name = r.string()?;
                let mut faces = Vec::new();
                for _ in 0..r.u32()? {
                    faces.push(RawFace {
                        path: OsString::from_vec(r.bytes()?.to_vec()),
                        index: r.u32()?,
                        weight: r.option_i32()?,
                        width: r.option_i32()?,
                        slant: r.option_i32()?,
                    });
                }
                self.families.insert(name, faces);
            }
            FALLBACK => {
                let script: [u8; 4] = r.take(4)?.try_into().ok()?;
                let locale = r.option_string()?;
                let name = r.option_string()?;
                self.fallbacks.insert((script, locale), name);
            }
            _ => return None,
        }
        Some(())
    }

    /// Appends one record. A failed write loses the record and nothing else.
    fn append(&self, tag: u8, payload: &[u8]) {
        let mut w = Writer::default();
        w.u8(tag);
        w.u64(self.identity);
        w.bytes(payload);
        w.u64(fnv(payload));
        // One write call per record, so that records from two processes do not interleave.
        let _ = fs::OpenOptions::new()
            .append(true)
            .open(&self.file)
            .and_then(|mut file| file.write_all(&w.buf));
    }
}

/// The paths whose stamps decide if the file is valid.
///
/// These are the font directories and configuration files fontconfig reported, the directories
/// that hold those files (a new file in one of them can be an include), and the user
/// configuration paths that fontconfig reads when they exist. The home directory itself is left
/// out: it changes too often to be useful.
fn stamped_paths(reported: &Reported) -> Vec<Vec<u8>> {
    let home = std::env::var_os("HOME").map(OsString::into_vec);
    let mut paths = BTreeSet::new();
    paths.extend(reported.font_dirs.iter().cloned());
    for file in &reported.config_files {
        paths.insert(file.clone());
        if let Some(parent) = Path::new(OsStr::from_bytes(file)).parent() {
            let parent = parent.as_os_str().as_bytes().to_vec();
            if parent.len() > 1 && Some(&parent) != home.as_ref() {
                paths.insert(parent);
            }
        }
    }
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| {
            home.as_ref()
                .map(|home| Path::new(OsStr::from_bytes(home)).join(".config"))
        });
    if let Some(config_home) = config_home {
        let dir = config_home.join("fontconfig");
        for path in [dir.join("fonts.conf"), dir.join("conf.d"), dir] {
            paths.insert(path.into_os_string().into_vec());
        }
    }
    if let Some(home) = &home {
        let home = Path::new(OsStr::from_bytes(home));
        for path in [home.join(".fonts.conf"), home.join(".fonts.conf.d")] {
            paths.insert(path.into_os_string().into_vec());
        }
    }
    paths.into_iter().collect()
}

fn stamp_of(meta: &fs::Metadata) -> [u64; 6] {
    [
        meta.dev(),
        meta.ino(),
        meta.size(),
        u64::from(meta.mode()),
        meta.mtime() as u64,
        meta.mtime_nsec() as u64,
    ]
}

fn default_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|dir| dir.is_absolute())
                .map(|home| home.join(".cache"))
        })?;
    Some(base.join("zgui"))
}

/// The program name that a `prgname` test in a configuration file compares against.
fn prgname() -> OsString {
    std::env::args_os()
        .next()
        .and_then(|arg| Path::new(&arg).file_name().map(OsStr::to_owned))
        .unwrap_or_default()
}

/// Whether a configuration file has a rule that depends on the program name.
fn mentions_prgname(file: &Path) -> bool {
    fs::read(file).is_ok_and(|data| data.windows(7).any(|window| window == b"prgname"))
}

/// FNV-1a, 64 bit: stable across processes and builds.
fn fnv(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[derive(Default)]
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, value: u8) {
        self.buf.push(value);
    }

    fn u32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn bytes(&mut self, value: &[u8]) {
        self.u32(value.len() as u32);
        self.buf.extend_from_slice(value);
    }

    fn string(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    fn strings(&mut self, values: &[String]) {
        self.u32(values.len() as u32);
        for value in values {
            self.string(value);
        }
    }

    fn option_string(&mut self, value: Option<&str>) {
        match value {
            Some(value) => {
                self.u8(1);
                self.string(value);
            }
            None => self.u8(0),
        }
    }

    fn option_i32(&mut self, value: Option<i32>) {
        match value {
            Some(value) => {
                self.u8(1);
                self.u32(value as u32);
            }
            None => self.u8(0),
        }
    }
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(len)?;
        let out = self.data.get(self.at..end)?;
        self.at = end;
        Some(out)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn bytes(&mut self) -> Option<&'a [u8]> {
        let len = self.u32()? as usize;
        self.take(len)
    }

    fn string(&mut self) -> Option<String> {
        String::from_utf8(self.bytes()?.to_vec()).ok()
    }

    fn strings(&mut self) -> Option<Vec<String>> {
        let len = self.u32()?;
        let mut out = Vec::new();
        for _ in 0..len {
            out.push(self.string()?);
        }
        Some(out)
    }

    fn option_string(&mut self) -> Option<Option<String>> {
        match self.u8()? {
            0 => Some(None),
            1 => Some(Some(self.string()?)),
            _ => None,
        }
    }

    fn option_i32(&mut self) -> Option<Option<i32>> {
        match self.u8()? {
            0 => Some(None),
            1 => Some(Some(self.u32()? as i32)),
            _ => None,
        }
    }

    /// One record: the identity of its base, its tag and its payload.
    ///
    /// Returns `None` when the record is cut short or its checksum is wrong.
    fn record(&mut self) -> Option<(u64, u8, &'a [u8])> {
        let tag = self.u8()?;
        let owner = self.u64()?;
        let payload = self.bytes()?;
        let sum = self.u64()?;
        (sum == fnv(payload)).then_some((owner, tag, payload))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A font directory that changed after the scan started is not trusted, even when the change
    /// is older than [`SETTLE`] at the time of the store.
    #[test]
    fn a_change_after_the_scan_started_writes_no_file() {
        let root =
            std::env::temp_dir().join(format!("zgui-fontique-settle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let fonts = root.join("fonts");
        fs::create_dir_all(&fonts).unwrap();
        let now = SystemTime::now();
        fs::File::open(&fonts)
            .and_then(|dir| dir.set_modified(now - Duration::from_secs(3)))
            .unwrap();
        let place = Place {
            file: root.join("cache").join("fonts.bin"),
            key: Vec::new(),
        };
        let base = Base {
            names: Vec::new(),
            generics: Vec::new(),
        };
        let reported = Reported {
            font_dirs: std::vec![fonts.into_os_string().into_vec()],
            config_files: Vec::new(),
        };

        let during = place.store(&base, &reported, now - Duration::from_secs(4));
        assert!(during.is_none(), "a change during the scan writes no file");
        assert!(!place.file.exists());
        let after = place.store(&base, &reported, now);
        assert!(after.is_some(), "a change before the scan writes the file");
        assert!(place.file.exists());
        let _ = fs::remove_dir_all(&root);
    }
}
