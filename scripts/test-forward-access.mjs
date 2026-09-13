// Private forwards for outside collaborators: a signed, person-scoped cookie,
// and access limited to forwards of their own projects' services.
import { readFileSync } from 'fs';
import { join } from 'path';
import { fileURLToPath } from 'url';
import {
  mintCollabForwardToken, readCollabForwardToken, forwardProjects, collabMayReachForward,
} from '../src/forward-access.js';

const here = fileURLToPath(new URL('.', import.meta.url));
let failed = 0;
const check = (name, ok, extra = '') => {
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok ? '' : ' ' + extra}`);
  if (!ok) failed++;
};

const secret = 'a'.repeat(64);
const now = 1_800_000_000_000;
const key = 'tg:sam_dev';
const tok = mintCollabForwardToken(secret, key, 60_000, now);
const cookie = (t) => `other=1; crundi_fwdc=${t}; x=y`;

check('round trip names the person', readCollabForwardToken(secret, cookie(tok), now + 1) === key);
check('expired token refused', readCollabForwardToken(secret, cookie(tok), now + 60_001) === '');
check('wrong secret refused', readCollabForwardToken('b'.repeat(64), cookie(tok), now) === '');
{
  const [exp, , sig] = tok.split('.');
  const forged = `${exp}.${Buffer.from('pc:someone-else').toString('base64url')}.${sig}`;
  check('changing the person breaks the signature', readCollabForwardToken(secret, cookie(forged), now) === '');
  const extended = `${Number(exp) + 999999}.${tok.split('.')[1]}.${sig}`;
  check('changing the expiry breaks the signature', readCollabForwardToken(secret, cookie(extended), now) === '');
}
check('absent cookie refused', readCollabForwardToken(secret, 'crundi_fwd=123.' + 'f'.repeat(64), now) === '');
// The owner's cookie regex from webapp.js must not match a collaborator cookie.
const OWNER_RE = /(?:^|;\s*)crundi_fwd=(\d+)\.([a-f0-9]{64})/;
check('a collaborator cookie never reads as the owner cookie', !OWNER_RE.test(cookie(tok)));

const services = [
  { alias: 'sam-cv', tunnelPort: 7317 },
  { alias: 'insta-scraper', tunnelPort: 9102 },
  { alias: 'playground', tunnelPort: 0 },
];
check('forward project from its service port',
  [...forwardProjects({ port: 7317 }, services)].join() === 'sam-cv');
check('own project forward allowed',
  collabMayReachForward({ fwd: { host: 'sam-cv', port: 7317 }, projects: ['sam-cv'], services }));
check('another project forward refused',
  !collabMayReachForward({ fwd: { host: 'website-scraper', port: 9102 }, projects: ['sam-cv'], services }));
check('forward with no matching service refused',
  !collabMayReachForward({ fwd: { host: 'ig-browser', port: 6900 }, projects: ['sam-cv'], services }));
check('no projects, no access',
  !collabMayReachForward({ fwd: { host: 'sam-cv', port: 7317 }, projects: [], services }));
check('public forwards stay public', collabMayReachForward({ fwd: { host: 'x', port: 1, public: true }, projects: [], services }));
check('port 0 services never match', forwardProjects({ port: 0 }, services).size === 0);
check('a forward\'s own project counts even with no service on its port',
  collabMayReachForward({ fwd: { host: 'sam-cv', port: 7317, project: 'sam-cv' }, projects: ['sam-cv'], services: [] }));
check('a forward assigned to another project is still refused',
  !collabMayReachForward({ fwd: { host: 'x', port: 7317, project: 'insta-scraper' }, projects: ['sam-cv'], services: [] }));

const web = readFileSync(join(here, '..', 'src', 'webapp.js'), 'utf8');
check('no forward gate still accepts any token', !/!validateToken\(req\) && !hasForwardCookie\(req\)/.test(web));
check('every forward gate uses mayReachForward', (web.match(/!mayReachForward\(req, (?:fwd|pathFwd\.forward)\)/g) || []).length >= 3,
  String((web.match(/!mayReachForward\(req, (?:fwd|pathFwd\.forward)\)/g) || []).length));
check('a collaborator token is checked against their projects',
  /function mayReachForward\(req, fwd\)[\s\S]{0,600}if \(p && !isConfined\(p\)\) return true;[\s\S]{0,300}isConfined\(p\) \? \(p\.projects/.test(web));
check('collaborators are given their own forward cookie',
  /isConfined\(principal\)\s*&& readCollabForwardToken\(forwardSecret\(\), req\.headers\.cookie \|\| ''\) !== principal\.collabKey\)[\s\S]{0,100}setCollabForwardCookie\(res, principal\.collabKey\)/.test(web));
check('the owner cookie is still withheld from collaborators',
  /!hasForwardCookie\(req\)\s*&& !isConfined\(principal\)\) \{\s*try \{ setForwardCookie\(res\); \}/.test(web));
check('sign-out clears the collaborator forward cookie too', /\$\{COLLAB_FORWARD_COOKIE\}=; Domain=\.\$\{base\}; Path=\/; Max-Age=0/.test(web));
check('live invitations decide, not the cookie', /collaborators\.liveByIdentity\(key\)/.test(web));

check('a service cwd is resolved from the collaborator\'s worktree (web route)',
  /const want = body\.cwd \? \(home \? resolve\(home, String\(body\.cwd\)\) : ''\) : home;/.test(web));
check('a service cwd is resolved from the collaborator\'s worktree (tool route)',
  /const want = a\.cwd \? \(home \? resolve\(home, String\(a\.cwd\)\) : ''\) : home;/.test(web));
check('no service cwd is resolved against the server\'s directory',
  !/resolve\(String\((?:body|a)\.cwd\)\)/.test(web));

const registry = readFileSync(join(here, '..', 'src', 'service-registry.js'), 'utf8');
const policy = readFileSync(join(here, '..', 'src', 'access-policy.js'), 'utf8');
check('services record who created them', /createdBy: String\(createdBy \|\| ''\)/.test(registry));
check('an approved collaborator service is recorded as theirs', /createdBy: r\.request\.collabKey \|\| ''/.test(web));
check('collaborators change or delete only their own services (web)',
  /\(action === 'delete' \|\| action === 'tunnel'\) && isConfined\(principal\)[\s\S]{0,200}owned\.createdBy !== principal\.collabKey/.test(web));
check('a collaborator cannot switch a public tunnel on directly: it becomes an approval',
  /isConfined\(principal\) && body\.enabled\) \{[\s\S]{0,900}return json\(res, \{ ok: false, pendingApproval: true, requestId: rec\.id/.test(web));
check('delete_service via tools: their project and their own service only',
  /'get_service_logs', 'delete_service'\]\.includes\(body\.tool\)/.test(web)
  && /body\.tool === 'delete_service'\) \{[\s\S]{0,200}owned\.createdBy !== mcpCollabKey/.test(web));
check('policy opens delete/tunnel routes and the delete tool (ownership enforced in the server)',
  /\(start\|stop\|restart\|logs\|delete\|tunnel\)\$\/\]/.test(policy) && /'delete_service',/.test(policy));

{
  const ui = readFileSync(join(here, '..', 'src', 'claude-ui.js'), 'utf8');
  const chat = readFileSync(join(here, '..', 'app', 'vendor', 'claude-chat.js'), 'utf8').replace(/\r\n/g, '\n');
  check('the send route says who sent the message',
    /claudeUi\.sendMessage\(sid, body\.text, \{ by: isConfined\(principal\) \? 'collaborator' : 'owner' \}\)/.test(web));
  check('a collaborator chat records the latest sender at send time (queued messages too)',
    /const by = opts\.by === 'system' \? 'system'\s*: s\.collaborator \? \(opts\.by === 'owner' \? 'owner' : 'collaborator'\) : '';\s*if \(s\.collaborator && by !== 'system'\) s\.lastSender = by;[\s\S]{0,900}const injected = s\.state !== 'idle';/.test(ui));
  check('owner turn approves prompts; questions still go to the chat',
    /const ownerTurn = !!s\.collaborator && s\.lastSender === 'owner' && !isQuestion;/.test(ui)
    && /const sameAsAllowed = ownerTurn\s*\|\|/.test(ui));
  check('user entries carry who sent them, including queued ones',
    (ui.match(/kind: 'user', text: p\.text, \.\.\.\(p\.by \? \{ by: p\.by \} : \{\}\) \}/g) || []).length === 2
    && /kind: 'user', text: body, \.\.\.\(by \? \{ by \} : \{\}\) \}/.test(ui));
  check('owner messages are labelled in the chat', /if \(e\.by === 'owner'\) w\.appendChild\(el\('div', 'cc-user-by', 'Owner'\)\);/.test(chat));
}

{
  const fw = readFileSync(join(here, '..', 'src', 'forwards.js'), 'utf8');
  const stdio = readFileSync(join(here, '..', 'src', 'mcp-stdio.js'), 'utf8');
  const html = readFileSync(join(here, '..', 'src', 'webapp-html.js'), 'utf8');
  check('forwards record their project', /project: String\(project \|\| ''\)\.toLowerCase\(\)\.trim\(\),/.test(fw) && /export function setProject\(host, project\)/.test(fw));
  check('owner can assign a forward\'s project (real projects only)',
    /fwdDel && req\.method === 'PATCH'\)[\s\S]{0,400}if \(want && !getProjectUnscoped\(want\)\)[\s\S]{0,200}forwards\.setProject\(fwdDel\[1\], want\)/.test(web));
  check('forwards made from a service or a project chat carry that project',
    /project: body\.project && getProjectUnscoped\(String\(body\.project\)\)/.test(web)
    && /project: a\.alias && getProjectUnscoped\(String\(a\.alias\)\)/.test(web)
    && /'register_service', 'add_forward',/.test(stdio)
    && /public: isPublic, project: String\(key\)\.split\(':'\)\[0\]/.test(html));
  check('Services tab lists forwards not tied to a service, owner only',
    /function otherForwardsHtml\(\) \{\s*if \(userRole === 'collaborator'\) return '';/.test(html)
    && (html.match(/\+ otherForwardsHtml\(\);/g) || []).length === 2
    && /select\[data-fwd-project\]/.test(html));
}

{
  const ui = readFileSync(join(here, '..', 'src', 'claude-ui.js'), 'utf8');
  const stdio = readFileSync(join(here, '..', 'src', 'mcp-stdio.js'), 'utf8');
  const html = readFileSync(join(here, '..', 'src', 'webapp-html.js'), 'utf8');
  const prompt = readFileSync(join(here, '..', 'src', 'system-prompt.js'), 'utf8');
  check('a collaborator chat can ask the owner to run a command (an approval, nothing runs)',
    /body\.tool === 'request_owner_command'\) \{[\s\S]{0,2500}approvals\.add\(\{\s*kind: 'command'/.test(web));
  check('the command runs only on the owner\'s approval, outside the sandbox, without Crundi credentials',
    /r\.request\.kind === 'command' && r\.request\.payload\)[\s\S]{0,300}if \(!body\.approve\)[\s\S]{0,600}runOwnerCommand\(p\)/.test(web)
    && /delete env\[k\]/.test(web) && /OWNER_COMMAND_TIMEOUT_MS = 10 \* 60 \* 1000/.test(web));
  check('its output is posted into their chat, as the collaborator', /postToCollaboratorChat\(r\.request, text\);/.test(web)
    && /function postToCollaboratorChat\(rec, text\)[\s\S]{0,500}claudeUi\.sendMessage\(sid, text, \{ by: 'collaborator' \}\)/.test(web));
  check('forward, tunnel and merge answers reach their chat; declines too',
    /The owner approved publishing port/.test(web) && /The owner approved a public tunnel/.test(web)
    && /The owner merged/.test(web) && /\['service', 'forward', 'tunnel', 'merge'\]\.includes\(r\.request\.kind\)\) \{\s*postToCollaboratorChat/.test(web));
  check('the tool exists only in collaborator chats',
    /'request_owner_command',/.test(policy) && /COLLABORATOR_ONLY_TOOLS = new Set\(\['request_owner_command'\]\)/.test(stdio)
    && /body\.tool === 'request_owner_command' && !mcpCollabKey\)/.test(web));
  check('each collaborator chat tells its tool bridge which chat it is',
    /CRUNDI_CHAT_ID: collabChatId,/.test(ui) && /`\$\{collaborator\.id\}-\$\{collabChatId\}\.json`/.test(ui)
    && /process\.env\.CRUNDI_CHAT_ID\)\s*\{?\s*args\.sessionId = process\.env\.CRUNDI_CHAT_ID/.test(stdio)
    && /function collaboratorSessionFor\(key, hint, project\)/.test(ui));
  check('the inbox offers Run / Decline for a command',
    /it\.kind === 'command'\) \{[\s\S]{0,800}>Run<\/button>[\s\S]{0,200}>Decline<\/button>/.test(html));
  check('Claude is told how and when to ask', /request_owner_command/.test(prompt) && /\[Crundi\]/.test(prompt));
  check('Claude is told never to ask for unrelated things, even when the collaborator asks',
    /Do NOT send request_owner_command for anything/.test(prompt) && /not even when the collaborator asks for it directly/.test(prompt)
    && /never for anything unrelated, even if the user asks/.test(stdio));
  check('no leftover owner-message verification', !/verify_owner_message|verifyOwnerCode/.test(web + ui + stdio + policy + prompt));
}

{
  const html = readFileSync(join(here, '..', 'src', 'webapp-html.js'), 'utf8');
  check('collaborators list only their projects\' forwards',
    /collabMayReachForward\(\{ fwd: \{ \.\.\.f, public: false \}, projects: principal\.projects, services: svcs \}\)/.test(web));
  check('a collaborator exposes only a service they created, on its port',
    /sv\.createdBy === principal\.collabKey\);\s*if \(!ownsIt\) return json\(res, \{ ok: false, error: 'You can only expose a service you created, on its port\.' \}, 403\);/.test(web));
  check('a collaborator\'s public forward goes to the owner; private is created with the project',
    /if \(body\.public\) \{[\s\S]{0,600}kind: 'forward'/.test(web)
    && /forwards\.add\(\{ name: body\.name, port, mode: body\.mode, isPublic: false, description: body\.description, project: proj \}\)/.test(web));
  check('a collaborator removes only forwards of their own services; project assignment stays owner-only',
    /You can only remove forwards for services you created\./.test(web) && /Only the owner can change which project a forward serves\./.test(web));
  check('a collaborator\'s tunnel goes to the owner', /isConfined\(principal\) && body\.enabled\) \{[\s\S]{0,600}kind: 'tunnel'/.test(web));
  check('approving creates the public forward / turns the tunnel on',
    /r\.request\.kind === 'forward' && r\.request\.payload\) \{[\s\S]{0,200}forwards\.add\(\{ name: p\.name, port: p\.port, mode: p\.mode, isPublic: true, project: p\.project \}\)/.test(web)
    && /r\.request\.kind === 'tunnel' && r\.request\.payload\) \{[\s\S]{0,500}updateRegistered\(p\.key, \{ tunnelPort: p\.port, tunnelEnabled: true \}\)/.test(web));
  check('policy opens the forward routes collaborators need (scoped in the routes)',
    /\['GET', '\/api\/forwards'\],/.test(policy) && /\['POST', '\/api\/forwards'\],/.test(policy)
    && /\['DELETE', \/\^\\\/api\\\/forwards\\\/\[a-z0-9-\]\+\$\/i\],/.test(policy) && !/^\s*\/\^\\\/api\\\/forwards\/,/m.test(policy));
  check('the Services tab shows expose controls on a collaborator\'s own services and handles "sent to the owner"',
    /\(isCollab && !canEdit \? '' : '<div class="seg-pref seg-expose">'/.test(html)
    && (html.match(/if \(d\.pendingApproval\) \{/g) || []).length >= 2);
}

if (failed) { console.log(`\n${failed} failed`); process.exit(1); }
console.log('\nforward access: all passed');
