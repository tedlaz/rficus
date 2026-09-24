//! yt-dlp version reporting, and the HTTP bits the bootstrap uses.
//! The Python version's `update` button called youtube-dl's dead self-updater
//! and blocked the GUI on os.popen().read(); nothing here runs on the UI thread.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const LATEST_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest";
/// rficus itself, for the About window.
pub const REPO: &str = "https://github.com/tedlaz/rficus";

/// Schannel via native-tls, trusting the Windows certificate store. The rustls
/// stack plus its bundled roots would be a large share of this binary; the
/// provider is never picked up from the feature flag alone.
///
/// `timeout_secs` is the deadline for the whole call, so it must be `None` for
/// a download: a fixed budget would abort a large file on a slow line.
fn agent(max_redirects: u32, timeout_secs: Option<u64>) -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .tls_config(tls)
        .max_redirects(max_redirects)
        .http_status_as_error(false)
        .timeout_global(timeout_secs.map(std::time::Duration::from_secs))
        .build()
        .new_agent()
}

/// `yt-dlp --version`. No network, returns fast.
pub fn local_version(exe: &Path) -> Result<String, String> {
    let out = crate::job::command(&exe.to_path_buf())
        .arg("--version")
        .output()
        .map_err(|e| e.to_string())?;
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if v.is_empty() {
        Err("no version output".into())
    } else {
        Ok(v)
    }
}

/// GitHub redirects /releases/latest to /releases/tag/<version>; with redirects
/// disabled the tag is right there in the Location header, so no JSON dep.
/// ponytail: scraping the redirect. If GitHub changes its shape, switch to
/// /repos/yt-dlp/yt-dlp/releases/latest + serde_json.
pub fn latest_version() -> Option<String> {
    latest_tag(LATEST_URL)
}

/// The tag `url` (a GitHub `/releases/latest`) redirects to.
fn latest_tag(url: &str) -> Option<String> {
    let resp = agent(0, Some(15)).get(url).call().ok()?;
    let loc = resp.headers().get("location")?.to_str().ok()?;
    let tag = loc.rsplit('/').next()?.trim();
    (!tag.is_empty()).then(|| tag.to_string())
}

/// Stream a url to `dest`, reporting (bytes_done, total) as it goes.
///
/// Writes to a sibling `.part` file and renames on success, so an interrupted
/// download never leaves something that looks like a working exe.
pub fn download(
    url: &str,
    dest: &Path,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let resp = agent(10, None).get(url).call().map_err(|e| e.to_string())?;
    if resp.status() != 200 {
        return Err(format!("{url} returned HTTP {}", resp.status()));
    }
    let total = resp
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    let part = dest.with_extension("part");
    if let Some(parent) = part.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut reader = resp.into_body().into_reader();
    let mut buf = vec![0u8; 64 * 1024];
    let mut done = 0u64;
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                return Err(e.to_string());
            }
        };
        if let Err(e) = file.write_all(&buf[..n]) {
            let _ = std::fs::remove_file(&part);
            return Err(e.to_string());
        }
        done += n as u64;
        progress(done, total);
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    // Windows will not rename onto an existing file.
    let _ = std::fs::remove_file(dest);
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}

/// Is this URL still served? A HEAD, so it costs nothing even for a 65 MB zip.
///
/// Only the tests that guard the bootstrap's download URLs use it, so it is not
/// compiled into the app: those URLs are the one part of this app that other
/// people can break without touching the repo, but checking them is not
/// something the app itself ever needs to do.
#[cfg(test)]
pub fn head_ok(url: &str) -> Result<(), String> {
    let resp = agent(10, Some(30))
        .head(url)
        .call()
        .map_err(|e| e.to_string())?;
    if resp.status() == 200 {
        Ok(())
    } else {
        Err(format!("{url} returned HTTP {}", resp.status()))
    }
}

/// The newest rficus release, if it is newer than this build.
pub fn newer_app_version() -> Result<Option<String>, String> {
    let tag = latest_tag(&format!("{REPO}/releases/latest")).ok_or("could not reach GitHub")?;
    let latest = tag.trim_start_matches('v');
    match is_newer(latest, env!("CARGO_PKG_VERSION")) {
        Some(newer) => Ok(newer.then(|| latest.to_owned())),
        None => Err(format!("unexpected release tag {tag}")),
    }
}

/// `0.1.10` beats `0.1.9`: compared as numbers, not text.
fn is_newer(latest: &str, current: &str) -> Option<bool> {
    let parse = |v: &str| -> Option<Vec<u32>> { v.split('.').map(|n| n.parse().ok()).collect() };
    Some(parse(latest)? > parse(current)?)
}

/// Swap the running exe for the release's portable `rficus.exe`. Windows lets
/// a running exe be renamed, not overwritten, so it steps aside as `.old`
/// (removed on the next start) and the new one takes its name. Works the same
/// for an installed copy and a portable one; an installed copy's Add/Remove
/// Programs entry keeps showing the old version until the next installer run.
pub fn replace_self(version: &str, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let new = exe.with_extension("new");
    download(&format!("{REPO}/releases/download/v{version}/rficus.exe"), &new, progress)?;
    let old = exe.with_extension("old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(&exe, &old).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::rename(&new, &exe) {
        let _ = std::fs::rename(&old, &exe);
        return Err(e.to_string());
    }
    Ok(())
}

/// What `replace_self` left behind. Fails harmlessly while the previous
/// process is still exiting; the next start gets it.
pub fn remove_old_self() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(exe.with_extension("old"));
    }
}

/// Where a freshly downloaded tool should land: next to the running exe.
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    #[test]
    fn compares_versions_as_numbers() {
        use super::is_newer;
        assert_eq!(is_newer("0.1.10", "0.1.9"), Some(true));
        assert_eq!(is_newer("0.2.0", "0.1.2"), Some(true));
        assert_eq!(is_newer("0.1.2", "0.1.2"), Some(false));
        assert_eq!(is_newer("0.1.1", "0.1.2"), Some(false));
        assert_eq!(is_newer("releases", "0.1.2"), None);
    }

    /// Hits the network, so it is ignored by default:
    /// `cargo test -- --ignored`. Proves the TLS provider is wired up, which
    /// a successful compile does not.
    #[test]
    #[ignore]
    fn fetches_latest_version() {
        let v = super::latest_version().expect("no version from github");
        assert!(v.starts_with("20"), "unexpected tag: {v}");
    }
}
