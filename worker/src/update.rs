//! Self-update (not in embedded mode: the PC client updates the worker it
//! bundles). `welcome` may carry `update: { version, url, sigUrl }` for this
//! worker's os/arch; when `version` is not ours the worker downloads both
//! (https, at most 100 MB), checks the ed25519 signature over the whole file
//! against the key built in here, replaces its own executable and restarts,
//! but only while no job or chat is running.

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use ed25519_dalek::{Signature, VerifyingKey};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The release signing key (raw 32 bytes, base64).
pub const PUBLIC_KEY_B64: &str = "rb0dpBCbCZm059OQrIh6ydmOsXKcxzfmdDx8Mdowiec=";
/// The most an update download may be.
pub const MAX_DOWNLOAD: u64 = 100 * 1024 * 1024;
/// Set in the restarted worker's environment (it may wait for the old one's lock).
pub const RESTARTED_ENV: &str = "CRUNDI_WORKER_RESTARTED";

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

/// What the server offers in `welcome.update`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Offer {
    pub version: String,
    pub url: String,
    pub sig_url: String,
}

impl Offer {
    /// From `welcome.update`; None when absent, incomplete or not newer-or-different.
    pub fn from_welcome(v: &serde_json::Value, ours: &str) -> Option<Offer> {
        let u = v.get("update")?;
        let g = |k: &str| u.get(k).and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let o = Offer { version: g("version"), url: g("url"), sig_url: g("sigUrl") };
        (o.wanted(ours) && !o.url.is_empty() && !o.sig_url.is_empty()).then_some(o)
    }

    /// The server's version differs from ours (it decides what is current).
    pub fn wanted(&self, ours: &str) -> bool {
        !self.version.is_empty() && self.version != ours
    }
}

/// The built-in release key.
pub fn release_key() -> [u8; 32] {
    let b = B64.decode(PUBLIC_KEY_B64).expect("the built-in key is base64");
    b.try_into().expect("the built-in key is 32 bytes")
}

/// A signature file: 64 raw bytes, or their base64 (whitespace ignored).
pub fn parse_signature(sig: &[u8]) -> Result<Signature> {
    if sig.len() == 64 {
        return Ok(Signature::from_slice(sig)?);
    }
    let text: String = String::from_utf8_lossy(sig).chars().filter(|c| !c.is_whitespace()).collect();
    let raw = B64
        .decode(&text)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(text.trim_end_matches('=')))
        .map_err(|_| anyhow!("the signature is neither 64 bytes nor base64"))?;
    if raw.len() != 64 {
        bail!("the signature is {} bytes, not 64", raw.len());
    }
    Ok(Signature::from_slice(&raw)?)
}

/// Check `sig` over the whole of `data` with `key` (verify_strict).
pub fn verify_with(key: &[u8; 32], data: &[u8], sig: &[u8]) -> Result<()> {
    let vk = VerifyingKey::from_bytes(key).map_err(|e| anyhow!("bad public key: {e}"))?;
    let sig = parse_signature(sig)?;
    vk.verify_strict(data, &sig).map_err(|_| anyhow!("the signature does not match the file"))
}

/// Check with the built-in release key.
pub fn verify(data: &[u8], sig: &[u8]) -> Result<()> {
    verify_with(&release_key(), data, sig)
}

/// GET an https URL, at most `cap` bytes.
pub fn download(url: &str, cap: u64) -> Result<Vec<u8>> {
    if !url.to_ascii_lowercase().starts_with("https://") {
        bail!("refusing a non-https update address: {url}");
    }
    crate::util::install_crypto();
    let mut resp = ureq::get(url)
        .config()
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .call()
        .with_context(|| format!("downloading {url}"))?;
    if let Some(len) = resp.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok()) {
        if len > cap {
            bail!("{url} is {len} bytes, more than the {cap} allowed");
        }
    }
    let data = resp
        .body_mut()
        .with_config()
        .limit(cap)
        .read_to_vec()
        .with_context(|| format!("reading {url} (at most {cap} bytes)"))?;
    Ok(data)
}

/// Download the file and its signature and check them; returns the file.
pub fn fetch_verified(url: &str, sig_url: &str, key: &[u8; 32]) -> Result<Vec<u8>> {
    let data = download(url, MAX_DOWNLOAD)?;
    let sig = download(sig_url, 64 * 1024)?;
    verify_with(key, &data, &sig).context("refused the update")?;
    Ok(data)
}

