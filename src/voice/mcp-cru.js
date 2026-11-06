#!/usr/bin/env node
/**
 * voice/mcp-cru.js — Cru's tools for the Claude Code brain (the experiment in
 * agent-cli.js), as a stdio MCP server that Claude Code starts.
 *
 * Nothing runs here. The list of tools is the one the API brain sends (a file
 * Crundi writes), and every call goes back to Crundi, which runs it with the
 * same code and the same rules as for the API: confirmations (D32), critical
 * settings refused (D17), no secret values (D16), never a terminal (D21).
 *
 * Environment (set by Crundi in the --mcp-config it writes):
 *   CRU_API_URL     http://127.0.0.1:<port> of this Crundi
 *   CRU_TOKEN       a random token for this run of the brain; it reaches
 *                   Cru's tools and nothing else
 *   CRU_TOOLS_FILE  the tool list, [{name, description, input_schema}]
 */

import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { ListToolsRequestSchema, CallToolRequestSchema } from '@modelcontextprotocol/sdk/types.js';
import { readFileSync } from 'node:fs';

const API_URL = process.env.CRU_API_URL || '';
const TOKEN = process.env.CRU_TOKEN || '';
const tools = JSON.parse(readFileSync(process.env.CRU_TOOLS_FILE || '', 'utf-8'));

const server = new Server({ name: 'cru', version: '1.0.0' }, { capabilities: { tools: {} } });

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: tools.map(t => ({ name: t.name, description: t.description, inputSchema: t.input_schema })),
}));

server.setRequestHandler(CallToolRequestSchema, async (req) => {
  try {
    const r = await fetch(`${API_URL}/api/voice/cli-tool`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Cru-Token': TOKEN },
      body: JSON.stringify({ tool: req.params.name, args: req.params.arguments || {} }),
    });
    const out = await r.json();
    // Text, or the API's text and image blocks (view_widget), as MCP content.
    const content = Array.isArray(out.content)
      ? out.content.map((b) => (b.type === 'image' ? { type: 'image', data: b.source.data, mimeType: b.source.media_type } : { type: 'text', text: String(b.text || '') }))
      : [{ type: 'text', text: String(out.content ?? out.error ?? '') }];
    return { content, isError: !!out.isError || !r.ok };
  } catch (e) {
    return { content: [{ type: 'text', text: `Crundi could not be reached: ${e.message || e}` }], isError: true };
  }
});

await server.connect(new StdioServerTransport());
