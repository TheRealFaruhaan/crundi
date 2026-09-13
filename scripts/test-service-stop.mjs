// Stopping a service must stop every process of it.
//
// On Linux, Stop used to spawn `taskkill` (Windows only). That failed silently:
// the service was marked stopped while its shell and children kept running,
// and Delete then removed the registration of a service still alive.
//
// Runs real processes in an isolated DATA_DIR. Skipped on Windows, where the
// taskkill path is the one that applies.
import { mkdtempSync, rmSync, readFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { fileURLToPath } from 'url';

const here = fileURLToPath(new URL('.', import.meta.url));
let failed = 0;
const check = (name, ok, extra = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : ' ' + extra}`);
  if (!ok) failed++;
};
const wait = (ms) => new Promise(r => setTimeout(r, ms));
const alive = (pid) => { try { process.kill(pid, 0); return true; } catch { return false; } };

// Static checks run everywhere.
const web = readFileSync(join(here, '..', 'src', 'webapp.js'), 'utf8');
const svc = readFileSync(join(here, '..', 'src', 'services.js'), 'utf8');
check('Linux stop kills the process tree, not taskkill',
  /else if \(process\.platform === 'win32'\) \{\s*spawn\('taskkill'[\s\S]{0,400}killTree\(entry\.proc\.pid\);/.test(svc));
check('services start unbuffered and without Crundi credentials',
  /'CRUNDI_API_KEY', 'CRUNDI_PASSWORD_HASH', 'CRUNDI_TOTP_SECRET', 'CLOUDFLARE_TUNNEL_TOKEN',/.test(svc)
  && /if \(e\.PYTHONUNBUFFERED === undefined\) e\.PYTHONUNBUFFERED = '1';/.test(svc));
check('a requested stop is not recorded as a crash', /entry\.status = code === 0 \|\| entry\.stopRequested \? 'stopped' : 'crashed';/.test(svc));
check('state pushes carry service ownership (no flicker)',
  /createdBy: \(\(\) => \{ try \{ return \(getRegistered\(s\.key\) \|\| \{\}\)\.createdBy/.test(web)
  && /\.map\(\(\{ createdBy, byCollaborator, \.\.\.x \}\) => \(\{ \.\.\.x, mine: !!createdBy && createdBy === principal\.collabKey \}\)\)/.test(web));

const html = readFileSync(join(here, '..', 'src', 'webapp-html.js'), 'utf8');
const policy = readFileSync(join(here, '..', 'src', 'access-policy.js'), 'utf8');
check('collaborators get stats for their projects only, and no machine stats',
  /const confinedStats = isConfined\(principal\);\s*const system = confinedStats \? null : await getSystemStats\(\);/.test(web)
  && /if \(confinedStats && !principal\.projects\.includes\(String\(s\.alias \|\| ''\)\.toLowerCase\(\)\)\) continue;/.test(web));
check('the stats route is open to collaborators (scoped in the route)',
  /\['GET', '\/api\/stats'\],/.test(policy) && !/\/\^\\\/api\\\/stats\//.test(policy));
check('a failed stats poll does not blank the service cards',
  /const res = await apiFetch\('\/api\/stats'\);[\s\S]{0,300}if \(!res\.ok\) return;[\s\S]{0,200}if \(!d \|\| typeof d !== 'object' \|\| d\.error\) return;/.test(html));

{
  const registry = readFileSync(join(here, '..', 'src', 'service-registry.js'), 'utf8');
  const stdio = readFileSync(join(here, '..', 'src', 'mcp-stdio.js'), 'utf8');
  check('the registry can check a registration without saving', /export function checkRegistration\(\{ alias, name, cwd, command \}\)/.test(registry));
  check('both collaborator request routes refuse an invalid service up front',
    (web.match(/checkRegistration\(\{ alias(?:: inv\.project)?, name: (?:body|a)\.name, cwd: want, command: (?:body|a)\.command \}\)/g) || []).length === 2);
  check('approval re-checks before recording, and keeps it pending on failure',
    /pre\.kind === 'service' && pre\.payload\) \{\s*const chk = checkRegistration\(pre\.payload\);\s*if \(!chk\.ok\) return json/.test(web));
  check('the asking chat hears approve, fail and decline',
    /The owner approved the service/.test(web) && /could not be registered: \$\{reg\.error\}/.test(web) && /The owner declined: \$\{r\.request\.title\}/.test(web)
    && /name === 'register_service'\) && process\.env\.CRUNDI_CHAT_ID/.test(stdio));
  check('the owner sees whether the service was actually registered',
    /r\.service && !r\.service\.ok\) toast\('Approved, but it could not be registered/.test(html));
}

if (process.platform === 'win32') {
  console.log('\n(skipping live process checks on Windows)');
} else {
  const T = mkdtempSync(join(tmpdir(), 'crundi-svcstop-'));
  process.env.DATA_DIR = join(T, 'data');
  process.env.PROJECTS_DIR = join(T, 'projects');
  process.env.TELEGRAM_BOT_TOKEN = '';
  try {
    const reg = await import('../src/service-registry.js');
    const services = await import('../src/services.js');

    // The name Sam's Claude asked for, which the registry refuses.
    const bad = reg.checkRegistration({ alias: 'demo', name: "CV site (Sam's branch)", cwd: T, command: 'true' });
    check('a name with an apostrophe or brackets is refused up front, with the reason',
      !bad.ok && /not a valid service name/.test(bad.error), JSON.stringify(bad));
    check('a valid name passes the check', reg.checkRegistration({ alias: 'demo', name: 'CV site - Sam branch', cwd: T, command: 'true' }).ok);

    // A shell whose real work is two children, like `npm run dev` spawning a server.
    const r = reg.registerService({ alias: 'demo', name: 'tree', cwd: T, command: 'sleep 60 & sleep 60 & wait' });
    check('registered', r.ok, JSON.stringify(r));
    check('a duplicate name is refused by the check', !reg.checkRegistration({ alias: 'demo', name: 'tree', cwd: T, command: 'true' }).ok);
    const started = services.startService(r.key);
    check('started', started.ok, JSON.stringify(started));
    await wait(700);
    const status = services.getAllServiceStatus().find(s => s.key === r.key);
    const rootPid = status && status.pid;
    const kids = String(readFileSync(`/proc/${rootPid}/task/${rootPid}/children`, 'utf8') || '').trim().split(/\s+/).filter(Boolean).map(Number);
    check('the service has child processes', kids.length >= 2, JSON.stringify(kids));

    const stopped = services.stopService(r.key);
    check('stop reports ok', stopped.ok, JSON.stringify(stopped));
    await wait(1500);
    check('the shell is gone', !alive(rootPid));
    check('every child is gone', kids.every(p => !alive(p)), JSON.stringify(kids.filter(alive)));
    const after = services.getAllServiceStatus().find(s => s.key === r.key);
    check('status says stopped, not crashed', after && after.status === 'stopped', after && after.status);

    // A service marked stopped whose process survived (the old bug): Delete
    // must not leave it orphaned.
    const r2 = reg.registerService({ alias: 'demo', name: 'orphan', cwd: T, command: 'sleep 60 & wait' });
    services.startService(r2.key);
    await wait(700);
    const s2 = services.getAllServiceStatus().find(s => s.key === r2.key);
    const pid2 = s2 && s2.pid;
    const kids2 = String(readFileSync(`/proc/${pid2}/task/${pid2}/children`, 'utf8') || '').trim().split(/\s+/).filter(Boolean).map(Number);
    // Simulate the broken stop: status flipped, nothing killed.
    const entry = services.getService(r2.key);
    entry.status = 'stopped';
    const del = services.deleteService(r2.key);
    check('delete of a stopped-but-alive service succeeds', del.ok, JSON.stringify(del));
    await wait(1500);
    check('and kills what was still running', !alive(pid2) && kids2.every(p => !alive(p)), JSON.stringify([pid2, ...kids2].filter(alive)));
    services.deleteService(r.key);
  } finally {
    try { rmSync(T, { recursive: true, force: true }); } catch { /* temp */ }
  }
}

if (failed) { console.log(`\n${failed} failed`); process.exit(1); }
console.log('\nservice stop: all passed');
process.exit(0);
