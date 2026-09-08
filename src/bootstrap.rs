//! Startup check for the tools rficus drives, on a worker thread.
//!
//! yt-dlp is useless without ffmpeg (merging video with audio, and mp3
//! extraction both need it), so all three are treated as required. Anything
//! missing is downloaded next to the exe, and yt-dlp is brought up to the
//! latest release. The UI thread only ever reads the messages this sends.

use crate::config;
use crate::update;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};

const YTDLP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe";
/// The smallest build that still does everything yt-dlp asks of ffmpeg.
///
/// Shared, so one copy of the libraries serves both exes instead of each
/// statically linking its own, and LGPL, which drops the GPL-only encoders
/// nothing here uses. Measured against the alternatives: 65 MB to download and
/// 129 MB unpacked, where gyan.dev's static "essentials" is 106 MB and 205 MB,
/// and the full static GPL build is 163 MB and 290 MB. Verified this build
/// still merges by stream copy, writes mp3 through libmp3lame and embeds
/// thumbnails at id3v2 version 3.
const FFMPEG_URL: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-win64-lgpl-shared.zip";

/// We drive ffmpeg and ffprobe; ffplay is a 17 MB player nothing here opens.
const FFMPEG_SKIP: &str = "ffplay.exe";

enum Msg {
    Status(String),
    Progress(Option<f32>),
    /// Finished: the yt-dlp version we ended up with, and anything that failed.
    Done(Option<String>, Vec<String>),
}

pub struct Bootstrap {
    rx: Option<Receiver<Msg>>,
    pub status: String,
    pub progress: Option<f32>,
    pub version: Option<String>,
    pub problems: Vec<String>,
    pub busy: bool,
    /// Set for one frame when the worker finishes, so the app can re-resolve
    /// tool paths that may not have existed when it started.
    pub just_finished: bool,
}

impl Bootstrap {
    pub fn start(online: bool, repaint: impl Fn() + Send + 'static) -> Self {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            run(online, &tx);
            repaint();
        });
        Self {
            rx: Some(rx),
            status: "Checking tools…".to_owned(),
            progress: None,
            version: None,
            problems: Vec::new(),
            busy: true,
            just_finished: false,
        }
    }

    pub fn poll(&mut self) {
        self.just_finished = false;
        let Some(rx) = &self.rx else { return };
        loop {
            match rx.try_recv() {
                Ok(Msg::Status(s)) => self.status = s,
                Ok(Msg::Progress(p)) => self.progress = p,
                Ok(Msg::Done(version, problems)) => {
                    self.version = version;
                    self.problems = problems;
                    self.progress = None;
                    self.busy = false;
                    self.just_finished = true;
                    self.status = match (&self.version, self.problems.is_empty()) {
                        (Some(v), true) => format!("yt-dlp {v}"),
                        (Some(v), false) => format!("yt-dlp {v} - {}", self.problems.join("; ")),
                        (None, _) => self.problems.join("; "),
                    };
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.rx = None;
                    self.busy = false;
                    break;
                }
            }
        }
    }
}

fn say(tx: &Sender<Msg>, text: impl Into<String>) {
    let _ = tx.send(Msg::Status(text.into()));
}

/// Report download progress, and keep the UI repainting while bytes arrive.
fn progress_reporter(tx: &Sender<Msg>) -> impl FnMut(u64, Option<u64>) + '_ {
    let mut last = 0u64;
    move |done, total| {
        // One message per 256 KB; per-chunk would flood the channel.
        if done - last < 256 * 1024 && done != total.unwrap_or(u64::MAX) {
            return;
        }
        last = done;
        let _ = tx.send(Msg::Progress(
            total.map(|t| done as f32 / t.max(1) as f32 * 100.0),
        ));
    }
}

