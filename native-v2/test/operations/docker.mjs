// operations.mjs's "docker" section: run(T, t) with T from t.begin('ops') (test/lib.mjs).
//
// The machine's Docker is shared and live: this section only acts on what it
// makes itself (a container, a volume, a network, all named ops-native-dk-*),
// never prunes, never pulls a real image (a pull of a name that does not
// exist shows the job drawer), and removes what it made at the end.
//
// DK_EXPECT=unreachable or absent: the test server was started with
// DOCKER_HOST at something that is not Docker (see specs/ops/docker.md);
// only the gate's state is checked.
import { execFileSync } from 'node:child_process';
import net from 'node:net';

// A port free right now on 127.0.0.1 (the system's pick), so the container's
// published port stays the same across its stop and start below.
const freePort = () => new Promise((res) => { const s = net.createServer(); s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => res(p)); }); });

const sh = (...a) => { try { return execFileSync('docker', a, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim(); } catch (e) { return ''; } };

export async function run(T, t) {
  const dk = (p) => p.areas.ops.docker;
  const expect = process.env.DK_EXPECT || '';
  if (expect) {
    await T.ui('tab', 'docker');
    const p = await T.until(`docker: the ${expect} state`, (p) => dk(p).gate === expect, 30000);
    if (expect === 'unreachable') t.check('docker: its error is said', !!(p && dk(p).gateError), p && dk(p));
    await T.snap(`docker-${expect}`);
    if (expect === 'unreachable') {
      await T.act('docker-recheck');
      await T.until('docker: Try again asks again and stays out of reach', (p) => dk(p).gate === 'unreachable');
    }
    return;
  }

  const tag = Math.random().toString(36).slice(2, 8);
  const NAME = `ops-native-dk-${tag}`;
  const VOL = `ops-native-dk-vol-${tag}`;
  const NET = `ops-native-dk-net-${tag}`;
  // A container that logs a coloured line every half second, with a published port
  // (a fixed one: a random host port would change on the restart below, orphaning the forward).
  const cid = sh('run', '-d', '--name', NAME, '-p', `127.0.0.1:${await freePort()}:80`, '-e', 'API_TOKEN=hush', 'alpine:3', 'sh', '-c',
    'i=0; while true; do i=$((i+1)); printf "tick %s \\033[32mok\\033[0m\\n" $i; sleep 0.5; done');
  sh('volume', 'create', VOL);
  sh('network', 'create', NET);
  if (!t.check('docker: a test container to work on', !!cid, 'docker run failed')) return;
  const hostPort = (sh('port', NAME, '80/tcp').split(':').pop() || '').trim();

  try {
    await T.ui('tab', 'docker');
    let p = await T.until('docker: the containers load', (p) => dk(p).gate === 'ok' && dk(p).state === 'data' && dk(p).ctrs.some((c) => c.name === NAME), 30000);
    if (!p) return;
    let d = dk(p);
    t.check('docker: the nav has the seven views, with counts', d.segs.map((s) => s.label).join(',') === 'Containers,Stacks,Images,Volumes,Networks,Forwards,System' && d.segs[0].count !== '', d.segs);
    t.check('docker: the summary (running, paused, stopped, CPU, memory)', d.stats.length === 5 && d.stats[0][1] === 'RUNNING', d.stats);
    t.check('docker: the bay plan has every container', d.bay === d.ctrs.length, d.bay);
    const mine = d.ctrs.find((c) => c.name === NAME);
    t.check('docker: a running row offers Stop and shows its port', mine.act === 'stop' && mine.state === 'running' && mine.chips.some((c) => c.includes('→80')), mine);
    await T.snap('docker-containers');

    await T.act('docker-filter', NAME);
    await T.until('docker: the filter narrows the list', (p) => dk(p).ctrs.length === 1 && dk(p).ctrs[0].name === NAME);
    await T.act('docker-filter', 'zz-nothing-here-zz');
    await T.until('docker: nothing matches', (p) => dk(p).nomatch === true);
    await T.act('docker-filter', '');

    // The detail.
    await T.act('docker-open', NAME);
    p = await T.until('docker: the detail opens', (p) => dk(p).detail?.state === 'data' && dk(p).detail.name === NAME);
    d = dk(p).detail;
    t.check('docker: running: Stop, Restart, Pause, Kill', d.acts.join(',') === 'Stop,Restart,Pause,Kill', d.acts);
    t.check('docker: the overview has its state and what it runs', d.ovState.some(([k, v]) => k === 'Status' && v === 'running') && d.ovRuns.some(([k, v]) => k === 'Image' && v === 'alpine:3'), d);
    t.check('docker: its published port, with Add forward', d.ports.some((x) => x.port === hostPort && x.btn === 'Add forward'), d.ports);
    t.check('docker: its environment and networks', d.ovEnv >= 2 && d.ovNets.length >= 1, d);
    await T.snap('docker-detail-overview');

    // A forward for its port.
    await T.act('docker-fwd-add', hostPort);
    p = await T.until('docker: the forward form opens under the port', (p) => dk(p).form === hostPort && dk(p).formModes.length >= 1);
    t.check('docker: it suggests a name from the container', dk(p).formName === NAME.replace(/[^a-z0-9-]/g, '-').slice(0, 32).replace(/-+$/, ''), dk(p).formName);
    const pathMode = dk(p).formModes.length - 1;
    await T.act('docker-fwd-fill', `dk${tag}`, String(pathMode), 'false', 'false', '');
    await T.act('docker-fwd-create');
    await T.until('docker: the forward shows on its port', (p) => dk(p).detail?.ports.some((x) => x.port === hostPort && x.fwds.includes(`dk${tag}`) && x.btn === 'Another'), 20000);

    // Logs over /ws.
    await T.act('docker-tab', 'logs');
    p = await T.until('docker: the log follows live', (p) => (dk(p).detail?.tail || []).filter((l) => l.startsWith('tick')).length >= 3, 20000);
    const n0 = p ? dk(p).detail.lines : 0;
    await T.until('docker: new lines keep coming', (p) => dk(p).detail.lines > n0, 10000);
    await T.snap('docker-logs');
    await T.act('docker-log-filter', 'tick 1');
    p = await T.until('docker: the line filter narrows the rows', (p) => dk(p).detail.logRows > 0 && dk(p).detail.logRows < dk(p).detail.lines);
    await T.act('docker-log-filter', '');
    await T.act('docker-log-option', 'ts', 'true');
    const cp = await T.act('docker-log-copy');
    t.check('docker: Copy takes the lines, with their times, without colour codes', /\d{4}-\d\d-\d\dT.* tick \d+ ok/.test(cp.text || '') && !(cp.text || '').includes('\u001b'), (cp.text || '').slice(0, 200));
    await T.act('docker-log-option', 'ts', 'false');
    await T.act('docker-log-clear');
    await T.until('docker: Clear empties the log', (p) => dk(p).detail.lines < 5);

    // Stats.
    await T.act('docker-tab', 'stats');
    p = await T.until('docker: the charts draw', (p) => dk(p).detail.statsState === 'data' && dk(p).detail.charts.length === 4 && dk(p).detail.charts[0].line, 20000);
    if (p) t.check('docker: CPU, Memory, Network, Disk', dk(p).detail.charts.map((c) => c.title).join(',') === 'CPU,MEMORY,NETWORK,DISK' && /processes/.test(dk(p).detail.statsNote), dk(p).detail);

    // Shell over /ws.
    await T.act('docker-tab', 'shell');
    // Its prompt first: a shell's line editor drops what is typed before it starts reading.
    await T.until('docker: a shell opens, with its prompt', (p) => dk(p).detail.shellLive && dk(p).detail.shellBtn === 'New shell' && /[#$] ?$/.test(dk(p).detail.shellText.trim()), 15000);
    await T.act('docker-shell-send', `echo shell-$((6*7))-${tag}`);
    if (!await T.until('docker: the shell runs a command', (p) => dk(p).detail.shellText.includes(`shell-42-${tag}`), 15000)) { const d = (await T.mine()).docker.detail; console.log('     shell:', JSON.stringify({ live: d.shellLive, bar: d.shellBar, btn: d.shellBtn, text: d.shellText.slice(-400) })); }
    await T.snap('docker-shell');
    await T.act('docker-shell-send', 'exit');
    await T.until('docker: the shell ends and offers Open again', (p) => !dk(p).detail.shellLive && dk(p).detail.shellBtn === 'Open again' && /ended/.test(dk(p).detail.shellBar), 15000);
    await T.act('docker-shell-again');
    await T.until('docker: Open again opens a new one', (p) => dk(p).detail.shellLive, 15000);

    // Inspect.
    await T.act('docker-tab', 'inspect');
    p = await T.until('docker: the raw inspect shows', (p) => dk(p).detail.tab === 'inspect' && dk(p).detail.inspect > 100);
    const ij = await T.act('docker-inspect-copy');
    let parsed = null; try { parsed = JSON.parse(ij.text); } catch { /* no */ }
    t.check('docker: Copy JSON copies the inspect', parsed && parsed.Name === '/' + NAME, (ij.text || '').slice(0, 100));

    // Rename.
    await T.act('docker-tab', 'overview');
    await T.act('docker-prompt', 'rename');
    await T.until('docker: the rename line opens with its name', (p) => dk(p).detail.prompt === 'rename' && dk(p).detail.newname === NAME);
    const NAME2 = NAME + '-b';
    await T.act('docker-rename', NAME2);
    await T.until('docker: renamed', (p) => dk(p).detail.name === NAME2 && dk(p).detail.prompt === '', 15000);

    // Stop and start.
    await T.act('docker-ctr', '', 'stop');
    p = await T.until('docker: stopped, it offers Start', (p) => dk(p).detail.dot === 'exited' && dk(p).detail.acts.join(',') === 'Start', 30000);
    await T.act('docker-tab', 'stats');
    await T.until('docker: stats say there is nothing to measure', (p) => /not running/.test(dk(p).detail.statsMsg), 10000);
    await T.act('docker-tab', 'shell');
    await T.until('docker: the shell asks for a start', (p) => /Start the container/.test(dk(p).detail.shellBar) || /ended/.test(dk(p).detail.shellBar), 10000);
    await T.act('docker-ctr', '', 'start');
    await T.until('docker: started again', (p) => dk(p).detail.dot === 'running' && dk(p).detail.acts[0] === 'Stop', 30000);
    await T.act('docker-tab', 'overview');
    await T.act('docker-prompt', 'remove');
    await T.until('docker: the remove line, with Stop it first', (p) => dk(p).detail.prompt === 'remove' && dk(p).detail.rmforceShown);
    await T.act('docker-prompt', '');
    await T.act('docker-back');
    await T.until('docker: back to the list', (p) => dk(p).detail === null && dk(p).view === 'containers');

    // Stacks.
    await T.act('docker-view', 'stacks');
    p = await T.until('docker: stacks load', (p) => dk(p).view === 'stacks' && dk(p).state === 'data' && (dk(p).stacksEmpty || dk(p).stacks.length > 0), 20000);
    await T.snap('docker-stacks'); // (F98)
    if (p && !dk(p).stacksEmpty) t.check('docker: a stack says how many run', /\d+\/\d+ running/.test(dk(p).stacks[0].chip), dk(p).stacks[0]);

    // Images, and a pull that fails (a name that does not exist): the job drawer.
    await T.act('docker-view', 'images');
    p = await T.until('docker: images load', (p) => dk(p).view === 'images' && dk(p).state === 'data' && dk(p).rows.length > 0, 20000);
    // (F108: the picture shows the suite's own image only, not this shared machine's.)
    await T.act('docker-filter', 'alpine:3'); await t.sleep(300);
    await T.snap('docker-images');
    await T.act('docker-filter', '');
    if (p) t.check('docker: the images line', /images, \d+ in use/.test(dk(p).listHead) && dk(p).rows.some((r) => r.name === 'alpine:3' && !r.canRm), dk(p).listHead);
    await T.act('docker-pull', `ops-native-test/does-not-exist-${tag}:none`);
    await T.until('docker: the job drawer shows the pull', (p) => dk(p).job && /^Pull /.test(dk(p).job.title), 10000);
    p = await T.until('docker: the failed pull ends with its exit', (p) => dk(p).job?.done && !dk(p).job.ok, 60000);
    await T.act('docker-job-close');
    await T.until('docker: the drawer closes', (p) => dk(p).job === null);

    // Volumes: remove the test's own.
    await T.act('docker-view', 'volumes');
    await T.act('docker-filter', VOL);
    p = await T.until('docker: our volume, unused', (p) => dk(p).view === 'volumes' && dk(p).rows.length === 1 && dk(p).rows[0].canRm && dk(p).rows[0].chips[0] === 'unused', 20000);
    await T.snap('docker-volumes');
    await T.act('docker-rm', 'volumes', VOL);
    if (!await T.until('docker: the volume is removed', (p) => dk(p).rows.length === 0, 20000)) { const m = await T.mine(); console.log('     volumes:', JSON.stringify({ rows: m.docker.rows, nomatch: m.docker.nomatch, state: m.docker.state, toast: m.toast }).slice(0, 1500)); }
    t.check('docker: and gone from Docker', sh('volume', 'ls', '-q', '--filter', `name=${VOL}`) === '');

    // Networks: remove the test's own.
    await T.act('docker-view', 'networks');
    await T.act('docker-filter', '');
    p = await T.until('docker: networks load, the built-in ones locked', (p) => dk(p).view === 'networks' && dk(p).rows.some((r) => r.name === 'bridge' && !r.canRm && r.tags.includes('built in')), 20000);
    await T.act('docker-filter', NET); await t.sleep(300);
    await T.snap('docker-networks');
    await T.act('docker-filter', '');
    await T.act('docker-filter', NET);
    await T.until('docker: our network', (p) => dk(p).rows.length === 1 && dk(p).rows[0].canRm);
    const netKey = (await T.mine()).docker.rows[0].key;
    await T.act('docker-rm', 'networks', netKey);
    await T.until('docker: the network is removed', (p) => dk(p).rows.length === 0, 20000);
    await T.act('docker-filter', '');

    // Forwards: ours is matched to the container; watch it; one more on a free port (an orphan).
    await T.act('docker-view', 'forwards');
    p = await T.until('docker: the forwards, by what is behind them', (p) => dk(p).view === 'forwards' && dk(p).groups.some((g) => g.title.startsWith('RUNNING CONTAINER') && g.rows.some((r) => r.host === `dk${tag}` && r.chips[0].startsWith(NAME2))), 20000);
    if (!p) { const m = await T.mine(); console.log('     forwards:', JSON.stringify({ want: [`dk${tag}`, NAME2], groups: m.docker.groups.map((g) => ({ t: g.title, r: g.rows.map((r) => [r.host, r.chips]) })) }).slice(0, 2000)); }
    await T.act('docker-fwd-watch', `dk${tag}`, 'true');
    await T.until('docker: watched', (p) => dk(p).groups.some((g) => g.rows.some((r) => r.host === `dk${tag}` && r.watch && r.tags.includes('watched'))), 15000);
    await T.act('docker-fwd-add', 'any');
    await T.until('docker: the any-port form', (p) => dk(p).form === 'any');
    await T.act('docker-fwd-fill', `orphan${tag}`, String((await T.mine()).docker.formModes.length - 1), 'false', 'false', '1');
    await T.act('docker-fwd-create');
    await T.until('docker: an orphaned forward', (p) => dk(p).orphans === 1 && dk(p).groups.some((g) => g.title.startsWith('NOTHING BEHIND IT')), 20000);
    await T.snap('docker-forwards');
    await T.act('docker-rm-orphans');
    await T.until('docker: Remove orphaned removes it', (p) => dk(p).orphans === 0, 20000);
    await T.act('docker-rm-fwd', `dk${tag}`);
    await T.until('docker: the last forward removed: the empty state', (p) => dk(p).fwdEmpty, 20000);

    // System.
    await T.act('docker-view', 'system');
    p = await T.until('docker: the engine', (p) => dk(p).view === 'system' && dk(p).engine.some(([k]) => k === 'Docker'), 20000);
    await T.snap('docker-system');
    await T.until('docker: the disk use, measured', (p) => dk(p).diskState === 'data' && dk(p).disk.length === 4, 120000);

    // Remove the container from its detail (it is running: stopped first).
    await T.act('docker-view', 'containers');
    await T.act('docker-open', NAME2);
    await T.until('docker: open again', (p) => dk(p).detail?.state === 'data');
    await T.act('docker-prompt', 'remove');
    await T.act('docker-remove', 'true', 'true');
    await T.until('docker: removed, back to the list', (p) => dk(p).detail === null && dk(p).state === 'data' && !dk(p).ctrs.some((c) => c.name === NAME2), 30000);
  } finally {
    sh('rm', '-f', NAME); sh('rm', '-f', NAME + '-b');
    sh('volume', 'rm', VOL); sh('network', 'rm', NET);
  }
}
