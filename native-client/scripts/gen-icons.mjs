#!/usr/bin/env node
// The PC client's icons, from the web UI's own set (ICON_PATHS in
// src/webapp-html.js: flat stroke icons, never emoji). Writes one SVG per icon
// to ui/icons/ and ui/icons.slint, a global with one image per icon; the
// images are drawn in black and tinted where used (Image { colorize: … }).
//
//   node native-client/scripts/gen-icons.mjs      (run again when the web set changes)

import { readFileSync, writeFileSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const src = readFileSync(join(here, '..', '..', 'src', 'webapp-html.js'), 'utf8');
const start = src.indexOf('const ICON_PATHS = {');
const end = src.indexOf('\n    };', start);
if (start < 0 || end < 0) throw new Error('ICON_PATHS not found in src/webapp-html.js');
const block = src.slice(start, end);

const icons = {};
for (const m of block.matchAll(/^\s*(?:'([a-z0-9-]+)'|([a-z0-9]+)):\s*'([^']*)',?\s*$/gm)) {
  icons[m[1] || m[2]] = m[3];
}
const names = Object.keys(icons).sort();  // (turned variants are added below)
if (names.length < 20) throw new Error(`only ${names.length} icons parsed`);

const dir = join(here, '..', 'ui', 'icons');
mkdirSync(dir, { recursive: true });
for (const f of readdirSync(dir)) if (f.endsWith('.svg')) rmSync(join(dir, f));
for (const n of names) {
  writeFileSync(join(dir, `${n}.svg`),
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${icons[n]}</svg>\n`);
}
// Turned variants: Slint's software renderer can't rotate an image, so the
// pinned pin (the web's transform: rotate(-30deg)) is drawn turned.
const TURNED = { 'pin-turned': ['pin', 'rotate(-30 12 12)'], 'chevrons-right': ['chevrons-left', 'translate(24 0) scale(-1 1)'] };
for (const [n, [base, how]] of Object.entries(TURNED)) {
  writeFileSync(join(dir, `${n}.svg`),
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><g transform="${how}">${icons[base]}</g></svg>\n`);
  names.push(n);
}
// Icons the web draws inline rather than from ICON_PATHS (the workbench's
// split controls, Fit to screen, the Layout presets), as drawn there.
const svg = (vb, sw, body) => `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${vb}" fill="none" stroke="#000" stroke-width="${sw}" stroke-linecap="round" stroke-linejoin="round">${body}</svg>\n`;
const INLINE = {
  'split-right': svg('0 0 24 18', 1.8, '<rect x="1" y="1" width="22" height="16" rx="1.5"/><line x1="12" y1="1" x2="12" y2="17"/>'),
  'split-down': svg('0 0 24 18', 1.8, '<rect x="1" y="1" width="22" height="16" rx="1.5"/><line x1="1" y1="9" x2="23" y2="9"/>'),
  'fit': svg('0 0 24 24', 2, '<polyline points="7 8 3 12 7 16"/><polyline points="17 8 21 12 17 16"/><line x1="3" y1="12" x2="21" y2="12"/>'),
  'columns': svg('0 0 24 24', 2, '<rect x="3" y="3" width="18" height="18" rx="2"/><line x1="12" y1="3" x2="12" y2="21"/>'),
  'lay-cols2': svg('0 0 24 18', 1.6, '<rect x="1" y="1" width="10" height="16" rx="1.5"/><rect x="13" y="1" width="10" height="16" rx="1.5"/>'),
  'lay-cols3': svg('0 0 24 18', 1.6, '<rect x="1" y="1" width="6.3" height="16" rx="1.2"/><rect x="8.85" y="1" width="6.3" height="16" rx="1.2"/><rect x="16.7" y="1" width="6.3" height="16" rx="1.2"/>'),
  'lay-cross4': svg('0 0 24 18', 1.6, '<rect x="1" y="1" width="10" height="7.5" rx="1.2"/><rect x="13" y="1" width="10" height="7.5" rx="1.2"/><rect x="1" y="9.5" width="10" height="7.5" rx="1.2"/><rect x="13" y="9.5" width="10" height="7.5" rx="1.2"/>'),
  'lay-left-right2': svg('0 0 24 18', 1.6, '<rect x="1" y="1" width="10" height="16" rx="1.2"/><rect x="13" y="1" width="10" height="7.5" rx="1.2"/><rect x="13" y="9.5" width="10" height="7.5" rx="1.2"/>'),
  'lay-top-bottom2': svg('0 0 24 18', 1.6, '<rect x="1" y="1" width="22" height="7.5" rx="1.2"/><rect x="1" y="9.5" width="10" height="7.5" rx="1.2"/><rect x="13" y="9.5" width="10" height="7.5" rx="1.2"/>'),
};
for (const [n, body] of Object.entries(INLINE)) { writeFileSync(join(dir, `${n}.svg`), body); names.push(n); }
// The chat's permission modes (the composer's mode button), from the web
// chat's MODE_INFO (app/vendor/claude-chat.js): mode-<mode>.
const chat = readFileSync(join(here, '..', '..', 'app', 'vendor', 'claude-chat.js'), 'utf8');
let modes = 0;
for (const m of chat.matchAll(/^\s*'([A-Za-z]+)': \{ icon: MODE_SVG\('([^']*)'\)/gm)) {
  const n = `mode-${m[1].replace(/[A-Z]/g, (c) => '-' + c.toLowerCase())}`;
  writeFileSync(join(dir, `${n}.svg`), svg('0 0 24 24', 2, m[2]));
  names.push(n); modes++;
}
if (modes < 6) throw new Error(`only ${modes} mode icons parsed`);
names.sort();
// Slint names: a leading digit or a keyword gets a prefix.
const slintName = (n) => (/^[0-9]/.test(n) || ['if', 'for', 'in', 'out', 'property', 'callback', 'import', 'export'].includes(n) ? `i-${n}` : n);
writeFileSync(join(here, '..', 'ui', 'icons.slint'),
  `// Generated by scripts/gen-icons.mjs from the web UI's ICON_PATHS. Do not edit.\n` +
  `// Flat stroke icons in black: tint them with Image { colorize: … }.\n\n` +
  `export global Icons {\n` + names.map((n) => `    out property <image> ${slintName(n)}: @image-url("icons/${n}.svg");`).join('\n') + `\n}\n`);
console.log(`${names.length} icons`);
