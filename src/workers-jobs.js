/**
 * workers-jobs.js — commands on a worker PC (phase 2, specs/workers-protocol.md).
 *
 * A chat asks for a command on a PC (worker_run): this picks the PC, says
 * where the code comes from (a GitHub clone at the server's commit, with a
 * git bundle for commits GitHub lacks; or a bare copy of the project, sending
 * only what changed), keeps the job's log as it streams, takes the files the
 * job made back as a .tar.gz, and survives the link dropping (the PC carries
 * on; on reconnect it resends the log from where the server stopped).
 *
 * Created by workers.js, which owns the link and hands every job.* message here.
 */
import { mkdirSync, writeFileSync, readFileSync, renameSync, appendFileSync, statSync, openSync, readSync, closeSync, createReadStream, readdirSync, lstatSync, readlinkSync, unlinkSync, existsSync } from 'node:fs';
import { join, sep } from 'node:path';
import { tmpdir } from 'node:os';
import { randomBytes, createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { writeTar } from './tar-lite.js';

const KEEP_JOBS = 200;
const CHUNK = 512 * 1024;
const HIGH_WATER = 8 * 1024 * 1024;
const LOST_AFTER_MS = 30 * 60 * 1000;
const ARTIFACT_DAYS = 7;
const COPY_EXCLUDE = new Set(['.git', 'node_modules', 'target', 'dist', 'build', '.next', '__pycache__', '.crundi', '.crundi-jobs']);
const FINISHED = new Set(['done', 'failed', 'cancelled', 'refused', 'lost', 'error']);

const sleep = (ms) => new Promise(r => setTimeout(r, ms));
const str = (v, n) => (typeof v === 'string' ? v.slice(0, n) : '');

function git(args, cwd, { maxBuffer = 64 * 1024 * 1024, encoding = 'utf8' } = {}) {
  return new Promise((resolve) => {
    execFile('git', args, { cwd, maxBuffer, encoding, timeout: 120_000, windowsHide: true }, (err, stdout, stderr) => {
      resolve({ ok: !err, out: stdout, err: String(stderr || (err && err.message) || '') });
    });
  });
}

/** A GitHub SSH remote as https (the PC signs in with its own GitHub access, gh's https by default). */
export function httpsRemote(url) {
  const u = String(url || '').trim();
  let m = /^git@github\.com:(.+?)(\.git)?$/.exec(u);
  if (m) return `https://github.com/${m[1]}.git`;
  m = /^ssh:\/\/git@github\.com\/(.+?)(\.git)?$/.exec(u);
  if (m) return `https://github.com/${m[1]}.git`;
  return u;
}

/** Lines of a .crundiignore: a name matches anywhere, a path with "/" matches from the top. */
function ignoreOf(root) {
  let lines = [];
  try { lines = readFileSync(join(root, '.crundiignore'), 'utf8').split(/\r?\n/).map(l => l.trim()).filter(l => l && !l.startsWith('#')); } catch { /* none */ }
  return (rel) => {
    const parts = rel.split('/');
    if (parts.some(p => COPY_EXCLUDE.has(p))) return true;
    for (const l of lines) {
      const pat = l.replace(/\/+$/, '');
      if (pat.includes('/')) { const p = pat.replace(/^\/+/, ''); if (rel === p || rel.startsWith(p + '/')) return true; }
      else if (parts.includes(pat)) return true;
    }
    return false;
  };
}

/** The project's files for a copy: git's view when it is a repository, else a walk. */
async function projectFiles(root) {
  const ign = ignoreOf(root);
  const g = await git(['ls-files', '-co', '--exclude-standard', '-z'], root);
  let list;
  if (g.ok) list = g.out.split('\0').filter(Boolean);
  else {
    list = [];
    const walk = (dir) => {
      let names = [];
      try { names = readdirSync(join(root, dir)); } catch { return; }
      for (const n of names) {
        const rel = dir ? `${dir}/${n}` : n;
        if (ign(rel)) continue;
        let st; try { st = lstatSync(join(root, rel)); } catch { continue; }
        if (st.isDirectory()) walk(rel); else if (st.isFile() || st.isSymbolicLink()) list.push(rel);
        if (list.length > 200_000) return;
      }
    };
    walk('');
  }
  return list.map(p => p.split(sep).join('/')).filter(p => !ign(p) && !p.startsWith('../'));
}

export function createJobs({ dataDir, now = () => Date.now(), sendTo, bufferedOf = () => 0, workerOf, candidates, changed = () => {}, log = () => {} }) {
  const DIR = join(dataDir, 'workers');
  const LOGS = join(DIR, 'jobs');
  const ARTS = join(DIR, 'artifacts');
  const FILE = join(DIR, 'jobs.json');
  try { mkdirSync(LOGS, { recursive: true }); mkdirSync(ARTS, { recursive: true }); } catch { /* reported on use */ }

  let jobs = [];
  try { jobs = JSON.parse(readFileSync(FILE, 'utf8')).jobs || []; } catch { /* first run */ }
  // A restart forgets nothing it wrote; jobs that were live are now unknown.
  for (const j of jobs) if (!FINISHED.has(j.state)) { j.state = 'lost'; j.error = j.error || 'The server restarted while it ran.'; }
  const byId = new Map(jobs.map(j => [j.id, j]));
  const waiters = new Map();     // job -> [resolve]
  const live = new Map();        // job -> { artFd?, artTmp, lostTimer }
  const hashCache = new Map();   // abs path -> { size, mtimeMs, hash }
  // Per PC and project: the files the last copy had. Only those may be removed
  // on the PC; anything else there (what a build made) is the PC's and stays.
  const COPIES = join(DIR, 'copies.json');
  let copies = {};
  try { copies = JSON.parse(readFileSync(COPIES, 'utf8')) || {}; } catch { /* first run */ }
  function saveCopies() { try { writeFileSync(`${COPIES}.tmp`, JSON.stringify(copies), { mode: 0o600 }); renameSync(`${COPIES}.tmp`, COPIES); } catch { /* disk */ } }

  let saveTimer = null;
  function save() {
    if (saveTimer) return;
    saveTimer = setTimeout(() => {
      saveTimer = null;
      try {
        if (jobs.length > KEEP_JOBS) {
          for (const j of jobs.splice(0, jobs.length - KEEP_JOBS)) { byId.delete(j.id); try { unlinkSync(logPath(j.id)); } catch { /* gone */ } }
        }
        writeFileSync(`${FILE}.tmp`, JSON.stringify({ jobs }, null, 1), { mode: 0o600 });
        renameSync(`${FILE}.tmp`, FILE);
      } catch (err) { log(`[workers] jobs save failed: ${err.message}`); }
    }, 500);
    saveTimer.unref?.();
  }
  const logPath = (id) => join(LOGS, `${id}.log`);
  const iso = () => new Date(now()).toISOString();

  // Old artifacts go after a week.
  function sweepArtifacts() {
    const cut = now() - ARTIFACT_DAYS * 86400e3;
    try { for (const f of readdirSync(ARTS)) { const p = join(ARTS, f); try { if (statSync(p).mtimeMs < cut) unlinkSync(p); } catch { /* gone */ } } } catch { /* none */ }
    for (const j of jobs) if (j.artifact && !existsSync(j.artifact.path)) j.artifact.expired = true;
  }
  const sweepTimer = setInterval(sweepArtifacts, 6 * 3600e3); sweepTimer.unref?.();
  sweepArtifacts();

  function settle(j) {
    const ws = waiters.get(j.id) || [];
    waiters.delete(j.id);
    for (const r of ws) r();
    const l = live.get(j.id);
    if (l && l.lostTimer) clearTimeout(l.lostTimer);
    if (l && l.artFd != null) { try { closeSync(l.artFd); } catch { /* closed */ } }
    live.delete(j.id);
    save(); changed();
  }
  function finish(j, state, extra = {}) {
    if (FINISHED.has(j.state)) return;
    Object.assign(j, extra, { state, endedAt: iso() });
    settle(j);
  }

  /** Pick the PC: a named one, else the first that is free and has every label. */
  function pick({ worker, labels = [] }) {
    const want = String(worker || '').trim().toLowerCase();
    const list = candidates();
    const busy = new Set(jobs.filter(j => !FINISHED.has(j.state)).map(j => j.workerId));
    if (want) {
      const w = list.find(x => x.id.toLowerCase() === want || x.name.toLowerCase() === want);
      if (!w) return { error: `No worker "${worker}". Workers: ${list.map(x => x.name).join(', ') || 'none'}.` };
      if (!w.online) return { error: `${w.name} is offline (last seen ${w.lastSeenAt || 'never'}).` };
      if (!w.jobs) return { error: `${w.name} runs an older worker without commands. Update it on that PC.` };
      if (!w.allowCommands) return { error: `${w.name} does not allow commands (its switch in the Workers tab).` };
      if (w.paused) return { error: `${w.name} is paused.` };
      if (busy.has(w.id)) return { error: `${w.name} is running another job. Wait for it (worker_job) or pick another PC.` };
      return { w };
    }
    const need = (labels || []).map(l => String(l).toLowerCase());
    const fit = list.filter(x => need.every(l => x.labels.includes(l)));
    const ok = fit.find(x => x.online && x.jobs && x.allowCommands && !x.paused && !busy.has(x.id));
    if (ok) return { w: ok };
    if (!list.length) return { error: 'No PCs are joined as workers. Add one in the Workers tab.' };
    if (!fit.length) return { error: `No worker has every label ${need.join(', ')}.` };
    const why = fit.map(x => `${x.name}: ${!x.online ? 'offline' : !x.jobs ? 'older worker' : !x.allowCommands ? 'commands off' : x.paused ? 'paused' : 'busy'}`).join('; ');
    return { error: `No worker is free right now (${why}).` };
  }

  /** Start a job. Returns { ok, job } or { ok:false, error }. */
  async function run({ alias, projectPath, command, worker, labels, source, branch, cwd, artifacts, env, timeoutMinutes, chatId }) {
    const cmd = String(command || '').trim();
    if (!cmd) return { ok: false, error: 'No command given.' };
    if (!projectPath) return { ok: false, error: 'This chat has no project folder to send.' };
    const p = pick({ worker, labels: Array.isArray(labels) ? labels : (labels ? String(labels).split(/[\s,]+/).filter(Boolean) : []) });
    if (p.error) return { ok: false, error: p.error };
    const notes = [];
    let src = ['git', 'copy', 'none'].includes(source) ? source : '';
    let gitInfo = null;
    const isRepo = (await git(['rev-parse', '--is-inside-work-tree'], projectPath)).ok;
    if (src !== 'copy' && src !== 'none' && isRepo) {
      const url = httpsRemote((await git(['remote', 'get-url', 'origin'], projectPath)).out.trim());
      const br = String(branch || '').trim() || (await git(['rev-parse', '--abbrev-ref', 'HEAD'], projectPath)).out.trim();
      const commit = (await git(['rev-parse', `${br}^{commit}`], projectPath)).out.trim();
      if (url && /^https?:\/\//.test(url) && /^[0-9a-f]{40}$/.test(commit)) {
        gitInfo = { url, branch: br, commit };
        src = 'git';
        const dirty = (await git(['status', '--porcelain', '--untracked-files=no'], projectPath)).out.trim();
        if (dirty) notes.push('The project has uncommitted changes on the server; they are not in a git job (commit them, or use source "copy").');
      } else if (src === 'git') return { ok: false, error: url ? `Branch "${br}" was not found.` : 'The project has no https/GitHub remote; use source "copy".' };
    }
    if (!src) src = 'copy';
    if (src === 'git' && !gitInfo) return { ok: false, error: 'The project is not a git repository; use source "copy".' };
    const relCwd = String(cwd || '').replace(/\\/g, '/').replace(/^\/+/, '');
    if (relCwd.split('/').includes('..')) return { ok: false, error: 'cwd must stay inside the project.' };
    const id = 'j' + randomBytes(6).toString('hex');
    const j = {
      id, workerId: p.w.id, workerName: p.w.name, project: str(alias, 120), command: cmd.slice(0, 8000), source: src,
      git: gitInfo, cwd: relCwd, artifacts: (Array.isArray(artifacts) ? artifacts : artifacts ? [artifacts] : []).map(String).slice(0, 50),
      timeoutSec: Math.max(60, Math.min(24 * 3600, Math.round((Number(timeoutMinutes) || 120) * 60))),
      state: 'starting', phase: '', detail: '', createdAt: iso(), startedAt: null, endedAt: null,
      exitCode: null, durationMs: null, timedOut: false, cancelled: false, logBytes: 0, artifact: null, notes, error: '', chatId: str(chatId, 80),
    };
    writeFileSync(logPath(id), '', { mode: 0o600 });
    jobs.push(j); byId.set(id, j);
    live.set(id, { projectPath, artTmp: '' });
    const sent = sendTo(p.w.id, {
      type: 'job.start', job: id, project: j.project || 'project', command: j.command, source: src,
      ...(gitInfo ? { git: gitInfo } : {}), ...(relCwd ? { cwd: relCwd } : {}),
      ...(env && typeof env === 'object' ? { env: Object.fromEntries(Object.entries(env).filter(([k, v]) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(k) && typeof v === 'string').slice(0, 50)) } : {}),
      timeoutSec: j.timeoutSec, artifacts: j.artifacts, lowPriority: true,
    });
    if (!sent) { finish(j, 'error', { error: 'Could not reach the PC.' }); return { ok: false, error: 'Could not reach the PC.' }; }
    save(); changed();
    // Wait briefly for accepted / refused, so a refusal is the answer.
    const t0 = now();
    while (j.state === 'starting' && now() - t0 < 15_000) await sleep(50);
    if (j.state === 'refused') return { ok: false, job: id, error: j.error || 'The PC refused the job.' };
    return { ok: true, job: id, worker: p.w.name, source: src, notes };
  }

  // ── Streaming to the PC ──
  async function sendChunks(workerId, type, job, produce, extraLast = {}) {
    let buf = [], size = 0, seq = 0;
    const flush = async (last) => {
      while (bufferedOf(workerId) > HIGH_WATER) await sleep(20);
      const data = Buffer.concat(buf); buf = []; size = 0;
      if (!sendTo(workerId, { type, job, seq: seq++, b64: data.toString('base64'), last, ...(last ? extraLast : {}) })) throw new Error('The PC disconnected.');
    };
    await produce(async (b) => {
      buf.push(b); size += b.length;
      while (size >= CHUNK) {
        const all = Buffer.concat(buf);
        buf = [all.subarray(CHUNK)]; size = all.length - CHUNK;
        while (bufferedOf(workerId) > HIGH_WATER) await sleep(20);
        if (!sendTo(workerId, { type, job, seq: seq++, b64: all.subarray(0, CHUNK).toString('base64'), last: false })) throw new Error('The PC disconnected.');
      }
    });
    await flush(true);
  }

  async function serveBundle(j, have) {
    const root = live.get(j.id)?.projectPath;
    if (!root) return;
    const tmp = join(tmpdir(), `crundi-bundle-${j.id}.bundle`);
    const ref = `refs/heads/${j.git.branch}`;
    let args = ['bundle', 'create', tmp, ref];
    if (/^[0-9a-f]{40}$/.test(String(have || '')) && (await git(['cat-file', '-e', `${have}^{commit}`], root)).ok) args.push(`^${have}`);
    let r = await git(args, root);
    if (!r.ok && args.length > 4) r = await git(['bundle', 'create', tmp, ref], root);
    if (!r.ok) { sendTo(j.workerId, { type: 'job.cancel', job: j.id }); finish(j, 'error', { error: `Could not make a git bundle: ${r.err.slice(0, 300)}` }); return; }
    try {
      await sendChunks(j.workerId, 'job.bundle', j.id, async (out) => {
        for await (const b of createReadStream(tmp, { highWaterMark: 256 * 1024 })) await out(b);
      });
      j.notes.push('Commits GitHub does not have yet were sent from the server.');
    } catch (err) { log(`[workers] bundle for ${j.id}: ${err.message}`); }
    finally { try { unlinkSync(tmp); } catch { /* gone */ } }
  }

  async function hashOf(abs, st) {
    const c = hashCache.get(abs);
    if (c && c.size === st.size && c.mtimeMs === st.mtimeMs) return c.hash;
    let hash;
    if (st.isSymbolicLink()) hash = createHash('sha256').update('link:' + readlinkSync(abs)).digest('hex');
    else {
      const h = createHash('sha256');
      for await (const b of createReadStream(abs, { highWaterMark: 1024 * 1024 })) h.update(b);
      hash = h.digest('hex');
    }
    hashCache.set(abs, { size: st.size, mtimeMs: st.mtimeMs, hash });
    return hash;
  }

  async function serveCopy(j, theirs) {
    const root = live.get(j.id)?.projectPath;
    if (!root) return;
    const files = await projectFiles(root);
    const mine = new Set(files);
    const send = [];
    let bytes = 0;
    for (const rel of files) {
      let st; try { st = lstatSync(join(root, rel)); } catch { continue; }
      const t = theirs && theirs[rel];
      const size = st.isSymbolicLink() ? 0 : st.size;
      const same = Array.isArray(t) && t[0] === size && t[1] === await hashOf(join(root, rel), st);
      if (!same) { send.push(rel); bytes += size; }
    }
    const ck = `${j.workerId}:${j.project}`;
    const before = new Set(copies[ck] || []);
    const removed = Object.keys(theirs || {}).filter(p => !mine.has(p) && before.has(p)).slice(0, 100_000);
    j.copy = { files: files.length, sent: send.length, bytes, removed: removed.length };
    try {
      await sendChunks(j.workerId, 'job.files', j.id, (out) => writeTar(root, send, out), { removed });
      copies[ck] = files; saveCopies();
    } catch (err) { log(`[workers] copy for ${j.id}: ${err.message}`); }
    save(); changed();
  }

  function appendLog(j, off, data) {
    if (typeof off !== 'number' || off > j.logBytes) {
      // A gap (we missed output): ask again from what we have.
      sendTo(j.workerId, { type: 'job.attach', job: j.id, from: j.logBytes });
      return;
    }
    const skip = j.logBytes - off;
    if (skip >= data.length) return;   // already have it
    const piece = skip ? data.subarray(skip) : data;
    try { appendFileSync(logPath(j.id), piece); } catch { /* disk */ }
    j.logBytes += piece.length;
  }

  function artifactChunk(j, m) {
    let l = live.get(j.id);
    if (!l) { l = { artTmp: '' }; live.set(j.id, l); }
    if (l.artFd == null) {
      if (m.seq !== 0) return;
      l.artTmp = join(ARTS, `${j.id}.tar.gz.part`);
      l.artFd = openSync(l.artTmp, 'w', 0o600);
      l.artSeq = 0;
    }
    if (m.seq !== l.artSeq) return;   // out of order: drop (the PC resends whole on reattach)
    l.artSeq++;
    const b = Buffer.from(String(m.b64 || ''), 'base64');
    if (b.length) { try { writeFileSync(l.artFd, b); } catch { /* disk */ } }
    if (m.last) {
      try { closeSync(l.artFd); } catch { /* closed */ }
      l.artFd = null;
      const final = join(ARTS, `${j.id}.tar.gz`);
      if (Number(m.count) > 0) { try { renameSync(l.artTmp, final); j.artifact = { path: final, bytes: statSync(final).size, count: Number(m.count) }; } catch { /* disk */ } }
      else { try { unlinkSync(l.artTmp); } catch { /* gone */ } j.artifact = { path: '', bytes: 0, count: 0 }; if (j.artifacts.length) j.notes.push('No files matched the artifact patterns.'); }
      save(); changed();
    }
  }

  /** Every job.* message from a PC. */
  function onMessage(w, m) {
    const j = byId.get(String(m.job || ''));
    if (!j || j.workerId !== w.id) return;
    switch (m.type) {
      case 'job.accepted': if (j.state === 'starting') { j.state = 'preparing'; j.startedAt = iso(); save(); changed(); } return;
      case 'job.refused': finish(j, 'refused', { error: str(m.error, 400) || 'The PC refused the job.' }); return;
      case 'job.status': {
        const ph = ['preparing', 'running', 'collecting'].includes(m.phase) ? m.phase : '';
        if (ph && !FINISHED.has(j.state)) j.state = ph;
        j.phase = ph; j.detail = str(m.detail, 300);
        if (ph === 'running' && !j.runningAt) j.runningAt = iso();
        save(); changed(); return;
      }
      case 'job.need': if (m.kind === 'bundle' && j.git) serveBundle(j, m.have); return;
      case 'job.manifest': serveCopy(j, m.files && typeof m.files === 'object' ? m.files : {}); return;
      case 'job.out': if (typeof m.b64 === 'string') { appendLog(j, m.off, Buffer.from(m.b64, 'base64')); changed(); } return;
      case 'job.exit':
        Object.assign(j, { exitCode: typeof m.code === 'number' ? m.code : null, durationMs: typeof m.durationMs === 'number' ? m.durationMs : null, timedOut: !!m.timedOut, cancelled: !!m.cancelled || j.cancelled });
        // Failed before the command ran (the clone, the copy, a bad cwd): the PC says why.
        if (m.error) j.error = str(m.error, 600);
        if (!FINISHED.has(j.state)) j.state = 'collecting';
        save(); changed(); return;
      case 'job.artifact': artifactChunk(j, m); return;
      case 'job.done':
        if (m.unknown) { finish(j, 'lost', { error: 'The PC no longer had this job.' }); return; }
        finish(j, j.cancelled ? 'cancelled' : j.exitCode === 0 && !j.timedOut ? 'done' : 'failed'); return;
      default: return;
    }
  }

  /** A PC (re)connected: carry on its jobs from where the server stopped. */
  function onHello(w, list) {
    const known = new Set();
    for (const x of Array.isArray(list) ? list : []) {
      const j = byId.get(String(x?.job || ''));
      if (!j || j.workerId !== w.id) continue;
      known.add(j.id);
      if (j.state === 'lost') { j.state = x.state === 'finished' ? 'collecting' : 'running'; j.error = ''; }
      if (FINISHED.has(j.state)) continue;
      const l = live.get(j.id);
      if (l && l.lostTimer) { clearTimeout(l.lostTimer); l.lostTimer = null; }
      j.offline = false;
      sendTo(w.id, { type: 'job.attach', job: j.id, from: j.logBytes });
    }
    // Jobs it no longer knows ended without us.
    for (const j of jobs) {
      if (j.workerId === w.id && !FINISHED.has(j.state) && j.state !== 'starting' && !known.has(j.id) && j.offline) finish(j, 'lost', { error: 'The PC no longer had this job when it came back.' });
    }
    save(); changed();
  }

  /** A PC's link dropped: its jobs carry on there; lost if it stays away. */
  function onDrop(workerId) {
    for (const j of jobs) {
      if (j.workerId !== workerId || FINISHED.has(j.state)) continue;
      j.offline = true;
      let l = live.get(j.id); if (!l) { l = {}; live.set(j.id, l); }
      if (l.lostTimer) clearTimeout(l.lostTimer);
      l.lostTimer = setTimeout(() => finish(j, 'lost', { error: 'The PC was away for more than 30 minutes.' }), LOST_AFTER_MS);
      l.lostTimer.unref?.();
    }
    changed();
  }

  function tail(id, bytes = 4096) {
    const j = byId.get(id);
    if (!j) return '';
    const n = Math.max(0, Math.min(j.logBytes, Math.min(1024 * 1024, bytes)));
    if (!n) return '';
    const fd = openSync(logPath(id), 'r');
    try { const b = Buffer.alloc(n); readSync(fd, b, 0, n, j.logBytes - n); return b.toString('utf8'); }
    finally { closeSync(fd); }
  }

  function view(j, tailBytes = 0) {
    if (!j) return null;
    const out = {
      job: j.id, worker: j.workerName, workerId: j.workerId, project: j.project, command: j.command, source: j.source,
      branch: j.git?.branch || null, commit: j.git?.commit?.slice(0, 12) || null, cwd: j.cwd || '',
      state: j.state, running: !FINISHED.has(j.state), phase: j.phase, detail: j.detail, offline: !!j.offline,
      exitCode: j.exitCode, durationMs: j.durationMs, timedOut: j.timedOut, cancelled: j.cancelled,
      createdAt: j.createdAt, startedAt: j.startedAt, endedAt: j.endedAt, logBytes: j.logBytes, log: logPath(j.id),
      artifacts: j.artifact && j.artifact.count ? { path: j.artifact.path, bytes: j.artifact.bytes, count: j.artifact.count, expired: !!j.artifact.expired } : null,
      copy: j.copy || null, notes: j.notes || [], error: j.error || '',
    };
    if (tailBytes) out.tail = tail(j.id, tailBytes);
    return out;
  }

  /** Wait until the job ends or `ms` passes. */
  function wait(id, ms) {
    const j = byId.get(id);
    if (!j || FINISHED.has(j.state)) return Promise.resolve();
    return new Promise((resolve) => {
      const t = setTimeout(done, ms); t.unref?.();
      function done() { clearTimeout(t); const a = waiters.get(id) || []; const i = a.indexOf(done); if (i >= 0) a.splice(i, 1); resolve(); }
      const a = waiters.get(id) || []; a.push(done); waiters.set(id, a);
    });
  }

  function cancel(id) {
    const j = byId.get(id);
    if (!j) return { ok: false, error: 'No such job.' };
    if (FINISHED.has(j.state)) return { ok: true, job: view(j) };
    j.cancelled = true;
    if (!sendTo(j.workerId, { type: 'job.cancel', job: id })) finish(j, 'cancelled', { error: 'Cancelled while the PC was away.' });
    save(); changed();
    return { ok: true, job: view(j) };
  }

  const list = ({ workerId, limit = 30 } = {}) => jobs.filter(j => !workerId || j.workerId === workerId).slice(-limit).reverse().map(j => view(j));
  const busyWorkers = () => new Set(jobs.filter(j => !FINISHED.has(j.state)).map(j => j.workerId));

  function dispose() { clearInterval(sweepTimer); for (const l of live.values()) if (l.lostTimer) clearTimeout(l.lostTimer); if (saveTimer) { clearTimeout(saveTimer); saveTimer = null; try { writeFileSync(FILE, JSON.stringify({ jobs }, null, 1)); } catch { /* fine */ } } }

  return { run, onMessage, onHello, onDrop, view: (id, t) => view(byId.get(id), t), wait, cancel, list, tail, busyWorkers, dispose, get: (id) => byId.get(id) || null, FINISHED };
}
