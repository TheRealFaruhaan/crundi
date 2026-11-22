//! File work for jobs (all blocking; call from spawn_blocking): the copy
//! source's manifest and tar extraction, and the artifact .tar.gz.

use anyhow::{bail, Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Folder names never part of a copy (at any depth).
pub const EXCLUDED_DIRS: &[&str] = &[".git", "node_modules", "target", "dist", "build", ".next", "__pycache__"];
/// The manifest cache kept inside the copy folder.
pub const MANIFEST_CACHE: &str = ".crundi-manifest.json";
/// Artifacts are capped at 2 GB (before compression).
pub const ARTIFACT_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

/// Is `rel` (a '/'-separated relative path) safe: no root, no `..`, not empty?
pub fn safe_rel(rel: &str) -> Option<PathBuf> {
    if rel.is_empty() || rel.contains('\0') {
        return None;
    }
    let p = Path::new(rel);
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

fn rel_string(p: &Path) -> String {
    p.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// `.crundiignore`: one glob per line (`#` comments). A line matches the path
/// or any of its parent folders; a bare name matches at any depth.
pub fn load_ignore(dir: &Path) -> GlobSet {
    let mut b = GlobSetBuilder::new();
    if let Ok(text) = std::fs::read_to_string(dir.join(".crundiignore")) {
        for line in text.lines() {
            let l = line.trim().trim_end_matches('/');
            let l = l.strip_prefix('/').unwrap_or(l);
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            for pat in [l.to_string(), format!("**/{l}")] {
                if let Ok(g) = Glob::new(&pat) {
                    b.add(g);
                }
            }
        }
    }
    b.build().unwrap_or_else(|_| GlobSet::empty())
}

fn ignored(rel: &str, ignore: &GlobSet) -> bool {
    if rel == MANIFEST_CACHE {
        return true;
    }
    let parts: Vec<&str> = rel.split('/').collect();
    if parts.iter().any(|p| EXCLUDED_DIRS.contains(p)) {
        return true;
    }
    if ignore.is_empty() {
        return false;
    }
    // The path or any parent folder.
    (1..=parts.len()).any(|n| ignore.is_match(parts[..n].join("/")))
}

/// Walk `dir`, calling `f(rel, full, metadata)` for files and symlinks
/// (symlinks are not followed). `skip_dir(rel)` prunes folders.
fn walk(
    dir: &Path,
    skip_dir: &dyn Fn(&str) -> bool,
    f: &mut dyn FnMut(&str, &Path, &std::fs::Metadata) -> Result<()>,
) -> Result<()> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let full = e.path();
            let Ok(md) = std::fs::symlink_metadata(&full) else { continue };
            let rel = rel_string(full.strip_prefix(dir).unwrap_or(&full));
            if md.is_dir() {
                if !skip_dir(&rel) {
                    stack.push(full);
                }
            } else {
                f(&rel, &full, &md)?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    /// path -> [size, mtime ns, sha256]
    files: BTreeMap<String, (u64, u128, String)>,
}

fn mtime_ns(md: &std::fs::Metadata) -> u128 {
    md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0)
}

fn sha256_file(p: &Path) -> Result<String> {
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}

fn sha256_bytes(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}

/// The manifest of a copy folder: path -> (size, sha256 hex). Only files
/// whose size or mtime changed since the cached manifest are hashed again.
/// Symlinks are listed with the hash of their target text.
pub fn manifest(dir: &Path) -> Result<BTreeMap<String, (u64, String)>> {
    std::fs::create_dir_all(dir)?;
    let cache_path = dir.join(MANIFEST_CACHE);
    let old: Cache = std::fs::read(&cache_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let ignore = load_ignore(dir);
    let mut new = Cache::default();
    let mut out = BTreeMap::new();
    walk(dir, &|rel| ignored(rel, &ignore), &mut |rel, full, md| {
        if ignored(rel, &ignore) {
            return Ok(());
        }
        let (size, sha) = if md.file_type().is_symlink() {
            let t = std::fs::read_link(full)?;
            let t = t.to_string_lossy().into_owned();
            (t.len() as u64, sha256_bytes(t.as_bytes()))
        } else {
            let size = md.len();
            let mt = mtime_ns(md);
            match old.files.get(rel) {
                Some((s, m, h)) if *s == size && *m == mt && mt != 0 => (size, h.clone()),
                _ => (size, sha256_file(full)?),
            }
        };
        new.files.insert(rel.to_string(), (size, mtime_ns(md), sha.clone()));
        out.insert(rel.to_string(), (size, sha));
        Ok(())
    })?;
    if let Ok(b) = serde_json::to_vec(&new) {
        let _ = std::fs::write(&cache_path, b);
    }
    Ok(out)
}

/// Seed a new copy folder `to` from an existing one `from`: the files a
/// manifest would list (build folders and ignored paths are left out), with
/// their modification times, plus the manifest cache, so the first manifest
/// of `to` hashes nothing again and the server sends only what changed.
/// Returns the files copied. Symlinks are recreated on Unix, skipped elsewhere.
pub fn seed_copy(from: &Path, to: &Path) -> Result<u64> {
    std::fs::create_dir_all(to)?;
    let ignore = load_ignore(from);
    let mut n = 0;
    walk(from, &|rel| ignored(rel, &ignore), &mut |rel, full, md| {
        if ignored(rel, &ignore) {
            return Ok(());
        }
        let Some(r) = safe_rel(rel) else { return Ok(()) };
        let dest = to.join(r);
        if let Some(p) = dest.parent() {
            std::fs::create_dir_all(p)?;
        }
        if md.file_type().is_symlink() {
            #[cfg(unix)]
            {
                let target = std::fs::read_link(full)?;
                let _ = std::fs::remove_file(&dest);
                std::os::unix::fs::symlink(target, &dest)?;
                n += 1;
            }
            return Ok(());
        }
        std::fs::copy(full, &dest).with_context(|| format!("copying {rel}"))?;
        if let Ok(t) = md.modified() {
            if let Ok(f) = std::fs::OpenOptions::new().write(true).open(&dest) {
                let _ = f.set_modified(t);
            }
        }
        n += 1;
        Ok(())
    })?;
    let cache = from.join(MANIFEST_CACHE);
    if cache.is_file() {
        let _ = std::fs::copy(&cache, to.join(MANIFEST_CACHE));
    }
    Ok(n)
}

/// Extract an (uncompressed) tar into `dir`, keeping exec bits and symlinks.
/// Entries that would land outside `dir` are refused. Returns files written.
pub fn extract_tar(tar_path: &Path, dir: &Path) -> Result<u64> {
    std::fs::create_dir_all(dir)?;
    let f = std::fs::File::open(tar_path)?;
    let mut ar = tar::Archive::new(f);
    ar.set_preserve_permissions(true);
    ar.set_preserve_mtime(true);
    ar.set_overwrite(true);
    let mut n = 0;
    for entry in ar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().into_owned();
        let Some(rel) = safe_rel(&path) else { bail!("refused a path outside the folder: {path}") };
        if rel_string(&rel) == MANIFEST_CACHE {
            continue;
        }
        let kind = entry.header().entry_type();
        if kind.is_hard_link() {
            bail!("refused a hard link in the copy: {path}");
        }
        // A file replacing a symlink (or the other way round): remove the old one first,
        // so writing never follows a link.
        let dest = dir.join(&rel);
        if let Ok(md) = std::fs::symlink_metadata(&dest) {
            if md.file_type().is_symlink() || (!md.is_dir() && kind.is_dir()) {
                let _ = std::fs::remove_file(&dest);
            }
        }
        if !entry.unpack_in(dir).with_context(|| format!("extracting {path}"))? {
            bail!("refused a path outside the folder: {path}");
        }
        if !kind.is_dir() {
            n += 1;
        }
    }
    Ok(n)
}

/// Remove paths (files or symlinks) from the copy; refuses unsafe paths.
pub fn remove_paths(dir: &Path, removed: &[String]) -> Result<u64> {
    let mut n = 0;
    for r in removed {
        let Some(rel) = safe_rel(r) else { bail!("refused to remove a path outside the folder: {r}") };
        let p = dir.join(rel);
        match std::fs::symlink_metadata(&p) {
            Ok(md) if md.is_dir() => {
                std::fs::remove_dir_all(&p)?;
                n += 1;
            }
            Ok(_) => {
                std::fs::remove_file(&p)?;
                n += 1;
            }
            Err(_) => {}
        }
        // Drop folders left empty.
        let mut parent = p.parent();
        while let Some(d) = parent {
            if d == dir || std::fs::remove_dir(d).is_err() {
                break;
            }
            parent = d.parent();
        }
    }
    Ok(n)
}

/// Pack files matching `globs` (relative to `dir`; `.git` skipped) into a
/// .tar.gz at `out`. Returns (files, bytes of the .tar.gz).
pub fn pack_artifacts(dir: &Path, globs: &[String], out: &Path) -> Result<(u64, u64, Vec<String>)> {
    let mut notes = Vec::new();
    let mut b = GlobSetBuilder::new();
    for g in globs {
        let g = g.trim().trim_start_matches("./");
        if g.is_empty() {
            continue;
        }
        match Glob::new(g) {
            Ok(glob) => {
                b.add(glob);
            }
            Err(e) => notes.push(format!("bad artifact pattern {g}: {e}")),
        }
        // `dist` or `dist/` also means everything under it.
        if let Ok(glob) = Glob::new(&format!("{}/**", g.trim_end_matches('/'))) {
            b.add(glob);
        }
    }
    let set = b.build()?;
    let mut files: Vec<(String, PathBuf, u64)> = Vec::new();
    if !globs.is_empty() {
        walk(dir, &|rel| rel == ".git" || rel.ends_with("/.git"), &mut |rel, full, md| {
            if set.is_match(rel) {
                files.push((rel.to_string(), full.to_path_buf(), md.len()));
            }
            Ok(())
        })?;
    }
    files.sort();
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let f = std::fs::File::create(out)?;
    let gz = flate2::write::GzEncoder::new(f, flate2::Compression::default());
    let mut tb = tar::Builder::new(gz);
    tb.follow_symlinks(false);
    let mut total = 0u64;
    let mut count = 0u64;
    for (rel, full, size) in files {
        if total + size > ARTIFACT_LIMIT {
            notes.push(format!("artifacts over 2 GB: {rel} and later files left out"));
            break;
        }
        tb.append_path_with_name(&full, &rel).with_context(|| format!("packing {rel}"))?;
        total += size;
        count += 1;
    }
    let gz = tb.into_inner()?;
    gz.finish()?;
    let bytes = std::fs::metadata(out)?.len();
    Ok((count, bytes, notes))
}

/// An uncompressed tar of `files` (relative paths under `dir`; symlinks kept
/// as links) at `out`. Paths that are gone or unsafe are skipped.
pub fn pack_tar(dir: &Path, files: &[String], out: &Path) -> Result<u64> {
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let f = std::fs::File::create(out)?;
    let mut tb = tar::Builder::new(f);
    tb.follow_symlinks(false);
    let mut n = 0;
    for rel in files {
        let Some(r) = safe_rel(rel) else { continue };
        let full = dir.join(r);
        if std::fs::symlink_metadata(&full).is_err() {
            continue;
        }
        tb.append_path_with_name(&full, rel).with_context(|| format!("packing {rel}"))?;
        n += 1;
    }
    tb.into_inner()?.sync_all()?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_paths() {
        assert!(safe_rel("a/b.txt").is_some());
        assert!(safe_rel("./a").is_some());
        assert!(safe_rel("../a").is_none());
        assert!(safe_rel("a/../../b").is_none());
        assert!(safe_rel("/etc/passwd").is_none());
        assert!(safe_rel("").is_none());
    }

    #[test]
    fn excluded() {
        let empty = GlobSet::empty();
        assert!(ignored("node_modules/x/y.js", &empty));
        assert!(ignored("a/target/debug/x", &empty));
        assert!(ignored(".git", &empty));
        assert!(!ignored("src/main.rs", &empty));
        let mut b = GlobSetBuilder::new();
        b.add(Glob::new("*.log").unwrap());
        b.add(Glob::new("**/*.log").unwrap());
        b.add(Glob::new("secret").unwrap());
        b.add(Glob::new("**/secret").unwrap());
        let set = b.build().unwrap();
        assert!(ignored("x/y.log", &set));
        assert!(ignored("a/secret/k", &set));
        assert!(!ignored("a/b.txt", &set));
    }
}
