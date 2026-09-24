# rficus

Rust rewrite of the download half of [ficus](https://github.com/tedlaz/ficus) —
a GUI for [yt-dlp](https://github.com/yt-dlp/yt-dlp), built with
[egui](https://github.com/emilk/egui).

Drag a link straight from the browser onto the window, or paste urls, pick a
format preset and a save path, and each url runs as its own yt-dlp process with
a live progress row. Right-click a row to stop, restart or remove it.

## Drag and drop

winit — the windowing layer under egui — registers an OLE drop target that asks
only for `CF_HDROP`, so it rejects a dragged link before egui ever sees it.
`dnd.rs` replaces that target with one that also reads `UniformResourceLocatorW`
(what a browser puts on a dragged link) and `CF_UNICODETEXT` (dragged text), and
still handles `CF_HDROP` so dropping a `.txt` or `.url` file of links keeps
working. Whatever is dropped lands in the url box.

Settings live in `ficus.ini` next to the exe, in the same format the Python
version used, so an existing ini keeps working.

## Tools

rficus drives three executables: `yt-dlp.exe`, plus `ffmpeg.exe` and
`ffprobe.exe` — yt-dlp needs those two to merge separate video and audio streams
and to write mp3, and without them a normal download dies with `WinError 2` at
the merge step.

You do not have to install any of them. At startup, on a worker thread so the
window stays responsive, rficus:

1. checks each one next to `rficus.exe` actually *runs*,
   not merely that the file is there — a shared ffmpeg missing a library exists
   and cannot start,
2. downloads whatever is missing into the folder holding `rficus.exe`,
3. compares yt-dlp against the latest release and runs `yt-dlp -U` if it is
   behind.

The header shows what it is doing, with a progress bar while bytes are moving,
and the yt-dlp version once it settles. Anything that fails is reported there in
red and does not stop the rest. `Check tools` runs the whole thing again on
demand.

Set `check_updates=false` under `[General]` to skip the network entirely; a
missing tool is still downloaded, since the app cannot work without it.

A first run therefore pulls about 82 MB and leaves ~146 MB installed. Bundle the
executables next to `rficus.exe` if you would rather install
offline — anything already present and working is left alone.

### Which ffmpeg

The **shared LGPL** build from
[BtbN](https://github.com/BtbN/FFmpeg-Builds), which is the smallest one that
still does the job. Shared means one copy of the libraries serves both exes
rather than each statically linking its own, and LGPL drops GPL-only encoders
nothing here uses. `ffplay.exe`, 17 MB of player, is not copied.

| Build | Download | On disk |
|---|---|---|
| win64-lgpl-shared *(used)* | 65 MB | 129 MB |
| gyan.dev static "essentials" | 106 MB | 205 MB |
| win64-gpl-shared | 73 MB | 161 MB |
| win64-gpl static | 163 MB | 290 MB |

Checked against what yt-dlp actually asks of ffmpeg before switching: merging by
stream copy, writing mp3 through `libmp3lame`, embedding a thumbnail at
`id3v2_version 3`, and ffprobe reading the result back.

## Developer

### Build

```
cargo build --release
```

The result is a single self-contained `target/release/rficus.exe`, about 4.8 MB.
`installer.iss` builds the Inno Setup installer around it.

The installer ships the exe alone — about 5 MB rather than the ~200 MB it would
be with yt-dlp and ffmpeg inside — and the app fetches those on first run, which
also means a fresh install starts with current ones. So the first launch needs a
network connection; to install offline, drop the executables next to
`rficus.exe` afterwards and it will leave them alone.

It installs per-user under `%USERPROFILE%` with `PrivilegesRequired=lowest`, on
purpose: rficus writes the downloaded tools into its own folder, which it could
not do from an elevated install under Program Files.

`cargo test` runs the output parsers. The GitHub version check is a network
test, kept out of the normal run: `cargo test -- --ignored`.

### Cutting a release

The version lives in `Cargo.toml` and nowhere else. The tag is derived from it
and `installer.iss` receives it as `/DMyAppVersion`, so there is no second place
to forget.

Once per machine:

```
cargo install cargo-release
```

Then, from a clean `main`:

```
cargo release patch --execute     # 0.1.1 -> 0.1.2; also minor / major
```

That bumps `Cargo.toml` and `Cargo.lock`, commits as `Release X.Y.Z`, tags
`vX.Y.Z` and pushes the branch and the tag. `release.toml` holds the settings —
notably `publish = false`, since this is an application and has no business on
crates.io, and `allow-branch`, which pins releases to `main`.

To release the version already in the manifest rather than bumping, name it:

```
cargo release 0.1.1 --execute
```

Pushing the tag starts `.github/workflows/release.yml` on a Windows runner,
which:

1. refuses the run if the tag and `Cargo.toml` disagree — otherwise the
   installer filename, the Add/Remove Programs entry and the exe resource would
   each claim a different version,
2. runs `cargo test --locked` and `cargo build --release --locked`,
3. compiles the installer with Inno Setup, which the runner already has,
4. publishes a release with generated notes and two artifacts:
   `setup_rficus.X.Y.Z.exe` and the portable `rficus.exe`.

Watch it with `gh run watch`, or the Actions tab.

Without cargo-release, the same thing by hand:

```
# edit version in Cargo.toml, then
cargo check                      # refresh Cargo.lock
git commit -am "Release 0.2.0"
git tag v0.2.0
git push --follow-tags
```

Two things that bite:

- **Tags must be plain `vX.Y.Z`.** A pre-release tag such as `v0.2.0-rc1` can
  never equal the manifest version, so the guard rejects it.
- **A failed run leaves the tag behind.** Fix the cause, then delete the tag
  locally and remotely before retrying, or the workflow has nothing new to
  trigger on:

  ```
  git tag -d v0.2.0
  git push origin :v0.2.0
  ```

The installer is unsigned, so SmartScreen warns until a download builds enough
reputation.

### Where the size went

Down from 18 MB to 4.4 MB, in order of what mattered — the icon resource below
then puts it back to 4.8 MB:

| Change | Saved |
|---|---|
| `opt-level="z"`, fat LTO, 1 codegen unit, `panic="abort"`, `strip` | 7.6 MB |
| No eframe, no GPU: winit + softbuffer, egui rasterized on the CPU by a vendored `egui_software_backend` | — |
| System fonts instead of eframe's bundled ~1.4 MB (`default_fonts` off) | 1.3 MB |
| No `regex`; the one progress pattern is a dozen lines of `str` | 1.6 MB |
| `ureq` on native-tls (schannel) instead of rustls + ring + webpki roots | 0.1 MB |
| 64px `icon.png` instead of the 448px, 125 KB `ficus.png` | 0.1 MB |

Fonts come from `%SystemRoot%\Fonts` — Segoe UI, falling back to Tahoma, Arial
and Verdana, with Consolas or Courier for monospace. Segoe UI covers Greek and
Cyrillic better than the font eframe bundles.

`build.rs` embeds `ficus.ico` as the exe's icon resource, which is what Explorer
and the taskbar show; that is the 410 KB the binary sits above 4.4 MB. The
window icon is separate, set at runtime from the 64px `icon.png`. All eight
sizes in `ficus.ico` are uncompressed BMP — re-encoding them as PNG (which
Windows has read since Vista) would roughly halve that cost.

## Not ported

The Python version's second tab (mp3 + image to video, via ffmpeg and Pillow)
is not part of this rewrite.


