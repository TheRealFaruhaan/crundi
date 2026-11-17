//! Starting at login (tray) and the systemd user unit (headless, Linux).

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

const APP_ID: &str = "crundi-worker";

fn exe() -> Result<PathBuf> {
    std::env::current_exe().context("finding this program's path")
}

/// Extra arguments that point the started program at a non-default config.
fn config_args(config: Option<&Path>) -> Vec<String> {
    match config {
        Some(p) => vec!["--config".into(), p.display().to_string()],
        None => vec![],
    }
}

fn quote(s: &str) -> String {
    if s.contains([' ', '"', '\'', '\\']) {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

/// The systemd user unit text.
pub fn systemd_unit(exe: &Path, config: Option<&Path>) -> String {
    let mut cmd = vec![quote(&exe.display().to_string()), "--headless".into()];
    cmd.extend(config_args(config).iter().map(|a| quote(a)));
    format!(
        "[Unit]\n\
         Description=Crundi worker\n\
         After=network-online.target\n\
         Wants=network-online.target\n\
         \n\
         [Service]\n\
         ExecStart={}\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         # Exit 3 means the server removed this PC: do not restart.\n\
         RestartPreventExitStatus=3\n\
         Nice=10\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        cmd.join(" ")
    )
}

fn systemd_unit_path() -> Result<PathBuf> {
    let base = dirs::config_dir().context("no config folder")?;
    Ok(base.join("systemd").join("user").join(format!("{APP_ID}.service")))
}

fn systemctl(args: &[&str]) -> Result<()> {
    let st = std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .context("running systemctl --user")?;
    if !st.success() {
        bail!("systemctl --user {} failed ({st})", args.join(" "));
    }
    Ok(())
}

pub fn install_service(config: Option<&Path>) -> Result<String> {
    if !cfg!(target_os = "linux") {
        bail!("install-service is for Linux (systemd); use `autostart on` on this system");
    }
    let path = systemd_unit_path()?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, systemd_unit(&exe()?, config))
        .with_context(|| format!("writing {}", path.display()))?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", "--now", &format!("{APP_ID}.service")])?;
    Ok(format!(
        "Installed and started {}.\nLogs: journalctl --user -u {APP_ID} -f\n\
         To keep it running when you are logged out: sudo loginctl enable-linger $USER",
        path.display()
    ))
}

pub fn uninstall_service() -> Result<String> {
    let path = systemd_unit_path()?;
    if !path.exists() {
        return Ok("No service was installed.".into());
    }
    let _ = systemctl(&["disable", "--now", &format!("{APP_ID}.service")]);
    std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
    let _ = systemctl(&["daemon-reload"]);
    Ok(format!("Removed {}.", path.display()))
}

/// Linux: `~/.config/autostart/crundi-worker.desktop`.
pub fn xdg_desktop_entry(exe: &Path, config: Option<&Path>) -> String {
    let mut cmd = vec![quote(&exe.display().to_string())];
    cmd.extend(config_args(config).iter().map(|a| quote(a)));
    format!(
        "[Desktop Entry]\nType=Application\nName=Crundi worker\nComment=Lets your Crundi server use this PC\nExec={}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        cmd.join(" ")
    )
}

/// macOS: `~/Library/LaunchAgents/com.crundi.worker.plist`.
pub fn launch_agent_plist(exe: &Path, config: Option<&Path>) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let mut args = vec![exe.display().to_string()];
    args.extend(config_args(config));
    let args: String = args.iter().map(|a| format!("    <string>{}</string>\n", esc(a))).collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n  <key>Label</key>\n  <string>com.crundi.worker</string>\n  <key>ProgramArguments</key>\n  <array>\n{args}  </array>\n  <key>RunAtLoad</key>\n  <true/>\n  <key>ProcessType</key>\n  <string>Interactive</string>\n</dict>\n</plist>\n"
    )
}

fn autostart_file() -> Result<Option<PathBuf>> {
    if cfg!(target_os = "macos") {
        let home = dirs::home_dir().context("no home folder")?;
        Ok(Some(home.join("Library/LaunchAgents/com.crundi.worker.plist")))
    } else if cfg!(windows) {
        Ok(None)
    } else {
        let base = dirs::config_dir().context("no config folder")?;
        Ok(Some(base.join("autostart").join(format!("{APP_ID}.desktop"))))
    }
}

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "CrundiWorker";

fn reg(args: &[&str]) -> Result<bool> {
    let mut c = std::process::Command::new("reg");
    c.args(args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    Ok(c.status().context("running reg.exe")?.success())
}

/// Is start-at-login on?
pub fn autostart_enabled() -> bool {
    if cfg!(windows) {
        reg(&["query", RUN_KEY, "/v", RUN_VALUE]).unwrap_or(false)
    } else {
        autostart_file().ok().flatten().is_some_and(|p| p.exists())
    }
}

pub fn set_autostart(on: bool, config: Option<&Path>) -> Result<String> {
    let exe = exe()?;
    if cfg!(windows) {
        if on {
            let mut cmd = vec![format!("\"{}\"", exe.display())];
            cmd.extend(config_args(config).iter().map(|a| format!("\"{a}\"")));
            let data = cmd.join(" ");
            if !reg(&["add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &data, "/f"])? {
                bail!("could not write the Run key");
            }
            return Ok("Crundi worker will start at login.".into());
        }
        let _ = reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]);
        return Ok("Crundi worker will not start at login.".into());
    }
    let path = autostart_file()?.context("no autostart location")?;
    if on {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let text = if cfg!(target_os = "macos") {
            launch_agent_plist(&exe, config)
        } else {
            xdg_desktop_entry(&exe, config)
        };
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
        Ok(format!("Crundi worker will start at login ({}).", path.display()))
    } else {
        if path.exists() {
            std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        }
        Ok("Crundi worker will not start at login.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_and_entries() {
        let u = systemd_unit(Path::new("/opt/cw/crundi-worker"), Some(Path::new("/etc/my cfg.json")));
        assert!(u.contains("ExecStart=/opt/cw/crundi-worker --headless --config \"/etc/my cfg.json\""));
        assert!(u.contains("RestartPreventExitStatus=3"));
        let d = xdg_desktop_entry(Path::new("/usr/bin/crundi-worker"), None);
        assert!(d.contains("Exec=/usr/bin/crundi-worker\n"));
        let p = launch_agent_plist(Path::new("/Applications/W.app/w"), None);
        assert!(p.contains("<string>/Applications/W.app/w</string>"));
    }
}