/// `<exe>.old`, where Windows keeps the replaced executable until the next start.
pub fn old_path(exe: &Path) -> PathBuf {
    let mut s = exe.as_os_str().to_owned();
    s.push(".old");
    PathBuf::from(s)
}

/// Delete what an earlier update left (`<exe>.old`). Called at start.
pub fn cleanup_old(exe: &Path) {
    let _ = std::fs::remove_file(old_path(exe));
}

/// Replace the executable at `exe` with `data`, atomically.
/// Unix: write next to it, make it executable, rename over it (the running
/// process keeps the old inode). Windows: the running exe cannot be
/// overwritten but can be renamed: rename it to `.old`, write the new one,
/// and `.old` is deleted on the next start.
pub fn replace_exe(exe: &Path, data: &[u8]) -> Result<()> {
    if data.is_empty() {
        bail!("the update is empty");
    }
    let dir = exe.parent().ok_or_else(|| anyhow!("{} has no folder", exe.display()))?;
    let name = exe.file_name().ok_or_else(|| anyhow!("{} has no name", exe.display()))?.to_string_lossy().into_owned();
    let tmp = dir.join(format!(".{name}.new-{}", crate::util::random_hex(4)));
    let write = |p: &Path| -> Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(p).with_context(|| format!("writing {}", p.display()))?;
        f.write_all(data)?;
        f.sync_all()?;
        Ok(())
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let res = (|| -> Result<()> {
            write(&tmp)?;
            let mode = std::fs::metadata(exe).map(|m| m.permissions().mode() & 0o7777).unwrap_or(0o755) | 0o755;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))?;
            std::fs::rename(&tmp, exe).with_context(|| format!("replacing {}", exe.display()))?;
            Ok(())
        })();
        if res.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        res
    }
    #[cfg(not(unix))]
    {
        // Write first, so a failed download never leaves no executable.
        write(&tmp)?;
        let old = old_path(exe);
        let _ = std::fs::remove_file(&old);
        if let Err(e) = std::fs::rename(exe, &old) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e).with_context(|| format!("moving {} aside", exe.display()));
        }
        if let Err(e) = std::fs::rename(&tmp, exe) {
            let _ = std::fs::rename(&old, exe);
            let _ = std::fs::remove_file(&tmp);
            return Err(e).with_context(|| format!("putting the new {} in place", exe.display()));
        }
        Ok(())
    }
}

/// Download, verify and install over the running executable.
pub fn install(url: &str, sig_url: &str) -> Result<PathBuf> {
    let exe = std::env::current_exe().context("finding this executable")?;
    let data = fetch_verified(url, sig_url, &release_key())?;
    replace_exe(&exe, &data)?;
    Ok(exe)
}

/// Start the (new) executable with the same arguments in place of this one.
/// Unix: exec. Windows: start it detached and exit.
pub fn restart() -> ! {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("crundi-worker"));
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let e = std::process::Command::new(&exe).args(&args).env(RESTARTED_ENV, "1").exec();
        eprintln!("crundi-worker: could not restart {}: {e}", exe.display());
        std::process::exit(1);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        let r = std::process::Command::new(&exe)
            .args(&args)
            .env(RESTARTED_ENV, "1")
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        if let Err(e) = r {
            eprintln!("crundi-worker: could not restart {}: {e}", exe.display());
            std::process::exit(1);
        }
        std::process::exit(0);
    }
    #[cfg(not(any(unix, windows)))]
    {
        std::process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_built_in_key_is_a_key() {
        assert!(VerifyingKey::from_bytes(&release_key()).is_ok());
    }

    #[test]
    fn offers() {
        let w = serde_json::json!({"update": {"version": "1.2.3", "url": "https://x/a", "sigUrl": "https://x/a.sig"}});
        assert!(Offer::from_welcome(&w, "1.2.3").is_none(), "same version");
        let o = Offer::from_welcome(&w, "1.2.2").unwrap();
        assert_eq!(o.sig_url, "https://x/a.sig");
        assert!(Offer::from_welcome(&serde_json::json!({}), "1").is_none());
        assert!(Offer::from_welcome(&serde_json::json!({"update": {"version": "2"}}), "1").is_none());
    }

    #[test]
    fn non_https_is_refused() {
        assert!(download("http://127.0.0.1:1/x", 10).unwrap_err().to_string().contains("non-https"));
    }
}
