//! Scrapers for yt-dlp output. Port of filter_functions.py.

/// yt-dlp separates progress updates with a bare `\r`, so a line-based reader
/// that only splits on `\n` sees one giant line and the progress bar never moves.
pub fn split_lines(chunk: &str) -> impl Iterator<Item = &str> {
    chunk
        .split(['\r', '\n'])
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
}

/// `[download]  42.3% of ...` -> 42.3
///
/// Hand-rolled rather than a `Regex`, which is the same
/// `\[download\]\s+(\d+(?:\.\d+)?)%` in a tenth of the binary size.
pub fn percent(line: &str) -> Option<f32> {
    let rest = line.strip_prefix("[download]")?;
    let pct = rest.find('%')?;
    let number = &rest[..pct];
    // Walk back over the digits and dot that lead up to the '%'. Stepping by
    // char, not byte: a multi-byte character here would otherwise put the
    // slice index inside it and panic.
    let start = number
        .char_indices()
        .rev()
        .find(|(_, c)| !(c.is_ascii_digit() || *c == '.'))
        .map_or(0, |(i, c)| i + c.len_utf8());
    // Only whitespace may sit between the tag and the number, so a filename
    // that happens to contain a '%' is not read as progress.
    if !number[..start].chars().all(char::is_whitespace) {
        return None;
    }
    number[start..].parse().ok()
}

/// The speed and, when there is one, the ETA of a `[download]` line.
///
/// The last line of a stream reports `in 00:02` instead of an ETA, so the
/// speed alone is a valid answer.
fn speed_eta(line: &str) -> Option<(&str, Option<&str>)> {
    let rest = line.strip_prefix("[download]")?;
    let after = rest.split(" at ").nth(1)?;
    let (speed, eta) = match after.split_once(" ETA ") {
        Some((speed, eta)) => (speed.trim(), Some(eta.trim())),
        None => (after.trim(), None),
    };
    (!speed.is_empty()).then_some((speed, eta))
}

/// `[download]  49.7% of 3.50MiB at 2.00MiB/s ETA 00:01` -> "2.00MiB/s · 00:01"
fn rate(line: &str) -> Option<String> {
    match speed_eta(line)? {
        (speed, Some(eta)) => Some(format!("{speed} · {eta}")),
        (speed, None) => Some(speed.to_string()),
    }
}

