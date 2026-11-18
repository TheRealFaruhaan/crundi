#!/usr/bin/env node
/**
 * mcp-stdio.js — Standalone MCP server for Claude Code CLI sessions.
 *
 * Runs as a child process of Claude Code CLI via stdio transport.
 * Communicates with the main Crundi server via HTTP for Telegram
 * notifications and service management.
 *
 * Environment variables (set by Crundi when spawning Claude terminals):
 *   CRUNDI_API_URL  — http://localhost:<port> of the Crundi webapp
 *   CRUNDI_API_KEY  — internal API key for auth
 *   CRUNDI_PROJECT  — project alias (for context in notifications)
 */

import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { ListToolsRequestSchema, CallToolRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { COLLABORATOR_MCP_TOOLS } from './access-policy.js';
import { IMAGE_TOOLS, listedTools, prepareToolArgs } from './mcp-tools.js';
import { readFileSync, existsSync } from 'node:fs';
import http from 'node:http';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);

const API_URL = process.env.CRUNDI_API_URL || 'http://localhost:8888';
const API_KEY = process.env.CRUNDI_API_KEY || '';
const PROJECT = process.env.CRUNDI_PROJECT || '';

// ─── HTTP client for Crundi webapp ───

function apiCall(tool, args = {}) {
  return new Promise((resolve, reject) => {
    const url = new URL('/api/mcp/call', API_URL);
    const body = JSON.stringify({ tool, args });
    const req = http.request(url, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Api-Key': API_KEY,
        'Content-Length': Buffer.byteLength(body),
      },
    }, (res) => {
      let data = '';
      res.on('data', (c) => { data += c; });
      res.on('end', () => {
        try { resolve(JSON.parse(data)); }
        catch { resolve({ ok: false, error: 'Invalid response' }); }
      });
    });
    req.on('error', (err) => reject(err));
    req.write(body);
    req.end();
  });
}

// ─── Tool definitions (src/mcp-tools.js, shared with chats on a worker PC) ───

// ─── Tool handler ───

async function handleToolCall(name, args) {
  if (name === 'syntax_check') return runSyntaxCheck(args.files || []);

  // The arguments that are this process's own (its project, its chat and the
  // token proving it), never the model's: see prepareToolArgs.
  prepareToolArgs(name, args, { ...process.env, CRUNDI_PROJECT: PROJECT });

  const result = await apiCall(name, args);

  // Tools that answer with ready-made content blocks (text and images mixed).
  if (result.ok && Array.isArray(result.content)) return { content: result.content };

  if (IMAGE_TOOLS.has(name) && result.ok && result.data) {
    return { content: [{ type: 'image', data: result.data, mimeType: 'image/png' }] };
  }

  if (!result.ok && result.error) {
    return { content: [{ type: 'text', text: `Error: ${result.error}` }], isError: true };
  }
  return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }] };
}

// ─── Syntax check (local) ───

function runSyntaxCheck(files) {
  const { execFileSync } = require('child_process');
  const results = [];
  for (const file of files) {
    if (!existsSync(file)) { results.push(`[FAIL] ${file}\nFile not found`); continue; }
    const ext = file.substring(file.lastIndexOf('.')).toLowerCase();
    let cmd, cmdArgs;
    switch (ext) {
      case '.js': case '.mjs': case '.cjs':
        cmd = 'node'; cmdArgs = ['--check', file]; break;
      case '.py':
        cmd = 'python'; cmdArgs = ['-m', 'py_compile', file]; break;
      case '.json':
        try { JSON.parse(readFileSync(file, 'utf-8')); results.push(`[PASS] ${file}\nOK`); } catch (e) { results.push(`[FAIL] ${file}\n${e.message}`); }
        continue;
      default:
        results.push(`[SKIP] ${file}\nNo checker for ${ext}`); continue;
    }
    try {
      execFileSync(cmd, cmdArgs, { timeout: 10000, encoding: 'utf-8' });
      results.push(`[PASS] ${file}\nOK`);
    } catch (err) {
      results.push(`[FAIL] ${file}\n${err.stderr || err.message}`);
    }
  }
  const passed = results.filter(r => r.startsWith('[PASS]')).length;
  const failed = results.filter(r => r.startsWith('[FAIL]')).length;
  return { content: [{ type: 'text', text: `${passed} passed, ${failed} failed out of ${files.length} file(s)\n\n${results.join('\n\n')}` }] };
}

// ─── Server setup ───

const server = new Server(
  { name: 'crundi', version: '5.0.0' },
  { capabilities: { tools: {} } }
);

// A collaborator's session is told only about the tools it may call. The
// server refuses the rest regardless (COLLABORATOR_MCP_TOOLS is checked on
// every call); this is so their Claude does not see secret_get and
// owner-only tools listed, try them, and report a wall of 403s as "blockers".
const TOOL_SCOPE = process.env.CRUNDI_TOOL_SCOPE || '';
const IN_CHAT = !!process.env.CRUNDI_CHAT_ID;
// Only a factory run's chats are offered factory_handoff.
const IN_FACTORY = !!process.env.CRUNDI_FACTORY_RUN;
const LISTED_TOOLS = listedTools({ scope: TOOL_SCOPE, inChat: IN_CHAT, inFactory: IN_FACTORY, collaboratorTools: COLLABORATOR_MCP_TOOLS });

server.setRequestHandler(ListToolsRequestSchema, async () => {
  return { tools: LISTED_TOOLS };
});

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;
  try {
    return await handleToolCall(name, args || {});
  } catch (err) {
    return { content: [{ type: 'text', text: `Error calling ${name}: ${err.message}` }], isError: true };
  }
});

// ─── Start ───

const transport = new StdioServerTransport();
await server.connect(transport);
