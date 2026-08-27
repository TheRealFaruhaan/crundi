#!/usr/bin/env node
/**
 * make-login.mjs — generate password + TOTP credentials for Crundi.
 *
 * For installs that do not use Telegram. Prints the two environment variables
 * to set and the otpauth:// URI to add to an authenticator app.
 *
 *   node scripts/make-login.mjs                 # prompts for a password
 *   node scripts/make-login.mjs 'my password'   # non-interactive
 *
 * Inside a container:
 *   docker compose exec crundi node scripts/make-login.mjs 'my password'
 */

import { createInterface } from 'node:readline';
import { hashPassword, generateTotpSecret, totpUri, totpNow } from '../src/auth-password.js';

async function ask(question) {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  try {
    return await new Promise(resolve => rl.question(question, a => resolve(a)));
  } finally { rl.close(); }
}

const argPassword = process.argv.slice(2).join(' ').trim();
let password = argPassword;

if (!password) {
  if (!process.stdin.isTTY) {
    console.error('No password given and nothing to prompt on.');
    console.error("Pass it as an argument:  node scripts/make-login.mjs 'my password'");
    process.exit(1);
  }
  // Note: this echoes. Hiding input portably across shells is more trouble than
  // it is worth for a one-off run you are watching; pass it as an argument if
  // that matters, or clear your scroll-back afterwards.
  password = (await ask('Choose a password: ')).trim();
}

if (password.length < 12) {
  console.error(`\nThat password is ${password.length} characters. Use at least 12.`);
  console.error('This login can reach a shell on the machine running Crundi.');
  process.exit(1);
}

const hash = hashPassword(password);
const secret = generateTotpSecret();

console.log('\n─── Add these to your environment (docker/.env, or your .env file) ───\n');
console.log(`CRUNDI_PASSWORD_HASH=${hash}`);
console.log(`CRUNDI_TOTP_SECRET=${secret}`);
console.log('\n─── Add this to your authenticator app ───\n');
console.log(totpUri(secret));
console.log(`\nSecret, if typing it in by hand: ${secret}`);
console.log(`Current code (check your app agrees): ${totpNow(secret)}`);
console.log('\nThe password itself is not stored anywhere — only the hash above.');
console.log('Keep CRUNDI_TOTP_SECRET secret: it is the second factor, not a public id.\n');
