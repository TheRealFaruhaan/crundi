#!/usr/bin/env node
// The frame floor (perf/README.md, "The floor"): what GPUI 0.3.12 costs per
// frame on this machine, apart from the client's screens. Runs the floor
// binary (crates/app/src/floor.rs) in each mode, one window at a time,
// no server, with a fresh config folder of its own (removed after), and
// sweeps the same wheel as tools/measure-pc.mjs (tools/sweep.mjs).
//
//   CRUNDI_BUILD_JOBS=8 native-v2/tools/build.sh --release -p crundi-app
//   node native-v2/tools/measure-floor.mjs [modes…]      (default: all)
//
//   FLOOR_BIN=<path>   the binary (default the shared release build)
//   FLOOR_OUT=<dir>    where report.json and each mode's log go
//   CRUNDI_TEST_PORT   the driver's port (default 4882)
//   FLOOR_HEADLESS=1   under xvfb-run: WAYLAND_DISPLAY is dropped, or GPUI
//                      opens its window on the desktop's Wayland session
//                      instead of the virtual display
//   PROFILE=<gdb script>  after the sweep, a stack sampler on the UI thread
//                      while the wheel goes on (perf/gdb-sample.py, SAMPLES;
//                      the binary started through perf/ptracer-exec.py),
//                      samples in <FLOOR_OUT>/<mode>-samples.txt
//
// Per mode: idle (4 s, nothing changes: 0 frames), the sweep (8 s), with
// frames, p50, p95, max, the share ≤ 7 ms, and which views rendered.
import { spawn } from 'node:child_process';
import { createWriteStream, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { homedir, tmpdir } from 'node:os';
import { driver, sleep } from '../test/lib.mjs';
import { FrameMeter } from './frames.mjs';
import { sweep } from './sweep.mjs';

const BIN = process.env.FLOOR_BIN || join(homedir(), '.cache/crundi-native-target/release/crundi-v2-floor');
const OUT = process.env.FLOOR_OUT || '/tmp/crundi-v2-floor';
const PORT = Number(process.env.CRUNDI_TEST_PORT || 4882);
const ALL = ['empty', 'list', 'uniform', 'workbench', 'chatlog', 'chatlog5', 'stream5'];
const modes = process.argv.slice(2).length ? process.argv.slice(2) : ALL;
mkdirSync(OUT, { recursive: true });
const report = { bin: BIN, when: new Date().toISOString(), modes: {} };

let app = null, config = null;
const stop = () => {
  if (app && app.exitCode === null) { try { process.kill(app.pid); } catch { /* gone */ } }
  if (config) { try { rmSync(config, { recursive: true, force: true }); } catch { /* gone */ } }
};
process.on('exit', stop);
for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP']) process.on(sig, () => { stop(); process.exit(130); });

for (const mode of modes) {
  config = mkdtempSync(join(tmpdir(), 'crundi-v2-floor-config-'));
  const log = createWriteStream(join(OUT, `${mode}.log`));
  const [cmd, args] = process.env.PROFILE ? ['python3', ['-I', join(import.meta.dirname, '../perf/ptracer-exec.py'), mode]] : [BIN, [mode]];
  const base = { ...process.env };
  if (process.env.FLOOR_HEADLESS === '1') delete base.WAYLAND_DISPLAY;
  app = spawn(cmd, args, { env: { ...base, NATIVE_BIN: BIN, CRUNDI_TEST_PORT: String(PORT), CRUNDI_CONFIG_DIR: config, ZED_MEASUREMENTS: '1' }, stdio: ['ignore', 'pipe', 'pipe'] });
  app.stdout.pipe(log);
  const fm = new FrameMeter(app.stderr, (l) => log.write(l));
  let dead = false;
  app.on('exit', () => { dead = true; });
  try {
    const d = await driver(PORT, () => !dead);
    await d.send({ cmd: 'resize', w: 1920, h: 1040 });
    await sleep(3000); // laid out, the syntax set warmed
    const r = {};
    let m = fm.mark(); await sleep(4000); r.idle = fm.since(m);
    const r0 = (await d.send({ cmd: 'state' })).state?.renders || {};
    m = fm.mark(); const n = await sweep(d, 8000); await sleep(300); r.sweep = { ...fm.since(m), wheelEvents: n };
    const r1 = (await d.send({ cmd: 'state' })).state?.renders || {};
    r.sweep.renders = Object.fromEntries(Object.entries(r1).map(([k, v]) => [k, v - (r0[k] || 0)]).filter(([, v]) => v));
    if (process.env.PROFILE) {
      const g = spawn('gdb', ['-q', '-p', String(app.pid), '-batch', '-x', process.env.PROFILE], { env: { ...process.env, SAMPLE_OUT: join(OUT, `${mode}-samples.txt`) }, stdio: ['ignore', 'ignore', 'inherit'] });
      let gone = false; g.on('exit', () => { gone = true; });
      const until = Date.now() + 240000;
      while (!gone && Date.now() < until) await sweep(d, 2000);
    }
    report.modes[mode] = r;
    console.log(mode.padEnd(10), 'idle', r.idle.frames, 'frames; sweep', JSON.stringify(r.sweep));
    d.close();
  } catch (e) {
    report.modes[mode] = { error: String(e) };
    console.log(mode, 'failed:', String(e));
  }
  stop();
  await new Promise((res) => (app.exitCode !== null || app.signalCode ? res() : app.on('exit', res)));
  app = null; config = null;
  await sleep(500);
}
writeFileSync(join(OUT, 'report.json'), JSON.stringify(report, null, 1));
console.log('report', join(OUT, 'report.json'));
