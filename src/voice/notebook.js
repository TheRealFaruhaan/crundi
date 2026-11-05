/**
 * voice/notebook.js: the voice assistant's long-term notebook (Phase 2,
 * §2.5.3): Claude's memory tool (memory_20250818), with its files kept on
 * this server in <dataDir>/voice-memory/memories, private to this user.
 *
 * The model writes what is worth keeping across conversations: Master's
 * preferences, standing facts ("the API project is called…"), threads in
 * progress. It survives compaction, and Master can read and edit it on the
 * /voice page.
 *
 * Paths are always under /memories; anything that climbs out is refused.
 * Secrets never belong here (D16): a file that looks like it holds a key or
 * password is refused too.
 */

import { existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, statSync, writeFileSync, chmodSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';

const MAX_FILE = 64 * 1024;
const SECRETISH = /\b(sk-ant-[a-z0-9-]{10,}|sk-[a-z0-9]{20,}|ghp_[a-z0-9]{20,}|xox[bp]-[a-z0-9-]{10,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----)/i;

export function createNotebook(dir, { onChange = () => {} } = {}) {
  const root = resolve(dir);
  const ensure = () => { if (!existsSync(root)) { mkdirSync(root, { recursive: true, mode: 0o700 }); } };

  /** /memories/x.md → the file on disk, or throw. */
  function file(p) {
    const want = String(p || '').replace(/\\/g, '/');
    if (!want.startsWith('/memories')) throw new Error('Paths start with /memories');
    const rest = want.slice('/memories'.length).replace(/^\/+/, '');
    const full = resolve(root, rest || '.');
    const rel = relative(root, full);
    if (rel === '..' || rel.startsWith('..' + sep)) throw new Error('That path is outside /memories');
    return full;
  }
  const shown = (full) => '/memories' + (relative(root, full) ? '/' + relative(root, full).split(sep).join('/') : '');

  function view({ path = '/memories', view_range } = {}) {
    ensure();
    const full = file(path);
    if (!existsSync(full)) throw new Error(`${path} does not exist`);
    if (statSync(full).isDirectory()) {
      const list = [];
      const walk = (d, depth) => {
        for (const e of readdirSync(d, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
          if (e.name.startsWith('.')) continue;
          const f = join(d, e.name);
          list.push(`${e.isDirectory() ? '' : statSync(f).size + '\t'}${shown(f)}${e.isDirectory() ? '/' : ''}`);
          if (e.isDirectory() && depth < 2) walk(f, depth + 1);
        }
      };
      walk(full, 0);
      return `Directory ${shown(full)}:\n${list.join('\n') || '(empty)'}`;
    }
    const lines = readFileSync(full, 'utf-8').split('\n');
    const [a, b] = Array.isArray(view_range) ? view_range : [1, lines.length];
    return lines.slice(Math.max(0, a - 1), b === -1 ? lines.length : b).map((l, i) => `${String(i + Math.max(1, a)).padStart(6)}\t${l}`).join('\n');
  }

  function write(full, text) {
    if (text.length > MAX_FILE) throw new Error('That is too long for one memory file (64 KB)');
    if (SECRETISH.test(text)) throw new Error('That looks like a key or password. Secrets are never kept in memory.');
    mkdirSync(dirname(full), { recursive: true, mode: 0o700 });
    writeFileSync(full, text, { mode: 0o600 });
    try { chmodSync(full, 0o600); } catch { /* not ours to fix */ }
    onChange(shown(full), text);
  }

  const commands = {
    view,
    create({ path, file_text = '' }) { ensure(); const f = file(path); write(f, String(file_text)); return `File created: ${shown(f)}`; },
    str_replace({ path, old_str, new_str = '' }) {
      const f = file(path);
      if (!existsSync(f)) throw new Error(`${path} does not exist`);
      const text = readFileSync(f, 'utf-8');
      const n = text.split(String(old_str)).length - 1;
      if (n !== 1) throw new Error(n === 0 ? 'That text is not in the file' : `That text is in the file ${n} times; make it unique`);
      write(f, text.replace(String(old_str), String(new_str)));
      return `Edited ${shown(f)}`;
    },
    insert({ path, insert_line = 0, insert_text = '' }) {
      const f = file(path);
      const lines = existsSync(f) ? readFileSync(f, 'utf-8').split('\n') : [];
      const at = Math.max(0, Math.min(lines.length, Number(insert_line) || 0));
      lines.splice(at, 0, ...String(insert_text).split('\n'));
      write(f, lines.join('\n'));
      return `Inserted into ${shown(f)}`;
    },
    delete({ path }) {
      const f = file(path);
      if (f === root) throw new Error('The /memories folder itself is kept');
      if (!existsSync(f)) throw new Error(`${path} does not exist`);
      rmSync(f, { recursive: true, force: true });
      onChange(shown(f), null);
      return `Deleted ${path}`;
    },
    rename({ old_path, new_path }) {
      const a = file(old_path), b = file(new_path);
      if (!existsSync(a)) throw new Error(`${old_path} does not exist`);
      if (existsSync(b)) throw new Error(`${new_path} already exists`);
      mkdirSync(dirname(b), { recursive: true, mode: 0o700 });
      renameSync(a, b);
      onChange(shown(a), null);
      onChange(shown(b), readFileSync(b, 'utf-8'));
      return `Renamed to ${new_path}`;
    },
  };

  return {
    /** The tool, as the agent's tool list takes it: the Anthropic-defined memory tool. */
    tool: {
      name: 'memory',
      def: { type: 'memory_20250818', name: 'memory' },
      run: (input = {}) => {
        const fn = commands[input.command];
        if (!fn) throw new Error(`Unknown memory command: ${input.command}`);
        return fn(input);
      },
    },
    /** For the /voice page: every file and its text. */
    list() {
      ensure();
      const out = [];
      const walk = (d) => {
        for (const e of readdirSync(d, { withFileTypes: true })) {
          const f = join(d, e.name);
          if (e.isDirectory()) walk(f);
          else out.push({ path: shown(f), text: readFileSync(f, 'utf-8'), size: statSync(f).size });
        }
      };
      walk(root);
      return out.sort((a, b) => a.path.localeCompare(b.path));
    },
    /** Master edits a file on the /voice page ('' text deletes it). */
    save(path, text) {
      ensure();
      const f = file(path);
      if (f === root) throw new Error('Choose a file under /memories');
      if (text === '' || text == null) { if (existsSync(f)) rmSync(f, { force: true }); onChange(shown(f), null); return { ok: true }; }
      write(f, String(text));
      return { ok: true };
    },
    commands,
  };
}
