//! Start at login (specs/desktop/inbox-tray-bubble.md A.8; app/main.js
//! 1202-1235): the OS is the source of truth. Linux: the autostart entry
//! ($XDG_CONFIG_HOME or ~/.config)/autostart/Crundi.desktop, present = on.
//! Windows: the per-user Run key. macOS: a LaunchAgent. Settings' "Launch
//! at startup" row (Operations) reads `enabled` and calls `set`.

use std::path::PathBuf;

const NAME: &str = "Crundi";

fn exe() -> String {
    std::env::var("APPIMAGE").ok().filter(|s| !s.is_empty())
        .or_else(|| std::env::current_exe().ok().map(|p| p.to_string_lossy().to_string()))
        .unwrap_or_default()
}

/// The Exec line's quoting (A.8: escape " \ ` $).
pub fn quote(p: &str) -> String {
    let mut s = String::from('"');
    for c in p.chars() { if matches!(c, '"' | '\\' | '`' | '$') { s.push('\\'); } s.push(c); }
    s.push('"');
    s
}

/// The autostart entry's text (Linux).
pub fn desktop_entry(exe: &str) -> String {
    format!("[Desktop Entry]\nType=Application\nName={NAME}\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n", quote(exe))
}

fn linux_file() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("autostart").join(format!("{NAME}.desktop"))
}

fn mac_file() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join("Library/LaunchAgents/com.crundi.client.plist")
}

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

/// Does the client start at login now?
pub fn enabled() -> bool {
    if cfg!(target_os = "windows") {
        std::process::Command::new("reg").args(["query", RUN_KEY, "/v", NAME]).output().is_ok_and(|o| o.status.success())
    } else if cfg!(target_os = "macos") {
        mac_file().exists()
    } else {
        linux_file().exists()
    }
}

/// Turn starting at login on or off; the new state, read back from the OS.
pub fn set(on: bool) -> Result<bool, String> {
    let exe = exe();
    if cfg!(target_os = "windows") {
        let st = if on {
            std::process::Command::new("reg").args(["add", RUN_KEY, "/v", NAME, "/t", "REG_SZ", "/d", &format!("\"{exe}\""), "/f"]).status()
        } else {
            std::process::Command::new("reg").args(["delete", RUN_KEY, "/v", NAME, "/f"]).status()
        };
        st.map_err(|e| e.to_string())?;
    } else if cfg!(target_os = "macos") {
        let f = mac_file();
        if on {
            let plist = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\"><dict><key>Label</key><string>com.crundi.client</string><key>ProgramArguments</key><array><string>{exe}</string></array><key>RunAtLoad</key><true/></dict></plist>\n");
            if let Some(d) = f.parent() { std::fs::create_dir_all(d).map_err(|e| e.to_string())?; }
            std::fs::write(&f, plist).map_err(|e| e.to_string())?;
        } else if f.exists() { std::fs::remove_file(&f).map_err(|e| e.to_string())?; }
    } else {
        let f = linux_file();
        if on {
            if let Some(d) = f.parent() { std::fs::create_dir_all(d).map_err(|e| e.to_string())?; }
            std::fs::write(&f, desktop_entry(&exe)).map_err(|e| e.to_string())?;
        } else if f.exists() { std::fs::remove_file(&f).map_err(|e| e.to_string())?; }
    }
    eprintln!("[startup] Launch at login {}", if on { "enabled" } else { "disabled" });
    Ok(enabled())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry() {
        assert_eq!(quote("/a b/c$\"d"), "\"/a b/c\\$\\\"d\"");
        let e = desktop_entry("/opt/crundi");
        assert!(e.starts_with("[Desktop Entry]\nType=Application\nName=Crundi\nExec=\"/opt/crundi\"\n"));
        assert!(e.contains("X-GNOME-Autostart-enabled=true"));
    }
}
