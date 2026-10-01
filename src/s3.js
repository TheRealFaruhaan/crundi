/**
 * s3.js — a small S3 client for backups, with no dependencies.
 *
 * Speaks the plain S3 REST API signed with AWS Signature Version 4, which is
 * what every S3-compatible store accepts: AWS S3, Cloudflare R2, Backblaze B2,
 * MinIO, Wasabi, DigitalOcean Spaces and the rest. Only what backups need:
 * put (single or multipart, from a file), list, download to a file, delete,
 * and a bucket check.
 *
 *   const s3 = createS3({ endpoint, region, bucket, accessKeyId, secretAccessKey, pathStyle })
 *
 * endpoint   https://<account>.r2.cloudflarestorage.com, https://s3.us-east-1.amazonaws.com, …
 * region     'auto' for R2; the bucket's region for AWS (default us-east-1)
 * pathStyle  true: <endpoint>/<bucket>/<key> (works almost everywhere);
 *            false: <bucket>.<endpoint host>/<key>
 */

import { createHash, createHmac } from 'node:crypto';
import { createWriteStream, statSync, openSync, readSync, closeSync } from 'node:fs';
import { pipeline } from 'node:stream/promises';
import { Readable } from 'node:stream';

const EMPTY_SHA = 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855';
export const PART_SIZE = 16 * 1024 * 1024;   // multipart above this; also the part size

const sha256hex = (b) => createHash('sha256').update(b).digest('hex');
const hmac = (k, s) => createHmac('sha256', k).update(s).digest();

/** RFC 3986 encoding as SigV4 wants it ('/' kept only in paths). */
export function uriEncode(s, keepSlash) {
  return Array.from(Buffer.from(String(s), 'utf-8')).map((b) => {
    const c = String.fromCharCode(b);
    if (/[A-Za-z0-9\-_.~]/.test(c) || (keepSlash && c === '/')) return c;
    return '%' + b.toString(16).toUpperCase().padStart(2, '0');
  }).join('');
}

/**
 * Sign a request (SigV4). Returns the headers to send, Authorization included.
 * Pure, so it can be checked against AWS's published examples.
 */
export function signRequest({ method, host, path, query = {}, headers = {}, payloadHash = EMPTY_SHA, region, accessKeyId, secretAccessKey, date = new Date() }) {
  const amzDate = date.toISOString().replace(/[:-]|\.\d{3}/g, '');   // 20130524T000000Z
  const day = amzDate.slice(0, 8);
  const h = { host, 'x-amz-content-sha256': payloadHash, 'x-amz-date': amzDate };
  for (const [k, v] of Object.entries(headers)) h[k.toLowerCase()] = String(v).trim().replace(/\s+/g, ' ');
  const names = Object.keys(h).sort();
  const canonicalHeaders = names.map(n => n + ':' + h[n] + '\n').join('');
  const signedHeaders = names.join(';');
  const canonicalQuery = Object.keys(query).sort().map(k => uriEncode(k) + '=' + uriEncode(query[k] == null ? '' : query[k])).join('&');
  const canonicalRequest = [method, uriEncode(path, true), canonicalQuery, canonicalHeaders, signedHeaders, payloadHash].join('\n');
  const scope = `${day}/${region}/s3/aws4_request`;
  const stringToSign = ['AWS4-HMAC-SHA256', amzDate, scope, sha256hex(canonicalRequest)].join('\n');
  const key = hmac(hmac(hmac(hmac('AWS4' + secretAccessKey, day), region), 's3'), 'aws4_request');
  const signature = createHmac('sha256', key).update(stringToSign).digest('hex');
  const out = { ...h };
  delete out.host;   // fetch sets it from the URL
  out.authorization = `AWS4-HMAC-SHA256 Credential=${accessKeyId}/${scope}, SignedHeaders=${signedHeaders}, Signature=${signature}`;
  return out;
}

const xmlDecode = (s) => String(s).replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&apos;/g, "'").replace(/&amp;/g, '&');
const tag = (xml, name) => { const m = String(xml).match(new RegExp('<' + name + '>([\\s\\S]*?)</' + name + '>')); return m ? xmlDecode(m[1]) : ''; };

/** A readable error from an S3 error response. */
async function s3Error(res, what) {
  let body = '';
  try { body = await res.text(); } catch { /* ignore */ }
  const code = tag(body, 'Code'), msg = tag(body, 'Message');
  const hint = res.status === 403 ? ' Check the access key, secret and bucket permissions.'
    : res.status === 404 && code === 'NoSuchBucket' ? ' Check the bucket name.'
    : (res.status === 400 || res.status === 301) && /region|AuthorizationHeaderMalformed|PermanentRedirect/i.test(code + msg) ? ' Check the region (R2 uses "auto").' : '';
  const e = new Error(`${what} failed: ${res.status} ${code || res.statusText}${msg ? ' — ' + msg : ''}.${hint}`);
  e.status = res.status; e.code = code;
  return e;
}

