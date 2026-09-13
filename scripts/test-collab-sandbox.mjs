// Collaborator sandbox, token limits, and approvals dismissal.
//
// Runs entirely in temp folders: its own DATA_DIR, PROJECTS_DIR and worktree
// root, and a throwaway git repository. Nothing here touches real projects.
import { mkdtempSync, mkdirSync, writeFileSync, existsSync, readFileSync, rmSync, realpathSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { execFileSync } from 'child_process';
import { fileURLToPath } from 'url';

const here = fileURLToPath(new URL('.', import.meta.url));
let failed = 0;
const check = (name, ok, extra = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : ' ' + extra}`);
  if (!ok) failed++;
};

const T = mkdtempSync(join(tmpdir(), 'crundi-collab-sb-'));
process.env.DATA_DIR = join(T, 'data');
process.env.PROJECTS_DIR = join(T, 'projects');
process.env.COLLAB_WORKTREES_DIR = join(T, 'wt');
process.env.TELEGRAM_BOT_TOKEN = '';
mkdirSync(process.env.DATA_DIR, { recursive: true });
const repo = join(process.env.PROJECTS_DIR, 'demo');
mkdirSync(repo, { recursive: true });
const git = (...a) => execFileSync('git', a, { cwd: repo, stdio: 'pipe' }).toString();
git('init', '-q', '-b', 'main');
git('config', 'user.email', 't@t'); git('config', 'user.name', 't');
writeFileSync(join(repo, 'a.txt'), 'a');
git('add', '.'); git('commit', '-qm', 'init');

try {
  // ─── Sandbox settings ───
  const { collaboratorSettings, collaboratorSandbox } = await import('../src/access-policy.js');
  const sb = collaboratorSandbox({
    root: '/w/me', gitDir: '/p/demo/.git/worktrees/me', commonDir: '/p/demo/.git', branch: 'collab/sam-abc',
    cacheDir: '/w-cache/id1', worktreesRoot: '/w', projectsDir: '/p', dataDir: '/d', home: '/h', extraDenyRead: ['/w-cache'],
  });
  check('sandbox on, fails closed, no unsandboxed escape',
    sb.enabled === true && sb.failIfUnavailable === true && sb.allowUnsandboxedCommands === false && sb.autoAllowBashIfSandboxed === true);
  const w = sb.filesystem.allowWrite;
  check('git writes limited to objects, own worktree meta, own branch refs, cache',
    w.includes('/p/demo/.git/objects') && w.includes('/p/demo/.git/worktrees/me')
    && w.includes('/p/demo/.git/refs/heads/collab') && w.includes('/w-cache/id1'), JSON.stringify(w));
  check('main repo hooks and config are never writable',
    !w.some(p => p === '/p/demo/.git' || p.includes('hooks') || p.endsWith('/config')), JSON.stringify(w));
  const dr = sb.filesystem.denyRead;
  check('other projects, worktrees, caches, data and credentials unreadable',
    ['/p', '/w', '/w-cache', '/d', '/h/.claude', '/h/.ssh', '/h/.config', '/h/.npm'].every(p => dr.includes(p)), JSON.stringify(dr));
  check('own worktree readable', sb.filesystem.allowRead.includes('/w/me'));
  check('network limited to an allowlist including npm', sb.network.allowedDomains.includes('registry.npmjs.org'));
  check('always-allowed sites join the network allowlist',
    collaboratorSandbox({ extraDomains: ['httpbin.org'] }).network.allowedDomains.includes('httpbin.org')
    && collaboratorSandbox({ extraDomains: ['registry.npmjs.org'] }).network.allowedDomains.filter(d => d === 'registry.npmjs.org').length === 1);
  check('settings carry the sandbox only when asked', !collaboratorSettings().sandbox && !!collaboratorSettings({ root: '/x' }).sandbox);
  check('deny list still present', collaboratorSettings({ root: '/x' }).permissions.deny.includes('Bash(sudo:*)'));

  // ─── Worktree git paths ───
  const col = await import('../src/collaborators.js');
  const { worktreeGitPaths } = await import('../src/collab-sandbox.js');
  const r1 = await col.create({ name: 'Sam', project: 'demo', hours: 5, withPasscode: true, tokenLimit: 1000 });
  check('invitation created', r1.ok, JSON.stringify(r1).slice(0, 200));
  const a = r1.collaborator;
  const gp = worktreeGitPaths(a.worktreePath);
  // Compared as real paths: a Windows runner's temp dir comes back in 8.3 short
  // form (RUNNER~1) while git may record the long one.
  const samePath = (x, y) => {
    try {
      const p = realpathSync.native(x), q = realpathSync.native(y);
      return process.platform === 'win32' ? p.toLowerCase() === q.toLowerCase() : p === q;
    } catch { return false; }
  };
  check('worktree git paths found',
    !!gp && samePath(gp.commonDir, join(repo, '.git')) && existsSync(join(gp.gitDir, 'HEAD')) && /worktrees/.test(gp.gitDir),
    JSON.stringify(gp));
  check('cache dir sits beside the worktrees root, not inside it',
    col.cacheDirOf(a.id).startsWith(col.cacheRoot()) && !col.cacheDirOf(a.id).startsWith(col.worktreesRoot() + '/'));

  // ─── Token usage ───
  let u = col.addUsage(a.id, 400);
  check('usage counted', u.used === 400 && u.limit === 1000 && !u.over, JSON.stringify(u));
  u = col.addUsage(a.id, 700);
  check('over the limit once crossed', u.over && u.used === 1100, JSON.stringify(u));
  const listed = col.list().find(x => x.id === a.id);
  check('pending usage flushed into the list', listed && listed.tokensUsed === 1100, JSON.stringify(listed && listed.tokensUsed));
  const r2 = await col.create({ name: 'Sam', project: 'demo', hours: 5, withPasscode: true });
  check('second project inherits the person\'s limit', r2.ok && r2.collaborator.tokenLimit === 1000, JSON.stringify(r2.collaborator && r2.collaborator.tokenLimit));
  check('usage is per person across projects', col.usageOf(r2.collaborator.id).used === 1100 && col.usageOf(r2.collaborator.id).over);
  col.update(a.id, { tokenLimit: 5000 });
  check('raising the limit applies to every invitation of the person',
    col.usageOf(r2.collaborator.id).limit === 5000 && !col.usageOf(a.id).over);
  col.resetUsage(a.id);
  check('reset clears the person\'s count', col.usageOf(r2.collaborator.id).used === 0);
  check('no limit means never over', (() => { col.update(a.id, { tokenLimit: 0 }); col.addUsage(a.id, 1e9); return !col.usageOf(a.id).over; })());

  // ─── Always-allowed sites ───
  const bad = col.addAllowedDomain(a.id, 'not a host; rm -rf /');
  check('invalid host refused', !bad.ok);
  const good = col.addAllowedDomain(a.id, 'HTTPBin.org.');
  check('host normalised and stored for the person', good.ok && good.host === 'httpbin.org'
    && col.allowedDomainsOf(r2.collaborator.id).includes('httpbin.org'), JSON.stringify(good));
  col.addAllowedDomain(r2.collaborator.id, 'httpbin.org');
  check('no duplicates', col.allowedDomainsOf(a.id).filter(d => d === 'httpbin.org').length === 1);
  col.removeAllowedDomain(a.id, 'httpbin.org');
  check('removal applies to every invitation of the person', col.allowedDomainsOf(r2.collaborator.id).length === 0);

  // ─── Cleanup on delete ───
  const cache = col.cacheDirOf(a.id);
  mkdirSync(join(cache, 'npm'), { recursive: true });
  const mcpDir = join(process.env.DATA_DIR, 'collab-mcp');
  mkdirSync(mcpDir, { recursive: true });
  writeFileSync(join(mcpDir, `${a.id}.json`), '{}');
  const rm = await col.remove(a.id);
  check('remove succeeds', rm.ok);
  check('remove deletes their cache and tools config', !existsSync(cache) && !existsSync(join(mcpDir, `${a.id}.json`)));
  check('remove deletes their worktree', !existsSync(a.worktreePath));
  await col.remove(r2.collaborator.id);

  // ─── Wiring (static) ───
  const ui = readFileSync(join(here, '..', 'src', 'claude-ui.js'), 'utf8');
  check('collaborator chats need a working sandbox', /sandboxStatus\(\)[\s\S]{0,80}if \(!sb\.ok\)/.test(ui));
  check('collaborator chats launch with capabilities dropped', /capDropLauncher\(\)/.test(ui) && /launcher \? \[\.\.\.launcher\.prefix, bin, \.\.\.args\]/.test(ui));
  check('collaborator env is not process.env', /env: collaborator \? collaboratorEnv\(collaborator\)/.test(ui) && !/function collaboratorEnv[\s\S]{0,600}\.\.\.process\.env/.test(ui));
  check('edits auto-accepted for collaborators', /'--permission-mode', 'acceptEdits'/.test(ui));
  check('limit stops the turn', /function stopForLimit[\s\S]{0,200}interrupt\(s\.id\)/.test(ui));
  check('pending card changes notify the inbox', (ui.match(/pendingChanged\(s\);/g) || []).length >= 3);
  const web = readFileSync(join(here, '..', 'src', 'webapp.js'), 'utf8');
  check('dismiss declines secrets and collab requests, hides chat cards',
    /body\.action === 'dismiss'/.test(web) && /entry\.reject\(/.test(web) && /dismissedChatCards\.add\(id\)/.test(web));
  check('invites refused without the sandbox', (web.match(/needsSandbox: true/g) || []).length >= 2);
  const html = readFileSync(join(here, '..', 'src', 'webapp-html.js'), 'utf8');
  check('inbox has Dismiss and Clear all', html.includes('data-action="appr-dismiss"') && html.includes('data-action="appr-clear-all"'));
  check('invite form has a token limit', html.includes('id="collab-limit"'));
  check('collaborator permission prompts notify, default when away',
    /collabPermission: 'away'/.test(web) && /notifyEvent\('collabPermission'/.test(web)
    && /collabPermission: 'away'/.test(html) && html.includes("['collabPermission',"));
  check('collaborators cannot open the usage chart',
    /function openUsageModal\(\) \{[\s\S]{0,200}if \(userRole === 'collaborator'\) return;/.test(html));
  check('a finished turn closes questions it left open',
    /function handleResult\(s, msg\) \{\s*\/\/[^\n]*\n\s*expireStalePending\(s\);/.test(ui)
    && /function expireStalePending\(s\)[\s\S]{0,500}escalationGoneCb\?\.\(p\.entry\.approvalId\)[\s\S]{0,200}s\.pending\.clear\(\);/.test(ui));
  check('an abandoned escalation is withdrawn from the inbox',
    /onEscalationGone\(\(approvalId\) => \{\s*if \(approvals\.cancel\(approvalId, 'system'\)\) broadcastApprovals\(\);/.test(web));
  {
    const ap = await import('../src/approvals.js');
    let called = false;
    const rec = ap.add({ kind: 'tool', collabId: 'x', name: 'Sam', project: 'demo', title: 't', resolve: () => { called = true; } });
    const first = ap.cancel(rec.id);
    const again = ap.cancel(rec.id);
    const late = ap.resolve(rec.id, true);
    check('cancel withdraws without answering, and a late approve is refused',
      first === true && again === false && !called && !late.ok && ap.get(rec.id).status === 'cancelled');
  }
  check('website approvals offer once, session and always',
    html.includes('data-action="appr-yes-session"') && html.includes('data-action="appr-yes-always"')
    && /body\.scope === 'always' && pre\.collabId\) collaborators\.addAllowedDomain/.test(web)
    && /claudeUi\.allowHostForSession\(pre\.sessionId, host\)/.test(web));
  check('a host allowed for the session is answered without escalating',
    /req\.tool_name === 'SandboxNetworkAccess'[\s\S]{0,200}s\.allowedHosts\.has\([\s\S]{0,200}respond\(s\.id, \{ requestId, behavior: 'allow' \}\);/.test(ui));
  check('saved sites reach new chats', /allowedDomains: collaborators\.allowedDomainsOf\(inv\.id\)/.test(web)
    && /extraDomains: collaborator\.allowedDomains \|\| \[\]/.test(ui));
  check('chat list marks collaborator chats without their API key',
    /collaborator: s\.collaborator\s*\?\s*\{ id: s\.collaborator\.id, name: s\.collaborator\.name \|\| '', key: s\.collaborator\.key \|\| '' \}/.test(ui));
  check('collaborators see only their own chats',
    /t\.kind === 'ui' && mine\.has\(String\(t\.project \|\| ''\)\.toLowerCase\(\)\)\s*&& !!t\.collaborator && t\.collaborator\.key === principal\.collabKey/.test(web));
  check('owner sees whose chat it is', /term-collab-tag/.test(html) && /userRole !== 'collaborator' && t\.collaborator/.test(html));
  check('"This session" for any collaborator permission uses session-scoped rules',
    html.includes('data-action="appr-yes-session"') && /r\.scope === 'session' \|\| r\.scope === 'always'\) \? 'session'/.test(web)
    && /const sessionOnly = always === 'session' \|\| !!s\.collaborator;/.test(ui));
  {
    const chat = readFileSync(join(here, '..', 'app', 'vendor', 'claude-chat.js'), 'utf8').replace(/\r\n/g, '\n');
    check('a task bubble shows its command and output',
      /var taskTool = rec\.meta\.kind === 'task' \? findToolEntry\(toolUseId\) : null;/.test(chat)
      && /if \(!rec\.messages\.length && !taskTool\)/.test(chat)
      && /function findToolEntry\(toolUseId\)[\s\S]{0,300}r\.data\.kind === 'tool' && r\.data\.toolUseId === toolUseId/.test(chat));
    check('an open task panel updates when its command finishes',
      /rec\.data\.toolUseId === openAgent\)[\s\S]{0,200}taskToolNode\(rec\.data\)/.test(chat));
  }
  check('collaborator chats do not send the owner finished / needs-input pings',
    /\(cur === 'idle' \|\| cur === 'needs-input'\) && !term\.background && !term\.collaborator\)/.test(web));
  check('a collaborator page never counts as the owner being present',
    /msg\.type === 'presence'\) \{[\s\S]{0,500}if \(confined\) return;\s*if \(msg\.active\) presentClients\.set/.test(web));
  check('account usage is pushed to owners only', /event: usage[\s\S]{0,200}if \(isConfined\(client\.principal\)\) continue;/.test(web)
    && !/broadcastSSE\('usage'/.test(web));
  check('collaborators get their own time + token bar',
    /function renderCollabBar\(\)/.test(html) && /if \(userRole === 'collaborator'\) \{ renderCollabBar\(\); return; \}/.test(html)
    && /usage: \(principal\.invitations \|\| \[\]\)\[0\] \? collaborators\.usageOf/.test(web) && /createdAt: c\.createdAt \|\| 0/.test(web));
} finally {
  // Windows can briefly hold a handle on a just-removed worktree; a temp
  // folder left behind must not fail the checks.
  try { rmSync(T, { recursive: true, force: true, maxRetries: 3, retryDelay: 200 }); } catch { /* temp dir */ }
}

if (failed) { console.log(`\n${failed} failed`); process.exit(1); }
console.log('\ncollab sandbox: all passed');
process.exit(0);
