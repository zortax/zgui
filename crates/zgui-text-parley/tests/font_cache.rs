//! The on-disk cache of the system font collection: same answers, and a stale file is not used.
//!
//! Each case runs the `worker` test in a child process, because the cache is read once per
//! collection from the environment and is meant to serve the next process. The worker prints what
//! the collection resolves and what the cache did, and the parent compares the runs.

#![cfg(any(target_os = "linux", target_os = "freebsd"))]

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use fontique::{
    Collection, CollectionOptions, FallbackKey, FamilyId, GenericFamily, Language, Script,
    SourceKind, system_cache_counts,
};

/// The variable that makes the `worker` test do its work.
const WORKER: &str = "ZGUI_FONT_CACHE_TEST_WORKER";

/// The variable that moves the worker's sample to other families and adds other fallbacks.
const SHIFTED: &str = "ZGUI_FONT_CACHE_TEST_SHIFTED";

/// The generic families the fontconfig backend binds.
const GENERICS: [GenericFamily; 8] = [
    GenericFamily::Serif,
    GenericFamily::SansSerif,
    GenericFamily::Monospace,
    GenericFamily::Cursive,
    GenericFamily::Fantasy,
    GenericFamily::SystemUi,
    GenericFamily::Emoji,
    GenericFamily::Math,
];

/// What one run resolved, and the cache counts of that run.
struct Run {
    summary: Vec<String>,
    /// Hits, misses, stores and configuration loads.
    counts: [u64; 4],
}

/// Prints what a system collection resolves, when the parent asks for it.
#[test]
fn worker() {
    if std::env::var_os(WORKER).is_none() {
        return;
    }
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: true,
    });
    let mut lines = Vec::new();
    let mut names: Vec<String> = collection.family_names().map(String::from).collect();
    names.sort();
    names.dedup();
    let shifted = std::env::var_os(SHIFTED).is_some();
    let mut wanted: Vec<FamilyId> = Vec::new();
    for name in &names {
        let id = collection.family_id(name).expect("a listed name has an id");
        let primary = collection.family_name(id).unwrap_or("?").to_owned();
        lines.push(format!("name {name} -> {primary}"));
    }
    // A sample of all families: on a system with thousands of faces, resolving each one takes
    // too long for a test.
    for name in names.iter().skip(if shifted { 4 } else { 0 }).step_by(8) {
        wanted.extend(collection.family_id(name));
    }
    for generic in GENERICS {
        let ids: Vec<FamilyId> = collection.generic_families(generic).collect();
        let line = ids
            .iter()
            .map(|id| collection.family_name(*id).unwrap_or("?").to_owned())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("generic {generic:?}: {line}"));
        wanted.extend(ids);
    }
    let japanese = Language::parse("ja").expect("parses");
    let mut fallbacks = vec![
        (*b"Latn", None),
        (*b"Arab", None),
        (*b"Cyrl", None),
        (*b"Grek", None),
        (*b"Hebr", None),
        (*b"Deva", None),
        (*b"Thai", None),
        (*b"Hang", None),
        (*b"Hani", None),
        (*b"Hani", Some(&japanese)),
        (*b"Zsye", None),
    ];
    if shifted {
        fallbacks.extend([(*b"Armn", None), (*b"Geor", None), (*b"Ethi", None)]);
    }
    for (script, locale) in fallbacks {
        let key = FallbackKey::new(Script::from_bytes(script), locale);
        let ids: Vec<FamilyId> = collection.fallback_families(key).collect();
        let line = ids
            .iter()
            .map(|id| collection.family_name(*id).unwrap_or("?").to_owned())
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "fallback {} {locale:?}: {line}",
            String::from_utf8_lossy(&script)
        ));
        wanted.extend(ids);
    }
    let mut seen = std::collections::HashSet::new();
    for id in wanted {
        if !seen.insert(id) {
            continue;
        }
        let name = collection.family_name(id).unwrap_or("?").to_owned();
        let Some(family) = collection.family(id) else {
            lines.push(format!("family {name}: none"));
            continue;
        };
        for font in family.fonts() {
            let path = match font.source().kind() {
                SourceKind::Path(path) => path.display().to_string(),
                SourceKind::Memory(_) => "memory".to_owned(),
            };
            let axes: Vec<String> = font
                .axes()
                .iter()
                .map(|axis| format!("{}:{}:{}:{}", axis.tag, axis.min, axis.default, axis.max))
                .collect();
            lines.push(format!(
                "family {name}: {path} #{} {:?} {:?} {:?} [{}]",
                font.index(),
                font.weight(),
                font.width(),
                font.style(),
                axes.join(" ")
            ));
        }
    }
    for line in lines {
        println!("S {line}");
    }
    let counts = system_cache_counts();
    println!(
        "C {} {} {} {}",
        counts.hits, counts.misses, counts.stores, counts.config_loads
    );
}

/// Runs the worker in a child process with `cache` as `ZGUI_FONT_CACHE`.
fn run(cache: &str, env: &[(&str, &OsStr)]) -> Run {
    let mut command = Command::new(std::env::current_exe().expect("the test binary"));
    command
        .args(["--exact", "worker", "--nocapture", "--test-threads=1"])
        .env(WORKER, "1")
        .env("ZGUI_FONT_CACHE", cache);
    for (name, value) in env {
        command.env(name, value);
    }
    let output = command.output().expect("the worker runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "the worker failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: Vec<String> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("S "))
        .map(str::to_owned)
        .collect();
    let counts = stdout
        .lines()
        .find_map(|line| line.strip_prefix("C "))
        .expect("the worker prints its counts")
        .split(' ')
        .map(|count| count.parse().expect("a count"))
        .collect::<Vec<u64>>()
        .try_into()
        .expect("four counts");
    Run { summary, counts }
}

