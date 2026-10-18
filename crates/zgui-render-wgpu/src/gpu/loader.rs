//! Keeping the software Vulkan driver out of a process that has a hardware one.
//!
//! The Vulkan loader loads every installed driver when an instance is created. Mesa's lavapipe
//! links LLVM, so a machine that has it installed loads and initialises LLVM in every process that
//! opens a device: 26 MB of resident memory, measured, also when a hardware device opens. The
//! loader reads `VK_LOADER_DRIVERS_DISABLE` when it creates an instance, so setting it before then
//! keeps lavapipe out.

#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};

/// The loader variable that excludes drivers by manifest file name.
#[cfg(target_os = "linux")]
const DISABLE_VARIABLE: &str = "VK_LOADER_DRIVERS_DISABLE";

/// The manifest file names that [`SOFTWARE_DRIVER`] matches contain this.
#[cfg(target_os = "linux")]
const SOFTWARE_NAME: &str = "lvp";

/// The value given to [`DISABLE_VARIABLE`].
#[cfg(target_os = "linux")]
const SOFTWARE_DRIVER: &str = "*lvp*";

/// The loader variables that select drivers. When one of them is set, the user has chosen, and
/// this module changes nothing.
#[cfg(target_os = "linux")]
const DRIVER_VARIABLES: [&str; 5] = [
    "VK_LOADER_DRIVERS_SELECT",
    "VK_LOADER_DRIVERS_DISABLE",
    "VK_DRIVER_FILES",
    "VK_ICD_FILENAMES",
    "VK_ADD_DRIVER_FILES",
];

/// Keeps lavapipe out of this process, where that is safe and a hardware driver is installed.
///
/// Call it before the first instance exists and before the process starts a thread. It sets the
/// variable only when all of these are true:
///
/// * the process has one thread, so no other thread can read the environment while it changes;
/// * none of the loader's driver variables is set;
/// * a driver manifest other than lavapipe's is installed.
///
/// On a machine where the hardware driver then opens no device, the GL tier still finds Mesa's
/// software rasteriser, so a window opens in software.
///
/// Returns `true` when it set the variable.
pub fn prefer_hardware_vulkan() -> bool {
    #[cfg(target_os = "linux")]
    {
        let lookup = |name: &str| std::env::var_os(name);
        if !wanted(lookup, &search_directories(lookup)) || thread_count() != Some(1) {
            return false;
        }
        // SAFETY: the process has one thread, checked immediately above, so no other thread can
        // read or write the environment at the same time.
        unsafe { std::env::set_var(DISABLE_VARIABLE, SOFTWARE_DRIVER) };
        tracing::debug!("the software Vulkan driver is excluded: a hardware driver is installed");
        true
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

/// Whether the environment and the installed manifests ask for lavapipe to be excluded.
#[cfg(target_os = "linux")]
fn wanted(lookup: impl Fn(&str) -> Option<std::ffi::OsString>, directories: &[PathBuf]) -> bool {
    if DRIVER_VARIABLES.iter().any(|name| lookup(name).is_some()) {
        return false;
    }
    directories
        .iter()
        .any(|directory| has_other_driver(directory))
}

/// Whether `directory` holds a driver manifest whose file name is not lavapipe's.
///
/// The loader's filter matches manifest file names, so the same match decides here.
#[cfg(target_os = "linux")]
fn has_other_driver(directory: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        name.ends_with(".json") && !name.contains(SOFTWARE_NAME)
    })
}

