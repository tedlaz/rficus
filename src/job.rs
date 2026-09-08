//! One subprocess per row. Replaces ted_qprocess.py + ted_process_manager.py.

use crate::parse;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, TryRecvError};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    Queued,
    Starting,
    Running,
    Finished,
    Error,
    Stopped,
}

impl State {
    /// Key into the [Tprocess] ini section.
    pub fn ini_key(self) -> &'static str {
        match self {
            // Deliberately the same key: an existing ficus.ini has no `queued`
            // entry, and a missing key reads as black.
            State::Queued | State::Starting => "starting",
            State::Running => "running",
            State::Finished => "finished",
            State::Error => "error",
            State::Stopped => "stopped",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            State::Queued => "Queued",
            State::Starting => "Starting",
            State::Running => "Running",
            State::Finished => "Finished",
            State::Error => "Error",
            State::Stopped => "Stopped",
        }
    }
}

/// Everything needed to (re)start a job.
#[derive(Clone)]
pub struct Spec {
    pub name: String,
    pub exe: PathBuf,
    pub args: Vec<String>,
    /// Working directory. Per-child, unlike ted_qprocess.py's global os.chdir().
    pub cwd: Option<PathBuf>,
}

/// A Command with the console window suppressed on Windows.
pub fn command(exe: &PathBuf) -> Command {
    let cmd = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = cmd;
        cmd.creation_flags(CREATE_NO_WINDOW);
        return cmd;
    }
    #[allow(unreachable_code)]
    cmd
}

pub struct Job {
    pub spec: Spec,
    pub state: State,
    pub percent: f32,
    /// Speed and ETA while running, empty otherwise.
    pub rate: String,
    /// Recent speed samples in bytes/s, oldest first: the sparkline.
    pub speeds: Vec<f32>,
    pub filename: String,
    /// Every file yt-dlp downloaded for this job, for the cleanup on success.
    downloads: Vec<String>,
    /// Every file a merge produced from those downloads.
    merged: Vec<String>,
    pub log: String,
    child: Option<Child>,
    rx: Option<Receiver<String>>,
    killed: bool,
    /// When the process was reaped; the leftover sweep waits briefly on the
    /// output pumps after that.
    exited_at: Option<std::time::Instant>,
    /// The one-shot leftover sweep has run.
    cleaned: bool,
    progress: parse::Progress,
}

impl Job {
    /// Waiting for a slot. Nothing is spawned until `start_queued` says so.
    pub fn new(spec: Spec) -> Self {
        Self {
            spec,
            state: State::Queued,
            percent: 0.0,
            rate: String::new(),
            speeds: Vec::new(),
            filename: String::new(),
            downloads: Vec::new(),
            merged: Vec::new(),
            log: String::new(),
            child: None,
            rx: None,
            killed: false,
            exited_at: None,
            cleaned: false,
            progress: parse::Progress::default(),
        }
    }

    #[cfg(test)]
    pub fn start(spec: Spec, repaint: impl Fn() + Send + Clone + 'static) -> Self {
        let mut job = Self::new(spec);
        job.spawn(repaint);
        job
    }

    /// Back to the end of the queue, as good as new.
    pub fn requeue(&mut self) {
        self.stop();
        self.reset();
        self.state = State::Queued;
    }

    fn reset(&mut self) {
        self.percent = 0.0;
        self.progress = parse::Progress::default();
        self.rate.clear();
        self.speeds.clear();
        self.filename.clear();
        self.downloads.clear();
        self.merged.clear();
        self.log.clear();
        self.killed = false;
        self.cleaned = false;
        self.exited_at = None;
    }

