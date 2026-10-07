/**
 * public-url.js — the address a person outside this machine uses to reach it.
 *
 * Worked out in one place because getting it wrong is silent: a link is built,
 * handed over, and only fails when someone taps it. send_file_to_user did
 * exactly that on a TLS install, where there is no tunnel and the listening
 * port is 443: it produced http://localhost:443/dl/..., which opens nothing
 * anywhere.
 *
 * Order: a tunnel if one is up (it is the public name), else the server's own
 * TLS name, else localhost (a desktop install used on the machine itself).
 */

/**
 * @param {object} o
 * @param {string|null} [o.tunnelUrl]  public URL of a running tunnel
 * @param {string} [o.tlsMode]         'off' or a TLS mode
 * @param {string} [o.tlsDomain]
 * @param {number} [o.tlsPort]         the HTTPS port (443 unless moved)
 * @param {number|null} [o.port]       the port the web server listens on
 * @returns {string|null} base URL without a trailing slash, or null if unknown
 */
export function publicBaseUrl({ tunnelUrl, tlsMode, tlsDomain, tlsPort, port } = {}) {
  if (tunnelUrl) return String(tunnelUrl).replace(/\/+$/, '');
  if (String(tlsMode || 'off').toLowerCase() !== 'off' && tlsDomain) {
    const p = Number(tlsPort) || 443;
    return `https://${tlsDomain}${p === 443 ? '' : ':' + p}`;
  }
  return port ? `http://localhost:${port}` : null;
}
