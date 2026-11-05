#!/usr/bin/env node
/**
 * prune-voice-deps.mjs: shrink the voice assistant's optional embeddings
 * runtime (D47) to what this machine runs.
 *
 * @huggingface/transformers brings onnxruntime-node with binaries for every
 * platform and the CUDA/TensorRT GPU providers (~540 MB), and onnxruntime-web
 * (~90 MB). The server embeds a sentence on the CPU in ~10 ms, so it needs
 * one platform's CPU library: ~45 MB. Run after npm ci, in release builds.
 *
 *   node scripts/prune-voice-deps.mjs [node_modules dir]
 */
import { existsSync, readdirSync, rmSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

const nm = resolve(process.argv[2] || 'node_modules');
let freed = 0;
const size = (p) => { try { const s = statSync(p); if (!s.isDirectory()) return s.size; return readdirSync(p).reduce((n, f) => n + size(join(p, f)), 0); } catch { return 0; } };
const drop = (p) => { if (existsSync(p)) { freed += size(p); rmSync(p, { recursive: true, force: true }); } };

const ort = join(nm, 'onnxruntime-node', 'bin');
if (existsSync(ort)) {
  for (const napi of readdirSync(ort)) {
    const base = join(ort, napi);
    for (const plat of readdirSync(base)) {
      if (plat !== process.platform) { drop(join(base, plat)); continue; }
      for (const arch of readdirSync(join(base, plat))) {
        const dir = join(base, plat, arch);
        if (arch !== process.arch) { drop(dir); continue; }
        for (const f of readdirSync(dir)) if (/providers_(cuda|tensorrt|rocm|migraphx|openvino)/i.test(f)) drop(join(dir, f));
      }
    }
  }
}
drop(join(nm, 'onnxruntime-web'));
console.log(`prune-voice-deps: freed ${(freed / 1048576).toFixed(0)} MB`);
