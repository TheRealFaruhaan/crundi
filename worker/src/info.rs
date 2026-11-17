//! What the worker reports in `hello` and `info` (spec: "Worker -> server").

use crate::util::{find_program, run_with_timeout};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub os: String,
    pub os_version: String,
    pub arch: String,
    pub hostname: String,
    pub cpu: Cpu,
    pub ram_mb: u64,
    pub ram_free_mb: u64,
    /// [1m, 5m, 15m], or null where there is no load average (Windows).
    pub load: Option<[f64; 3]>,
    pub gpus: Vec<Gpu>,
    pub disk: Disk,
    pub work_dir: String,
    pub on_battery: Option<bool>,
    pub idle_sec: Option<u64>,
    /// Found tools only: name -> version string.
    pub tools: BTreeMap<String, String>,
    pub allow: Allow,
    pub paused: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cpu {
    pub model: String,
    pub cores: u32,
    pub threads: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    /// "nvidia" | "amd" | "intel" | "apple" | "microsoft" | "other"
    pub vendor: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vram_mb: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub path: String,
    pub free_mb: u64,
    pub total_mb: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Allow {
    pub chats: bool,
    pub commands: bool,
}

/// Tools probed, as (key in `tools`, programs to try in order, args).
const TOOLS: &[(&str, &[&str], &[&str])] = &[
    ("git", &["git"], &["--version"]),
    ("gh", &["gh"], &["--version"]),
    ("node", &["node"], &["--version"]),
    ("npm", &["npm"], &["--version"]),
    ("python", &["python3", "python"], &["--version"]),
    ("cargo", &["cargo"], &["--version"]),
    ("rustc", &["rustc"], &["--version"]),
    ("go", &["go"], &["version"]),
    ("java", &["java"], &["--version"]),
    ("ffmpeg", &["ffmpeg"], &["-version"]),
    ("docker", &["docker"], &["--version"]),
    ("claude", &["claude"], &["--version"]),
];

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const CACHE_FOR: Duration = Duration::from_secs(600);

struct Cached {
    at: Instant,
    tools: BTreeMap<String, String>,
    gpus: Vec<Gpu>,
}

static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

/// Pull the first version-looking token (`1.2`, `v20.11.1`, `2.43.0.windows.1`)
/// out of a tool's output.
pub fn parse_version(text: &str) -> Option<String> {
    for line in text.lines().filter(|l| !l.trim().is_empty()).take(3) {
        for word in line.split(|c: char| c.is_whitespace() || c == ',' || c == '"' || c == '(' || c == ')') {
            let w = word.trim_start_matches(['v', 'V']).trim_start_matches("go");
            let w = w.trim_end_matches(['.', ':']);
            let mut parts = w.split('.');
            let first = parts.next().unwrap_or("");
            if !first.is_empty() && first.chars().all(|c| c.is_ascii_digit()) && w.contains('.') {
                let second = w.split('.').nth(1).unwrap_or("");
                if second.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    return Some(w.to_string());
                }
            }
        }
    }
    None
}

async fn probe_tool(programs: &'static [&'static str], args: &'static [&'static str]) -> Option<String> {
    for prog in programs {
        let Some(path) = find_program(prog) else { continue };
        if let Some((_ok, out, err)) = run_with_timeout(&path, args, PROBE_TIMEOUT).await {
            if let Some(v) = parse_version(&out).or_else(|| parse_version(&err)) {
                return Some(v);
            }
        }
    }
    None
}

/// Probe all tools in parallel.
pub async fn probe_tools() -> BTreeMap<String, String> {
    let handles: Vec<_> = TOOLS
        .iter()
        .map(|(key, progs, args)| (*key, tokio::spawn(probe_tool(progs, args))))
        .collect();
    let mut out = BTreeMap::new();
    for (key, h) in handles {
        if let Ok(Some(v)) = h.await {
            out.insert(key.to_string(), v);
        }
    }
    out
}

fn gpu_vendor(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    if n.contains("nvidia") || n.contains("geforce") || n.contains("quadro") || n.contains("tesla") {
        "nvidia"
    } else if n.contains("amd") || n.contains("radeon") || n.contains("ati ") {
        "amd"
    } else if n.contains("intel") {
        "intel"
    } else if n.contains("apple") {
        "apple"
    } else if n.contains("microsoft") {
        "microsoft"
    } else {
        "other"
    }
}

/// Parse `nvidia-smi --query-gpu=name,memory.total --format=csv,noheader`.
pub fn parse_nvidia_smi(text: &str) -> Vec<Gpu> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split(',');
            let name = it.next()?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let vram = it
                .next()
                .and_then(|m| m.trim().split_whitespace().next().map(str::to_string))
                .and_then(|m| m.parse::<u64>().ok());
            Some(Gpu { vendor: "nvidia".into(), name, vram_mb: vram })
        })
        .collect()
}

