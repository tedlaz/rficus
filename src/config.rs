//! ficus.ini load/save. Port of qconfig.py, minus the video sections.

use ini::Ini;
#[cfg(debug_assertions)]
use std::path::Path;
use std::path::PathBuf;

pub const INITIAL_INI: &str = include_str!("../ficus.ini");

pub struct Config {
    pub path: PathBuf,
    pub dl_exe: Option<PathBuf>,
    ini: Ini,
}

/// Look next to the exe. Replaces qconfig.py's frozen/_MEIPASS split.
///
/// A debug build also searches the source tree, so `cargo run` picks up the
/// checked-out tools. A release build must not: the build path may well exist
/// on the developer's own machine, and an installed copy silently reading
/// config and exes out of a source checkout is a trap.
fn search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        dirs.push(dir.to_path_buf());
    }
    #[cfg(debug_assertions)]
    dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf());
    dirs
}

/// Search those directories for the first of `names` that exists.
pub fn find(names: &[&str]) -> Option<PathBuf> {
    search_dirs()
        .iter()
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

/// Copy keys the bundled ini has and the user's does not.
///
/// An installed copy keeps its own ficus.ini forever, so without this a preset
/// added in a new version — a new format, say — would never reach anyone who
/// already ran the app once. Existing values are left alone: this only fills
/// gaps, it does not overwrite settings.
fn add_new_defaults(ini: &mut Ini) {
    let defaults = Ini::load_from_str(INITIAL_INI).expect("bundled ficus.ini is valid");
    for (section, props) in defaults.iter() {
        for (key, value) in props.iter() {
            let missing = ini
                .section(section)
                .is_none_or(|s| s.get(key).is_none());
            if missing {
                ini.with_section(section).set(key, value);
            }
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let path = find(&["ficus.ini"]).unwrap_or_else(|| {
            let p = std::env::current_exe()
                .ok()
                .and_then(|e| e.parent().map(|d| d.join("ficus.ini")))
                .unwrap_or_else(|| PathBuf::from("ficus.ini"));
            let _ = std::fs::write(&p, INITIAL_INI);
            p
        });
        let mut ini = Ini::load_from_file(&path).unwrap_or_else(|_| {
            Ini::load_from_str(INITIAL_INI).expect("bundled ficus.ini is valid")
        });
        add_new_defaults(&mut ini);
        // Plain name on Windows, no extension elsewhere.
        let dl_exe = find(&["yt-dlp.exe", "yt-dlp"]);
        Self { path, dl_exe, ini }
    }

    /// Re-resolve the downloader; the bootstrap may have just installed it.
    pub fn refresh_dl_exe(&mut self) {
        self.dl_exe = find(&["yt-dlp.exe", "yt-dlp"]);
    }

    pub fn get(&self, section: &str, key: &str) -> String {
        self.ini
            .section(Some(section))
            .and_then(|s| s.get(key))
            .unwrap_or_default()
            .to_string()
    }

    pub fn get_bool(&self, section: &str, key: &str) -> bool {
        self.get(section, key) == "true"
    }

    pub fn set(&mut self, section: &str, key: &str, value: impl Into<String>) {
        self.ini
            .with_section(Some(section))
            .set(key, value.into());
    }

    pub fn set_bool(&mut self, section: &str, key: &str, value: bool) {
        self.set(section, key, if value { "true" } else { "false" });
    }

    /// Section as (key, whitespace-split args) pairs, in file order.
    /// Port of qconfig.ini2dic; order matters, it drives the combo boxes.
    pub fn args_map(&self, section: &str) -> Vec<(String, Vec<String>)> {
        self.ini
            .section(Some(section))
            .map(|s| {
                s.iter()
                    .map(|(k, v)| {
                        (
                            k.to_string(),
                            v.split_whitespace().map(str::to_string).collect(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `#rrggbb` from [Tprocess] -> rgb bytes, black if the key is missing.
    pub fn color(&self, key: &str) -> [u8; 3] {
        let v = self.get("Tprocess", key);
        let hex = v.trim().trim_start_matches('#');
        let n = u32::from_str_radix(hex, 16).unwrap_or(0);
        [(n >> 16) as u8, (n >> 8) as u8, n as u8]
    }

    pub fn save(&self) {
        if let Err(e) = self.ini.write_to_file(&self.path) {
            eprintln!("could not write {}: {e}", self.path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An ini written by an older version gains new presets, keeps its own edits.
    #[test]
    fn new_defaults_fill_gaps_without_overwriting() {
        let old = "[General]\ntheme=light\n\n[Typoi]\nvideo=\nmp3=--extract-audio\n";
        let mut ini = Ini::load_from_str(old).unwrap();
        add_new_defaults(&mut ini);

        let typoi = ini.section(Some("Typoi")).unwrap();
        assert!(typoi.get("both").is_some(), "new format preset not added");
        assert_eq!(typoi.get("mp3"), Some("--extract-audio"), "user's own value was overwritten");
        assert_eq!(ini.section(Some("General")).unwrap().get("theme"), Some("light"));
        // and a key the old file never had
        assert_eq!(ini.section(Some("General")).unwrap().get("max_jobs"), Some("3"));
    }
}
