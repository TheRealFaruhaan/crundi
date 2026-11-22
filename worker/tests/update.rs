//! Self-update: the signature check (a test key) and replacing a file.

use crundi_worker::update::{parse_signature, replace_exe, verify, verify_with};
use ed25519_dalek::{Signer, SigningKey};
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("cw-update-{name}-{}", crundi_worker::util::random_hex(6)));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn test_key() -> SigningKey {
    SigningKey::from_bytes(&[7u8; 32])
}

#[test]
fn a_good_signature_is_accepted_raw_or_base64() {
    use base64::Engine;
    let k = test_key();
    let data = b"the new worker binary".repeat(1000);
    let sig = k.sign(&data).to_bytes();
    let public = k.verifying_key().to_bytes();
    verify_with(&public, &data, &sig).unwrap();
    let b64 = format!("{}\r\n", base64::engine::general_purpose::STANDARD.encode(sig));
    verify_with(&public, &data, b64.as_bytes()).unwrap();
    assert!(parse_signature(b"not a signature").is_err());
}

#[test]
fn a_tampered_file_or_another_key_is_refused() {
    let k = test_key();
    let data = b"the new worker binary".to_vec();
    let sig = k.sign(&data).to_bytes();
    let public = k.verifying_key().to_bytes();
    let mut bad = data.clone();
    bad[3] ^= 1;
    assert!(verify_with(&public, &bad, &sig).unwrap_err().to_string().contains("does not match"));
    let mut longer = data.clone();
    longer.push(0);
    assert!(verify_with(&public, &longer, &sig).is_err());
    // Signed with the test key: the release key refuses it.
    assert!(verify(&data, &sig).is_err());
}

#[test]
fn replacing_the_executable() {
    let d = tmp("replace");
    let exe = d.join(if cfg!(windows) { "crundi-worker.exe" } else { "crundi-worker" });
    std::fs::write(&exe, b"old").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    replace_exe(&exe, b"new version").unwrap();
    assert_eq!(std::fs::read(&exe).unwrap(), b"new version");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&exe).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode & 0o111, 0o111, "executable: {mode:o}");
    }
    // Nothing left beside it but (Windows) the old one, cleaned up at the next start.
    let mut names: Vec<String> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    if cfg!(windows) {
        assert_eq!(names, ["crundi-worker.exe", "crundi-worker.exe.old"]);
        crundi_worker::update::cleanup_old(&exe);
        assert!(!crundi_worker::update::old_path(&exe).exists());
    } else {
        assert_eq!(names, ["crundi-worker"]);
    }
    // Again (an .old from before is replaced), and an empty file is refused.
    replace_exe(&exe, b"newer").unwrap();
    assert_eq!(std::fs::read(&exe).unwrap(), b"newer");
    assert!(replace_exe(&exe, b"").is_err());
    assert_eq!(std::fs::read(&exe).unwrap(), b"newer");
    std::fs::remove_dir_all(&d).ok();
}
