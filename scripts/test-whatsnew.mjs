#!/usr/bin/env node
// "What's new": who is shown which edition.
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, readFileSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { compareVersions, isPrerelease, listEditions, plan, loadState, markSeen, parseVersion } from '../src/whatsnew.js';

let failed = 0;
const ok = (cond, name, detail = '') => { console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}${cond || !detail ? '' : '\n      ' + detail}`); if (!cond) failed++; };
const t = mkdtempSync(join(tmpdir(), 'crundi-wn-'));
const ed = (v, ids) => { mkdirSync(join(t, 'wn', v), { recursive: true }); writeFileSync(join(t, 'wn', v, 'steps.json'), JSON.stringify({ steps: ids.map((id) => ({ id, title: 'T ' + id, art: id + '.html' })) })); for (const id of ids) writeFileSync(join(t, 'wn', v, id + '.html'), '<div>' + id + '</div>'); };
const ids = (p) => p.steps.map((s) => s.id).join(',');

// ─── versions ───
const order = ['1.19.24-dev.1', '1.19.24-dev.2', '1.19.24-dev.10', '1.19.24', '1.19.25-dev.1', '1.20.0'];
ok(order.every((v, i) => i === 0 || compareVersions(order[i - 1], v) < 0), 'versions sort as releases do: dev builds below their release, dev.10 above dev.2');
ok(compareVersions('1.19.24', '1.19.24') === 0 && compareVersions('v1.19.24', '1.19.24') === 0, 'equal versions are equal, with or without the v');
ok(isPrerelease('1.19.24-dev.1') && !isPrerelease('1.19.24') && parseVersion('latest') === null, 'a dev build is known as one, and a word is not a version');

// ─── editions on disk ───
ed('1.19.24', ['media', 'embeds']); ed('1.19.25', ['boards']); ed('1.19.26-dev.1', ['voice']); ed('1.19.26', ['voice', 'themes']);
mkdirSync(join(t, 'wn', 'notes')); writeFileSync(join(t, 'wn', 'README.md'), 'x');
mkdirSync(join(t, 'wn', '1.19.27')); writeFileSync(join(t, 'wn', '1.19.27', 'steps.json'), '{ not json');
const E = listEditions(join(t, 'wn'));
ok(E.map((e) => e.version).join(' ') === '1.19.24 1.19.25 1.19.26-dev.1 1.19.26', 'editions are read oldest first; other folders and a broken one are skipped', E.map((e) => e.version).join(' '));
ok(E[0].steps[0].art === '<div>media</div>' && E[0].steps[0].version === '1.19.24', 'a step carries its animation and the version it came with');
writeFileSync(join(t, 'wn', '1.19.24', 'steps.json'), JSON.stringify({ steps: [{ id: 'media', title: 'M', art: '../../../etc/passwd' }, { id: 'embeds', title: 'E', art: 'embeds.html' }, { id: '../x', title: 'bad id' }, { id: 'untitled' }] }));
const E2 = listEditions(join(t, 'wn'));
ok(E2[0].steps.length === 2 && E2[0].steps[0].art === '' && E2[0].steps[1].art === '<div>embeds</div>', 'an animation name that reaches outside the folder is ignored; a step with a bad id or no title is dropped');

// ─── who sees what ───
const P = (o) => plan({ editions: E, channel: 'production', ...o });
ok(ids(P({ current: '1.19.24', state: { seen: '1.19.23' } })) === 'media,embeds', 'an update shows the new edition');
ok(ids(P({ current: '1.19.26', state: { seen: '1.19.23' } })) === 'media,embeds,boards,voice,themes', 'jumping several versions shows each edition on the way, oldest first');
ok(ids(P({ current: '1.19.26', state: { seen: '1.19.25' } })) === 'voice,themes', 'and only the ones not yet seen');
ok(ids(P({ current: '1.19.26', state: { seen: '1.19.26' } })) === '', 'nothing new, nothing shown');
ok(ids(P({ current: '1.19.25', state: { seen: '1.19.23' } })) === 'media,embeds,boards', 'an edition newer than what is installed is not shown');
ok(ids(P({ current: '1.19.26', state: { seen: '0.0.0', fresh: true } })) === 'voice,themes', 'a new install sees the latest edition only');
ok(ids(P({ current: '1.19.26', state: { seen: '1.19.26' }, all: true })) === 'voice,themes', 'asked for from Settings: the latest edition, seen or not');

// ─── dev editions ───
ok(P({ current: '1.19.26', state: { seen: '1.19.25' } }).editions.join() === '1.19.26', 'a production install is never shown a dev edition, though its version sorts above it');
ok(ids(P({ current: '1.19.26-dev.1', state: { seen: '1.19.25' } })) === '', 'not even when it is running that dev build');
ok(ids(plan({ editions: E, current: '1.19.26-dev.1', channel: 'dev', state: { seen: '1.19.25' } })) === 'voice', 'the dev channel is');
const afterDev = { seen: '1.19.26-dev.1', ids: ['media', 'embeds', 'boards', 'voice'] };
ok(ids(plan({ editions: E, current: '1.19.26', channel: 'dev', state: afterDev })) === 'themes', 'and when the release follows, a step already seen in the dev edition is not repeated');

// ─── the record ───
const f1 = join(t, 'new.json'), f2 = join(t, 'old.json');
ok(loadState(f1, { usedBefore: false }).fresh === true && JSON.parse(readFileSync(f1, 'utf8')).fresh === true, 'no record and no sign of use: a new install, and that is written down');
ok(loadState(f1, { usedBefore: true }).fresh === true, 'once decided it stays decided');
const old = loadState(f2, { usedBefore: true });
ok(old.fresh === false && ids(P({ current: '1.19.25', state: old })) === 'media,embeds,boards', 'no record on an install already in use: everything it has not been shown');
const done = markSeen(f2, { editions: E, current: '1.19.25', channel: 'production' });
ok(done.seen === '1.19.25' && done.ids.join() === 'media,embeds,boards' && ids(P({ current: '1.19.25', state: loadState(f2) })) === '', 'marking it seen records the version and the steps, and nothing is shown again');
markSeen(f1, { editions: E, current: '1.19.26', channel: 'production' });
ok(loadState(f1).fresh === false && !loadState(f1).ids.includes('voice-dev'), 'a new install is an ordinary one from then on');

// ─── what ships ───
// fileURLToPath, not URL.pathname: on Windows the latter is "/D:/..." and names nothing.
const realDir = fileURLToPath(new URL('../app/vendor/whatsnew/', import.meta.url));
const R = listEditions(realDir);
ok(R.length > 0, 'there is at least one edition in the repo');
const pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version;
ok(R.every((e) => e.steps.every((s) => s.art && s.body && s.tag)), 'every step that ships has an animation, a tag and a line of text', R.flatMap((e) => e.steps.filter((s) => !s.art || !s.body || !s.tag).map((s) => e.version + '/' + s.id)).join(', '));
const allIds = R.filter((e) => !isPrerelease(e.version)).flatMap((e) => e.steps.map((s) => s.id));
ok(new Set(allIds).size === allIds.length, 'no step id is used twice across the released editions', allIds.join(','));
ok(R.every((e) => e.steps.every((s) => !/<script|javascript:|\son[a-z]+\s*=/i.test(s.art))), 'the animations are markup and styles only: no scripts');
ok(R.every((e) => readdirSync(join(realDir, e.version)).filter((n) => n.endsWith('.html')).every((n) => e.steps.some((s) => readFileSync(join(realDir, e.version, n), 'utf8') === s.art))), 'no animation file is left over without a step');
const newest = R.length ? R[R.length - 1].version : '';
ok(!!newest && (compareVersions(newest, pkg) <= 0 || isPrerelease(pkg)), `the newest edition (${newest || 'none found'}) is not ahead of the package version (${pkg})`);

try { rmSync(t, { recursive: true, force: true }); } catch { /* leave it */ }
console.log(failed ? `\n${failed} FAILED` : '\nAll whats-new checks passed.');
process.exit(failed ? 1 : 0);
