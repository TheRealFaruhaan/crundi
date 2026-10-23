#!/usr/bin/env node
// The Git tab's history: commits newest first, paging, one commit's changes,
// and an empty repository answering with no commits rather than an error.
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { gitLog, gitShow, parseLog } from '../src/git-log.js';

let failed = 0;
const ok = (cond, name, detail = '') => { console.log(`${cond ? 'PASS' : 'FAIL'}  ${name}${cond || !detail ? '' : '\n      ' + detail}`); if (!cond) failed++; };
const dir = mkdtempSync(join(tmpdir(), 'crundi-gitlog-'));
const git = (...a) => execFileSync('git', a, { cwd: dir, encoding: 'utf8', env: { ...process.env, GIT_AUTHOR_NAME: 'Ada', GIT_AUTHOR_EMAIL: 'ada@example.com', GIT_COMMITTER_NAME: 'Ada', GIT_COMMITTER_EMAIL: 'ada@example.com' } });
try {
  git('init', '-q', '-b', 'main');
  ok(gitLog(dir).commits.length === 0 && gitLog(dir).more === false, 'a repository with no commits has an empty history');

  writeFileSync(join(dir, 'a.txt'), 'one\n'); git('add', '.'); git('commit', '-q', '-m', 'First');
  writeFileSync(join(dir, 'a.txt'), 'one\ntwo\n'); writeFileSync(join(dir, 'b.txt'), 'b\n'); git('add', '.');
  git('commit', '-q', '-m', 'Second: add b', '-m', 'Body line with | and, commas');
  for (let i = 3; i <= 5; i++) { writeFileSync(join(dir, 'c.txt'), String(i)); git('add', '.'); git('commit', '-q', '-m', `Commit ${i}`); }

  const all = gitLog(dir);
  ok(all.commits.length === 5 && !all.more, 'all five commits, nothing more', JSON.stringify(all.commits.map(c => c.subject)));
  ok(all.commits[0].subject === 'Commit 5' && all.commits[4].subject === 'First', 'newest first');
  ok(all.commits[0].refs.some(r => r.includes('main')), 'the branch shows on its commit', all.commits[0].refs.join(','));
  ok(all.commits[3].body === 'Body line with | and, commas' && all.commits[3].author === 'Ada' && all.commits[3].short.length === 7, 'body, author and short id come through');
  ok(/^\d{4}-\d\d-\d\dT/.test(all.commits[0].date), 'dates are ISO', all.commits[0].date);

  const page = gitLog(dir, { limit: 2, skip: 1 });
  ok(page.commits.length === 2 && page.more && page.commits[0].subject === 'Commit 4', 'a page in the middle says there is more');

  const second = gitShow(dir, all.commits[3].hash);
  ok(second.subject === 'Second: add b' && second.files.length === 2, 'a commit lists its files', JSON.stringify(second.files));
  ok(second.files.find(f => f.file === 'a.txt')?.add === 1 && /\+two/.test(second.diff) && !second.truncated, 'with line counts and the patch');

  let threw = '';
  try { gitShow(dir, 'HEAD; rm -rf /'); } catch (e) { threw = e.message; }
  ok(threw === 'Not a commit id', 'only a commit id is taken');

  ok(parseLog('garbage\x1e').length === 0, 'output that is not ours parses to nothing');
} finally { rmSync(dir, { recursive: true, force: true }); }
if (failed) { console.log(`\n${failed} failed`); process.exit(1); }
console.log('\nall passed');
