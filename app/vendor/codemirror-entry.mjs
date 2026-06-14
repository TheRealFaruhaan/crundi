// CodeMirror 6 bundle entry point — built into codemirror.js via esbuild
import { EditorView, keymap, lineNumbers, highlightActiveLineGutter, highlightSpecialChars, drawSelection, highlightActiveLine, rectangularSelection, crosshairCursor } from '@codemirror/view';
import { EditorState, Compartment } from '@codemirror/state';
import { defaultKeymap, indentWithTab, history, historyKeymap } from '@codemirror/commands';
import { syntaxHighlighting, defaultHighlightStyle, indentOnInput, bracketMatching, foldGutter, foldKeymap, HighlightStyle } from '@codemirror/language';
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
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
import { oneDark } from '@codemirror/theme-one-dark';
import { tags } from '@lezer/highlight';

// Language extension lookup by file extension
const LANG_MAP = {
  '.js': javascript, '.mjs': javascript, '.cjs': javascript,
  '.jsx': () => javascript({ jsx: true }),
  '.ts': () => javascript({ typescript: true }),
  '.tsx': () => javascript({ jsx: true, typescript: true }),
  '.html': html, '.htm': html, '.svelte': html, '.vue': html,
  '.css': css, '.scss': css, '.less': css,
  '.json': json, '.jsonl': json,
  '.md': markdown, '.mdx': markdown,
  '.py': python, '.pyw': python,
  '.java': java,
  '.c': cpp, '.h': cpp, '.cpp': cpp, '.cxx': cpp, '.cc': cpp, '.hpp': cpp,
  '.xml': xml, '.svg': xml, '.xsl': xml, '.xsd': xml,
  '.sql': sql,
  '.rs': rust,
  '.php': php,
  '.yaml': yaml, '.yml': yaml,
  '.go': go,
};

function getLangExtension(filePath) {
  const ext = filePath.includes('.') ? filePath.slice(filePath.lastIndexOf('.')).toLowerCase() : '';
  const factory = LANG_MAP[ext];
  if (!factory) return [];
  return [typeof factory === 'function' ? factory() : factory()];
}

// Expose on window for the renderer
window.CM = {
  EditorView,
  EditorState,
  Compartment,
  getLangExtension,
  basicSetup: [
    lineNumbers(),
    highlightActiveLineGutter(),
    highlightSpecialChars(),
    history(),
    foldGutter(),
    drawSelection(),
    indentOnInput(),
    syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
    bracketMatching(),
    closeBrackets(),
    rectangularSelection(),
    crosshairCursor(),
    highlightActiveLine(),
    highlightSelectionMatches(),
    keymap.of([
      ...closeBracketsKeymap,
      ...defaultKeymap,
      ...searchKeymap,
      ...historyKeymap,
      ...foldKeymap,
      indentWithTab,
    ]),
  ],
  oneDark,
};