    pub fn spawn(&mut self, repaint: impl Fn() + Send + Clone + 'static) {
        self.reset();

        let mut cmd = command(&self.spec.exe);
        cmd.args(&self.spec.args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());
        if let Some(dir) = &self.spec.cwd {
            cmd.current_dir(dir);
        }

        match cmd.spawn() {
            Ok(mut child) => {
                let (tx, rx) = channel();
                for stream in [
                    child.stdout.take().map(reader_of),
                    child.stderr.take().map(reader_of),
                ]
                .into_iter()
                .flatten()
                {
                    let tx = tx.clone();
                    let repaint = repaint.clone();
                    std::thread::spawn(move || pump(stream, tx, repaint));
                }
                self.child = Some(child);
                self.rx = Some(rx);
                self.state = State::Starting;
            }
            Err(e) => {
                self.log = format!("could not start {}: {e}", self.spec.exe.display());
                self.state = State::Error;
            }
        }
    }

    /// Drain output and check for exit. Called every frame.
    pub fn poll(&mut self) {
        if let Some(rx) = &self.rx {
            loop {
                match rx.try_recv() {
                    Ok(line) => {
                        if self.state == State::Starting {
                            self.state = State::Running;
                        }
                        self.progress.feed(&line);
                        if let Some(name) = parse::destination(&line) {
                            self.filename = name.to_string();
                        }
                        if let Some(name) = parse::download_destination(&line) {
                            self.downloads.push(name.to_string());
                        }
                        if let Some(name) = parse::merged_destination(&line) {
                            self.merged.push(name.to_string());
                        }
                        self.log = line;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.rx = None;
                        break;
                    }
                }
            }
        }

        if let Some(child) = &mut self.child
            && let Ok(Some(status)) = child.try_wait() {
                self.child = None;
                self.exited_at = Some(std::time::Instant::now());
                self.state = if self.killed {
                    State::Stopped
                } else if status.success() {
                    State::Finished
                } else {
                    State::Error
                };
            }

        // Only once the pumps have hung up: the process can exit with its last
        // lines still in the channel, and cleaning then would run against a
        // half-read picture — `filename` still a stream part, the [Merger] and
        // [ExtractAudio] lines unseen — and delete or keep the wrong files.
        // The timeout is for the case where something outlives yt-dlp holding
        // the pipe open, so that the sweep still happens rather than never.
        let drained = self.rx.is_none()
            || self
                .exited_at
                .is_some_and(|t| t.elapsed() > std::time::Duration::from_secs(2));
        if self.state == State::Finished && drained && !self.cleaned {
            self.cleaned = true;
            self.clean_intermediates();
        }

        // A row that stopped must not keep claiming 2 MiB/s. Its shape stays:
        // `speeds` is a record of the download, cleared only on a restart.
        self.rate = if self.is_running() {
            // One sample per poll, ~15s of history at the 120ms repaint tick.
            self.speeds.push(self.progress.bps() as f32);
            if self.speeds.len() > 120 {
                self.speeds.remove(0);
            }
            self.progress.rate().to_string()
        } else {
            String::new()
        };

        self.percent = if self.state == State::Finished {
            100.0
        } else if self.is_running() {
            // Merging streams, writing mp3 and embedding thumbnails all happen
            // after the last byte arrives. Only the process exiting is 100%.
            self.progress.percent().min(99.0)
        } else {
            self.progress.percent()
        };
    }

