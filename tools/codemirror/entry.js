// Source for app/vendor/codemirror.js (built with esbuild into an IIFE that
// sets window.CM). The file viewer uses EditorView, EditorState, Compartment,
// getLangExtension, basicSetup and oneDark; notes code blocks use the rest.
import { EditorView, keymap, drawSelection, highlightSpecialChars, placeholder } from '@codemirror/view';
import { EditorState, Compartment, Prec } from '@codemirror/state';
import { basicSetup } from 'codemirror';
import { indentOnInput, bracketMatching, syntaxHighlighting, defaultHighlightStyle, StreamLanguage, indentUnit } from '@codemirror/language';
import { history, defaultKeymap, historyKeymap, indentWithTab } from '@codemirror/commands';
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
import { oneDark, oneDarkHighlightStyle } from '@codemirror/theme-one-dark';
import { javascript } from '@codemirror/lang-javascript';
import { html } from '@codemirror/lang-html';
import { css } from '@codemirror/lang-css';
import { json } from '@codemirror/lang-json';
import { markdown } from '@codemirror/lang-markdown';
import { python } from '@codemirror/lang-python';
import { java } from '@codemirror/lang-java';
import { cpp } from '@codemirror/lang-cpp';
import { xml } from '@codemirror/lang-xml';
import { sql } from '@codemirror/lang-sql';
import { rust } from '@codemirror/lang-rust';
import { php } from '@codemirror/lang-php';
import { yaml } from '@codemirror/lang-yaml';
import { go } from '@codemirror/lang-go';
import { vue } from '@codemirror/lang-vue';
import { shell } from '@codemirror/legacy-modes/mode/shell';
import { dockerFile } from '@codemirror/legacy-modes/mode/dockerfile';
import { toml } from '@codemirror/legacy-modes/mode/toml';
import { ruby } from '@codemirror/legacy-modes/mode/ruby';
import { lua } from '@codemirror/legacy-modes/mode/lua';
import { powerShell } from '@codemirror/legacy-modes/mode/powershell';
import { swift } from '@codemirror/legacy-modes/mode/swift';
import { kotlin, csharp, scala } from '@codemirror/legacy-modes/mode/clike';
import { diff } from '@codemirror/legacy-modes/mode/diff';
import { properties } from '@codemirror/legacy-modes/mode/properties';

const legacy = (m) => () => StreamLanguage.define(m);