/// A clean directory for one case, on disk under the target directory.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("font_cache")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("creates the scratch directory");
    dir
}

/// Sets the modification time of `path` to `age` in the past.
fn age(path: &Path, age: Duration) {
    std::fs::File::open(path)
        .and_then(|file| file.set_modified(SystemTime::now() - age))
        .unwrap_or_else(|error| panic!("setting the time of {}: {error}", path.display()));
}

/// The installed fonts resolve the same with the cache off, on a miss, and on a hit.
///
/// The hit loads no fontconfig configuration: the names, generic lists, families and fallbacks
/// all come from the file that the miss wrote.
#[test]
fn a_cached_collection_resolves_what_fontconfig_resolves() {
    let cache = scratch("system");
    let cache = cache.to_str().expect("a UTF-8 path");

    let off = run("off", &[]);
    assert_eq!(
        off.counts,
        [0, 0, 0, 1],
        "the cache off reads and writes nothing"
    );
    assert!(
        off.summary.iter().any(|line| line.starts_with("family ")),
        "this test needs a machine with fonts installed"
    );

    let miss = run(cache, &[]);
    assert_eq!(miss.counts, [0, 1, 1, 1], "the first run writes the file");
    assert_eq!(miss.summary, off.summary);

    let hit = run(cache, &[]);
    assert_eq!(
        hit.counts,
        [1, 0, 0, 0],
        "the second run loads no configuration"
    );
    assert_eq!(hit.summary, off.summary);

    // Families and fallbacks the file does not hold load the configuration, give the same
    // answers, and go into the file for the next run.
    let shifted = [(SHIFTED, OsStr::new("1"))];
    let shifted_off = run("off", &shifted);
    let lazy = run(cache, &shifted);
    assert_eq!(
        lazy.counts,
        [1, 0, 0, 1],
        "a question the file cannot answer loads the configuration"
    );
    assert_eq!(lazy.summary, shifted_off.summary);
    let appended = run(cache, &shifted);
    assert_eq!(
        appended.counts,
        [1, 0, 0, 0],
        "the next run reads the appended answers"
    );
    assert_eq!(appended.summary, shifted_off.summary);
}

/// Installing or removing a font makes the file stale, and a stale file is not used.
#[test]
fn a_font_install_or_removal_invalidates_the_cache() {
    let root = scratch("install");
    let (fonts, conf, cache) = (root.join("fonts"), root.join("conf"), root.join("cache"));
    std::fs::create_dir_all(&fonts).expect("creates the font directory");
    std::fs::create_dir_all(&conf).expect("creates the configuration directory");
    let config = conf.join("fonts.conf");
    std::fs::write(
        &config,
        format!(
            "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"urn:fontconfig:fonts.dtd\">\n\
             <fontconfig><dir>{}</dir><cachedir>{}</cachedir></fontconfig>\n",
            fonts.display(),
            root.join("fccache").display()
        ),
    )
    .expect("writes the configuration");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts");
    for file in ["NotoSans-Regular.ttf", "NotoSansArabic-Regular.ttf"] {
        std::fs::copy(source.join(file), fonts.join(file)).expect("installs a font");
    }
    // Stamps younger than two seconds are not written, so the test sets them back.
    for path in [&fonts, &conf, &config] {
        age(path, Duration::from_secs(3600));
    }
    let env = [("FONTCONFIG_FILE", config.as_os_str())];
    let cache = cache.to_str().expect("a UTF-8 path");
    let has_rubik = |run: &Run| run.summary.iter().any(|line| line.contains("Rubik"));

    let first = run(cache, &env);
    assert_eq!(first.counts, [0, 1, 1, 1]);
    assert!(!has_rubik(&first));
    let hit = run(cache, &env);
    assert_eq!(hit.counts, [1, 0, 0, 0]);
    assert_eq!(hit.summary, first.summary);

    // An install changes the directory, so the next run does not use the file.
    std::fs::copy(
        source.join("RubikSubset-Variable.ttf"),
        fonts.join("RubikSubset-Variable.ttf"),
    )
    .expect("installs a font");
    let installed = run(cache, &env);
    assert_eq!(installed.counts[..2], [0, 1], "an install is a miss");
    assert!(has_rubik(&installed));
    assert_eq!(installed.summary, run("off", &env).summary);
    age(&fonts, Duration::from_secs(7200));
    assert_eq!(run(cache, &env).counts, [0, 1, 1, 1]);
    let hit = run(cache, &env);
    assert_eq!(hit.counts, [1, 0, 0, 0]);
    assert_eq!(hit.summary, installed.summary);

    // A removal changes the directory too.
    std::fs::remove_file(fonts.join("RubikSubset-Variable.ttf")).expect("removes a font");
    age(&fonts, Duration::from_secs(10800));
    let removed = run(cache, &env);
    assert_eq!(removed.counts, [0, 1, 1, 1], "a removal is a miss");
    assert_eq!(removed.summary, first.summary);
}