/// The directories the loader reads driver manifests from on Linux.
#[cfg(target_os = "linux")]
fn search_directories(lookup: impl Fn(&str) -> Option<std::ffi::OsString>) -> Vec<PathBuf> {
    let home = lookup("HOME").map(PathBuf::from);
    let single = |name: &str, fallback: &str| {
        lookup(name)
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|home| home.join(fallback)))
    };
    let list = |name: &str, fallback: &str| {
        let value = lookup(name).unwrap_or_else(|| fallback.into());
        std::env::split_paths(&value).collect::<Vec<_>>()
    };
    let mut bases = Vec::new();
    bases.extend(single("XDG_CONFIG_HOME", ".config"));
    bases.extend(list("XDG_CONFIG_DIRS", "/etc/xdg"));
    bases.push(PathBuf::from("/usr/local/etc"));
    bases.push(PathBuf::from("/etc"));
    bases.extend(single("XDG_DATA_HOME", ".local/share"));
    bases.extend(list("XDG_DATA_DIRS", "/usr/local/share:/usr/share"));
    bases
        .into_iter()
        .map(|base| base.join("vulkan").join("icd.d"))
        .collect()
}

/// The number of threads in this process, from `/proc/self/status`.
#[cfg(target_os = "linux")]
fn thread_count() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("Threads:"))
        .and_then(|count| count.trim().parse().ok())
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;

    use super::{DISABLE_VARIABLE, prefer_hardware_vulkan, search_directories, wanted};

    #[test]
    fn a_process_with_several_threads_keeps_its_environment() {
        // The test harness runs each test on a thread of its own, so this process has several.
        let before = std::env::var_os(DISABLE_VARIABLE);
        assert!(!prefer_hardware_vulkan());
        assert_eq!(std::env::var_os(DISABLE_VARIABLE), before);
    }

    /// A new directory holding empty manifests with `names`.
    fn manifests(test: &str, names: &[&str]) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("zgui-loader-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("the test directory can be created");
        for name in names {
            std::fs::write(directory.join(name), "{}").expect("the manifest can be written");
        }
        directory
    }

    fn unset(_: &str) -> Option<OsString> {
        None
    }

    #[test]
    fn a_hardware_manifest_beside_lavapipe_excludes_lavapipe() {
        let directory = manifests("both", &["lvp_icd.x86_64.json", "nvidia_icd.json"]);
        assert!(wanted(unset, &[directory]));
    }

    #[test]
    fn lavapipe_alone_is_kept() {
        let directory = manifests("alone", &["lvp_icd.x86_64.json", "lvp_icd.i686.json"]);
        assert!(!wanted(unset, &[directory]));
    }

    #[test]
    fn files_other_than_manifests_do_not_count() {
        let directory = manifests("other", &["lvp_icd.x86_64.json", "README"]);
        assert!(!wanted(unset, &[directory]));
    }

    #[test]
    fn a_driver_variable_set_by_the_user_wins() {
        let directory = manifests("chosen", &["lvp_icd.x86_64.json", "radeon_icd.x86_64.json"]);
        let chosen = |name: &str| (name == "VK_DRIVER_FILES").then(|| OsString::from("/x.json"));
        assert!(!wanted(chosen, &[directory]));
    }

    #[test]
    fn missing_directories_mean_no_driver() {
        assert!(!wanted(
            unset,
            &[PathBuf::from("/nonexistent/vulkan/icd.d")]
        ));
    }

    #[test]
    fn the_search_follows_the_xdg_variables_and_their_defaults() {
        let lookup = |name: &str| match name {
            "HOME" => Some(OsString::from("/home/user")),
            "XDG_DATA_DIRS" => Some(OsString::from("/a:/b")),
            _ => None,
        };
        let directories = search_directories(lookup);
        for expected in [
            "/home/user/.config/vulkan/icd.d",
            "/etc/xdg/vulkan/icd.d",
            "/etc/vulkan/icd.d",
            "/home/user/.local/share/vulkan/icd.d",
            "/a/vulkan/icd.d",
            "/b/vulkan/icd.d",
        ] {
            assert!(
                directories.contains(&PathBuf::from(expected)),
                "{expected} is searched: {directories:?}"
            );
        }
        assert!(!directories.contains(&PathBuf::from("/usr/share/vulkan/icd.d")));
    }
}