fn run(online: bool, tx: &Sender<Msg>) {
    let dir = update::exe_dir();
    let mut problems = Vec::new();

    // --- yt-dlp ------------------------------------------------------------
    let mut yt = config::find(&["yt-dlp.exe", "yt-dlp"]);
    if yt.is_none() {
        let dest = dir.join("yt-dlp.exe");
        say(tx, "Downloading yt-dlp…");
        match update::download(YTDLP_URL, &dest, &mut progress_reporter(tx)) {
            Ok(()) => yt = Some(dest),
            Err(e) => problems.push(format!("yt-dlp download failed: {e}")),
        }
        let _ = tx.send(Msg::Progress(None));
    }

    let mut version = yt.as_deref().and_then(|p| update::local_version(p).ok());

    if online && let Some(exe) = yt.as_deref() {
        say(tx, "Checking for a newer yt-dlp…");
        if let Some(latest) = update::latest_version() {
            if version.as_deref() != Some(latest.as_str()) {
                say(tx, format!("Updating yt-dlp to {latest}…"));
                match self_update(exe) {
                    // yt-dlp reports its new version itself; ask it again.
                    Ok(()) => version = update::local_version(exe).ok(),
                    Err(e) => problems.push(format!("yt-dlp update failed: {e}")),
                }
            }
        } else {
            problems.push("could not reach github for the version check".to_owned());
        }
    }

    // --- ffmpeg ------------------------------------------------------------
    // yt-dlp needs both to merge streams and to write mp3.
    let missing: Vec<&str> = ["ffmpeg.exe", "ffprobe.exe"]
        .into_iter()
        .filter(|n| !usable(n))
        .collect();
    if !missing.is_empty() {
        say(tx, format!("Downloading ffmpeg ({})…", missing.join(", ")));
        if let Err(e) = install_ffmpeg(&dir, tx) {
            problems.push(format!("ffmpeg download failed: {e}"));
        }
        let _ = tx.send(Msg::Progress(None));
    }

    let _ = tx.send(Msg::Done(version, problems));
}

/// yt-dlp replaces its own exe correctly, including the Windows dance of
/// renaming a running binary. Downloading over it ourselves would not.
fn self_update(exe: &Path) -> Result<(), String> {
    let out = crate::job::command(&exe.to_path_buf())
        .arg("-U")
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.trim().lines().next().unwrap_or("non-zero exit").to_owned())
    }
}

fn install_ffmpeg(dir: &Path, tx: &Sender<Msg>) -> Result<(), String> {
    let tmp = std::env::temp_dir().join("rficus-ffmpeg");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let zip = tmp.join("ffmpeg.zip");

    update::download(FFMPEG_URL, &zip, &mut progress_reporter(tx))?;

    say(tx, "Extracting ffmpeg…");
    let _ = tx.send(Msg::Progress(None));
    // bsdtar ships with Windows 10 1803+ and reads zip, so no zip crate.
    let tar = PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()))
        .join("System32")
        .join("tar.exe");
    if !tar.is_file() {
        return Err("tar.exe not found; unzip ffmpeg manually into the app folder".to_owned());
    }
    let out = crate::job::command(&tar)
        .arg("-xf")
        .arg(&zip)
        .arg("-C")
        .arg(&tmp)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!(
            "tar: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    // A shared build is useless without its libraries beside it, so take the
    // whole bin/ rather than picking out the two exes.
    let ffmpeg = find_under(&tmp, "ffmpeg.exe", 4)
        .ok_or_else(|| "ffmpeg.exe not found in the archive".to_owned())?;
    let bin = ffmpeg
        .parent()
        .ok_or_else(|| "archive has no bin directory".to_owned())?;
    for entry in std::fs::read_dir(bin).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name();
        if name == FFMPEG_SKIP || !entry.path().is_file() {
            continue;
        }
        std::fs::copy(entry.path(), dir.join(&name)).map_err(|e| e.to_string())?;
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(())
}

/// Present *and* working. A shared build whose libraries went missing leaves an
/// ffmpeg.exe that exists and cannot start, which a file check would call fine.
fn usable(name: &str) -> bool {
    config::find(&[name]).is_some_and(|exe| {
        crate::job::command(&exe)
            .arg("-version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// The archive nests the binaries under `<build name>/bin/`, so search rather
/// than hard-coding a folder name that changes with every release.
fn find_under(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        } else if path.file_name().is_some_and(|f| f == name) {
            return Some(path);
        }
    }
    dirs.into_iter().find_map(|d| find_under(&d, name, depth - 1))
}

#[cfg(test)]
mod tests {
    use super::find_under;

    /// The gyan archive nests binaries under `<build name>/bin/`, and the
    /// build name changes every release, so the walk has to find them.
    #[test]
    fn finds_a_nested_binary_within_the_depth_limit() {
        let root = std::env::temp_dir().join("rficus-find-under-test");
        let bin = root.join("ffmpeg-9.0.1-essentials_build").join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("ffmpeg.exe"), b"x").unwrap();

        assert_eq!(find_under(&root, "ffmpeg.exe", 4), Some(bin.join("ffmpeg.exe")));
        assert_eq!(find_under(&root, "ffprobe.exe", 4), None);
        // Too shallow to reach it: root -> build dir -> bin needs 3.
        assert_eq!(find_under(&root, "ffmpeg.exe", 2), None);

        std::fs::remove_dir_all(&root).unwrap();
    }
}