export function createS3(cfg) {
  const endpoint = String(cfg.endpoint || '').trim().replace(/\/+$/, '');
  if (!/^https?:\/\//i.test(endpoint)) throw new Error('The endpoint must be a URL starting with https://');
  const base = new URL(endpoint);
  const bucket = String(cfg.bucket || '').trim();
  if (!bucket) throw new Error('A bucket name is required');
  const region = String(cfg.region || '').trim() || 'us-east-1';
  const pathStyle = cfg.pathStyle !== false;
  const creds = { accessKeyId: String(cfg.accessKeyId || '').trim(), secretAccessKey: String(cfg.secretAccessKey || '').trim() };
  if (!creds.accessKeyId || !creds.secretAccessKey) throw new Error('The access key ID and secret are required');
  const basePath = base.pathname.replace(/\/+$/, '');

  function target(key) {
    const host = pathStyle ? base.host : bucket + '.' + base.host;
    const path = basePath + (pathStyle ? '/' + bucket : '') + '/' + (key || '');
    return { host, path };
  }

  async function request(method, key, { query = {}, headers = {}, body, payloadHash, what } = {}) {
    const { host, path } = target(key);
    const ph = payloadHash || (body ? sha256hex(body) : EMPTY_SHA);
    const signed = signRequest({ method, host, path, query, headers, payloadHash: ph, region, ...creds });
    const qs = Object.keys(query).sort().map(k => uriEncode(k) + '=' + uriEncode(query[k] == null ? '' : query[k])).join('&');
    const url = base.protocol + '//' + host + uriEncode(path, true) + (qs ? '?' + qs : '');
    let res;
    try {
      res = await fetch(url, { method, headers: signed, body, duplex: body && typeof body.pipe === 'function' ? 'half' : undefined });
    } catch (err) {
      throw new Error(`${what || method} failed: could not reach ${base.host} (${err.cause?.code || err.message})`);
    }
    if (!res.ok) throw await s3Error(res, what || method);
    return res;
  }

  /** Bucket reachable and the keys can list it. */
  async function check(prefix = '') {
    const res = await request('GET', '', { query: { 'list-type': '2', 'max-keys': '1', prefix }, what: 'Listing the bucket' });
    await res.text();
    return true;
  }

  /** Every object under a prefix: [{ key, size, lastModified }]. */
  async function list(prefix = '') {
    const out = [];
    let token = '';
    for (let i = 0; i < 1000; i++) {
      const query = { 'list-type': '2', prefix };
      if (token) query['continuation-token'] = token;
      const xml = await (await request('GET', '', { query, what: 'Listing backups' })).text();
      for (const m of xml.matchAll(/<Contents>([\s\S]*?)<\/Contents>/g)) {
        out.push({ key: tag(m[1], 'Key'), size: Number(tag(m[1], 'Size')) || 0, lastModified: tag(m[1], 'LastModified') });
      }
      if (tag(xml, 'IsTruncated') !== 'true') break;
      token = tag(xml, 'NextContinuationToken');
      if (!token) break;
    }
    return out;
  }

  function readChunk(fd, pos, len) {
    const buf = Buffer.alloc(len);
    let got = 0;
    while (got < len) { const n = readSync(fd, buf, got, len - got, pos + got); if (!n) break; got += n; }
    return got === len ? buf : buf.subarray(0, got);
  }

  /** Upload a file; multipart when it is larger than one part. */
  async function putFile(key, file, { contentType = 'application/octet-stream', onProgress } = {}) {
    const size = statSync(file).size;
    if (size <= PART_SIZE) {
      const fd = openSync(file, 'r');
      let body;
      try { body = readChunk(fd, 0, size); } finally { closeSync(fd); }
      await (await request('PUT', key, { headers: { 'content-type': contentType }, body, what: 'Uploading the backup' })).text();
      if (onProgress) onProgress(size, size);
      return { size };
    }
    const init = await (await request('POST', key, { query: { uploads: '' }, headers: { 'content-type': contentType }, what: 'Starting the upload' })).text();
    const uploadId = tag(init, 'UploadId');
    if (!uploadId) throw new Error('Starting the upload failed: no upload id in the reply');
    const fd = openSync(file, 'r');
    const parts = [];
    try {
      for (let n = 1, pos = 0; pos < size; n++, pos += PART_SIZE) {
        const body = readChunk(fd, pos, Math.min(PART_SIZE, size - pos));
        let etag = '';
        for (let attempt = 1; ; attempt++) {
          try {
            const res = await request('PUT', key, { query: { partNumber: String(n), uploadId }, body, what: `Uploading part ${n}` });
            etag = res.headers.get('etag') || '';
            await res.text();
            break;
          } catch (err) { if (attempt >= 3) throw err; await new Promise(r => setTimeout(r, 1000 * attempt)); }
        }
        parts.push({ n, etag });
        if (onProgress) onProgress(Math.min(size, pos + body.length), size);
      }
      const xml = '<CompleteMultipartUpload>' + parts.map(p => `<Part><PartNumber>${p.n}</PartNumber><ETag>${p.etag.replace(/&/g, '&amp;').replace(/</g, '&lt;')}</ETag></Part>`).join('') + '</CompleteMultipartUpload>';
      const done = await (await request('POST', key, { query: { uploadId }, headers: { 'content-type': 'application/xml' }, body: Buffer.from(xml), what: 'Finishing the upload' })).text();
      // S3 can answer 200 with an error document here.
      if (/<Error>/.test(done)) throw new Error('Finishing the upload failed: ' + (tag(done, 'Message') || tag(done, 'Code')));
      return { size };
    } catch (err) {
      try { await request('DELETE', key, { query: { uploadId }, what: 'Abort' }); } catch { /* best effort */ }
      throw err;
    } finally { closeSync(fd); }
  }

  /** Download an object to a file. */
  async function getToFile(key, file, { onProgress } = {}) {
    const res = await request('GET', key, { what: 'Downloading the backup' });
    const total = Number(res.headers.get('content-length')) || 0;
    let got = 0;
    const src = Readable.fromWeb(res.body);
    if (onProgress) src.on('data', (c) => { got += c.length; onProgress(got, total); });
    await pipeline(src, createWriteStream(file));
    return { size: total || got };
  }

  async function remove(key) {
    await (await request('DELETE', key, { what: 'Deleting a backup' })).text();
  }

  return { check, list, putFile, getToFile, remove, bucket, region };
}
