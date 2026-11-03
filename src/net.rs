//! Downloads, with `curl` (shipped with Windows 10 and later, macOS and Linux), so the binary needs no TLS
//! stack of its own. Files go to a `.part` file first and are renamed when complete.

use std::fs;
use std::path::Path;
use std::process::Command;

/// Download `url` to `dest`. `accept` sets the Accept header (for servers that negotiate the format).
pub fn fetch(url: &str, dest: &Path, accept: Option<&str>) -> Result<(), String> {
    fetch_capped(url, dest, accept, 0)
}

/// As [`fetch`], refusing files larger than `max_mb` megabytes (0: no limit).
pub fn fetch_capped(
    url: &str,
    dest: &Path,
    accept: Option<&str>,
    max_mb: u64,
) -> Result<(), String> {
    if let Some(dir) = dest.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let part = dest.with_extension(match dest.extension() {
        Some(e) => format!("{}.part", e.to_string_lossy()),
        None => "part".into(),
    });
    eprintln!("ontoreport: downloading {url}");
    let mut cmd = Command::new("curl");
    cmd.args([
        "-fL",
        "--retry",
        "3",
        "--silent",
        "--show-error",
        "--max-time",
        "600",
        "-o",
    ])
    .arg(&part)
    .arg(url);
    if let Some(a) = accept {
        cmd.arg("-H").arg(format!("Accept: {a}"));
    }
    if max_mb > 0 {
        cmd.arg("--max-filesize")
            .arg((max_mb * 1024 * 1024).to_string());
    }
    if url.starts_with("https://huggingface.co/") {
        if let Ok(token) = std::env::var("HF_TOKEN") {
            cmd.arg("-H").arg(format!("Authorization: Bearer {token}"));
        }
    }
    let status = cmd.status().map_err(|e| {
        format!(
            "could not run curl ({e}); download {url} to {} by hand",
            dest.display()
        )
    })?;
    if !status.success() {
        let _ = fs::remove_file(&part);
        // curl exits with 63 when the file is larger than --max-filesize.
        if status.code() == Some(63) {
            return Err(format!(
                "{url} is larger than {max_mb} MB; raise [releases] max_mb in the configuration to fetch it"
            ));
        }
        return Err(format!("downloading {url} failed ({status})"));
    }
    fs::rename(&part, dest).map_err(|e| format!("{}: {e}", dest.display()))
}