/// The same speed as a number: `2.00MiB/s` -> 2097152.0. yt-dlp reports binary
/// units, and `Unknown B/s` while it has nothing to report yet.
fn speed_bps(line: &str) -> Option<f64> {
    let (speed, _) = speed_eta(line)?;
    let unit = speed.strip_suffix("/s")?;
    let digits = unit.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let scale = match &unit[digits.len()..] {
        "B" => 1.0,
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some(digits.trim().parse::<f64>().ok()? * scale)
}

/// `[info] xyz: Downloading 1 format(s): 401+251` -> 2 streams to fetch.
fn stream_count(line: &str) -> Option<usize> {
    let spec = line.split("format(s): ").nth(1)?;
    let n = spec.trim().split('+').filter(|s| !s.is_empty()).count();
    (n > 0).then_some(n)
}

/// A new stream is starting to download, as opposed to being post-processed.
fn stream_started(line: &str) -> bool {
    line.starts_with("[download] Destination: ")
}

/// Progress across every stream of one job.
///
/// A video is fetched as separate video and audio streams, each counting 0-100%
/// of itself. Keeping the highest number seen — what the Python version did with
/// `biggest_value` — parks the bar at 100% as soon as the first stream lands,
/// while the second one and then the merge are still to come.
#[derive(Default)]
pub struct Progress {
    /// Streams yt-dlp said it would fetch, when it said so.
    expected: usize,
    /// `[download] Destination:` lines seen so far.
    started: usize,
    /// Percent of the stream currently in flight.
    current: f32,
    /// Speed and ETA of that stream, as printed.
    rate: String,
    /// The same speed in bytes per second, for the sparkline.
    bps: f64,
}

impl Progress {
    pub fn feed(&mut self, line: &str) {
        if let Some(n) = stream_count(line) {
            self.expected = n;
        }
        if stream_started(line) {
            self.started += 1;
            self.current = 0.0;
            self.rate.clear();
            self.bps = 0.0;
        } else if line.ends_with("has already been downloaded") {
            self.started += 1;
            self.current = 100.0;
            self.rate.clear();
            self.bps = 0.0;
        } else if let Some(p) = percent(line) {
            self.current = p;
            self.rate = rate(line).unwrap_or_default();
            self.bps = speed_bps(line).unwrap_or(0.0);
        }
    }

    /// Speed and ETA of the stream in flight, empty when unknown.
    pub fn rate(&self) -> &str {
        &self.rate
    }

    /// Bytes per second of the stream in flight, 0 when unknown.
    pub fn bps(&self) -> f64 {
        self.bps
    }

    /// 0-100 over the whole job. Streams before the current one are complete,
    /// so the total is what yt-dlp announced, or however many have begun.
    pub fn percent(&self) -> f32 {
        let total = self.expected.max(self.started).max(1);
        let done = self.started.saturating_sub(1) as f32;
        ((done + self.current / 100.0) / total as f32 * 100.0).clamp(0.0, 100.0)
    }
}

/// Only what yt-dlp downloaded itself, as opposed to what a postprocessor
/// produced: with `--keep-video` the separate video and audio streams of a
/// merge are left behind, and they are ours to clean up.
pub fn download_destination(line: &str) -> Option<&str> {
    Some(line.strip_prefix("[download] Destination: ")?.trim())
}

/// `[Merger] Merging formats into "Rick.mkv"` -> `Rick.mkv`. The file the
/// streams were merged into, which is the video worth keeping.
pub fn merged_destination(line: &str) -> Option<&str> {
    Some(
        line.strip_prefix("[Merger] Merging formats into ")?
            .trim()
            .trim_matches('"'),
    )
}

/// `[download] Destination: foo.webm` / `[ffmpeg] Destination: foo.mp3` -> the filename.
/// Also the merged output, which is the name the user actually ends up with when
/// video and audio arrive as separate streams.
pub fn destination(line: &str) -> Option<&str> {
    for prefix in [
        "[download] Destination: ",
        "[ffmpeg] Destination: ",
        "[ExtractAudio] Destination: ",
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return Some(rest.trim());
        }
    }
    if let Some(rest) = line.strip_prefix("[Merger] Merging formats into ") {
        return Some(rest.trim().trim_matches('"'));
    }
    // Re-running a url yt-dlp already fetched prints no Destination line at all.
    line.strip_prefix("[download] ")?
        .strip_suffix(" has already been downloaded")
}

#[cfg(test)]
mod tests {
    use super::*;

    // One real read() off yt-dlp's stdout: progress separated by \r, no newlines.
    const CHUNK: &str = "[download] Destination: Rick Astley.webm\n\
                         [download]   0.0% of 3.50MiB at 1.00MiB/s ETA 00:03\r\
                         [download]  49.7% of 3.50MiB at 2.00MiB/s ETA 00:01\r\
                         [download] 100.0% of 3.50MiB in 00:02\r\n\
                         [ffmpeg] Destination: Rick Astley.mp3\n";

    #[test]
    fn splits_on_carriage_returns() {
        assert_eq!(split_lines(CHUNK).count(), 5);
    }

    #[test]
    fn scrapes_progress() {
        let vals: Vec<f32> = split_lines(CHUNK).filter_map(percent).collect();
        assert_eq!(vals, vec![0.0, 49.7, 100.0]);
        assert_eq!(percent("[download] Destination: x.webm"), None);
        // small/fast files report a whole percent with no decimal point
        assert_eq!(percent("[download] 100% of 3.50MiB"), Some(100.0));
        // a '%' inside a filename is not progress
        assert_eq!(percent("[download] Destination: 100% Real Mix.webm"), None);
        assert_eq!(percent("[info] 50% something"), None);
        // non-ascii right before the number must not split a char boundary
        assert_eq!(percent("[download] Τραγούδι50% of x"), None);
        assert_eq!(percent("[download] Ελληνικά 12.5% of x"), None);
    }

    #[test]
    fn scrapes_rate() {
        let rates: Vec<String> = split_lines(CHUNK).filter_map(rate).collect();
        assert_eq!(rates, vec!["1.00MiB/s · 00:03", "2.00MiB/s · 00:01"]);
        // the closing line has no ETA
        assert_eq!(rate("[download] 100.0% of 3.50MiB in 00:02"), None);
        assert_eq!(rate("[download] 12% of 1MiB at 500.00KiB/s"), Some("500.00KiB/s".into()));
        // " at " inside a filename is not a rate
        assert_eq!(rate("[ffmpeg] Destination: Look at Me.mp3"), None);
    }

