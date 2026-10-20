// Builds app/vendor/factory-flow.js (+ .css), stamped with the source hash.
import { build } from 'esbuild';
import { sourceHash } from './src-hash.mjs';

const hash = sourceHash();
await build({
  entryPoints: ['src/index.jsx'],
  bundle: true,
  minify: true,
  format: 'iife',
  target: 'es2020',
  jsx: 'automatic',
  loader: { '.js': 'jsx' },
  define: { 'process.env.NODE_ENV': '"production"', __FF_SRC_HASH__: JSON.stringify(hash) },
  outfile: '../../app/vendor/factory-flow.js',
  logLevel: 'info',
});
console.log('source hash', hash);
