//! The frame journal (perf/README.md, "Where a frame goes"): built only with
//! `--features frame-journal` (gpui's profiler), never in a release. With
//! CRUNDI_FRAME_JOURNAL=1 a background thread drains gpui's foreground
//! journal every 100 ms and prints, on stderr, one line per drawn frame
//! ("fj draw <request_layout µs> <prepaint µs> <paint µs> <draw µs>"), per
//! present ("fj present <µs>") and per view whose own work in a phase took
//! 0.5 ms or more ("fj view <phase> <µs> <name>"). The UI thread is never
//! woken for it; tools/frames.mjs reads the lines.
//!
//! With CRUNDI_PPROF_DIR=<dir> it also runs a stack sampler (pprof, 1 kHz of
//! CPU time, frame pointers) between `<dir>/start` appearing and `<dir>/stop`
//! appearing (the harness's MEASURE_PPROF=<scenario>), and writes the UI
//! thread's stacks, folded, to `<dir>/folded.txt`.

use gpui::{profiler::journal::{ForegroundEvent, ForegroundJournalEntry}, App};
use std::io::Write as _;

pub fn start(cx: &App) {
    if std::env::var("CRUNDI_FRAME_JOURNAL").ok().as_deref() != Some("1") { return; }
    gpui::set_trace_enabled(true);
    let mut c = cx.foreground_journal().collector();
    #[cfg(unix)]
    let mut sampler = Sampler { dir: std::env::var("CRUNDI_PPROF_DIR").ok().map(std::path::PathBuf::from), guard: None, done: false };
    std::thread::Builder::new().name("frame-journal".into()).spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        #[cfg(unix)]
        sampler.poll();
        let d = c.collect_unseen();
        if d.entries.is_empty() { continue; }
        let mut out = String::new();
        for e in d.entries {
            let ForegroundJournalEntry::Event(e) = e else { continue };
            match e {
                ForegroundEvent::Draw(f) => out.push_str(&format!("fj draw {} {} {} {}\n", f.phases.request_layout.as_micros(), f.phases.prepaint.as_micros(), f.phases.paint.as_micros(), f.draw_duration().as_micros())),
                ForegroundEvent::Present(p) => out.push_str(&format!("fj present {}\n", p.present_duration().as_micros())),
                ForegroundEvent::View(v) => out.push_str(&format!("fj view {} {} {}\n", v.phase.as_str(), v.self_duration.as_micros(), v.name)),
                _ => {}
            }
        }
        let _ = std::io::stderr().write_all(out.as_bytes());
    }).ok();
}

/// The stack sampler between `<dir>/start` and `<dir>/stop` (CRUNDI_PPROF_DIR).
#[cfg(unix)]
struct Sampler { dir: Option<std::path::PathBuf>, guard: Option<pprof::ProfilerGuard<'static>>, done: bool }

#[cfg(unix)]
impl Sampler {
    fn poll(&mut self) {
        let Some(dir) = self.dir.clone().filter(|_| !self.done) else { return };
        if self.guard.is_none() && dir.join("start").exists() { self.guard = pprof::ProfilerGuardBuilder::default().frequency(1000).build().ok(); }
        if self.guard.is_some() && dir.join("stop").exists() {
            if let Some(r) = self.guard.take().and_then(|g| g.report().build().ok()) {
                let mut folded = String::new();
                for (f, n) in r.data.iter() {
                    let names: Vec<String> = f.frames.iter().rev().map(|s| s.iter().map(|x| x.name()).collect::<Vec<_>>().join(";")).collect();
                    folded.push_str(&format!("{};{} {}\n", f.thread_name, names.join(";"), n));
                }
                let _ = std::fs::write(dir.join("folded.txt"), folded);
            }
            self.done = true;
        }
    }
}
