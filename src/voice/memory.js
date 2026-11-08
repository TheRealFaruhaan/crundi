/**
 * voice/memory.js: one memory for the whole assistant (Phase 2, D26, D30, D47).
 *
 *   conversation  the running conversation, kept on disk (append-only; it
 *                 survives a restart), compacted while idle into a summary;
 *   notebook      Claude's memory tool: standing facts and preferences;
 *   index         keyword + meaning search over every turn, every summary and
 *                 the notebook (memory-index.js), as the memory_search tool.
 *
 * Everything is in <dataDir>/voice-memory, private to this user.
 */

import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { createNotebook } from './notebook.js';
import { createMemoryIndex } from './memory-index.js';

export function createVoiceMemory({ dataDir, index: givenIndex } = {}) {
  const dir = join(dataDir, 'voice-memory');
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true, mode: 0o700 });
  const convFile = join(dir, 'conversation.json');
  const index = givenIndex || createMemoryIndex({ dir });
  const notebook = createNotebook(join(dir, 'memories'), {
    onChange: (path, text) => index.add('note', text ? `${path}\n${text}` : '', { ref: path }),
  });

  const store = {
    /** The saved conversation, or [] (a broken file is set aside, not lost). */
    load() {
      try { const h = JSON.parse(readFileSync(convFile, 'utf-8')); return Array.isArray(h?.history) ? h.history : []; }
      catch {
        if (existsSync(convFile)) { try { renameSync(convFile, convFile + '.broken-' + Date.now()); } catch { /* leave it */ } }
        return [];
      }
    },
    save(history) {
      const tmp = convFile + '.tmp';
      writeFileSync(tmp, JSON.stringify({ savedAt: new Date().toISOString(), history }), { mode: 0o600 });
      renameSync(tmp, convFile);
    },
  };

  const searchTool = {
    name: 'memory_search',
    description: 'Search everything said before, across every conversation (compacted ones too), and the notebook: by keywords and by meaning. Use it when the owner refers to something from earlier ("what did we decide about…", "that card from yesterday").',
    input_schema: {
      type: 'object', additionalProperties: false,
      properties: {
        query: { type: 'string', description: 'What to look for, in plain words' },
        since: { type: 'string', description: 'Optional: only after this ISO date' },
        project: { type: 'string', description: 'Optional: only this project (and general memories)' },
      },
      required: ['query'],
    },
    run: async ({ query, since, project }) => {
      const r = await index.search(query, { since: since ? Date.parse(since) || 0 : 0, project: project || null });
      return { results: r.results, by: r.meaning ? 'keywords and meaning' : 'keywords only (meaning search not loaded)' };
    },
  };

  return {
    store, notebook, index, searchTool,
    /** A finished turn, for later search. */
    remember({ said, speak = '', show = '', at = Date.now() }) {
      index.add('turn', `Owner: ${said}\nCru: ${speak}${show ? '\n' + show : ''}`, { at });
    },
    rememberSummary(text, at = Date.now()) { index.add('summary', text, { at }); },
    /** The latest turns, newest first (the voice screen's chat view). */
    turns: (opts) => index.turns(opts),
  };
}
