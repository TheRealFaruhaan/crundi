/**
 * platform.js — Single source of truth for OS capability gating.
 *
 * Crundi is primarily a Windows app. A handful of features rely on Windows-only
 * mechanisms (the RDP/tscon locked-screen trick, virtual-desktop COM enumeration).
 * Rather than scatter `process.platform` checks, every gate reads from CAPS here so
 * the tool dispatcher (index.js), the advertised MCP tool list (mcp-stdio.js), and
 * the web UI all agree on what is available.
 */

export const IS_WIN = process.platform === 'win32';
export const IS_MAC = process.platform === 'darwin';
export const IS_LINUX = process.platform === 'linux';

// Feature availability by platform. Keep this the only place that decides.
export const CAPS = {
  // RDP-disconnect / locked-screen capture — Windows-only (tscon.exe + schtasks).
  rdpDisconnect: IS_WIN,
  // Cross-virtual-desktop window enumeration — Windows-only (IVirtualDesktopManager COM).
  // On other platforms window listing is limited to the current Space/desktop.
  crossDesktopWindows: IS_WIN,
  // Window/display capture — node-screenshots supports Windows, macOS, and Linux.
  windowCapture: true,
  displayCapture: true,
  // Per-service memory sampling — Windows via wmic, POSIX via `ps`.
  serviceMemorySampler: true,
};

/**
 * Standard "this feature isn't available on this OS" result.
 * Shape mirrors the rest of the tool layer ({ ok, ... }) and adds `supported:false`
 * so callers/UI can distinguish "unsupported here" from a genuine error.
 */
export function unsupported(feature) {
  return { ok: false, supported: false, error: `${feature} is not supported on ${process.platform}` };
}
