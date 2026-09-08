//! yt-dlp version reporting, and the HTTP bits the bootstrap uses.
//! The Python version's `update` button called youtube-dl's dead self-updater
//! and blocked the GUI on os.popen().read(); nothing here runs on the UI thread.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const LATEST_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest";

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
    let resp = agent(0, Some(15)).get(LATEST_URL).call().ok()?;
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

/// Where a freshly downloaded tool should land: next to the running exe.
pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
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