    /// Delete the separate video and audio streams a `--keep-video` merge left
    /// behind (the "both" format), which yt-dlp would have deleted itself if it
    /// were not keeping the video.
    ///
    /// Deliberately narrow, because this deletes the user's files: only after a
    /// clean exit (a stopped job keeps its parts so it can be resumed), only for
    /// a job that asked for --keep-video, and only files yt-dlp named as its own
    /// download destinations that a merge then consumed — `Rick.f251.webm`
    /// against `[Merger] ... into "Rick.mkv"`.
    ///
    /// Matching against the merged file's own stem rather than the shape of the
    /// name is what makes this reliable: a format id is not always digits
    /// (`f251-drc`), and a video that was never merged — every entry of a
    /// progressive playlist — has no merged output to match, so it is kept.
    fn clean_intermediates(&mut self) {
        let downloads = std::mem::take(&mut self.downloads);
        let merged = std::mem::take(&mut self.merged);
        if !self.spec.args.iter().any(|a| a == "--keep-video") {
            return;
        }
        let dir = self.spec.cwd.clone().unwrap_or_default();
        for name in downloads {
            if !merged.iter().any(|m| is_part_of(&name, m)) {
                continue;
            }
            let path = dir.join(&name);
            if path.is_file() {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }

    /// Still needs frames: either downloading, or finished a moment ago with
    /// the leftover sweep still pending on the last of the output.
    pub fn needs_poll(&self) -> bool {
        self.is_running() || (self.state == State::Finished && !self.cleaned)
    }

    pub fn stop(&mut self) {
        // A queued job has no child to kill, but Stop still has to mean stop.
        if self.state == State::Queued {
            self.state = State::Stopped;
            return;
        }
        if let Some(child) = &mut self.child {
            self.killed = true;
            kill_tree(child.id());
            // Also the process we own, in case the tree walk missed it.
            let _ = child.kill();
            self.state = State::Stopped;
        }
    }
}

/// Is `name` one of the streams that were merged into `merged`? yt-dlp writes
/// the parts as `%(title)s.f%(format_id)s.%(ext)s`, so they are the merged
/// file's own stem plus an `.f<id>` segment — whatever the id looks like.
///
/// `Rick.f251.webm` is part of `Rick.mkv`; `Rick.mkv` is not part of itself,
/// and `Rick and Morty.mp4` is not part of `Rick.mkv`.
fn is_part_of(name: &str, merged: &str) -> bool {
    let Some(stem) = std::path::Path::new(merged).file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    let Some(rest) = name.strip_prefix(stem).and_then(|r| r.strip_prefix(".f")) else {
        return false;
    };
    // `.f<id>.<ext>`: an id, then the extension. Nothing else may follow.
    rest.split_once('.')
        .is_some_and(|(id, ext)| !id.is_empty() && !ext.is_empty() && !ext.contains('.'))
}

/// Spawn queued jobs, oldest first, until `max` are running. `max == 0` is
/// no cap. Called every frame; does nothing when the slots are full.
pub fn start_queued(jobs: &mut [Job], max: usize, repaint: impl Fn() + Send + Clone + 'static) {
    let mut running = jobs.iter().filter(|j| j.is_running()).count();
    for job in jobs {
        if max > 0 && running >= max {
            return;
        }
        if job.state == State::Queued {
            job.spawn(repaint.clone());
            // A spawn that failed took no slot with it.
            running += usize::from(job.is_running());
        }
    }
}

/// Kill a process and everything below it.
///
/// `Child::kill` is TerminateProcess on the one process we spawned, and
/// `yt-dlp.exe` is a PyInstaller bootloader that does the actual downloading in
/// a child of its own. Killing the bootloader orphans that child, which carries
/// on downloading with its socket and file handle intact — measured at 243 MB
/// written in the five seconds after the "stop". The tree is walked at kill
/// time, so it also catches an ffmpeg spawned for merging.
fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        let _ = command(&PathBuf::from("taskkill"))
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    let _ = pid;
}

impl Drop for Job {
    fn drop(&mut self) {
        // Removing a row must not leave an orphan download running.
        self.stop();
    }
}

fn reader_of<R: Read + Send + 'static>(r: R) -> Box<dyn Read + Send> {
    Box::new(r)
}