// Languages by id, with a display name and the names/extensions people type.
const LANGS = [
  { id: 'plain', name: 'Plain text', aliases: ['text', 'txt', 'plaintext', ''], make: null },
  { id: 'bash', name: 'Shell', aliases: ['sh', 'shell', 'zsh', 'bash', 'console'], make: legacy(shell) },
  { id: 'javascript', name: 'JavaScript', aliases: ['js', 'mjs', 'cjs', 'node'], make: () => javascript() },
  { id: 'typescript', name: 'TypeScript', aliases: ['ts', 'mts', 'cts'], make: () => javascript({ typescript: true }) },
  { id: 'jsx', name: 'JSX', aliases: ['jsx'], make: () => javascript({ jsx: true }) },
  { id: 'tsx', name: 'TSX', aliases: ['tsx'], make: () => javascript({ jsx: true, typescript: true }) },
  { id: 'json', name: 'JSON', aliases: ['json', 'jsonl', 'json5'], make: () => json() },
  { id: 'html', name: 'HTML', aliases: ['html', 'htm', 'svelte'], make: () => html() },
  { id: 'css', name: 'CSS', aliases: ['css', 'scss', 'less'], make: () => css() },
  { id: 'markdown', name: 'Markdown', aliases: ['md', 'mdx', 'markdown'], make: () => markdown() },
  { id: 'python', name: 'Python', aliases: ['py', 'pyw', 'python3'], make: () => python() },
  { id: 'go', name: 'Go', aliases: ['go', 'golang'], make: () => go() },
  { id: 'rust', name: 'Rust', aliases: ['rs', 'rust'], make: () => rust() },
  { id: 'java', name: 'Java', aliases: ['java'], make: () => java() },
  { id: 'kotlin', name: 'Kotlin', aliases: ['kt', 'kts', 'kotlin'], make: legacy(kotlin) },
  { id: 'csharp', name: 'C#', aliases: ['cs', 'c#', 'csharp'], make: legacy(csharp) },
  { id: 'scala', name: 'Scala', aliases: ['scala'], make: legacy(scala) },
  { id: 'c', name: 'C', aliases: ['c', 'h'], make: () => cpp() },
  { id: 'cpp', name: 'C++', aliases: ['cpp', 'cxx', 'cc', 'hpp', 'c++'], make: () => cpp() },
  { id: 'swift', name: 'Swift', aliases: ['swift'], make: legacy(swift) },
  { id: 'ruby', name: 'Ruby', aliases: ['rb', 'ruby'], make: legacy(ruby) },
  { id: 'php', name: 'PHP', aliases: ['php'], make: () => php() },
  { id: 'lua', name: 'Lua', aliases: ['lua'], make: legacy(lua) },
  { id: 'sql', name: 'SQL', aliases: ['sql', 'psql', 'mysql'], make: () => sql() },
  { id: 'yaml', name: 'YAML', aliases: ['yml', 'yaml'], make: () => yaml() },
  { id: 'toml', name: 'TOML', aliases: ['toml'], make: legacy(toml) },
  { id: 'ini', name: 'INI / .env', aliases: ['ini', 'env', 'properties', 'conf', 'cfg'], make: legacy(properties) },
  { id: 'xml', name: 'XML', aliases: ['xml', 'svg', 'xsl', 'xsd', 'plist'], make: () => xml() },
  { id: 'dockerfile', name: 'Dockerfile', aliases: ['dockerfile', 'docker'], make: legacy(dockerFile) },
  { id: 'powershell', name: 'PowerShell', aliases: ['ps1', 'psm1', 'powershell', 'pwsh'], make: legacy(powerShell) },
  { id: 'diff', name: 'Diff', aliases: ['diff', 'patch'], make: legacy(diff) },
  { id: 'vue', name: 'Vue', aliases: ['vue'], make: () => vue() },
];
const BY_ALIAS = {};
for (const l of LANGS) { BY_ALIAS[l.id] = l; for (const a of l.aliases) BY_ALIAS[a] = l; }

/** A language from an id or anything people write after ``` ('sh', 'ts', 'py'…). */
function findLang(name) { return BY_ALIAS[String(name || '').trim().toLowerCase()] || null; }
function langExtension(name) { const l = findLang(name); return l && l.make ? [l.make()] : []; }

/** The file viewer's lookup: by file extension (and a few whole names). */
function getLangExtension(path) {
  const p = String(path || '');
  const base = p.slice(p.lastIndexOf('/') + 1).toLowerCase();
  if (base === 'dockerfile' || base.startsWith('dockerfile.')) return langExtension('dockerfile');
  if (base === '.env' || base.startsWith('.env.')) return langExtension('ini');
  const ext = base.includes('.') ? base.slice(base.lastIndexOf('.') + 1) : '';
  return ext ? langExtension(ext) : [];
}

// A code block in a note: highlighted and wrapped, with undo, bracket help and
// Tab to indent — no line numbers, gutters or search, which a note does not need.
const notesSetup = [
  highlightSpecialChars(), history(), drawSelection(), indentOnInput(), bracketMatching(), closeBrackets(),
  indentUnit.of('  '), EditorState.tabSize.of(2),
  syntaxHighlighting(oneDarkHighlightStyle), syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
  EditorView.lineWrapping,
  keymap.of([...closeBracketsKeymap, ...defaultKeymap, ...historyKeymap, indentWithTab]),
];

window.CM = {
  EditorView, EditorState, Compartment, Prec, keymap, placeholder,
  getLangExtension, basicSetup, oneDark,
  langs: LANGS.map(({ id, name }) => ({ id, name })),
  findLang: (n) => { const l = findLang(n); return l ? { id: l.id, name: l.name } : null; },
  langExtension, notesSetup,
};
