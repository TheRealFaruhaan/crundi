/**
 * chat-token.js — proof that an MCP call comes from the chat it names.
 *
 * Every owner chat's tool bridge shares one internal API key, and the chat id
 * it sends is its own say-so: one chat could name another. Where that matters
 * (homepool access is granted per chat), the bridge also sends a token made
 * here: an HMAC of the chat id and project with a secret that exists only in
 * this process's memory. Crundi hands each chat its token when it starts it
 * (CRUNDI_CHAT_TOKEN), and a chat cannot make one for another.
 *
 * The secret is new at every start; chats are started again by a restart too.
 * The limit, said plainly: chats run as the same system user, so a chat that
 * goes looking through other processes' environments could still find another
 * chat's token. This closes the easy way, not that one.
 */

import { randomBytes, createHmac, timingSafeEqual } from 'node:crypto';

const SECRET = randomBytes(32);

export function chatToken(chatId, project = '') {
  return createHmac('sha256', SECRET).update(`${chatId}\n${String(project || '').toLowerCase()}`).digest('base64url');
}

export function checkChatToken(chatId, project, token) {
  if (!chatId || !token) return false;
  const want = Buffer.from(chatToken(chatId, project || ''));
  const got = Buffer.from(String(token));
  return want.length === got.length && timingSafeEqual(want, got);
}
