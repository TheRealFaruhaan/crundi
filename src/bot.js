/**
 * bot.js — Slim Telegram bot for Crundi.
 *
 * Responsibilities:
 *   1. Initialize the bot (for bot.botInfo / username)
 *   2. /start command → reply with webapp URL
 *   3. Send DM notifications to the authorized user
 *   4. Provide the bot instance for Telegram Login Widget validation
 *
 * Everything else (message handling, commands, supervisor, worker, sessions,
 * topics, permissions) has been removed — the web terminal is now the
 * primary interface.
 */

import { Bot } from 'grammy';
import { config } from './config.js';

/**
 * Create the slim Crundi Telegram bot.
 *
 * @param {{ webappUrl?: string }} opts
 * @returns {{ bot: Bot, notify: Function, getChatId: Function }}
 */
export function createBot({ webappUrl } = {}) {
  const bot = new Bot(config.botToken);

  // Cached chat ID for the authorized user (discovered on first /start or message)
  let authorizedChatId = null;
  let onChatIdChanged = null;

  // Global error handler
  bot.catch((err) => {
    console.error('[bot] Error:', err.error?.message || err.message || err);
  });

  // /start — greet user and link to webapp
  bot.command('start', async (ctx) => {
    const username = ctx.from?.username?.toLowerCase();
    const allowed = config.allowedUsername.replace(/^@/, '').toLowerCase();
    if (username !== allowed) {
      await ctx.reply('Unauthorized.');
      return;
    }

    // Cache the chat ID for notifications
    authorizedChatId = ctx.chat.id;
    if (onChatIdChanged) onChatIdChanged(authorizedChatId);

    const url = webappUrl || 'not configured yet';
    await ctx.reply(
      `Welcome to Crundi!\n\nOpen the web terminal:\n${url}\n\nYou'll receive notifications here when Claude finishes a task.`
    );
  });

  // Any other message from authorized user — cache chat ID, point to webapp
  bot.on('message', async (ctx) => {
    const username = ctx.from?.username?.toLowerCase();
    const allowed = config.allowedUsername.replace(/^@/, '').toLowerCase();
    if (username !== allowed) return;

    // Cache chat ID
    if (!authorizedChatId) {
      authorizedChatId = ctx.chat.id;
      if (onChatIdChanged) onChatIdChanged(authorizedChatId);
    }

    const url = webappUrl || 'not configured yet';
    await ctx.reply(`Use the Crundi web terminal: ${url}`);
  });

  /**
   * Send a notification to the authorized user.
   * @param {string} text — plain text or HTML message
   * @param {{ parse_mode?: string }} opts
   */
  async function notify(text, opts = {}) {
    if (!authorizedChatId) {
      console.warn('[bot] Cannot notify — no chat ID yet (user hasn\'t sent /start)');
      return { ok: false, error: 'No chat ID' };
    }
    try {
      await bot.api.sendMessage(authorizedChatId, text, {
        parse_mode: opts.parse_mode || undefined,
      });
      return { ok: true };
    } catch (err) {
      console.error('[bot] Notify error:', err.message);
      return { ok: false, error: err.message };
    }
  }

  /**
   * Get the cached authorized user chat ID.
   */
  function getChatId() {
    return authorizedChatId;
  }

  /**
   * Set the chat ID explicitly (e.g., from persisted state).
   */
  function setChatId(id) {
    authorizedChatId = id;
  }

  /**
   * Set the webapp URL (called after webapp starts and tunnel connects).
   */
  function setWebappUrl(url) {
    webappUrl = url;
  }

  function onChatId(fn) { onChatIdChanged = fn; }

  return { bot, notify, getChatId, setChatId, setWebappUrl, onChatId };
}