    #[test]
    fn scrapes_speed_as_a_number() {
        assert_eq!(
            speed_bps("[download]  49.7% of 3.50MiB at 2.00MiB/s ETA 00:01"),
            Some(2.0 * 1024.0 * 1024.0)
        );
        assert_eq!(
            speed_bps("[download] 12% of 1MiB at 500.00KiB/s"),
            Some(500.0 * 1024.0)
        );
        assert_eq!(speed_bps("[download] 1% of 1MiB at 900.00B/s"), Some(900.0));
        // yt-dlp before it has a measurement, and the closing line with no speed
        assert_eq!(speed_bps("[download] 0.0% of 3.50MiB at Unknown B/s ETA Unknown"), None);
        assert_eq!(speed_bps("[download] 100.0% of 3.50MiB in 00:02"), None);
    }

    #[test]
    fn scrapes_destination() {
        let names: Vec<&str> = split_lines(CHUNK).filter_map(destination).collect();
        assert_eq!(names, vec!["Rick Astley.webm", "Rick Astley.mp3"]);
    }

    #[test]
    fn scrapes_extracted_audio() {
        // The mp3 preset downloads a webm and converts; the mp3 is the name
        // the user actually wants to see.
        let chunk = "[download] Destination: Song.webm\n\
                     [ExtractAudio] Destination: Song.mp3\n";
        assert_eq!(
            split_lines(chunk).filter_map(destination).last(),
            Some("Song.mp3")
        );
    }

    #[test]
    fn scrapes_already_downloaded() {
        assert_eq!(
            destination("[download] Adele - Hello.mp3 has already been downloaded"),
            Some("Adele - Hello.mp3")
        );
        assert_eq!(percent("[download] Adele - Hello.mp3 has already been downloaded"), None);
    }

    #[test]
    fn scrapes_merged_output() {
        // Separate video+audio streams: the last Destination is an intermediate
        // file, and the merged name only shows up on the [Merger] line.
        let chunk = "[download] Destination: Rick.f401.mp4\n\
                     [download] Destination: Rick.f251.webm\n\
                     [Merger] Merging formats into \"Rick.mkv\"\n";
        assert_eq!(split_lines(chunk).filter_map(destination).last(), Some("Rick.mkv"));
    }

    fn run(lines: &[&str]) -> Progress {
        let mut p = Progress::default();
        for line in lines {
            p.feed(line);
        }
        p
    }

    /// The reported bug: the bar read 100% while the job was still running,
    /// because a finished video stream is not a finished download.
    #[test]
    fn a_finished_stream_is_not_a_finished_job() {
        let head = ["[info] x: Downloading 1 format(s): 401+251"];
        let first = ["[download] Destination: V.f401.mp4", "[download]  50.0% of 393.77MiB"];
        let first_done = ["[download] Destination: V.f401.mp4", "[download] 100% of 393.77MiB in 00:01:15"];

        assert_eq!(run(&[&head[..], &first[..]].concat()).percent(), 25.0);
        // The moment that pinned the bar at 100%: half the job, not all of it.
        assert_eq!(run(&[&head[..], &first_done[..]].concat()).percent(), 50.0);

        // The second stream restarts at 0%, and the bar must not fall back.
        let second = ["[download] Destination: V.f251.webm", "[download]   0.0% of 3.27MiB"];
        assert_eq!(run(&[&head[..], &first_done[..], &second[..]].concat()).percent(), 50.0);

        let second_done = ["[download] Destination: V.f251.webm", "[download] 100% of 3.27MiB in 00:00:00"];
        assert_eq!(run(&[&head[..], &first_done[..], &second_done[..]].concat()).percent(), 100.0);
    }

    #[test]
    fn single_stream_maps_straight_through() {
        let p = run(&[
            "[info] x: Downloading 1 format(s): 251",
            "[download] Destination: Song.webm",
            "[download]  42.0% of 3.27MiB",
        ]);
        assert_eq!(p.percent(), 42.0);
    }

    #[test]
    fn counts_streams_even_when_yt_dlp_announces_none() {
        // No "format(s):" line: the streams that have begun are all we know.
        let p = run(&[
            "[download] Destination: a.mp4",
            "[download] 100% of 1.00MiB",
            "[download] Destination: b.webm",
            "[download]  50.0% of 1.00MiB",
        ]);
        assert_eq!(p.percent(), 75.0);
    }

    #[test]
    fn an_already_downloaded_stream_counts_as_complete() {
        let p = run(&[
            "[info] x: Downloading 1 format(s): 251",
            "[download] Song.mp3 has already been downloaded",
        ]);
        assert_eq!(p.percent(), 100.0);
    }
}
