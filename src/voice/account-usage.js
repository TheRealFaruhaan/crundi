/**
 * voice/account-usage.js: what Anthropic itself reports the whole account
 * has cost this billing month, next to Crundi's own count of the voice
 * assistant's spend (settings.js). Different numbers on purpose: this one is
 * every key and project on the account; Crundi's is the voice assistant only,
 * against its own limit.
 *
 * It needs an Admin API key (sk-ant-admin…, created in the Claude Console),
 * kept in Voice assistant settings on the server and never sent to a client.
 * An ordinary API key is refused by this API.
 *
 * GET https://api.anthropic.com/v1/organizations/cost_report, one bucket per
 * day; `amount` is a decimal string in cents. Anthropic updates it with some
 * delay, so the answer is cached for a few minutes.
 */

import { getSettings, billingPeriod } from './settings.js';

const COST_URL = 'https://api.anthropic.com/v1/organizations/cost_report';
const CACHE_MS = 10 * 60 * 1000;
let cached = { key: '', at: 0, value: null };

/** Sum one page of cost buckets, in US dollars. Pure, so it can be tested. */
export function sumCost(body) {
  let cents = 0;
  for (const bucket of Array.isArray(body?.data) ? body.data : []) {
    for (const r of Array.isArray(bucket?.results) ? bucket.results : []) {
      if (r && (r.currency || 'USD') === 'USD') cents += Number(r.amount) || 0;
    }
  }
  return cents / 100;
}

/**
 * The account's cost since the start of the Claude billing month.
 * @returns {Promise<{ok:true, totalUsd:number, since:string, until:string, cachedAt:number} | {ok:false, error:string, needsKey?:boolean}>}
 */
export async function accountUsage({ now = new Date(), fetchImpl = fetch, force = false } = {}) {
  const s = getSettings();
  const key = s.anthropicAdminKey;
  if (!key) return { ok: false, needsKey: true, error: 'No Anthropic Admin API key is set. Add one in Settings › Voice assistant to see the account’s own usage.' };
  const { start } = billingPeriod(now, s.billingDay);
  const since = start.toISOString();
  const memo = `${since}`;
  if (!force && cached.value && cached.key === memo && Date.now() - cached.at < CACHE_MS) return cached.value;
  let total = 0;
  let page = '';
  try {
    for (let i = 0; i < 6; i++) {
      const q = new URLSearchParams({ starting_at: since, bucket_width: '1d', limit: '31' });
      if (page) q.set('page', page);
      const res = await fetchImpl(`${COST_URL}?${q}`, { headers: { 'x-api-key': key, 'anthropic-version': '2023-06-01' } });
      let body = null;
      try { body = await res.json(); } catch { body = null; }
      if (!res.ok) {
        const why = body?.error?.message || `status ${res.status}`;
        const hint = res.status === 401 || res.status === 403 ? ' (it must be an Admin API key, sk-ant-admin…)' : '';
        return { ok: false, error: `Anthropic refused the usage report: ${why}${hint}` };
      }
      total += sumCost(body);
      if (!body?.has_more || !body?.next_page) break;
      page = body.next_page;
    }
  } catch (e) {
    return { ok: false, error: `Couldn't reach Anthropic: ${e.message}` };
  }
  const value = { ok: true, totalUsd: Math.round(total * 10000) / 10000, since, until: now.toISOString(), cachedAt: Date.now() };
  cached = { key: memo, at: Date.now(), value };
  return value;
}

/** Tests only. */
export function _clearCache() { cached = { key: '', at: 0, value: null }; }