/// yt-dlp writes progress with a bare `\r`, so read bytes and split on both
/// terminators. BufRead::lines() would buffer a whole download into one item.
fn pump(mut stream: Box<dyn Read + Send>, tx: std::sync::mpsc::Sender<String>, repaint: impl Fn()) {
    let mut buf = [0u8; 4096];
    let mut pending = String::new();
    loop {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                pending.push_str(&String::from_utf8_lossy(&buf[..n]));
                // Keep the trailing fragment; it is not a complete line yet.
                let tail = match pending.rfind(['\r', '\n']) {
                    Some(i) => pending.split_off(i + 1),
                    None => continue,
                };
                for line in parse::split_lines(&pending) {
                    if tx.send(line.to_string()).is_err() {
                        return;
                    }
                }
                pending = tail;
                repaint();
            }
        }
    }
    if !pending.trim().is_empty() {
        let _ = tx.send(pending.trim().to_string());
    }
    repaint();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The "both" cleanup deletes the merge parts and nothing else.
    #[test]
    fn cleanup_takes_only_the_stream_parts() {
        assert!(is_part_of("Adele - Hello.f251.webm", "Adele - Hello.mkv"));
        assert!(is_part_of("Adele - Hello.f616.mp4", "Adele - Hello.mkv"));
        // a format id is not always digits
        assert!(is_part_of("Adele - Hello.f251-drc.webm", "Adele - Hello.mkv"));
        // the merged file itself, and the mp3 beside it, stay
        assert!(!is_part_of("Adele - Hello.mkv", "Adele - Hello.mkv"));
        assert!(!is_part_of("Adele - Hello.mp3", "Adele - Hello.mkv"));
        // a different video that merely starts the same way
        assert!(!is_part_of("Adele - Hello Again.f251.webm", "Adele - Hello.mkv"));
        // a title with its own dotted segment is not a part of this merge
        assert!(!is_part_of("Adele - Hello.feat.Someone.mp4", "Adele - Hello.mkv"));

        let dir = std::env::temp_dir().join("rficus-cleanup-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["Song.f616.mp4", "Song.f251-drc.webm", "Song.webm", "Song.mp3"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }

        let mut job = Job::new(Spec {
            name: "both".into(),
            exe: PathBuf::from("yt-dlp"),
            args: vec!["--extract-audio".into(), "--keep-video".into()],
            cwd: Some(dir.clone()),
        });
        // What poll() would have scraped off the [download] and [Merger] lines.
        job.downloads = vec!["Song.f616.mp4".into(), "Song.f251-drc.webm".into()];
        job.merged = vec!["Song.webm".into()];
        job.filename = "Song.mp3".into();
        job.clean_intermediates();

        assert!(!dir.join("Song.f616.mp4").exists(), "video part not cleaned");
        assert!(!dir.join("Song.f251-drc.webm").exists(), "audio part not cleaned");
        assert!(dir.join("Song.webm").exists(), "the merged video was deleted");
        assert!(dir.join("Song.mp3").exists(), "the mp3 was deleted");

        // A playlist entry that needed no merge is the video itself, not a part.
        let mut plain = Job::new(Spec {
            name: "playlist".into(),
            exe: PathBuf::from("yt-dlp"),
            args: vec!["--keep-video".into()],
            cwd: Some(dir.clone()),
        });
        std::fs::write(dir.join("First.mp4"), b"x").unwrap();
        plain.downloads = vec!["First.mp4".into()];
        plain.merged = vec!["Second.mkv".into()];
        plain.filename = "Second.mp3".into();
        plain.clean_intermediates();
        assert!(dir.join("First.mp4").exists(), "deleted a file it must not touch");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The process can be reaped while its last lines are still in flight.
    /// Cleaning at that moment saw `filename` as a stream part and left one
    /// behind; the sweep has to wait for the pumps to hang up.
    #[test]
    fn cleanup_waits_for_the_last_of_the_output() {
        let dir = std::env::temp_dir().join("rficus-drain-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["Song.f602.mp4", "Song.f233.mp4", "Song.mp4", "Song.mp3"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }

        let mut job = Job::new(Spec {
            name: "both".into(),
            exe: PathBuf::from("yt-dlp"),
            args: vec!["--keep-video".into()],
            cwd: Some(dir.clone()),
        });

        // The process has exited; its output is still arriving.
        let (tx, rx) = channel();
        job.rx = Some(rx);
        job.state = State::Finished;
        job.exited_at = Some(std::time::Instant::now());
        tx.send("[download] Destination: Song.f602.mp4".into()).unwrap();
        job.poll();
        assert!(!job.cleaned, "swept before the output was drained");

        // The rest of the output, then the pumps hang up.
        for line in [
            "[download] Destination: Song.f233.mp4",
            "[Merger] Merging formats into \"Song.mp4\"",
            "[ExtractAudio] Destination: Song.mp3",
        ] {
            tx.send(line.into()).unwrap();
        }
        drop(tx);
        job.poll();

        assert!(job.cleaned, "never swept");
        assert!(!dir.join("Song.f602.mp4").exists(), "video part kept");
        assert!(!dir.join("Song.f233.mp4").exists(), "audio part kept");
        assert!(dir.join("Song.mp4").exists(), "the video was deleted");
        assert!(dir.join("Song.mp3").exists(), "the mp3 was deleted");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The whole "both" preset against real yt-dlp: two files out, no leftovers.
    /// Network, so ignored by default: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn both_leaves_only_the_video_and_the_mp3() {
        let Some(exe) = crate::config::find(&["yt-dlp.exe", "yt-dlp"]) else {
            panic!("yt-dlp not found");
        };
        let dir = std::env::temp_dir().join("rficus-both-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Exactly what the app builds for `both` with both checkboxes on, plus
        // the smallest video+audio pair so this downloads megabytes rather than
        // hundreds of them.
        let mut args: Vec<String> = [
            "--output",
            "%(title)s.%(ext)s",
            "--ignore-errors",
            "--no-playlist",
            "--extract-audio",
            "--audio-format",
            "mp3",
            "--audio-quality",
            "0",
            "--keep-video",
            "--embed-thumbnail",
            "--postprocessor-args",
            "-id3v2_version 3",
            "--add-metadata",
            "-f",
            "worstvideo[ext=webm]+worstaudio[ext=m4a]",
        ]
        .iter()
        .map(|a| (*a).to_string())
        .collect();
        args.push("https://www.youtube.com/watch?v=YQHsXMglC9A".into());

        let mut job = Job::start(
            Spec {
                name: "both".into(),
                exe,
                args,
                cwd: Some(dir.clone()),
            },
            || {},
        );
        // The UI's own cadence, including the frames after the process exits:
        // the sweep waits for the last of yt-dlp's output, not for the exit.
        while job.needs_poll() {
            job.poll();
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
        assert_eq!(job.state, State::Finished, "log: {}", job.log);

        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        let exts: Vec<&str> = left
            .iter()
            .map(|n| n.rsplit('.').next().unwrap_or(""))
            .collect();
        assert_eq!(exts, vec!["mkv", "mp3"], "leftovers: {left:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Four jobs, two slots: the cap holds, and the rest start as slots free.
    #[test]
    #[cfg(windows)]
    fn queue_runs_at_most_max_jobs() {
        let spec = Spec {
            name: "wait".into(),
            exe: PathBuf::from("ping"),
            args: vec!["-n".into(), "3".into(), "127.0.0.1".into()],
            cwd: None,
        };
        let mut jobs: Vec<Job> = (0..4).map(|_| Job::new(spec.clone())).collect();

        start_queued(&mut jobs, 2, || {});
        assert_eq!(jobs.iter().filter(|j| j.is_running()).count(), 2);
        assert_eq!(
            jobs.iter().filter(|j| j.state == State::Queued).count(),
            2,
            "the cap let too many through"
        );

        // Let the first pair finish, then the queue must drain on its own.
        for _ in 0..200 {
            for job in &mut jobs {
                job.poll();
            }
            start_queued(&mut jobs, 2, || {});
            if jobs.iter().all(|j| j.state == State::Finished) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("queue never drained: {:?}", jobs.iter().map(|j| j.state).collect::<Vec<_>>());
    }

    #[test]
    #[ignore]
    fn debug_taskkill_available() {
        let out = command(&PathBuf::from("taskkill"))
            .args(["/?"])
            .output();
        match out {
            Ok(o) => println!("taskkill ran, status={:?}, stdout={} bytes", o.status, o.stdout.len()),
            Err(e) => println!("taskkill FAILED to run: {e}"),
        }
    }
    /// Sizes queried per path, not from the `DirEntry`: on Windows the entry
    /// carries cached directory information that lags far behind a file being
    /// actively written, which reads as growth long after the writer is dead.
    fn bytes_in(dir: &std::path::Path) -> u64 {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| std::fs::metadata(e.path()).ok())
            .map(|m| m.len())
            .sum()
    }

    /// Runs a real download and stops it, so it is ignored by default:
    /// `cargo test -- --ignored`. Guards the reported bug: "stop" left the
    /// PyInstaller child of yt-dlp.exe orphaned and still downloading.
    #[test]
    #[ignore]
    fn stop_actually_stops_the_download() {
        let Some(exe) = crate::config::find(&["yt-dlp.exe", "yt-dlp"]) else {
            panic!("yt-dlp not found");
        };
        let dir = std::env::temp_dir().join("rficus-stop-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut job = Job::start(
            Spec {
                name: "stop".into(),
                exe,
                // Big enough that it is certainly still downloading when stopped.
                args: vec![
                    "-o".into(),
                    "big.%(ext)s".into(),
                    "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
                ],
                cwd: Some(dir.clone()),
            },
            || {},
        );

        // Wait for bytes to actually be moving before stopping.
        let start = std::time::Instant::now();
        while bytes_in(&dir) == 0 && start.elapsed() < std::time::Duration::from_secs(60) {
            job.poll();
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        assert!(bytes_in(&dir) > 0, "download never started: {}", job.log);

        job.stop();
        // Let the write that was already in flight land before measuring.
        std::thread::sleep(std::time::Duration::from_secs(2));
        let at_stop = bytes_in(&dir);
        std::thread::sleep(std::time::Duration::from_secs(5));
        let after = bytes_in(&dir);

        assert_eq!(
            after, at_stop,
            "still downloading 5s after stop: {at_stop} -> {after} bytes"
        );
        job.poll();
        assert_eq!(job.state, State::Stopped);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Runs a real two-stream download, so it is ignored by default:
    /// `cargo test -- --ignored`. Guards the reported bug directly — the bar
    /// read 100% while the job was still running, because yt-dlp had only
    /// finished the video stream of a video+audio pair.
    #[test]
    #[ignore]
    fn progress_stays_below_100_until_the_process_exits() {
        let Some(exe) = crate::config::find(&["yt-dlp.exe", "yt-dlp"]) else {
            panic!("yt-dlp not found");
        };
        let dir = std::env::temp_dir().join("rficus-progress-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut job = Job::start(
            Spec {
                name: "progress".into(),
                exe,
                // Smallest video+audio pair: two streams and a merge.
                args: vec![
                    "-f".into(),
                    "worstvideo+worstaudio".into(),
                    "-o".into(),
                    "t.%(ext)s".into(),
                    "https://www.youtube.com/watch?v=YQHsXMglC9A".into(),
                ],
                cwd: Some(dir.clone()),
            },
            || {},
        );

        let mut peak_while_running = 0.0f32;
        loop {
            job.poll();
            // Sample only while the process is still alive: the poll that reaps
            // it is entitled to show 100%.
            if !job.is_running() {
                break;
            }
            peak_while_running = peak_while_running.max(job.percent);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        assert!(
            peak_while_running < 100.0,
            "showed {peak_while_running}% while still downloading"
        );
        assert_eq!(job.state, State::Finished, "log: {}", job.log);
        assert_eq!(job.percent, 100.0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
