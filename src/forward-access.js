/**
 * forward-access.js — who may open a private forward.
 *
 * A private forward (myapp.<domain>) is reached with a cookie, because a page
 * on another hostname has no access token. The owner's cookie opens every
 * private forward and carries no identity, so collaborators were never given
 * it — which left them unable to open even the forward for their own
 * project's dev server.
 *
 * Collaborators get their own cookie instead. It names the person, is signed
 * so it cannot be edited, and grants nothing by itself: on every request their
 * CURRENT invitations are looked up, so expiry and revocation apply at once,
 * and only forwards belonging to their projects are let through.
 *
 * A forward belongs to a project through its port. The Services tab creates a
 * forward FROM a service, on that service's exposed port (tunnelPort), and
 * that is the only link between the two that exists for forwards made before
 * this — so it is the link used.
 */
import { createHmac, timingSafeEqual } from 'crypto';

export const COLLAB_FORWARD_COOKIE = 'crundi_fwdc';

// <expiry>.<base64url person key>.<hmac>
const TOKEN_RE = /(?:^|;\s*)crundi_fwdc=(\d+)\.([A-Za-z0-9_-]+)\.([a-f0-9]{64})/;

// Domain-separated from the owner's forward signature (which covers only the
// expiry), so neither token can ever pass as the other.
function sign(secret, exp, key) {
  return createHmac('sha256', String(secret)).update(`collab-forward\n${exp}\n${key}`).digest('hex');
}

export function mintCollabForwardToken(secret, key, ttlMs, now = Date.now()) {
  const exp = now + ttlMs;
  return `${exp}.${Buffer.from(String(key), 'utf8').toString('base64url')}.${sign(secret, exp, key)}`;
}

/** The person key in a valid, unexpired collaborator forward cookie, or ''. */
export function readCollabForwardToken(secret, cookieHeader, now = Date.now()) {
  const m = TOKEN_RE.exec(String(cookieHeader || ''));
  if (!m) return '';
  const exp = Number(m[1]);
  if (!Number.isFinite(exp) || now >= exp) return '';
  let key = '';
  try { key = Buffer.from(m[2], 'base64url').toString('utf8'); } catch { return ''; }
  if (!key) return '';
  const want = Buffer.from(sign(secret, exp, key), 'utf8');
  const got = Buffer.from(m[3], 'utf8');
  return want.length === got.length && timingSafeEqual(want, got) ? key : '';
}

/**
 * Projects a forward belongs to: the one recorded on it (set when it was made
 * from a service or a project's chat, or assigned by the owner), plus any
 * project with a service exposed on its port.
 */
export function forwardProjects(fwd, services = []) {
  const out = new Set();
  if (fwd && fwd.project) out.add(String(fwd.project).toLowerCase());
  const port = Number(fwd && fwd.port);
  if (!port) return out;
  for (const s of services || []) {
    if (s && s.alias && Number(s.tunnelPort) === port) out.add(String(s.alias).toLowerCase());
  }
  return out;
}

/** May a collaborator holding these projects open this forward? */
export function collabMayReachForward({ fwd, projects = [], services = [] } = {}) {
  if (!fwd) return false;
  if (fwd.public) return true;
  const mine = new Set((projects || []).map(p => String(p).toLowerCase()));
  if (!mine.size) return false;
  for (const p of forwardProjects(fwd, services)) if (mine.has(p)) return true;
  return false;
}