async fn probe_gpus() -> Vec<Gpu> {
    let mut gpus = Vec::new();
    if let Some(p) = find_program("nvidia-smi") {
        if let Some((true, out, _)) = run_with_timeout(
            &p,
            &["--query-gpu=name,memory.total", "--format=csv,noheader,nounits"],
            PROBE_TIMEOUT,
        )
        .await
        {
            gpus = parse_nvidia_smi(&out);
        }
    }
    let have_nvidia = !gpus.is_empty();
    let push = |gpus: &mut Vec<Gpu>, name: String, vram_mb: Option<u64>| {
        let vendor = gpu_vendor(&name);
        if name.is_empty() || (have_nvidia && vendor == "nvidia") {
            return;
        }
        gpus.push(Gpu { vendor: vendor.into(), name, vram_mb });
    };
    #[cfg(windows)]
    {
        let ps = find_program("powershell").or_else(|| find_program("pwsh"));
        if let Some(ps) = ps {
            let script = "Get-CimInstance Win32_VideoController | ForEach-Object { \"$($_.Name)|$($_.AdapterRAM)\" }";
            if let Some((_, out, _)) =
                run_with_timeout(&ps, &["-NoProfile", "-NonInteractive", "-Command", script], PROBE_TIMEOUT).await
            {
                for line in out.lines() {
                    let mut it = line.trim().splitn(2, '|');
                    let name = it.next().unwrap_or("").trim().to_string();
                    // AdapterRAM is a 32-bit field: useless at 4 GB and over.
                    let vram = it
                        .next()
                        .and_then(|r| r.trim().parse::<u64>().ok())
                        .map(|b| b / 1_048_576)
                        .filter(|mb| *mb > 0 && *mb < 4095);
                    if name.to_ascii_lowercase().contains("basic display") || name.to_ascii_lowercase().contains("remote display") {
                        continue;
                    }
                    push(&mut gpus, name, vram);
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(sp) = find_program("system_profiler") {
            if let Some((true, out, _)) =
                run_with_timeout(&sp, &["SPDisplaysDataType", "-json"], Duration::from_secs(8)).await
            {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) {
                    for item in v["SPDisplaysDataType"].as_array().into_iter().flatten() {
                        let name = item["sppci_model"].as_str().unwrap_or("").to_string();
                        let vram = item["spdisplays_vram"]
                            .as_str()
                            .or(item["spdisplays_vram_shared"].as_str())
                            .and_then(parse_mac_vram);
                        push(&mut gpus, name, vram);
                    }
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(lspci) = find_program("lspci") {
            if let Some((true, out, _)) = run_with_timeout(&lspci, &[], PROBE_TIMEOUT).await {
                for line in out.lines() {
                    let l = line.to_ascii_lowercase();
                    if l.contains("vga compatible controller") || l.contains("3d controller") || l.contains("display controller") {
                        if let Some(idx) = line.find("controller: ") {
                            let name = line[idx + 12..].trim().to_string();
                            push(&mut gpus, name, None);
                        }
                    }
                }
            }
        }
    }
    let _ = &push;
    gpus
}

#[cfg(any(target_os = "macos", test))]
fn parse_mac_vram(s: &str) -> Option<u64> {
    let mut it = s.split_whitespace();
    let n: u64 = it.next()?.parse().ok()?;
    match it.next()?.to_ascii_uppercase().as_str() {
        "GB" => Some(n * 1024),
        "MB" => Some(n),
        _ => None,
    }
}

/// Tools and GPUs, from the cache when it is under 10 minutes old.
pub async fn slow_probes(force: bool) -> (BTreeMap<String, String>, Vec<Gpu>) {
    if !force {
        if let Some(c) = CACHE.lock().unwrap().as_ref() {
            if c.at.elapsed() < CACHE_FOR {
                return (c.tools.clone(), c.gpus.clone());
            }
        }
    }
    let (tools, gpus) = tokio::join!(probe_tools(), probe_gpus());
    *CACHE.lock().unwrap() = Some(Cached { at: Instant::now(), tools: tools.clone(), gpus: gpus.clone() });
    (tools, gpus)
}

/// What gather() needs from the worker's state.
#[derive(Debug, Clone)]
pub struct InfoInputs {
    pub work_dir: PathBuf,
    pub allow_chats: bool,
    pub allow_commands: bool,
    pub paused: bool,
}

/// Disk free/total of the file system holding `path`.
fn disk_of(path: &Path) -> Disk {
    let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut best: Option<(usize, u64, u64)> = None;
    if let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() {
        for d in disks.iter() {
            let mp = d.mount_point();
            if canon.starts_with(mp) {
                let depth = mp.components().count();
                if best.map_or(true, |(b, _, _)| depth > b) {
                    best = Some((depth, d.available_space(), d.total_space()));
                }
            }
        }
    }
    let (free, total) = best.map(|(_, f, t)| (f, t)).unwrap_or((0, 0));
    Disk {
        path: path.display().to_string(),
        free_mb: free / 1_048_576,
        total_mb: total / 1_048_576,
    }
}

/// Linux: on battery when a battery exists and no mains supply is online.
fn on_battery() -> Option<bool> {
    #[cfg(target_os = "linux")]
    {
        let dir = std::fs::read_dir("/sys/class/power_supply").ok()?;
        let mut has_battery = false;
        let mut mains_online = false;
        for e in dir.flatten() {
            let p = e.path();
            let kind = std::fs::read_to_string(p.join("type")).unwrap_or_default();
            match kind.trim() {
                "Battery" => has_battery = true,
                "Mains" | "USB" => {
                    if std::fs::read_to_string(p.join("online")).unwrap_or_default().trim() == "1" {
                        mains_online = true;
                    }
                }
                _ => {}
            }
        }
        if has_battery { Some(!mains_online) } else { Some(false) }
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// The fast part of info (no subprocesses), filled with the given tools/GPUs.
pub fn gather_fast(inp: &InfoInputs, tools: BTreeMap<String, String>, gpus: Vec<Gpu>) -> Info {
    use sysinfo::System;
    let mut model = String::new();
    let mut threads = 0u32;
    let mut ram_mb = 0;
    let mut ram_free_mb = 0;
    if let Ok(mut sys) = System::new() {
        sys.refresh_memory();
        sys.refresh_cpu_all();
        ram_mb = sys.total_memory() / 1_048_576;
        ram_free_mb = sys.available_memory() / 1_048_576;
        threads = sys.cpus().len() as u32;
        model = sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
    }
    if threads == 0 {
        threads = std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1);
    }
    let cores = System::physical_core_count().map(|c| c as u32).unwrap_or(threads);
    let load = if cfg!(windows) {
        None
    } else {
        System::load_average().ok().map(|l| [round2(l.one), round2(l.five), round2(l.fifteen)])
    };
    Info {
        os: crate::util::os_name().to_string(),
        os_version: System::long_os_version()
            .or_else(|_| System::os_version())
            .unwrap_or_default(),
        arch: std::env::consts::ARCH.to_string(),
        hostname: crate::util::hostname(),
        cpu: Cpu { model, cores, threads },
        ram_mb,
        ram_free_mb,
        load,
        gpus,
        disk: disk_of(&inp.work_dir),
        work_dir: inp.work_dir.display().to_string(),
        on_battery: on_battery(),
        idle_sec: None,
        tools,
        allow: Allow { chats: inp.allow_chats, commands: inp.allow_commands },
        paused: inp.paused,
    }
}

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Full info: system now, tools and GPUs from the 10-minute cache.
pub async fn gather(inp: InfoInputs) -> Info {
    let (tools, gpus) = slow_probes(false).await;
    tokio::task::spawn_blocking(move || gather_fast(&inp, tools, gpus))
        .await
        .expect("info thread")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("git version 2.43.0").as_deref(), Some("2.43.0"));
        assert_eq!(parse_version("v20.11.1\n").as_deref(), Some("20.11.1"));
        assert_eq!(parse_version("Python 3.12.3").as_deref(), Some("3.12.3"));
        assert_eq!(parse_version("cargo 1.83.0 (5ffbef321 2024-10-29)").as_deref(), Some("1.83.0"));
        assert_eq!(parse_version("go version go1.22.2 linux/amd64").as_deref(), Some("1.22.2"));
        assert_eq!(parse_version("openjdk 21.0.5 2024-10-15").as_deref(), Some("21.0.5"));
        assert_eq!(parse_version("Docker version 27.3.1, build ce12230").as_deref(), Some("27.3.1"));
        assert_eq!(parse_version("2.0.14 (Claude Code)").as_deref(), Some("2.0.14"));
        assert_eq!(parse_version("ffmpeg version 6.1.1-3ubuntu5 Copyright").as_deref(), Some("6.1.1-3ubuntu5"));
        assert_eq!(parse_version("gh version 2.45.0 (2024-03-04)").as_deref(), Some("2.45.0"));
        assert_eq!(parse_version("nothing here"), None);
    }

    #[test]
    fn nvidia() {
        let g = parse_nvidia_smi("NVIDIA GeForce RTX 4090, 24564\nTesla T4, 15360 MiB\n");
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].vram_mb, Some(24564));
        assert_eq!(g[1].vram_mb, Some(15360));
        assert_eq!(g[1].vendor, "nvidia");
        assert_eq!(parse_mac_vram("16 GB"), Some(16384));
        assert_eq!(gpu_vendor("AMD Radeon RX 7900"), "amd");
    }
}
