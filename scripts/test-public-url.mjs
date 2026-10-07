#!/usr/bin/env node
// The address handed to a person outside this machine. A wrong one is silent:
// the link is built, sent, and fails only when tapped.
import { publicBaseUrl } from '../src/public-url.js';

let failed = 0;
const eq = (got, want, name) => {
  const ok = got === want;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${ok ? '' : `\n      got ${got}, want ${want}`}`);
  if (!ok) failed++;
};

// The case that shipped broken: own TLS, no tunnel, listening on 443.
eq(publicBaseUrl({ tunnelUrl: null, tlsMode: 'letsencrypt', tlsDomain: 'dev.example.com', tlsPort: 443, port: 443 }),
  'https://dev.example.com', 'TLS install: its own name over https, never localhost:443');
eq(publicBaseUrl({ tlsMode: 'letsencrypt', tlsDomain: 'dev.example.com', tlsPort: 8443, port: 8443 }),
  'https://dev.example.com:8443', 'TLS on a moved port keeps the port');
eq(publicBaseUrl({ tlsMode: 'LetsEncrypt', tlsDomain: 'dev.example.com', port: 443 }),
  'https://dev.example.com', 'TLS mode is case-insensitive and the port defaults to 443');
eq(publicBaseUrl({ tunnelUrl: 'https://abc.trycloudflare.com/', tlsMode: 'off', port: 8888 }),
  'https://abc.trycloudflare.com', 'a tunnel is the public name, without a trailing slash');
eq(publicBaseUrl({ tunnelUrl: 'https://t.example.com', tlsMode: 'letsencrypt', tlsDomain: 'dev.example.com', port: 443 }),
  'https://t.example.com', 'a running tunnel wins over the TLS name');
eq(publicBaseUrl({ tlsMode: 'off', tlsDomain: 'dev.example.com', port: 8888 }),
  'http://localhost:8888', 'TLS off: localhost, even with a domain configured');
eq(publicBaseUrl({ tlsMode: 'letsencrypt', tlsDomain: '', port: 8888 }),
  'http://localhost:8888', 'TLS on with no domain falls back to localhost');
eq(publicBaseUrl({}), null, 'nothing known: null, not a made-up address');

console.log(failed ? `\n${failed} FAILED` : '\nAll public-url checks passed.');
process.exit(failed ? 1 : 0);
