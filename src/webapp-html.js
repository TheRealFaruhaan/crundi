/**
 * webapp-html.js — Crundi web terminal UI.
 *
 * Returns a self-contained HTML string with embedded CSS and JS.
 * xterm.js and addon-fit are loaded from /vendor/ routes served by webapp.js.
 * Telegram Login Widget is used for authentication.
 */

import { readFileSync, existsSync } from 'fs';
import { join, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));

// Inline the logo images as data URIs so they always render — even inside
// Telegram's in-app browser, which fails to load the separate /assets request.
function assetDataUri(file) {
  for (const dir of [join(__dirname, '..', 'assets'), join(__dirname, '..', '..', 'assets')]) {
    const p = join(dir, file);
    if (existsSync(p)) {
      try { return 'data:image/png;base64,' + readFileSync(p).toString('base64'); } catch { /* fall through */ }
    }
  }
  return '/assets/' + file; // fallback to the served URL
}
const LOGO_SM = assetDataUri('icon_64x64.png');   // topbar
const LOGO_LG = assetDataUri('icon_128x128.png'); // login screen

export function getWebappHtml(botUsername) {
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no, viewport-fit=cover">
  <title>Crundi</title>
  <!-- PWA -->
  <link rel="manifest" href="/manifest.webmanifest">
  <meta name="theme-color" content="#0a0a0f">
  <meta name="mobile-web-app-capable" content="yes">
  <meta name="apple-mobile-web-app-capable" content="yes">
  <meta name="apple-mobile-web-app-status-bar-style" content="black-translucent">
  <meta name="apple-mobile-web-app-title" content="Crundi">
  <link rel="icon" type="image/png" sizes="256x256" href="/assets/icon_256x256.png">
  <link rel="icon" type="image/png" sizes="32x32" href="/assets/icon_32x32.png">
  <link rel="apple-touch-icon" href="/assets/icon_256x256.png">
  <script src="https://telegram.org/js/telegram-web-app.js"><\/script>
  <link rel="stylesheet" href="/vendor/xterm.css">
  <style>
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

    :root {
      --bg-primary: #0a0a0f;
      --bg-secondary: #12121a;
      --bg-tertiary: #1a1a28;
      --bg-hover: #22223a;
      --bg-card: #14141f;
      --border: #2a2a3d;
      --border-subtle: #1e1e30;
      --text-primary: #e8e8f0;
      --text-secondary: #8888a8;
      --text-muted: #5a5a78;
      --accent: #6366f1;
      --accent-hover: #818cf8;
      --accent-dim: rgba(99, 102, 241, 0.15);
      --green: #10b981;
      --green-dim: rgba(16, 185, 129, 0.15);
      --red: #ef4444;
      --red-dim: rgba(239, 68, 68, 0.15);
      --yellow: #f59e0b;
      --yellow-dim: rgba(245, 158, 11, 0.15);
      --sidebar-width: 240px;
      --topbar-height: 48px;
      --radius: 10px;
      --radius-sm: 6px;
      --mono: "Cascadia Code", "Fira Code", "JetBrains Mono", "Consolas", monospace;
      /* depth + signature accent (terminal-native console aesthetic) */
      --accent-grad: linear-gradient(135deg, #6366f1 0%, #818cf8 100%);
      --shadow-sm: 0 1px 2px rgba(0,0,0,0.45);
      --shadow-md: 0 6px 20px rgba(0,0,0,0.5);
      --shadow-lg: 0 24px 60px rgba(0,0,0,0.6);
      --surface-hi: inset 0 1px 0 rgba(255,255,255,0.045);  /* top edge highlight */
      --ring: 0 0 0 3px rgba(99,102,241,0.28);               /* focus glow */
      --glow: 0 0 24px rgba(99,102,241,0.35);
    }

    body {
      font-family: -apple-system, "Segoe UI", system-ui, sans-serif;
      /* atmospheric depth: faint indigo glows in opposite corners over near-black */
      background:
        radial-gradient(1100px 560px at 82% -12%, rgba(99,102,241,0.10), transparent 60%),
        radial-gradient(820px 480px at -8% 112%, rgba(129,140,248,0.06), transparent 60%),
        var(--bg-primary);
      background-attachment: fixed;
      color: var(--text-primary);
      height: 100dvh;
      overflow: hidden;
      -webkit-tap-highlight-color: transparent;
    }

    /* ─── Login Screen ─── */
    #login-screen {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      height: 100dvh;
      gap: 32px;
    }
    #login-screen .login-logo {
      width: 96px;
      height: 96px;
      border-radius: 22px;
      box-shadow: 0 10px 44px rgba(99, 102, 241, 0.4);
      margin-bottom: -16px;
      animation: loginReveal 0.6s cubic-bezier(0.22,1,0.36,1) both;
    }
    #login-screen h1 {
      font-family: var(--mono);
      font-size: 2.3rem;
      font-weight: 700;
      letter-spacing: 0.06em;
      animation: loginReveal 0.6s cubic-bezier(0.22,1,0.36,1) 0.08s both;
    }
    #login-screen .subtitle {
      color: var(--text-secondary);
      font-size: 0.95rem;
      margin-top: -20px;
      animation: loginReveal 0.6s cubic-bezier(0.22,1,0.36,1) 0.16s both;
    }
    #login-screen .login-box {
      background: linear-gradient(180deg, rgba(255,255,255,0.025), transparent 40%), var(--bg-secondary);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 40px 48px;
      text-align: center;
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 24px;
      box-shadow: var(--shadow-lg), var(--surface-hi);
      animation: loginReveal 0.6s cubic-bezier(0.22,1,0.36,1) 0.24s both;
    }
    @keyframes loginReveal { from { opacity: 0; transform: translateY(14px); } to { opacity: 1; transform: none; } }
    @media (prefers-reduced-motion: reduce) {
      #login-screen .login-logo, #login-screen h1, #login-screen .subtitle, #login-screen .login-box { animation: none; }
    }
    #login-screen .login-box p {
      color: var(--text-secondary);
      font-size: 0.9rem;
    }
    #telegram-login-container {
      min-height: 44px;
      display: flex;
      align-items: center;
      justify-content: center;
    }

    /* ─── Import Dialog ─── */
    #import-dialog {
      display: none;
      position: fixed; inset: 0;
      background: rgba(0,0,0,0.7);
      z-index: 1000;
      align-items: center;
      justify-content: center;
    }
    #import-dialog.visible { display: flex; }
    #import-dialog .dialog {
      background: var(--bg-secondary);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 32px;
      max-width: 440px;
      width: 90%;
    }
    #import-dialog h2 { margin-bottom: 12px; font-size: 1.2rem; }
    #import-dialog p { color: var(--text-secondary); font-size: 0.9rem; margin-bottom: 20px; line-height: 1.5; }
    #import-dialog .buttons { display: flex; gap: 12px; justify-content: flex-end; }
    #import-dialog button {
      padding: 8px 20px;
      border-radius: var(--radius-sm);
      border: 1px solid var(--border);
      background: var(--bg-tertiary);
      color: var(--text-primary);
      cursor: pointer;
      font-size: 0.9rem;
    }
    #import-dialog button:hover { background: var(--bg-hover); }
    #import-dialog button.primary {
      background: var(--accent);
      border-color: var(--accent);
    }
    #import-dialog button.primary:hover { background: var(--accent-hover); }

    /* ─── Main App ─── */
    #app {
      display: none;
      height: 100dvh;
      flex-direction: column;
    }
    #app.visible { display: flex; }

    /* ─── Top Bar ─── */
    .topbar {
      height: var(--topbar-height);
      background: var(--bg-secondary);
      border-bottom: 1px solid var(--border);
      display: flex;
      align-items: center;
      padding: 0 16px;
      gap: 12px;
      flex-shrink: 0;
      position: relative;
      overflow: hidden;
    }
    /* Real Claude usage behind the topbar content, split into two stacked rows:
       weekly (top) and 5-hour (bottom). Each row has two overlapping translucent
       bars — usage % and time-to-reset % — plus dim trailing labels. */
    .topbar .usage-bg { position: absolute; inset: 0; pointer-events: none; z-index: 0; display: flex; flex-direction: column; }
    .topbar .urow { position: relative; flex: 1; overflow: hidden; }
    .topbar .urow + .urow { border-top: 1px solid rgba(255,255,255,0.05); }
    .topbar .ub {
      position: absolute; left: 0; top: 0; bottom: 0; width: 0%;
      transition: width 0.6s ease, background 0.4s ease;
    }
    .topbar .ub-time { background: rgba(125,170,255,0.16); }  /* time-to-reset (cool) */
    .topbar .ub-usage { background: rgba(99,102,241,0.22); overflow: hidden; }  /* usage (recoloured by level in JS) */
    /* a light streak sweeps across the usage bar when its value changes */
    .topbar .ub-usage::after {
      content: ''; position: absolute; top: 0; bottom: 0; left: 0; width: 55%;
      background: linear-gradient(100deg, transparent, rgba(255,255,255,0.45), transparent);
      transform: translateX(-180%); opacity: 0; pointer-events: none;
    }
    .topbar .ub-usage.shine::after { animation: ubShine 0.9s ease; }
    @keyframes ubShine {
      0% { transform: translateX(-180%); opacity: 0; }
      12% { opacity: 1; } 88% { opacity: 1; }
      100% { transform: translateX(300%); opacity: 0; }
    }
    @media (prefers-reduced-motion: reduce) { .topbar .ub-usage.shine::after { animation: none; } }
    .topbar .ublabel {
      position: absolute; left: 0; transform: translateX(6px);
      font-size: 0.56rem; font-weight: 700; letter-spacing: 0.02em; white-space: nowrap;
      transition: left 0.6s ease, right 0.6s ease, color 0.4s ease; font-variant-numeric: tabular-nums;
    }
    .topbar .ublabel-usage { top: 1px; color: var(--text-primary); opacity: 0.9; }
    .topbar .ublabel-time { bottom: 1px; color: var(--text-secondary); opacity: 0.7; }
    /* foreground content stays above the fills */
    .topbar > *:not(.usage-bg) { position: relative; z-index: 2; }
    .topbar .hamburger {
      display: none;
      background: none;
      border: none;
      color: var(--text-primary);
      font-size: 1.4rem;
      cursor: pointer;
      padding: 4px 8px;
      -webkit-app-region: no-drag;
    }
    .topbar .logo {
      font-family: var(--mono);
      font-weight: 700;
      font-size: 1.02rem;
      letter-spacing: 0.04em;
      display: inline-flex;
      align-items: center;
      gap: 9px;
    }
    .topbar .logo img {
      width: 22px;
      height: 22px;
      border-radius: 6px;
      box-shadow: 0 0 0 1px var(--border), 0 2px 10px rgba(99,102,241,0.45);
    }
    .topbar .separator {
      color: var(--text-muted);
    }
    .topbar .project-name {
      color: var(--text-secondary);
      font-size: 0.9rem;
    }
    .topbar .spacer { flex: 1; }
    .topbar .status-badge {
      font-size: 0.75rem;
      padding: 3px 10px;
      border-radius: 12px;
      font-family: var(--mono);
      white-space: nowrap;
      flex-shrink: 0;
      -webkit-app-region: no-drag;
    }
    .topbar .status-badge.connected { background: var(--green-dim); color: var(--green); }
    .topbar .status-badge.disconnected { background: var(--red-dim); color: var(--red); }
    .topbar .tunnel-url {
      color: var(--text-muted);
      font-size: 0.75rem;
      font-family: var(--mono);
      text-decoration: none;
    }
    .topbar .tunnel-url:hover { color: var(--text-secondary); }
    .topbar .win-controls {
      display: none;
      gap: 2px;
      margin-left: 8px;
      -webkit-app-region: no-drag;
    }
    .topbar .win-controls button {
      width: 36px;
      height: 28px;
      border: none;
      background: transparent;
      color: var(--text-secondary);
      font-size: 1rem;
      cursor: pointer;
      display: flex;
      align-items: center;
      justify-content: center;
    }
    .topbar .win-controls button:hover { background: var(--bg-tertiary); color: var(--text-primary); }
    .topbar .win-controls button.win-close:hover { background: #ef4444; color: white; }

    /* ─── Layout ─── */
    .main-layout {
      display: flex;
      flex: 1;
      overflow: hidden;
    }

    /* ─── Sidebar ─── */
    .sidebar {
      width: var(--sidebar-width);
      background: var(--bg-secondary);
      border-right: 1px solid var(--border);
      display: flex;
      flex-direction: column;
      flex-shrink: 0;
      overflow: hidden;
    }
    .sidebar-header {
      padding: 12px 16px;
      font-size: 0.75rem;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      color: var(--text-muted);
      border-bottom: 1px solid var(--border-subtle);
    }
    .sidebar-list {
      flex: 1;
      overflow-y: auto;
      padding: 4px 8px;
    }
    .sidebar-item {
      position: relative;
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 8px 12px;
      border-radius: var(--radius-sm);
      cursor: pointer;
      font-size: 0.88rem;
      color: var(--text-secondary);
      transition: background 0.12s, color 0.12s;
      user-select: none;
    }
    .sidebar-item:hover { background: var(--bg-hover); color: var(--text-primary); }
    .sidebar-item.active { background: var(--accent-dim); color: var(--accent-hover); }
    .sidebar-item.active::before {
      content: ''; position: absolute; left: 0; top: 7px; bottom: 7px; width: 3px;
      border-radius: 0 3px 3px 0; background: var(--accent-grad);
    }
    .sidebar-item .dot {
      width: 8px; height: 8px;
      border-radius: 50%;
      background: var(--text-muted);
      flex-shrink: 0;
    }
    .sidebar-item.active .dot { background: var(--green); }
    .sidebar-item.has-terminal .dot { background: var(--green); }
    .sidebar-item .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .sidebar-item .svc-heart { flex-shrink: 0; display: inline-flex; align-items: center; color: var(--text-muted); }
    .sidebar-item .svc-heart .svc-ecg { display: block; }
    .sidebar-item .svc-heart.live { color: var(--green); }
    /* live = the ECG trace sweeps across, like a monitor */
    .sidebar-item .svc-heart.live .svc-ecg path { stroke-dasharray: 12 44; animation: ecgSweep 1.6s linear infinite; }
    @keyframes ecgSweep { from { stroke-dashoffset: 56; } to { stroke-dashoffset: 0; } }
    @media (prefers-reduced-motion: reduce) { .sidebar-item .svc-heart.live .svc-ecg path { animation: none; stroke-dasharray: none; } }
    .sidebar-item .close-btn {
      display: none;
      background: none;
      border: none;
      color: var(--text-muted);
      cursor: pointer;
      font-size: 1rem;
      padding: 0 4px;
      line-height: 1;
    }
    .sidebar-item:hover .close-btn { display: block; }
    .sidebar-item .close-btn:hover { color: var(--red); }
    .sidebar-item .remove-btn {
      display: none;
      background: none;
      border: none;
      color: var(--text-muted);
      cursor: pointer;
      font-size: 0.85rem;
      padding: 0 4px;
      line-height: 1;
    }
    .sidebar-item:hover .remove-btn { display: block; }
    .sidebar-item .remove-btn:hover { color: var(--red); }

    .sidebar-footer {
      padding: 8px;
      border-top: 1px solid var(--border-subtle);
    }
    .sidebar-footer button {
      width: 100%;
      padding: 8px;
      border-radius: var(--radius-sm);
      border: 1px dashed var(--border);
      background: transparent;
      color: var(--text-muted);
      cursor: pointer;
      font-size: 0.85rem;
      transition: all 0.15s;
    }
    .sidebar-footer button:hover {
      border-color: var(--accent);
      color: var(--accent);
      background: var(--accent-dim);
    }

    /* ─── Terminal Area ─── */
    .terminal-area {
      flex: 1;
      display: flex;
      flex-direction: column;
      overflow: hidden;
      position: relative;
    }
    .terminal-container {
      flex: 1;
      padding: 4px;
      overflow: hidden;
    }
    .terminal-container .xterm { height: 100%; flex: 1; }

    .terminal-placeholder {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      height: 100%;
      color: var(--text-muted);
      gap: 16px;
    }
    .terminal-placeholder .icon { font-size: 3rem; opacity: 0.3; }
    .terminal-placeholder p { font-size: 0.95rem; }
    .terminal-placeholder .hint { font-size: 0.8rem; color: var(--text-muted); }

    /* ─── Tab Bar ─── */
    .tab-bar {
      display: none;
      background: var(--bg-secondary);
      border-bottom: 1px solid var(--border);
      padding: 0 12px;
      gap: 0;
      flex-shrink: 0;
      overflow-x: auto;
      -webkit-overflow-scrolling: touch;
      scrollbar-width: none;
    }
    .tab-bar::-webkit-scrollbar { display: none; }
    .tab-bar.visible { display: flex; }
    .tab-btn {
      position: relative;
      padding: 10px 14px;
      white-space: nowrap;
      border: none;
      background: none;
      color: var(--text-muted);
      font-size: 0.82rem;
      font-weight: 500;
      cursor: pointer;
      transition: color 0.15s;
    }
    .tab-btn::after {
      content: ''; position: absolute; left: 50%; right: 50%; bottom: 0; height: 2px;
      background: var(--accent-grad); border-radius: 2px 2px 0 0;
      transition: left 0.22s cubic-bezier(0.22,1,0.36,1), right 0.22s cubic-bezier(0.22,1,0.36,1);
    }
    .tab-btn:hover { color: var(--text-primary); }
    .tab-btn.active { color: var(--accent-hover); }
    .tab-btn.active::after { left: 12px; right: 12px; }

    /* ─── Tab Panels ─── */
    .tab-panel { display: none; flex: 1; overflow: auto; }
    .tab-panel.visible { display: flex; flex-direction: column; }

    /* ─── Services Panel ─── */
    .services-panel { padding: 16px; gap: 12px; }
    .services-empty {
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      flex: 1;
      color: var(--text-muted);
      gap: 12px;
    }
    .services-empty .icon { font-size: 2.5rem; opacity: 0.3; }
    .svc-card {
      background: var(--bg-secondary);
      border: 1px solid var(--border);
      border-radius: var(--radius-sm);
      padding: 14px 16px;
      display: flex;
      flex-direction: column;
      gap: 8px;
    }
    .svc-card .svc-header {
      display: flex;
      align-items: center;
      gap: 10px;
    }
    .svc-card .svc-name {
      font-weight: 600;
      font-size: 0.9rem;
      flex: 1;
    }
    .svc-card .svc-status {
      font-size: 0.75rem;
      padding: 2px 8px;
      border-radius: 10px;
      font-family: var(--mono);
    }
    .svc-card .svc-status.running { background: var(--green-dim); color: var(--green); }
    .svc-card .svc-status.stopped { background: var(--bg-tertiary); color: var(--text-muted); }
    .svc-card .svc-status.error { background: var(--red-dim); color: var(--red); }
    .svc-card .svc-meta {
      font-size: 0.78rem;
      color: var(--text-muted);
      font-family: var(--mono);
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .svc-card .svc-meta .tunnel-link {
      color: var(--accent);
      text-decoration: none;
    }
    .svc-card .svc-meta .tunnel-link:hover { text-decoration: underline; }
    .svc-card .svc-actions {
      display: flex;
      gap: 6px;
    }
    .svc-card .svc-actions button {
      padding: 4px 12px;
      border-radius: var(--radius-sm);
      border: 1px solid var(--border);
      background: var(--bg-tertiary);
      color: var(--text-secondary);
      cursor: pointer;
      font-size: 0.78rem;
    }
    .svc-card .svc-actions button:hover { background: var(--bg-hover); color: var(--text-primary); }
    .svc-card .svc-actions button.danger:hover { background: var(--red-dim); color: var(--red); border-color: var(--red); }

    /* ─── Service Logs ─── */
    .svc-logs {
      display: none;
      background: var(--bg-primary);
      border: 1px solid var(--border);
      border-radius: var(--radius-sm);
      padding: 10px;
      max-height: 200px;
      overflow-y: auto;
      font-family: var(--mono);
      font-size: 0.72rem;
      line-height: 1.5;
      color: var(--text-muted);
      white-space: pre-wrap;
      word-break: break-all;
    }
    .svc-logs.visible { display: block; }

    /* ─── Info Panel ─── */
    .info-panel { padding: 20px; gap: 16px; }
    .info-section {
      background: var(--bg-secondary);
      border: 1px solid var(--border);
      border-radius: var(--radius-sm);
      padding: 16px;
    }
    .info-section h4 {
      font-size: 0.78rem;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      color: var(--text-muted);
      margin-bottom: 10px;
    }
    .info-row {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 6px 0;
      font-size: 0.85rem;
      border-bottom: 1px solid var(--border-subtle);
    }
    .info-row:last-child { border-bottom: none; }
    .info-row .label { color: var(--text-secondary); }
    .info-row .value { color: var(--text-primary); font-family: var(--mono); font-size: 0.8rem; text-align: right; word-break: break-all; max-width: 60%; }
    .info-row .value a { color: var(--accent); text-decoration: none; }
    .info-row .value a:hover { text-decoration: underline; }

    /* ─── Add Project Modal ─── */
    #add-project-modal {
      display: none;
      position: fixed; inset: 0;
      background: rgba(0,0,0,0.7);
      z-index: 900;
      align-items: center;
      justify-content: center;
    }
    #add-project-modal.visible { display: flex; }
    #add-project-modal .modal {
      background: var(--bg-secondary);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 28px;
      width: 90%;
      max-width: 400px;
    }
    #add-project-modal h3 { margin-bottom: 16px; font-size: 1.1rem; }
    #add-project-modal label {
      display: block;
      font-size: 0.8rem;
      color: var(--text-muted);
      margin-bottom: 4px;
      text-transform: uppercase;
      letter-spacing: 0.05em;
    }
    #add-project-modal input {
      width: 100%;
      padding: 8px 12px;
      border-radius: var(--radius-sm);
      border: 1px solid var(--border);
      background: var(--bg-tertiary);
      color: var(--text-primary);
      font-size: 0.9rem;
      margin-bottom: 12px;
      outline: none;
    }
    #add-project-modal input:focus { border-color: var(--accent); }
    #add-project-modal .modal-buttons { display: flex; gap: 10px; justify-content: flex-end; margin-top: 8px; }
    #add-project-modal button {
      padding: 8px 18px;
      border-radius: var(--radius-sm);
      border: 1px solid var(--border);
      background: var(--bg-tertiary);
      color: var(--text-primary);
      cursor: pointer;
      font-size: 0.88rem;
    }
    #add-project-modal button:hover { background: var(--bg-hover); }
    #add-project-modal button.primary { background: var(--accent); border-color: var(--accent); }
    #add-project-modal button.primary:hover { background: var(--accent-hover); }

    /* ─── Notification Toast ─── */
    .toast {
      position: fixed;
      bottom: 20px;
      right: 20px;
      background: var(--bg-secondary);
      border: 1px solid var(--border);
      border-radius: var(--radius-sm);
      padding: 12px 20px;
      font-size: 0.85rem;
      color: var(--text-primary);
      z-index: 2000;
      opacity: 0;
      transform: translateY(10px);
      transition: all 0.2s;
      pointer-events: none;
    }
    .toast.visible { opacity: 1; transform: translateY(0); pointer-events: auto; }
    .toast.error { border-color: var(--red); }
    .toast.success { border-color: var(--green); }

    /* ─── Git Panel & Files Panel layout ─── */
    .git-panel, .files-panel { flex-direction: column; height: 100%; overflow: hidden; }

    /* ─── Git Panel ─── */
    .git-branch { padding: 8px 12px; font-size: 12px; color: var(--text-secondary); border-bottom: 1px solid var(--border-subtle); display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
    .git-branch .gb-name { color: var(--accent-hover); font-weight: 600; }
    .git-branch .gb-sync { font-size: 11px; color: var(--text-muted); }
    .git-files { flex: 1; overflow-y: auto; padding: 4px 0; }
    .git-section-head { padding: 6px 12px; font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.5px; color: var(--text-muted); display: flex; align-items: center; gap: 6px; }
    .git-section-head .gs-count { color: var(--text-secondary); }
    .git-section-head .gs-actions { margin-left: auto; display: flex; gap: 2px; }
    .git-section-head .gs-actions button { background: none; border: none; color: var(--text-muted); cursor: pointer; font-size: 12px; padding: 1px 4px; border-radius: 3px; }
    .git-section-head .gs-actions button:hover { color: var(--text-primary); background: var(--bg-hover); }
    .git-file { display: flex; align-items: center; gap: 6px; padding: 3px 12px 3px 20px; font-size: 12px; font-family: var(--mono); cursor: pointer; }
    .git-file:hover { background: var(--bg-hover); }
    .git-file .gf-status { width: 14px; flex-shrink: 0; font-size: 11px; font-weight: 700; text-align: center; }
    .git-file .gf-status.gf-M { color: var(--yellow); }
    .git-file .gf-status.gf-A { color: var(--green); }
    .git-file .gf-status.gf-D { color: var(--red); }
    .git-file .gf-status.gf-U { color: var(--accent); }
    .git-file .gf-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-primary); font-size: 12px; }
    .git-file .gf-name .gf-dir { color: var(--text-muted); }
    .git-file .gf-stats { font-size: 10px; flex-shrink: 0; display: flex; gap: 4px; }
    .git-file .gf-stats .gf-add { color: var(--green); }
    .git-file .gf-stats .gf-del { color: var(--red); }
    .git-file .gf-actions { display: flex; gap: 2px; }
    .git-file .gf-actions button { background: none; border: none; color: var(--text-muted); cursor: pointer; font-size: 12px; padding: 1px 4px; border-radius: 3px; }
    .git-file .gf-actions button:hover { color: var(--text-primary); background: var(--bg-tertiary); }
    .git-commit-bar { border-top: 1px solid var(--border); padding: 10px 12px; flex-shrink: 0; }
    .git-commit-bar textarea { width: 100%; resize: none; border: 1px solid var(--border); border-radius: 6px; padding: 6px 10px; font-size: 12px; font-family: inherit; line-height: 1.4; background: var(--bg-primary); color: var(--text-primary); min-height: 48px; }
    .git-commit-bar textarea:focus { outline: none; border-color: var(--accent); }
    .git-commit-bar textarea::placeholder { color: var(--text-muted); }
    .git-commit-btns { display: flex; gap: 6px; margin-top: 8px; }
    .git-commit-btns button { flex: 1; padding: 6px 10px; border-radius: var(--radius-sm); border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-primary); cursor: pointer; font-size: 12px; }
    .git-commit-btns button:hover { background: var(--accent); border-color: var(--accent); }
    .git-commit-btns button.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
    .git-commit-btns button.primary:hover { background: var(--accent-hover); }
    .git-empty { padding: 20px 12px; text-align: center; color: var(--text-muted); font-size: 12px; }

    /* ─── Files Panel ─── */
    .files-breadcrumb { padding: 8px 12px; font-size: 12px; color: var(--text-muted); border-bottom: 1px solid var(--border-subtle); display: flex; align-items: center; gap: 4px; flex-shrink: 0; flex-wrap: wrap; }
    .files-breadcrumb a { color: var(--accent); cursor: pointer; text-decoration: none; }
    .files-breadcrumb a:hover { text-decoration: underline; }
    .files-list { flex: 1; overflow-y: auto; }
    .file-item { display: flex; align-items: center; gap: 8px; padding: 4px 12px; font-size: 12px; font-family: var(--mono); cursor: pointer; }
    .file-item:hover { background: var(--bg-hover); }
    .file-item .fi-icon { width: 16px; text-align: center; flex-shrink: 0; }
    .file-item .fi-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .file-item .fi-size { color: var(--text-muted); font-size: 11px; flex-shrink: 0; }
    .file-item.dir .fi-icon { color: var(--yellow); }
    .file-item.file .fi-icon { color: var(--text-muted); }

    /* ─── File Editor Modal ─── */
    .fe-editor { position: fixed; inset: 0; z-index: 2000; background: rgba(0,0,0,0.6); display: none; flex-direction: column; }
    .fe-editor.visible { display: flex; }
    .fe-header { padding: 8px 12px; background: var(--bg-secondary); border-bottom: 1px solid var(--border); display: flex; align-items: center; gap: 8px; }
    .fe-header .fe-path { flex: 1; font-size: 12px; font-family: var(--mono); color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .fe-header button { padding: 4px 12px; border-radius: var(--radius-sm); border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-primary); cursor: pointer; font-size: 12px; }
    .fe-header button:hover { background: var(--accent); border-color: var(--accent); }
    .fe-header button.save { background: var(--green); border-color: var(--green); color: #fff; }
    .fe-content { flex: 1; overflow: hidden; display: flex; flex-direction: column; }
    .fe-content .cm-editor { height: 100%; }
    .fe-content .cm-scroller { overflow: auto; }

    /* ─── Mobile ─── */
    @media (max-width: 768px) {
      :root { --sidebar-width: 280px; }
      .topbar .hamburger { display: block; }
      .sidebar {
        position: fixed;
        left: 0; top: var(--topbar-height);
        bottom: 0;
        z-index: 500;
        transform: translateX(-100%);
        transition: transform 0.2s ease;
        box-shadow: 4px 0 24px rgba(0,0,0,0.4);
      }
      .sidebar.open { transform: translateX(0); }
      .sidebar-overlay {
        display: none;
        position: fixed; inset: 0;
        top: var(--topbar-height);
        background: rgba(0,0,0,0.4);
        z-index: 499;
      }
      .sidebar-overlay.visible { display: block; }
      .topbar .tunnel-url { display: none; }
      /* Keep the topbar tidy on phones: shrink gaps, let the project name
         truncate, and keep the usage meter compact so nothing gets clipped. */
      .topbar { gap: 8px; padding: 0 10px; }
      .topbar .project-name { overflow: hidden; text-overflow: ellipsis; min-width: 0; }
      .topbar .ublabel { font-size: 0.52rem; }
      /* Reclaim width so badges never overflow: drop the wordmark text (icon
         stays) and collapse the connection badge to just its status dot. */
      .topbar .logo .logo-text { display: none; }
      .topbar .status-badge.connected, .topbar .status-badge.disconnected {
        font-size: 0; padding: 6px; gap: 0; border-radius: 50%;
      }
      .tab-btn { padding: 8px 12px; font-size: 0.76rem; }
      .svc-card { padding: 12px; }
      .info-panel { padding: 12px; }
      .info-row { flex-direction: column; gap: 2px; }
      .info-row .value { text-align: left; max-width: 100%; }
      /* Always show close/remove buttons on mobile (no hover) */
      .sidebar-item .close-btn { display: block; }
      .sidebar-item .remove-btn { display: block; }
      /* Terminal touch scrolling */
      .terminal-container .xterm .xterm-viewport { overflow-y: scroll !important; -webkit-overflow-scrolling: touch; touch-action: pan-y; }
      .terminal-container .xterm .xterm-screen { touch-action: pan-y; pointer-events: auto; }

      /* Kanban: stack columns vertically so the board is usable on a phone.
         Extra specificity so these win over base rules later in the sheet. */
      .kanban-panel .kanban-board { flex-direction: column; align-items: stretch; gap: 10px; }
      .kanban-panel .kanban-col { max-width: 100%; min-width: 0; max-height: none; }
      .kanban-panel .kanban-col-body { max-height: none; }
      .kanban-panel .kanban-toolbar { position: sticky; top: 0; z-index: 5; }

      /* Mindmap: tap a node to open its detail modal (no hover on touch). */
      .mindmap-panel .mm-node { width: 168px; }

      /* Secrets: let request actions wrap nicely */
      .secret-request .sec-actions { width: 100%; }
      .secret-item { flex-wrap: wrap; }
      .secret-item .sec-actions { width: 100%; justify-content: flex-end; }
    }

    /* ─── Terminal overlays (outside media query) ─── */
    .term-select-overlay {
      position: absolute; inset: 0; z-index: 10;
      overflow: auto; -webkit-overflow-scrolling: touch;
      white-space: pre; font-family: var(--mono); font-size: 14px;
      line-height: 1.2; color: var(--text-primary);
      background: var(--bg-primary); padding: 4px;
      user-select: text; -webkit-user-select: text;
      display: none;
    }
    .term-select-overlay.visible { display: block; }
    .term-select-toggle {
      display: none;
      position: absolute; top: 8px; right: 8px; z-index: 11;
      padding: 4px 10px; border-radius: var(--radius-sm);
      border: 1px solid var(--border); background: var(--bg-secondary);
      color: var(--text-secondary); cursor: pointer; font-size: 11px;
    }
    .term-select-toggle.active { background: var(--accent); color: #fff; border-color: var(--accent); }
    @media (max-width: 768px) { .term-select-toggle { display: block; } }
    .term-scroll-bottom {
      display: none;
      position: absolute; bottom: 16px; left: 50%; transform: translateX(-50%); z-index: 11;
      width: 36px; height: 36px; border-radius: 50%;
      border: 1px solid var(--border); background: var(--bg-secondary);
      color: var(--text-secondary); cursor: pointer; font-size: 16px;
      align-items: center; justify-content: center;
      opacity: 0.85; box-shadow: 0 2px 8px rgba(0,0,0,0.4);
    }
    .term-scroll-bottom:hover { background: var(--bg-tertiary); color: var(--text-primary); }
    .term-scroll-bottom.visible { display: flex; }
    .term-input-bar {
      display: flex; gap: 6px; padding: 6px 8px;
      background: var(--bg-secondary); border-top: 1px solid var(--border);
      align-items: flex-end;
    }
    .term-input {
      flex: 1; resize: none; padding: 6px 10px;
      border-radius: var(--radius-sm); border: 1px solid var(--border);
      background: var(--bg-primary); color: var(--text-primary);
      font-family: var(--mono); font-size: 13px; line-height: 1.4;
      max-height: 120px; overflow-y: auto;
      outline: none;
    }
    .term-input:focus { border-color: var(--accent); }
    .term-send-btn {
      padding: 6px 14px; border-radius: var(--radius-sm);
      border: 1px solid var(--accent); background: var(--accent);
      color: #fff; cursor: pointer; font-size: 12px; font-weight: 600;
      white-space: nowrap; align-self: flex-end;
    }
    .term-send-btn:hover { background: var(--accent-hover); }
    .term-attach-btn {
      align-self: flex-end; padding: 6px 9px; border-radius: var(--radius-sm);
      border: 1px solid var(--border); background: var(--bg-tertiary);
      color: var(--text-secondary); cursor: pointer; font-size: 14px; line-height: 1;
    }
    .term-attach-btn:hover { color: var(--accent-hover); border-color: var(--accent); }
    .term-attach-btn.busy { opacity: 0.6; pointer-events: none; }

    /* ─── Tool Panel ─── */
    .term-tool-toggle {
      width: 34px; height: 34px; border-radius: var(--radius-sm);
      border: 1px solid var(--border); background: var(--bg-primary);
      color: var(--text-secondary); cursor: pointer; font-size: 16px;
      display: flex; align-items: center; justify-content: center;
      flex-shrink: 0;
    }
    .term-tool-toggle:hover, .term-tool-toggle.active { background: var(--accent); color: #fff; border-color: var(--accent); }
    .term-tool-panel {
      display: none;
      background: var(--bg-secondary); border: 1px solid var(--border);
      border-radius: var(--radius); padding: 8px;
      box-shadow: 0 4px 16px rgba(0,0,0,0.5);
      flex-direction: column; gap: 6px;
    }
    .term-tool-panel.visible { display: flex; }
    .term-tool-row { display: flex; gap: 4px; justify-content: center; }
    .term-tool-btn {
      min-width: 38px; height: 34px; border-radius: var(--radius-sm);
      border: 1px solid var(--border); background: var(--bg-primary);
      color: var(--text-primary); cursor: pointer;
      font-family: var(--mono); font-size: 12px; font-weight: 600;
      display: flex; align-items: center; justify-content: center;
      padding: 0 8px; white-space: nowrap;
    }
    .term-tool-btn:active { background: var(--accent); color: #fff; border-color: var(--accent); }
    .term-tool-btn.wide { min-width: 56px; }

    /* ─── Kanban ─── */
    .kanban-panel { padding: 0; }
    .kanban-toolbar {
      display: flex; align-items: center; gap: 8px; padding: 10px 14px;
      border-bottom: 1px solid var(--border); flex-wrap: wrap;
    }
    .kanban-toolbar .spacer { flex: 1; }
    .kanban-btn {
      background: var(--bg-tertiary); border: 1px solid var(--border); color: var(--text-primary);
      border-radius: var(--radius-sm); padding: 6px 12px; font-size: 0.82rem; cursor: pointer;
    }
    .kanban-btn:hover { background: var(--bg-hover); }
    .kanban-btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
    .kanban-btn.primary:hover { background: var(--accent-hover); }
    .kanban-btn.toggled { background: var(--accent-dim); border-color: var(--accent); color: var(--accent-hover); }
    .kanban-board { display: flex; gap: 12px; padding: 14px; overflow-x: auto; align-items: flex-start; flex: 1; }
    .kanban-col {
      flex: 1 1 0; min-width: 220px; max-width: 360px; background: var(--bg-secondary);
      border: 1px solid var(--border); border-radius: var(--radius); display: flex; flex-direction: column;
      max-height: 100%;
    }
    .kanban-col.drag-over { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent) inset; }
    .kanban-col-head {
      display: flex; align-items: center; justify-content: space-between; padding: 10px 12px;
      font-size: 0.8rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em;
      color: var(--text-secondary); border-bottom: 1px solid var(--border-subtle);
    }
    .kanban-col-head .count { color: var(--text-muted); font-weight: 500; }
    .kanban-col-body { padding: 8px; display: flex; flex-direction: column; gap: 8px; overflow-y: auto; }
    .kanban-card {
      background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius-sm);
      padding: 10px; cursor: grab;
    }
    .kanban-card:active { cursor: grabbing; }
    .kanban-card.deleted { opacity: 0.6; }
    .kanban-card .card-title { font-weight: 600; font-size: 0.9rem; margin-bottom: 4px; word-break: break-word; }
    .kanban-card .card-mm {
      font-size: 0.68rem; font-weight: 600; color: var(--accent-hover);
      background: var(--accent-dim); border-radius: 8px; padding: 0 6px;
      white-space: nowrap; cursor: pointer; vertical-align: middle;
    }
    .kanban-card .card-desc { font-size: 0.8rem; color: var(--text-secondary); margin-bottom: 8px; white-space: pre-wrap; word-break: break-word; }
    .kanban-card .card-actions { display: flex; gap: 6px; margin-top: 8px; flex-wrap: wrap; }
    .kanban-card .card-actions button {
      background: none; border: 1px solid var(--border); color: var(--text-muted);
      border-radius: 4px; font-size: 0.72rem; padding: 2px 7px; cursor: pointer;
    }
    .kanban-card .card-actions button:hover { color: var(--text-primary); border-color: var(--text-muted); }
    .kanban-card .card-actions select {
      background: var(--bg-primary); border: 1px solid var(--border); color: var(--text-secondary);
      border-radius: 4px; font-size: 0.72rem; padding: 2px 4px; cursor: pointer;
    }
    .kanban-todos { display: flex; flex-direction: column; gap: 3px; margin-top: 6px; }
    .kanban-todo { display: flex; align-items: flex-start; gap: 6px; font-size: 0.8rem; }
    .kanban-todo input[type=checkbox] { margin-top: 2px; cursor: pointer; }
    .kanban-todo .todo-text { flex: 1; word-break: break-word; }
    .kanban-todo .todo-text.done { text-decoration: line-through; color: var(--text-muted); }
    .kanban-todo .todo-del { background: none; border: none; color: var(--text-muted); cursor: pointer; font-size: 0.85rem; }
    .kanban-todo .todo-del:hover { color: var(--red); }
    .kanban-todo-add { display: flex; gap: 4px; margin-top: 6px; }
    .kanban-todo-add input {
      flex: 1; background: var(--bg-primary); border: 1px solid var(--border); color: var(--text-primary);
      border-radius: 4px; padding: 3px 6px; font-size: 0.78rem;
    }
    .kanban-progress { font-size: 0.72rem; color: var(--text-muted); margin-top: 6px; }
    .kanban-empty, .secrets-empty { color: var(--text-muted); padding: 24px; text-align: center; font-size: 0.85rem; }
    .kanban-history { padding: 14px; overflow-y: auto; flex: 1; }
    .kanban-history .hist-row { font-size: 0.78rem; color: var(--text-secondary); padding: 4px 0; border-bottom: 1px solid var(--border-subtle); }
    .kanban-history .hist-row .ts { color: var(--text-muted); margin-right: 8px; font-family: var(--mono); font-size: 0.72rem; }

    /* ─── Secrets ─── */
    .secrets-panel { padding: 14px; gap: 16px; }
    .secret-badge {
      display: inline-flex; min-width: 16px; height: 16px; padding: 0 4px; border-radius: 8px;
      background: var(--red); color: #fff; font-size: 0.65rem; font-weight: 700;
      align-items: center; justify-content: center; margin-left: 2px;
    }
    .secrets-section-title { font-size: 0.8rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-secondary); margin-bottom: 8px; }
    .secrets-requests { background: var(--yellow-dim); border: 1px solid var(--yellow); border-radius: var(--radius); padding: 12px; margin-bottom: 16px; }
    .secret-request { display: flex; align-items: center; gap: 10px; padding: 8px 0; border-bottom: 1px solid rgba(245,158,11,0.25); flex-wrap: wrap; }
    .secret-request:last-child { border-bottom: none; }
    .secret-request .req-info { flex: 1; min-width: 160px; }
    .secret-request .req-name { font-weight: 600; font-size: 0.9rem; }
    .secret-request .req-meta { font-size: 0.76rem; color: var(--text-secondary); }
    .secret-list { display: flex; flex-direction: column; gap: 8px; }
    .secret-item {
      background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius-sm);
      padding: 10px 12px; display: flex; align-items: center; gap: 10px;
    }
    .secret-item .sec-info { flex: 1; min-width: 0; }
    .secret-item .sec-name { font-weight: 600; font-size: 0.9rem; word-break: break-word; }
    .secret-item .sec-desc { font-size: 0.78rem; color: var(--text-secondary); word-break: break-word; }
    .secret-item .sec-value { font-family: var(--mono); font-size: 0.8rem; color: var(--green); margin-top: 4px; word-break: break-all; }
    .secret-item .sec-actions { display: flex; gap: 6px; flex-shrink: 0; }
    .secret-item .sec-actions button {
      background: var(--bg-tertiary); border: 1px solid var(--border); color: var(--text-secondary);
      border-radius: 4px; font-size: 0.74rem; padding: 4px 9px; cursor: pointer;
    }
    .secret-item .sec-actions button:hover { color: var(--text-primary); }
    .secret-item .sec-actions button.danger:hover { color: var(--red); border-color: var(--red); }
    .secrets-add {
      background: var(--bg-secondary); border: 1px solid var(--border); border-radius: var(--radius);
      padding: 14px; display: flex; flex-direction: column; gap: 8px;
    }
    .secrets-add input, .secrets-add textarea {
      background: var(--bg-primary); border: 1px solid var(--border); color: var(--text-primary);
      border-radius: var(--radius-sm); padding: 8px 10px; font-size: 0.85rem; font-family: inherit;
    }
    .secrets-add textarea { resize: vertical; min-height: 44px; }
    .secrets-add input:focus, .secrets-add textarea:focus { border-color: var(--accent); outline: none; }
    .secrets-add .hint { font-size: 0.74rem; color: var(--text-muted); }

    /* ─── PIN modal ─── */
    .pin-modal {
      display: none; position: fixed; inset: 0; z-index: 200;
      background: rgba(0,0,0,0.6); align-items: center; justify-content: center; padding: 20px;
    }
    .pin-modal.visible { display: flex; }
    .pin-modal .pin-box {
      background: var(--bg-secondary); border: 1px solid var(--border); border-radius: var(--radius);
      padding: 22px; width: 100%; max-width: 320px; text-align: center;
    }
    .pin-modal h3 { font-size: 1rem; margin-bottom: 6px; }
    .pin-modal p { font-size: 0.82rem; color: var(--text-secondary); margin-bottom: 14px; word-break: break-word; }
    .pin-modal input {
      width: 100%; text-align: center; letter-spacing: 0.5em; font-size: 1.4rem; font-family: var(--mono);
      background: var(--bg-primary); border: 1px solid var(--border); color: var(--text-primary);
      border-radius: var(--radius-sm); padding: 10px; margin-bottom: 12px;
    }
    .pin-modal input:focus { border-color: var(--accent); outline: none; }
    .pin-modal .pin-error { color: var(--red); font-size: 0.78rem; min-height: 16px; margin-bottom: 8px; }
    .pin-modal .pin-buttons { display: flex; gap: 10px; }
    .pin-modal .pin-buttons button { flex: 1; border-radius: var(--radius-sm); padding: 9px; font-size: 0.85rem; cursor: pointer; border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-primary); }
    .pin-modal .pin-buttons button.primary { background: var(--accent); border-color: var(--accent); color: #fff; }

    /* ─── Mindmap ─── */
    .mindmap-panel { padding: 0; }
    .mindmap-canvas { position: relative; flex: 1; overflow: auto; background:
      radial-gradient(circle, var(--border-subtle) 1px, transparent 1px) 0 0 / 22px 22px; }
    .mindmap-inner { position: relative; min-width: 100%; min-height: 100%; }
    /* Scaled content layer (zoom). Base-sized; transform: scale applied via JS. */
    .mm-scale { position: absolute; top: 0; left: 0; transform-origin: 0 0; }
    .mindmap-edges { position: absolute; top: 0; left: 0; pointer-events: none; overflow: visible; }
    .mm-node {
      position: absolute; transform: translateY(-50%);
      background: var(--bg-card); border: 1px solid var(--border); border-radius: 14px;
      padding: 8px 11px; width: 180px; box-shadow: 0 2px 8px rgba(0,0,0,0.35);
      cursor: pointer; transition: opacity 0.12s, box-shadow 0.12s, border-color 0.12s;
    }
    .mm-node:hover { border-color: var(--accent); box-shadow: 0 4px 14px rgba(0,0,0,0.5); }
    .mm-node.root { border-color: var(--accent); background: var(--accent-dim); }
    .mm-node.linked { border-color: var(--green); }
    .mm-node.scoped { border-color: var(--accent); }
    .mm-node .mm-text {
      font-size: 0.83rem; font-weight: 600; word-break: break-word; line-height: 1.3;
      display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
    }
    /* compact meta chips row */
    .mm-node .mm-meta { display: flex; gap: 5px; flex-wrap: wrap; margin-top: 6px; }
    .mm-node .mm-meta:empty { display: none; }
    .mm-chip {
      display: inline-flex; align-items: center; gap: 3px; max-width: 100%;
      font-size: 0.66rem; padding: 1px 7px; border-radius: 8px; white-space: nowrap;
      overflow: hidden; text-overflow: ellipsis;
    }
    .mm-chip.link { color: var(--green); background: var(--green-dim); }
    .mm-chip.link.missing { color: var(--text-muted); background: var(--bg-tertiary); }
    .mm-chip.scope { color: var(--accent-hover); background: var(--accent-dim); }
    .mm-chip.inherited { color: var(--text-muted); background: var(--bg-tertiary); border: 1px dashed var(--border); }
    .mm-chip.note { color: var(--text-secondary); background: var(--bg-tertiary); }
    /* search filter states */
    .mm-node.mm-dim { opacity: 0.18; }
    .mm-node.mm-hit { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent); }
    /* persistent dashed "add idea" ghost node */
    .mm-node.mm-ghost {
      border: 1.5px dashed var(--border); background: transparent; box-shadow: none;
      display: flex; align-items: center; justify-content: center; gap: 6px;
      min-height: 40px; opacity: 0.5; color: var(--text-muted);
    }
    .mm-node.mm-ghost:hover { border-color: var(--accent); color: var(--accent-hover); opacity: 1; box-shadow: none; }
    .mm-ghost .mm-ghost-plus { font-size: 1.05rem; font-weight: 700; line-height: 1; }
    .mm-ghost .mm-ghost-label { font-size: 0.74rem; font-weight: 500; }
    /* mindmap entrance (only on a fresh render — gated by .mm-anim) */
    @keyframes mmFadeIn { from { opacity: 0; } to { opacity: 1; } }
    @keyframes mmGhostIn { from { opacity: 0; } to { opacity: 0.5; } }
    @keyframes mmChipIn { from { opacity: 0; transform: translateY(-4px); } to { opacity: 1; transform: none; } }
    .mindmap-canvas.mm-anim .mm-node:not(.mm-ghost) { animation: mmFadeIn 0.34s ease both; }
    .mindmap-canvas.mm-anim .mm-ghost { animation: mmGhostIn 0.4s ease 0.12s both; }
    .mindmap-canvas.mm-anim .mindmap-edges { animation: mmFadeIn 0.5s ease both; }
    .mm-proj-bar.mm-anim .mm-proj-chip { animation: mmChipIn 0.3s ease both; }
    @media (prefers-reduced-motion: reduce) {
      .mindmap-canvas.mm-anim .mm-node:not(.mm-ghost), .mindmap-canvas.mm-anim .mindmap-edges, .mm-proj-bar.mm-anim .mm-proj-chip { animation: none; opacity: 1; transform: none; }
      .mindmap-canvas.mm-anim .mm-ghost { animation: none; opacity: 0.5; }
    }
    .mm-collapse {
      position: absolute; right: -9px; top: 50%; transform: translateY(-50%);
      width: 18px; height: 18px; border-radius: 50%; border: 1px solid var(--border);
      background: var(--bg-tertiary); color: var(--text-secondary); font-size: 0.7rem;
      cursor: pointer; display: flex; align-items: center; justify-content: center; padding: 0;
    }

    /* ─── Drag & drop (kanban cards + mindmap nodes; mouse + long-press touch) ─── */
    .drag-clone { box-shadow: 0 12px 32px rgba(0,0,0,0.55); pointer-events: none; }
    .kanban-card.dragging, .mm-node.dragging { opacity: 0.35; }
    .kb-drop-line { height: 0; border-top: 2px solid var(--accent); margin: 3px 2px; border-radius: 1px; }
    .mm-node.mm-drop-target { outline: 2px solid var(--accent); outline-offset: 1px; }
    body.dragging-active, body.dragging-active * { cursor: grabbing !important; }
    body.dragging-active { user-select: none; -webkit-user-select: none; }

    /* ─── Text input modal (replaces window.prompt, which Electron doesn't support) ─── */
    .input-modal { display: none; position: fixed; inset: 0; z-index: 210; background: rgba(0,0,0,0.6); align-items: center; justify-content: center; padding: 20px; }
    .input-modal.visible { display: flex; }
    .input-modal .im-box { background: var(--bg-secondary); border: 1px solid var(--border); border-radius: var(--radius); padding: 20px; width: 100%; max-width: 440px; }
    .input-modal h3 { font-size: 1rem; margin-bottom: 8px; }
    .input-modal .im-label { font-size: 0.82rem; color: var(--text-secondary); margin-bottom: 10px; white-space: pre-wrap; max-height: 220px; overflow: auto; }
    .input-modal input, .input-modal textarea { width: 100%; background: var(--bg-primary); border: 1px solid var(--border); color: var(--text-primary); border-radius: var(--radius-sm); padding: 9px 11px; font-size: 0.9rem; font-family: inherit; }
    .input-modal textarea { resize: vertical; min-height: 70px; }
    .input-modal input:focus, .input-modal textarea:focus { border-color: var(--accent); outline: none; }
    .input-modal .im-buttons { display: flex; gap: 10px; margin-top: 12px; }
    .input-modal .im-buttons button { flex: 1; border-radius: var(--radius-sm); padding: 9px; font-size: 0.85rem; cursor: pointer; border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-primary); }
    .input-modal .im-buttons button.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
    /* link picker */
    .input-modal select { background: var(--bg-primary); border: 1px solid var(--border); color: var(--text-primary); border-radius: var(--radius-sm); padding: 8px 10px; font-size: 0.85rem; max-width: 45%; }
    .input-modal .lm-row { display: flex; gap: 8px; margin-bottom: 8px; }
    .input-modal .lm-list { max-height: 320px; overflow: auto; border: 1px solid var(--border); border-radius: var(--radius-sm); }
    .input-modal .lm-item { padding: 8px 10px; cursor: pointer; border-bottom: 1px solid var(--border-subtle); font-size: 0.85rem; }
    .input-modal .lm-item:last-child { border-bottom: none; }
    .input-modal .lm-item:hover { background: var(--bg-hover); }
    .input-modal .lm-item.todo { padding-left: 28px; font-size: 0.8rem; color: var(--text-secondary); }
    .input-modal .lm-item .st { color: var(--text-muted); font-size: 0.72rem; }
    .input-modal .lm-empty { padding: 16px; text-align: center; color: var(--text-muted); font-size: 0.82rem; }
    .input-modal .lm-locked { font-size: 0.76rem; color: var(--amber, #f59e0b); margin-bottom: 8px; }
    .input-modal .lm-scope-btn { width: 100%; text-align: left; padding: 9px 11px; border-radius: var(--radius-sm); border: 1px solid var(--accent); background: var(--accent-dim); color: var(--text-primary); font-size: 0.85rem; cursor: pointer; }
    .input-modal .lm-scope-btn:hover { background: var(--accent); color: #fff; }
    .input-modal .lm-or { text-align: center; font-size: 0.72rem; color: var(--text-muted); margin: 10px 0 8px; }

    /* mindmap drag: sibling insert indicators */
    .mm-node.mm-insert-above { box-shadow: 0 -3px 0 0 var(--accent); }
    .mm-node.mm-insert-below { box-shadow: 0 3px 0 0 var(--accent); }

    /* mindmap node detail modal */
    /* notes as a list — view */
    #mm-detail-modal .mmd-notes { list-style: none; margin: 6px 0 10px; padding: 0; max-height: 42vh; overflow: auto; display: flex; flex-direction: column; gap: 6px; }
    #mm-detail-modal .mmd-notes:empty { display: none; }
    #mm-detail-modal .mmd-notes li {
      position: relative; font-size: 0.86rem; color: var(--text-secondary); white-space: pre-wrap; word-break: break-word;
      padding: 8px 10px 8px 26px; background: var(--bg-tertiary); border-radius: var(--radius-sm); border: 1px solid var(--border-subtle);
    }
    #mm-detail-modal .mmd-notes li::before { content: ''; position: absolute; left: 11px; top: 14px; width: 5px; height: 5px; border-radius: 50%; background: var(--accent); }
    /* notes editor */
    #mm-detail-modal .mmd-notes-edit { display: flex; flex-direction: column; gap: 6px; }
    #mm-detail-modal .mmd-note-row { display: flex; gap: 6px; align-items: flex-start; }
    #mm-detail-modal .mmd-note-row textarea { flex: 1; min-height: 38px; }
    #mm-detail-modal .mmd-note-row .mmd-note-del { flex-shrink: 0; align-self: stretch; padding: 0 10px; border-radius: var(--radius-sm); border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-muted); cursor: pointer; }
    #mm-detail-modal .mmd-note-row .mmd-note-del:hover { color: var(--red); border-color: var(--red); }
    #mm-detail-modal .mmd-add-note { margin-top: 8px; width: 100%; padding: 7px; border-radius: var(--radius-sm); border: 1px dashed var(--border); background: transparent; color: var(--text-secondary); cursor: pointer; font-size: 0.82rem; }
    #mm-detail-modal .mmd-add-note:hover { border-color: var(--accent); color: var(--accent-hover); }
    #mm-detail-modal .mmd-link { font-size: 0.82rem; color: var(--green); background: var(--green-dim); border-radius: 8px; padding: 7px 10px; margin-bottom: 6px; word-break: break-word; }
    #mm-detail-modal .mmd-link.missing { color: var(--text-muted); background: var(--bg-tertiary); }
    #mm-detail-modal .mmd-link:empty { display: none; }
    #mm-detail-modal .mmd-link .st { color: var(--text-muted); }
    #mm-detail-modal label.im-label { display: block; font-size: 0.78rem; color: var(--text-secondary); margin: 0 0 4px; }
    #mm-detail-modal .mmd-actions { display: flex; gap: 8px; margin-top: 14px; flex-wrap: wrap; }
    #mm-detail-modal .mmd-actions button { flex: 1; min-width: 72px; border-radius: var(--radius-sm); padding: 8px; font-size: 0.82rem; cursor: pointer; border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-primary); }
    #mm-detail-modal .mmd-actions button.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
    #mm-detail-modal .mmd-actions button.danger { color: var(--red); }
    #mm-detail-modal .mmd-actions button.danger:hover { border-color: var(--red); background: rgba(255,80,80,0.12); }

    /* mindmap toolbar search */
    .mm-search { background: var(--bg-primary); border: 1px solid var(--border); color: var(--text-primary); border-radius: var(--radius-sm); padding: 6px 10px; font-size: 0.82rem; min-width: 160px; }
    .mm-search:focus { border-color: var(--accent); outline: none; }
    /* mindmap project filter bar */
    .mm-proj-bar { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; padding: 8px 12px; border-bottom: 1px solid var(--border); background: var(--bg-secondary); }
    .mm-proj-label { font-size: 0.74rem; color: var(--text-muted); margin-right: 2px; }
    .mm-proj-chip { display: inline-flex; align-items: center; gap: 5px; font-size: 0.76rem; padding: 3px 10px; border-radius: 20px; cursor: pointer; border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-primary); }
    .mm-proj-chip .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--green); }
    .mm-proj-chip.off { color: var(--text-muted); text-decoration: line-through; opacity: 0.7; }
    .mm-proj-chip.reset { border-style: dashed; color: var(--text-secondary); }
    .mm-proj-flag { font-size: 0.74rem; color: var(--amber, #f59e0b); margin-left: 4px; }

    /* usage last-updated badge (click → history chart) */
    .topbar .status-badge.usage-updated { background: var(--bg-tertiary); color: var(--text-secondary); cursor: pointer; }
    .topbar .status-badge.usage-updated:hover { color: var(--text-primary); }
    /* usage history chart modal */
    #usage-modal.visible .im-box { animation: ucBoxIn 0.26s cubic-bezier(0.22,1,0.36,1); }
    @keyframes ucBoxIn { from { opacity: 0; transform: translateY(10px) scale(0.985); } to { opacity: 1; transform: none; } }
    @media (prefers-reduced-motion: reduce) { #usage-modal.visible .im-box { animation: none; } }
    .uc-controls { display: flex; gap: 10px; margin-bottom: 12px; flex-wrap: wrap; }
    .uc-seg { display: flex; gap: 4px; background: var(--bg-tertiary); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 3px; }
    #uc-ranges { flex: 1; }
    .uc-seg button { flex: 1; border-radius: 5px; padding: 5px 10px; font-size: 0.78rem; cursor: pointer; border: none; background: transparent; color: var(--text-secondary); white-space: nowrap; }
    .uc-seg button.active { background: var(--accent); color: #fff; }
    .uc-chart-wrap { position: relative; width: 100%; }
    .uc-chart { width: 100%; }
    .uc-svg { width: 100%; height: auto; display: block; touch-action: none; }
    .uc-svg .uc-axis { fill: rgba(255,255,255,0.4); font-size: 10px; font-family: inherit; letter-spacing: 0.02em; }
    .uc-svg .uc-now { fill: rgba(255,255,255,0.5); font-size: 9px; font-family: inherit; }
    /* choreographed reveal: bands fade up, target fades, actual line draws in */
    @keyframes ucDraw { from { stroke-dashoffset: 1; } to { stroke-dashoffset: 0; } }
    @keyframes ucFade { from { opacity: 0; } to { opacity: 1; } }
    @keyframes ucRise { from { opacity: 0; transform: translateY(6px); } to { opacity: 1; transform: translateY(0); } }
    .uc-svg .uc-band { opacity: 0; transform-box: fill-box; animation: ucRise 0.5s cubic-bezier(0.22,1,0.36,1) 0.1s forwards; }
    .uc-svg .uc-line-target { opacity: 0; animation: ucFade 0.5s ease 0.28s forwards; }
    .uc-svg .uc-line-actual { stroke-dasharray: 1; stroke-dashoffset: 1; animation: ucDraw 0.75s cubic-bezier(0.65,0,0.35,1) forwards; }
    .uc-svg .uc-lbl-a, .uc-svg .uc-lbl-o { opacity: 0; animation: ucFade 0.4s ease 0.6s forwards; }
    @media (prefers-reduced-motion: reduce) {
      .uc-svg .uc-band, .uc-svg .uc-line-target, .uc-svg .uc-line-actual, .uc-svg .uc-lbl-a, .uc-svg .uc-lbl-o { animation: none; opacity: 1; stroke-dashoffset: 0; transform: none; }
    }
    .uc-svg .uc-dot { fill: #fff; stroke: #7da2f0; stroke-width: 1.5; }
    .uc-svg .uc-dot.dim { fill: rgba(255,255,255,0.85); stroke: none; }
    .uc-svg .uc-lbl-a { fill: #7da2f0; font-size: 11px; font-weight: 600; font-family: inherit; }
    .uc-svg .uc-lbl-o { fill: rgba(255,255,255,0.85); font-size: 11px; font-weight: 600; font-family: inherit; }
    .uc-toggle button { position: relative; }
    .uc-toggle button.active::before { content: '✓ '; }
    .uc-tip { position: absolute; display: none; pointer-events: none; background: var(--bg-primary); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 8px 10px; font-size: 0.74rem; color: var(--text-secondary); box-shadow: 0 6px 20px rgba(0,0,0,0.5); z-index: 5; min-width: 150px; }
    .uc-tip .uc-tip-t { color: var(--text-primary); font-weight: 600; margin-bottom: 5px; }
    .uc-tip .uc-tip-row { display: flex; align-items: center; gap: 6px; }
    .uc-tip .uc-tip-row b { color: var(--text-primary); margin-left: auto; }
    .uc-tip .uc-tip-tg { color: var(--text-muted); margin-left: 0; }
    .uc-tip i { width: 14px; height: 6px; border-radius: 2px; display: inline-block; }
    .uc-tip .uc-tip-v { display: block; margin: 1px 0 6px 20px; }
    .uc-tip .uc-tip-v:last-child { margin-bottom: 0; }
    .uc-legend { display: flex; flex-wrap: wrap; gap: 16px; margin-top: 12px; font-size: 0.74rem; color: var(--text-secondary); }
    .uc-legend .uc-lg-group { display: inline-flex; align-items: center; gap: 5px; }
    .uc-legend .uc-lg-group b { font-weight: 600; margin-right: 3px; }
    .uc-legend i { width: 15px; height: 9px; border-radius: 3px; display: inline-block; }
    .uc-legend i.dash { height: 0; width: 15px; border-top: 2px dashed; background: none !important; border-radius: 0; }
    .uc-empty { padding: 60px 16px; text-align: center; color: var(--text-muted); font-size: 0.84rem; }

    /* ─── Scrollbar ─── */
    ::-webkit-scrollbar { width: 6px; height: 6px; }
    ::-webkit-scrollbar-track { background: transparent; }
    ::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }
    ::-webkit-scrollbar-thumb:hover { background: var(--text-muted); }

    /* ─── UI polish pass (depth, signature accent, micro-interactions) ─── */
    /* topbar lifts off the content with a soft shadow */
    .topbar { box-shadow: 0 1px 0 rgba(255,255,255,0.03), 0 8px 22px rgba(0,0,0,0.28); }

    /* primary actions: signature indigo gradient + glow (overrides flat fills) */
    button.primary, .kanban-btn.primary {
      background: var(--accent-grad) !important; border-color: transparent !important; color: #fff;
      box-shadow: 0 2px 12px rgba(99,102,241,0.35); transition: filter 0.15s, box-shadow 0.15s, transform 0.1s;
    }
    button.primary:hover, .kanban-btn.primary:hover { filter: brightness(1.08); box-shadow: 0 4px 18px rgba(99,102,241,0.5); }
    button.primary:active, .kanban-btn.primary:active { transform: translateY(1px); }

    /* surfaces: subtle elevation + top highlight, tactile hover lift */
    .svc-card, .info-section, .secret-item {
      box-shadow: var(--shadow-sm), var(--surface-hi);
      transition: transform 0.15s ease, box-shadow 0.15s ease, border-color 0.15s ease;
    }
    .svc-card:hover, .info-section:hover, .secret-item:hover {
      transform: translateY(-1px); box-shadow: var(--shadow-md), var(--surface-hi); border-color: #34344d;
    }
    .kanban-card { box-shadow: var(--shadow-sm), var(--surface-hi); }

    /* modals: deep shadow + faint backdrop blur */
    #add-project-modal .modal, #import-dialog .dialog, .input-modal .im-box, .pin-modal .pin-box, #mm-detail-modal .im-box {
      box-shadow: var(--shadow-lg), var(--surface-hi);
    }
    #add-project-modal, #import-dialog, .input-modal, .pin-modal, .fe-editor { backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); }

    /* form fields: soft accent focus ring everywhere (xterm helper is offscreen) */
    input:focus, textarea:focus, select:focus { outline: none; box-shadow: var(--ring); border-color: var(--accent) !important; }

    /* connection badge → live status dot */
    .topbar .status-badge.connected, .topbar .status-badge.disconnected { display: inline-flex; align-items: center; gap: 6px; }
    .topbar .status-badge.connected::before { content: ''; width: 6px; height: 6px; border-radius: 50%; background: var(--green); animation: connPulse 2.2s ease-out infinite; }
    .topbar .status-badge.disconnected::before { content: ''; width: 6px; height: 6px; border-radius: 50%; background: var(--red); opacity: 0.8; }
    @keyframes connPulse {
      0% { box-shadow: 0 0 0 0 rgba(16,185,129,0.5); }
      70% { box-shadow: 0 0 0 6px rgba(16,185,129,0); }
      100% { box-shadow: 0 0 0 0 rgba(16,185,129,0); }
    }

    /* toast: depth + accent edge */
    .toast { box-shadow: var(--shadow-md); border-left: 3px solid var(--accent); }
    .toast.error { border-left-color: var(--red); }
    .toast.success { border-left-color: var(--green); }

    @media (prefers-reduced-motion: reduce) {
      .topbar .status-badge.connected::before { animation: none; }
      .svc-card, .info-section, .secret-item { transition: none; }
      .svc-card:hover, .info-section:hover, .secret-item:hover { transform: none; }
    }
  </style>
</head>
<body>
  <!-- ─── Login Screen ─── -->
  <div id="login-screen">
    <img class="login-logo" src="${LOGO_LG}" alt="Crundi" width="96" height="96">
    <h1>Crundi</h1>
    <p class="subtitle">Claude Code terminal in your browser</p>
    <div class="login-box">
      <p>Sign in with your Telegram account to continue</p>
      <div id="telegram-login-container"></div>
    </div>
  </div>

  <!-- ─── Import Dialog ─── -->
  <div id="import-dialog">
    <div class="dialog">
      <h2>Import existing configuration?</h2>
      <p>We found data from a previous Claude Telegram Bot installation.
         Would you like to import your projects and services?</p>
      <div class="buttons">
        <button data-action="import-skip">Skip</button>
        <button data-action="import-yes" class="primary">Import</button>
      </div>
    </div>
  </div>

  <!-- ─── Main App ─── -->
  <div id="app">
    <div class="topbar" id="topbar">
      <div class="usage-bg" id="usage-bg" title="Claude usage — loading…">
        <div class="urow" id="row-week">
          <div class="ub ub-time" id="wk-time"></div>
          <div class="ub ub-usage" id="wk-usage"></div>
          <div class="ublabel ublabel-usage" id="wk-usage-label"></div>
          <div class="ublabel ublabel-time" id="wk-time-label"></div>
        </div>
        <div class="urow" id="row-5h">
          <div class="ub ub-time" id="fh-time"></div>
          <div class="ub ub-usage" id="fh-usage"></div>
          <div class="ublabel ublabel-usage" id="fh-usage-label"></div>
          <div class="ublabel ublabel-time" id="fh-time-label"></div>
        </div>
      </div>
      <button class="hamburger" data-action="toggle-sidebar">&#9776;</button>
      <span class="logo"><img src="${LOGO_SM}" alt=""><span class="logo-text">Crundi</span></span>
      <span class="separator">/</span>
      <span class="project-name" id="current-project">No project</span>
      <span class="spacer"></span>
      <span class="status-badge usage-updated" id="usage-updated" title="Claude usage — click for history chart" style="display:none"></span>
      <span class="status-badge disconnected" id="tg-badge" title="Telegram bot" style="display:none">tg: offline</span>
      <span class="status-badge disconnected" id="conn-badge">disconnected</span>
      <div class="win-controls" id="win-controls">
        <button onclick="window.api?.minimize()" title="Minimize">&#x2013;</button>
        <button onclick="window.api?.maximize()" title="Maximize">&#x25A1;</button>
        <button class="win-close" onclick="window.api?.close()" title="Close">&#x2715;</button>
      </div>
    </div>
    <div class="main-layout">
      <div class="sidebar" id="sidebar">
        <div class="sidebar-header">Projects</div>
        <div class="sidebar-list" id="project-list"></div>
        <div class="sidebar-footer">
          <button data-action="add-project">+ Add project</button>
        </div>
      </div>
      <div class="sidebar-overlay" id="sidebar-overlay" data-action="close-sidebar"></div>
      <div class="terminal-area">
        <div class="tab-bar" id="tab-bar">
          <button class="tab-btn active" data-tab="terminal">Terminal</button>
          <button class="tab-btn" data-tab="git">Git</button>
          <button class="tab-btn" data-tab="files">Files</button>
          <button class="tab-btn" data-tab="kanban">Kanban</button>
          <button class="tab-btn" data-tab="mindmap">Mindmap</button>
          <button class="tab-btn" data-tab="services">Services</button>
          <button class="tab-btn" data-tab="terminals">Terminals</button>
          <button class="tab-btn" data-tab="browsers">Browsers</button>
          <button class="tab-btn" data-tab="info">Info</button>
          <button class="tab-btn" data-tab="secrets">Secrets <span class="secret-badge" id="secret-badge" style="display:none"></span></button>
          <button class="tab-btn" data-tab="settings">Settings</button>
        </div>
        <div class="terminal-wrap tab-panel visible" data-panel="terminal">
          <div class="terminal-container" id="terminal-container" style="position:relative;flex:1;overflow:hidden;">
            <div class="terminal-placeholder" id="terminal-placeholder">
              <div class="icon">&gt;_</div>
              <p>Select a project to open a Claude Code terminal</p>
              <p class="hint">Or add a new project from the sidebar</p>
            </div>
            <button class="term-select-toggle" data-action="term-select" title="Toggle text selection mode">Select</button>
            <button class="term-scroll-bottom" id="term-scroll-bottom" data-action="term-scroll-bottom" title="Scroll to bottom">&#8595;</button>
            <div class="term-select-overlay" id="term-select-overlay"></div>
          </div>
          <div class="term-tool-panel" id="term-tool-panel">
            <div class="term-tool-row">
              <button class="term-tool-btn" data-action="term-key" data-key="Escape">Esc</button>
              <button class="term-tool-btn" data-action="term-key" data-key="Tab">Tab</button>
              <button class="term-tool-btn wide" data-action="term-key" data-key="ctrl+c">Ctrl+C</button>
              <button class="term-tool-btn wide" data-action="term-key" data-key="ctrl+z">Ctrl+Z</button>
            </div>
            <div class="term-tool-row">
              <button class="term-tool-btn wide" data-action="term-key" data-key="ctrl+a">Ctrl+A</button>
              <button class="term-tool-btn wide" data-action="term-key" data-key="ctrl+e">Ctrl+E</button>
              <button class="term-tool-btn wide" data-action="term-key" data-key="ctrl+l">Ctrl+L</button>
              <button class="term-tool-btn wide" data-action="term-key" data-key="ctrl+d">Ctrl+D</button>
            </div>
            <div class="term-tool-row">
              <button class="term-tool-btn" data-action="term-key" data-key="ArrowUp">&#9650;</button>
              <button class="term-tool-btn" data-action="term-key" data-key="ArrowDown">&#9660;</button>
              <button class="term-tool-btn" data-action="term-key" data-key="ArrowLeft">&#9664;</button>
              <button class="term-tool-btn" data-action="term-key" data-key="ArrowRight">&#9654;</button>
              <button class="term-tool-btn" data-action="term-key" data-key="Home">Home</button>
              <button class="term-tool-btn" data-action="term-key" data-key="End">End</button>
            </div>
            <div class="term-tool-row">
              <button class="term-tool-btn wide" data-action="term-key" data-key="Enter">Enter</button>
              <button class="term-tool-btn wide" data-action="term-key" data-key="Space">Space</button>
            </div>
          </div>
          <div class="term-input-bar" id="term-input-bar">
            <button class="term-tool-toggle" id="term-tool-toggle" data-action="term-tool-toggle" title="Keyboard tools">&#9881;</button>
            <textarea id="term-input" class="term-input" rows="1" placeholder="Type here... (Ctrl+Enter or Send button)"></textarea>
            <button class="term-attach-btn" data-action="term-attach" title="Attach a file (uploads to crundi_attachments)">&#128206;</button>
            <input type="file" id="term-attach-input" style="display:none">
            <button class="term-send-btn" data-action="term-send" title="Send to terminal">Send</button>
          </div>
        </div>
        <div class="git-panel tab-panel" id="git-panel" data-panel="git"></div>
        <div class="files-panel tab-panel" id="files-panel" data-panel="files"></div>
        <div class="kanban-panel tab-panel" id="kanban-panel" data-panel="kanban"></div>
        <div class="secrets-panel tab-panel" id="secrets-panel" data-panel="secrets"></div>
        <div class="mindmap-panel tab-panel" id="mindmap-panel" data-panel="mindmap"></div>
        <div class="services-panel tab-panel" id="services-panel" data-panel="services"></div>
        <div class="services-panel tab-panel" id="terminals-panel" data-panel="terminals"></div>
        <div class="services-panel tab-panel" id="browsers-panel" data-panel="browsers"></div>
        <div class="info-panel tab-panel" id="info-panel" data-panel="info"></div>
        <div class="info-panel tab-panel" id="settings-panel" data-panel="settings"></div>
      </div>
    </div>
  </div>

  <!-- ─── File Editor Modal ─── -->
  <div class="fe-editor" id="file-editor">
    <div class="fe-header">
      <span class="fe-path" id="fe-path"></span>
      <button onclick="feClose()">Close</button>
      <button class="save" id="fe-save-btn" onclick="feSave()">Save</button>
    </div>
    <div class="fe-content" id="fe-container"></div>
  </div>

  <!-- ─── Add Project Modal ─── -->
  <div id="add-project-modal">
    <div class="modal">
      <h3>Add Project</h3>
      <!-- Single mode: just a name field -->
      <div id="modal-single-fields" style="display:none;">
        <label for="proj-name-single">Project Name</label>
        <input type="text" id="proj-name-single" placeholder="my-project" autocomplete="off">
        <div id="modal-single-hint" style="font-size:0.78rem;color:var(--text-muted);margin-top:-8px;margin-bottom:12px;"></div>
      </div>
      <!-- Multi mode: path + name fields -->
      <div id="modal-multi-fields" style="display:none;">
        <label for="proj-alias">Alias</label>
        <input type="text" id="proj-alias" placeholder="my-project" autocomplete="off">
        <label for="proj-path">Path</label>
        <input type="text" id="proj-path" placeholder="C:\\\\Projects\\\\my-project" autocomplete="off">
        <label for="proj-name">Display Name (optional)</label>
        <input type="text" id="proj-name" placeholder="My Project" autocomplete="off">
      </div>
      <div class="modal-buttons">
        <button data-action="modal-cancel">Cancel</button>
        <button data-action="modal-add" class="primary">Add</button>
      </div>
    </div>
  </div>

  <!-- ─── PIN Modal ─── -->
  <div class="pin-modal" id="pin-modal">
    <div class="pin-box">
      <h3 id="pin-title">Enter PIN</h3>
      <p id="pin-subtitle"></p>
      <input type="password" id="pin-input" inputmode="numeric" pattern="[0-9]*" maxlength="6" autocomplete="off" placeholder="••••••">
      <div class="pin-error" id="pin-error"></div>
      <div class="pin-buttons">
        <button id="pin-cancel-btn">Cancel</button>
        <button id="pin-submit-btn" class="primary">Unlock</button>
      </div>
    </div>
  </div>

  <!-- ─── Text Input Modal (replaces window.prompt) ─── -->
  <div class="input-modal" id="input-modal">
    <div class="im-box">
      <h3 id="im-title">Input</h3>
      <div class="im-label" id="im-label"></div>
      <input type="text" id="im-input" autocomplete="off">
      <textarea id="im-textarea" style="display:none"></textarea>
      <div class="im-buttons">
        <button id="im-cancel">Cancel</button>
        <button id="im-ok" class="primary">OK</button>
      </div>
    </div>
  </div>

  <!-- ─── Link Picker Modal (browse/search tasks & subtasks to link) ─── -->
  <div class="input-modal" id="link-modal">
    <div class="im-box" style="max-width:520px">
      <h3>Scope to a project — or link a task</h3>
      <div class="lm-row">
        <select id="lm-project"></select>
        <input type="text" id="lm-search" placeholder="Search tasks &amp; subtasks…" autocomplete="off" style="flex:1">
      </div>
      <div class="lm-locked" id="lm-locked" style="display:none"></div>
      <button class="lm-scope-btn" id="lm-scope-project"></button>
      <div class="lm-or">— or link a specific task —</div>
      <div class="lm-list" id="lm-list"></div>
      <div class="im-buttons"><button id="lm-cancel">Cancel</button></div>
    </div>
  </div>

  <!-- ─── Mindmap Node Detail Modal ─── -->
  <div class="input-modal" id="mm-detail-modal">
    <div class="im-box" style="max-width:520px">
      <h3 id="mmd-title">Idea</h3>
      <div id="mmd-view">
        <div class="mmd-link" id="mmd-link"></div>
        <ul class="mmd-notes" id="mmd-notes"></ul>
      </div>
      <div id="mmd-edit" style="display:none">
        <label class="im-label">Idea</label>
        <input type="text" id="mmd-text-input" autocomplete="off">
        <label class="im-label" style="margin-top:10px">Notes</label>
        <div class="mmd-notes-edit" id="mmd-notes-edit"></div>
        <button type="button" class="mmd-add-note" id="mmd-add-note">+ Add note</button>
      </div>
      <div class="mmd-actions">
        <button id="mmd-edit-btn">Edit</button>
        <button id="mmd-save-btn" class="primary" style="display:none">Save</button>
        <button id="mmd-addchild-btn">+ Child</button>
        <button id="mmd-link-btn">Link</button>
        <button id="mmd-del-btn" class="danger">Delete</button>
        <button id="mmd-close-btn">Close</button>
      </div>
    </div>
  </div>

  <!-- ─── Usage History Chart Modal ─── -->
  <div class="input-modal" id="usage-modal">
    <div class="im-box" style="max-width:680px">
      <h3>Claude usage over time</h3>
      <div class="uc-controls">
        <div class="uc-seg uc-toggle" id="uc-metric" title="Toggle each independently">
          <button data-metric="week" class="active">Weekly</button>
          <button data-metric="five">5-hour</button>
        </div>
        <div class="uc-seg" id="uc-ranges">
          <button data-range="5h">5h</button>
          <button data-range="day">Day</button>
          <button data-range="week" class="active">Week</button>
          <button data-range="month">Month</button>
        </div>
      </div>
      <div class="uc-chart-wrap"><div class="uc-chart" id="uc-chart"></div><div class="uc-tip" id="uc-tip"></div></div>
      <div class="uc-legend" id="uc-legend"></div>
      <div class="im-buttons"><button id="uc-close">Close</button></div>
    </div>
  </div>

  <!-- ─── Toast ─── -->
  <div class="toast" id="toast"></div>

  <script src="/vendor/xterm.js"><\/script>
  <script src="/vendor/addon-fit.js"><\/script>
  <script src="/vendor/codemirror.js"><\/script>
  <script>
  (function() {
    'use strict';

    // ─── State ───
    let token = localStorage.getItem('crundi_token');
    let ws = null;
    let term = null;
    let fitAddon = null;
    let currentProject = null;
    let currentTab = 'terminal';
    let projects = [];
    let terminals = []; // { project, status }
    let userTerminals = []; // { name, status, alias, command }
    let services = [];
    let reconnectTimer = null;
    let sse = null;             // EventSource (kept so we can health-check it)
    let sseReconnectTimer = null;
    let resizeTimer = null;
    let projectMode = 'multi'; // 'single' or 'multi'
    let projectsDir = '';
    let kanbanBoard = null;
    let kanbanView = 'board'; // 'board' | 'trash' | 'history'
    let kanbanDragTaskId = null;
    let secretsList = [];
    let secretRequests = [];
    const revealedSecrets = {}; // secretId → decrypted value (in-memory only)
    let pinOnSubmit = null;     // async (pin) => { ok, error }
    let mindmapNodes = [];
    const mindmapCollapsed = {}; // nodeId → true when its children are hidden
    let mindmapZoom = 1;          // current zoom scale
    let mindmapZoomInit = false;  // whether initial fit-vertical zoom was applied
    let mindmapBaseW = 0, mindmapBaseH = 0; // unscaled content size
    let mindmapSearch = '';       // active filter query
    let mmSearchSavedCollapsed = null; // collapse snapshot to restore when search clears
    let lastMmDrag = 0;           // timestamp of last node drag-end (suppress click-after-drag)
    let mmDetailId = null;        // node id open in detail modal
    let mmAnimate = false;        // play entrance animation on the next render (fresh load only)
    const mindmapHiddenProjects = new Set(); // project keys toggled off ('' = general)
    // Resolved scope from the backend (link > parent inheritance > own); '' = general.
    function mmNodeProjectKey(n) { return n.effectiveProject || ''; }
    // A node is hidden if its own project OR any ancestor's project is toggled
    // off — so hiding a project hides its whole subtree (children never orphan).
    function mmNodePassesProject(n) {
      let cur = n; const seen = new Set();
      while (cur && !seen.has(cur.id)) {
        seen.add(cur.id);
        if (mindmapHiddenProjects.has(cur.effectiveProject || '')) return false;
        cur = cur.parentId ? mindmapNodes.find(x => x.id === cur.parentId) : null;
      }
      return true;
    }

    const $ = (s) => document.querySelector(s);
    const $$ = (s) => document.querySelectorAll(s);

    // ─── Telegram Login Widget ───
    window.onTelegramAuth = async function(user) {
      try {
        const res = await apiFetch('/api/auth', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ telegramLogin: user }),
        });
        const data = await res.json();
        if (data.ok) {
          token = data.token;
          localStorage.setItem('crundi_token', token);
          showApp();
        } else {
          toast('Login failed: ' + (data.error || 'Unknown error'), 'error');
        }
      } catch (err) {
        toast('Login error: ' + err.message, 'error');
      }
    };

    function injectTelegramWidget() {
      const container = $('#telegram-login-container');
      if (!container) return;
      const botUser = ${JSON.stringify(botUsername || '')};
      if (!botUser) {
        container.innerHTML = '<p style="color:var(--red)">Bot username not configured</p>';
        return;
      }
      const script = document.createElement('script');
      script.async = true;
      script.src = 'https://telegram.org/js/telegram-widget.js?22';
      script.setAttribute('data-telegram-login', botUser);
      script.setAttribute('data-size', 'large');
      script.setAttribute('data-radius', '8');
      // Use the redirect flow (data-auth-url) instead of the JS callback
      // (data-onauth). The callback flow relies on a cross-site popup and
      // postMessage, which Microsoft Edge's tracking prevention blocks; the
      // redirect does a top-level navigation that works in every browser.
      // The server validates the auth params and bounces back with the token
      // in the URL fragment (handled by consumeRedirectToken on load).
      script.setAttribute('data-auth-url', location.origin + '/auth/telegram/callback');
      script.setAttribute('data-request-access', 'write');
      container.appendChild(script);
    }

    // ─── API ───
    function apiFetch(path, opts = {}) {
      if (token) {
        opts.headers = opts.headers || {};
        opts.headers['Authorization'] = 'Bearer ' + token;
      }
      return fetch(path, opts);
    }

    // ─── Auth Check ───
    async function checkAuth() {
      if (!token) return false;
      try {
        const res = await apiFetch('/api/status');
        return res.ok;
      } catch { return false; }
    }

    // ─── Toast ───
    function toast(msg, type = '') {
      const el = $('#toast');
      el.textContent = msg;
      el.className = 'toast visible' + (type ? ' ' + type : '');
      clearTimeout(el._timer);
      el._timer = setTimeout(() => { el.className = 'toast'; }, 3500);
    }

    // ─── Show App ───
    // ─── Claude usage meter (real, account-wide) ───
    let usageData = null;
    let usageUpdatedAt = 0; // epoch ms of the currently-shown sample
    const MS_HOUR = 3600000, MS_DAY = 24 * MS_HOUR;

    function fmtAgo(ms) {
      const s = Math.max(0, Math.floor((Date.now() - ms) / 1000));
      if (s < 60) return 'just now';
      const m = Math.floor(s / 60);
      if (m < 60) return m + 'm ago';
      const h = Math.floor(m / 60);
      if (h < 24) return h + 'h ago';
      return Math.floor(h / 24) + 'd ago';
    }
    function updateUsageBadge() {
      const el = $('#usage-updated');
      if (!el) return;
      if (!usageUpdatedAt) { el.style.display = 'none'; return; }
      el.style.display = '';
      el.textContent = '📊 ' + fmtAgo(usageUpdatedAt);
      el.title = 'Claude usage updated ' + new Date(usageUpdatedAt).toLocaleString() + ' · click for history chart';
    }

    // Elapsed fraction (0-100) of a window, i.e. progress toward its reset.
    function resetProgress(resetsAtISO, windowMs) {
      if (!resetsAtISO) return null;
      const end = new Date(resetsAtISO).getTime();
      if (isNaN(end)) return null;
      const start = end - windowMs;
      return Math.max(0, Math.min(100, (Date.now() - start) / windowMs * 100));
    }

    function fmtRemaining(resetsAtISO) {
      if (!resetsAtISO) return '—';
      let ms = new Date(resetsAtISO).getTime() - Date.now();
      if (isNaN(ms)) return '—';
      if (ms < 0) ms = 0;
      const d = Math.floor(ms / MS_DAY), h = Math.floor((ms % MS_DAY) / MS_HOUR), m = Math.floor((ms % MS_HOUR) / 60000);
      if (d > 0) return d + 'd ' + h + 'h';
      if (h > 0) return h + 'h ' + m + 'm';
      return m + 'm';
    }

    // Advance the time-to-reset bars (driven by wall-clock, not just data refresh)
    function tickResets() {
      if (!usageData || !usageData.ok) return;
      const wk = usageData.week && usageData.week.resetsAt;
      const fh = usageData.fiveHour && usageData.fiveHour.resetsAt;
      setTimeBar('wk-time', 'wk-time-label', resetProgress(wk, 7 * MS_DAY), wk);
      setTimeBar('fh-time', 'fh-time-label', resetProgress(fh, 5 * MS_HOUR), fh);
    }
    function setTimeBar(barId, labelId, pct, resetsAtISO) {
      const bar = $('#' + barId), label = $('#' + labelId);
      if (!bar) return;
      if (pct == null) { bar.style.width = '0%'; label.textContent = ''; return; }
      bar.style.width = pct + '%';
      label.textContent = '⏳ ' + fmtRemaining(resetsAtISO);
      label.title = 'resets in ' + fmtRemaining(resetsAtISO);
      placeUsageLabel(label, pct);
    }

    async function loadUsage() {
      // 1) instant: paint last stored sample right away (no network round-trip)
      try {
        const r = await apiFetch('/api/usage/latest');
        const d = await r.json();
        if (d && d.ok) renderUsage(d);
      } catch { /* ignore */ }
      // 2) live refresh (may be slow or rate-limited; SSE also pushes updates)
      try {
        const res = await apiFetch('/api/usage');
        renderUsage(await res.json());
      } catch { /* SSE will retry */ }
    }

    // Periodic check against the backend's stored sample, in addition to SSE.
    async function pollStoredUsage() {
      try {
        const r = await apiFetch('/api/usage/latest');
        const d = await r.json();
        if (d && d.ok && (Date.parse(d.fetchedAt) || 0) >= usageUpdatedAt) renderUsage(d);
      } catch { /* ignore */ }
    }

    // Position a usage label relative to its fill's leading edge.
    // ≤85%: just after the end, reading rightward. >85%: flip to right-aligned
    // just inside the end, reading leftward — so it never overflows the topbar.
    function placeUsageLabel(el, pct) {
      if (pct > 85) {
        el.style.left = 'auto';
        el.style.right = (100 - pct) + '%';
        el.style.transform = 'translateX(-6px)';
      } else {
        el.style.right = 'auto';
        el.style.left = pct + '%';
        el.style.transform = 'translateX(6px)';
      }
    }
    // translucent fill colour per level (green → amber → red)
    function ufColor(p, base) {
      if (p >= 90) return 'rgba(239,68,68,0.28)';
      if (p >= 70) return 'rgba(245,158,11,0.26)';
      return base;
    }
    function renderUsage(u) {
      const bg = $('#usage-bg');
      if (!bg) return;
      const bars = ['wk-usage', 'fh-usage', 'wk-time', 'fh-time'];
      const labels = ['wk-usage-label', 'fh-usage-label', 'wk-time-label', 'fh-time-label'];
      if (!u || !u.ok) {
        // Transient failure (e.g. rate-limit 429): keep the last-known bars
        // visible instead of blanking; only clear if we never had good data.
        if (usageData && usageData.ok) {
          bg.title = 'Claude usage (last known — refresh failed: ' + ((u && u.error) || '?') + ')';
          return;
        }
        bars.forEach(i => { const e = $('#' + i); if (e) e.style.width = '0%'; });
        labels.forEach(i => { const e = $('#' + i); if (e) e.textContent = ''; });
        bg.title = 'Claude usage unavailable: ' + ((u && u.error) || 'unknown');
        return;
      }
      usageData = u;
      usageUpdatedAt = Date.parse(u.fetchedAt) || Date.now();
      updateUsageBadge();
      const five = u.fiveHour ? u.fiveHour.utilization : 0;
      const week = u.week ? u.week.utilization : 0;
      setUsageBar('wk-usage', 'wk-usage-label', week, 'wk');
      setUsageBar('fh-usage', 'fh-usage-label', five, '5h');
      const rs = (w) => (w && w.resetsAt) ? new Date(w.resetsAt).toLocaleString() : '—';
      bg.title = 'Claude usage (real, account-wide)'
        + '\\nWeekly: ' + week + '%  (resets ' + rs(u.week) + ')'
        + '\\n5-hour: ' + five + '%  (resets ' + rs(u.fiveHour) + ')'
        + (u.subscriptionType ? '\\nPlan: ' + u.subscriptionType : '');
      tickResets(); // also fills the time bars from resets_at
    }
    const ubLast = {}; // last shown usage % per bar (to shine only on change)
    function setUsageBar(barId, labelId, pct, tag) {
      const bar = $('#' + barId), label = $('#' + labelId);
      if (!bar) return;
      bar.style.width = Math.min(100, pct) + '%';
      bar.style.background = ufColor(pct, 'rgba(99,102,241,0.22)');
      label.textContent = tag + ' ' + pct + '%';
      placeUsageLabel(label, pct);
      if (ubLast[barId] !== undefined && ubLast[barId] !== pct) {
        bar.classList.remove('shine'); void bar.offsetWidth; bar.classList.add('shine'); // restart sweep
      }
      ubLast[barId] = pct;
    }

    // ─── Usage history chart (hand-drawn SVG: smooth monotone curves with a
    // transparent over/under band — actual vs optimal pace) ───
    let usageChartRange = 'week';
    // Each metric toggles independently so both can be viewed at once or alone.
    const usageChartShow = { week: true, five: false };
    let usageChartRecords = [];
    const UC = { W: 660, H: 300, padL: 40, padR: 58, padT: 16, padB: 28 };
    // Distinct palette per metric so the two never interfere. Semantics:
    // over target = warm (warning), under target = cool (good).
    const UC_PAL = {
      week: {
        key: 'week', name: 'Weekly', short: 'Wk', aKey: 'week', oKey: 'weekOpt',
        line: '#6366f1', target: 'rgba(129,140,248,0.8)',
        over: 'rgba(239,68,68,0.42)', under: 'rgba(16,185,129,0.4)',
        overGrad: ['rgba(239,68,68,0.5)', 'rgba(239,68,68,0.08)'],
        underGrad: ['rgba(16,185,129,0.46)', 'rgba(16,185,129,0.06)'],
        overName: 'over (red)', underName: 'under (green)',
      },
      five: {
        key: 'five', name: '5-hour', short: '5h', aKey: 'five', oKey: 'fiveOpt',
        line: '#a78bfa', target: 'rgba(196,181,253,0.8)',
        over: 'rgba(245,158,11,0.46)', under: 'rgba(56,189,248,0.42)',
        overGrad: ['rgba(245,158,11,0.52)', 'rgba(245,158,11,0.08)'],
        underGrad: ['rgba(56,189,248,0.46)', 'rgba(56,189,248,0.06)'],
        overName: 'over (amber)', underName: 'under (blue)',
      },
    };

    function openUsageModal() {
      $('#usage-modal').classList.add('visible');
      loadUsageChart(usageChartRange);
    }
    function closeUsageModal() { $('#usage-modal').classList.remove('visible'); }
    async function loadUsageChart(range) {
      usageChartRange = range;
      $$('#uc-ranges button').forEach(b => b.classList.toggle('active', b.dataset.range === range));
      $$('#uc-metric button').forEach(b => b.classList.toggle('active', !!usageChartShow[b.dataset.metric]));
      $('#uc-chart').innerHTML = '<div class="uc-empty">Loading…</div>';
      try {
        const r = await apiFetch('/api/usage/history?range=' + encodeURIComponent(range));
        const d = await r.json();
        usageChartRecords = (d && d.records) || [];
      } catch { usageChartRecords = []; }
      renderUsageChart();
    }

    // Fritsch–Carlson monotone cubic interpolation → smooth, non-overshooting.
    function monotoneFn(xs, ys) {
      const n = xs.length;
      if (n < 2) return () => ys[0];
      const dx = [], slope = [];
      for (let i = 0; i < n - 1; i++) { dx[i] = xs[i + 1] - xs[i]; slope[i] = (ys[i + 1] - ys[i]) / dx[i]; }
      const m = new Array(n);
      m[0] = slope[0]; m[n - 1] = slope[n - 2];
      for (let i = 1; i < n - 1; i++) {
        if (slope[i - 1] * slope[i] <= 0) m[i] = 0;
        else { const w1 = 2 * dx[i] + dx[i - 1], w2 = dx[i] + 2 * dx[i - 1]; m[i] = (w1 + w2) / (w1 / slope[i - 1] + w2 / slope[i]); }
      }
      return (x) => {
        if (x <= xs[0]) return ys[0];
        if (x >= xs[n - 1]) return ys[n - 1];
        let i = 0; while (x > xs[i + 1]) i++;
        const h = dx[i], t = (x - xs[i]) / h, t2 = t * t, t3 = t2 * t;
        return (2 * t3 - 3 * t2 + 1) * ys[i] + (t3 - 2 * t2 + t) * h * m[i]
          + (-2 * t3 + 3 * t2) * ys[i + 1] + (t3 - t2) * h * m[i + 1];
      };
    }

    // Build the band + line + dot layers for one metric. Returns null when there
    // isn't enough data to draw a curve (the caller shows a friendly empty state).
    function buildMetricLayers(pal, t0, span, x, y) {
      const aKey = pal.aKey, oKey = pal.oKey;
      const recs = Array.isArray(usageChartRecords) ? usageChartRecords : [];
      // strictly-increasing timestamps (drop dupes) so the interpolator is safe
      const pts = recs
        .filter(r => r && r.at >= t0 && r[aKey] != null && r[oKey] != null)
        .filter((p, i, a) => i === 0 || p.at !== a[i - 1].at);
      if (pts.length < 2) return null; // need at least two samples for a curve
      const xs = pts.map(p => p.at);
      const fA = monotoneFn(xs, pts.map(p => p[aKey]));
      const fO = monotoneFn(xs, pts.map(p => p[oKey]));
      const x0p = x(xs[0]), x1p = x(xs[xs.length - 1]);
      const steps = Math.max(2, Math.min(260, Math.round((x1p - x0p) / 2)));
      const S = [];
      for (let i = 0; i <= steps; i++) {
        const t = xs[0] + (xs[xs.length - 1] - xs[0]) * (i / steps);
        const a = Math.max(0, Math.min(100, fA(t))), o = Math.max(0, Math.min(100, fO(t)));
        S.push({ px: x(t), ay: y(a), oy: y(o), d: a - o });
      }
      let overP = '', underP = '';
      const quad = (x0, oy0, ay0, x1, ay1, oy1) => 'M' + x0 + ',' + oy0 + ' L' + x0 + ',' + ay0 + ' L' + x1 + ',' + ay1 + ' L' + x1 + ',' + oy1 + ' Z ';
      const tri = (ax, ay, bx, by, cx, cy) => 'M' + ax + ',' + ay + ' L' + bx + ',' + by + ' L' + cx + ',' + cy + ' Z ';
      for (let i = 0; i < S.length - 1; i++) {
        const p = S[i], q = S[i + 1];
        if (p.d === 0 && q.d === 0) continue;
        if ((p.d >= 0 && q.d >= 0) || (p.d <= 0 && q.d <= 0)) {
          const seg = quad(p.px, p.oy, p.ay, q.px, q.ay, q.oy);
          if ((p.d + q.d) > 0) overP += seg; else underP += seg;
        } else {
          const fr = p.d / (p.d - q.d);
          const cx = p.px + (q.px - p.px) * fr, cy = p.ay + (q.ay - p.ay) * fr;
          const left = tri(p.px, p.oy, p.px, p.ay, cx, cy);
          const right = tri(cx, cy, q.px, q.ay, q.px, q.oy);
          if (p.d > 0) { overP += left; underP += right; } else { underP += left; overP += right; }
        }
      }
      const linePath = (key) => 'M' + S.map(s => s.px + ',' + s[key]).join(' L');
      // No per-point dots (they clutter); exact values come from the hover tooltip.
      const last = pts[pts.length - 1];
      const label = '<text x="' + (x(last.at) + 8) + '" y="' + (y(last[aKey]) + 4) + '" class="uc-lbl-a" style="fill:' + pal.line + '">' + pal.short + '</text>';
      const band = '<path d="' + underP + '" fill="url(#uc-g-' + pal.key + '-under)" class="uc-band"/>'
        + '<path d="' + overP + '" fill="url(#uc-g-' + pal.key + '-over)" class="uc-band"/>';
      const lines = '<path d="' + linePath('oy') + '" fill="none" stroke="' + pal.target + '" stroke-width="1.8" stroke-dasharray="6 5" stroke-linecap="round" class="uc-line-target"/>'
        + '<path d="' + linePath('ay') + '" pathLength="1" fill="none" stroke="' + pal.line + '" stroke-width="2.8" stroke-linejoin="round" stroke-linecap="round" class="uc-line-actual"/>';
      return { band, lines, dotsLabel: label };
    }

    function renderUsageChart() {
      const host = $('#uc-chart'); if (!host) return;
      const { W, H, padL, padR, padT, padB } = UC;
      const spans = { '5h': 5 * MS_HOUR, day: MS_DAY, week: 7 * MS_DAY, month: 30 * MS_DAY };
      const span = spans[usageChartRange] || spans.week;
      // For the 5h / week ranges, frame the CURRENT window (start → reset) using
      // the reset time of the latest sample, rather than a trailing window — so
      // you see the whole live window with the remaining time to reset.
      const recsAll = Array.isArray(usageChartRecords) ? usageChartRecords : [];
      const lastRec = recsAll[recsAll.length - 1];
      let t1 = Date.now();
      if (usageChartRange === '5h' && lastRec && lastRec.fiveReset) t1 = new Date(lastRec.fiveReset).getTime();
      else if (usageChartRange === 'week' && lastRec && lastRec.weekReset) t1 = new Date(lastRec.weekReset).getTime();
      const t0 = t1 - span;
      const x = (t) => padL + (W - padL - padR) * Math.max(0, Math.min(1, (t - t0) / span));
      const y = (v) => padT + (H - padT - padB) * (1 - Math.max(0, Math.min(100, v)) / 100);
      const now = Date.now();
      const nowMark = (now > t0 && now < t1)
        ? '<line x1="' + x(now) + '" y1="' + padT + '" x2="' + x(now) + '" y2="' + (H - padB) + '" stroke="rgba(255,255,255,0.32)" stroke-dasharray="2 3"/>'
          + '<text x="' + x(now) + '" y="' + (padT + 9) + '" text-anchor="middle" class="uc-now">now</text>'
        : '';

      $$('#uc-metric button').forEach(b => b.classList.toggle('active', !!usageChartShow[b.dataset.metric]));

      let grid = '';
      for (let v = 0; v <= 100; v += 25) {
        const yy = y(v);
        grid += '<line x1="' + padL + '" y1="' + yy + '" x2="' + (W - padR) + '" y2="' + yy + '" stroke="rgba(255,255,255,0.08)"/>';
        grid += '<text x="' + (padL - 6) + '" y="' + (yy + 3) + '" text-anchor="end" class="uc-axis">' + v + '%</text>';
      }
      const fmtT = (t) => { const dt = new Date(t); return span <= MS_DAY ? dt.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : (dt.getMonth() + 1) + '/' + dt.getDate(); };
      let xlabels = '';
      for (let i = 0; i <= 4; i++) { const t = t0 + span * i / 4; xlabels += '<text x="' + x(t) + '" y="' + (H - 8) + '" text-anchor="middle" class="uc-axis">' + fmtT(t) + '</text>'; }

      const enabled = ['week', 'five'].filter(m => usageChartShow[m]);
      const lg = $('#uc-legend');
      if (!enabled.length) {
        host.innerHTML = '<div class="uc-empty">Enable “Weekly” or “5-hour” above to see the chart.</div>';
        if (lg) lg.innerHTML = '';
        return;
      }

      // bands underneath, lines/dots on top — so the two metrics don't bury each other
      let bands = '', lines = '', dotsLabels = '', legend = '';
      const activePals = [];
      for (const m of enabled) {
        const pal = UC_PAL[m];
        const layers = buildMetricLayers(pal, t0, span, x, y);
        if (!layers) continue;
        bands += layers.band; lines += layers.lines; dotsLabels += layers.dotsLabel;
        activePals.push(pal);
        legend += '<span class="uc-lg-group"><b style="color:' + pal.line + '">' + pal.name + '</b>'
          + '<i style="background:' + pal.line + '"></i>actual'
          + '<i class="dash" style="border-color:' + pal.target + '"></i>target'
          + '<i style="background:' + pal.under + '"></i>' + pal.underName
          + '<i style="background:' + pal.over + '"></i>' + pal.overName + '</span>';
      }
      if (!activePals.length) {
        host.innerHTML = '<div class="uc-empty">No usage recorded in this range yet — the chart fills in as samples are collected.</div>';
        if (lg) lg.innerHTML = '';
        return;
      }

      const grad = (id, stops) => '<linearGradient id="' + id + '" x1="0" y1="' + padT + '" x2="0" y2="' + (H - padB)
        + '" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="' + stops[0] + '"/><stop offset="1" stop-color="' + stops[1] + '"/></linearGradient>';
      let defs = '';
      for (const pal of activePals) {
        defs += grad('uc-g-' + pal.key + '-under', pal.underGrad) + grad('uc-g-' + pal.key + '-over', pal.overGrad);
      }
      host.innerHTML =
        '<svg viewBox="0 0 ' + W + ' ' + H + '" class="uc-svg" preserveAspectRatio="none">'
        + '<defs>' + defs + '</defs>'
        + '<g>' + grid + xlabels + '</g>' + bands + lines + dotsLabels + nowMark
        + '<line id="uc-guide" x1="0" y1="' + padT + '" x2="0" y2="' + (H - padB) + '" stroke="rgba(255,255,255,0.25)" stroke-dasharray="3 3" style="display:none"/>'
        + '</svg>';
      if (lg) lg.innerHTML = legend;

      bindUsageHover(host, activePals, x, t0, span);
    }

    function bindUsageHover(host, pals, x, t0, span) {
      const svg = host.querySelector('.uc-svg');
      const guide = host.querySelector('#uc-guide');
      const tip = $('#uc-tip');
      if (!svg) return;
      const inRange = usageChartRecords.filter(r => r.at >= t0);
      const fmtFull = (t) => { const dt = new Date(t); return span <= MS_DAY ? dt.toLocaleString([], { hour: '2-digit', minute: '2-digit' }) : dt.toLocaleDateString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' }); };
      const onMove = (ev) => {
        const rect = svg.getBoundingClientRect();
        const relX = (ev.clientX - rect.left) / rect.width * UC.W;
        let best = null, bestDx = Infinity;
        for (const p of inRange) { const dxp = Math.abs(x(p.at) - relX); if (dxp < bestDx) { bestDx = dxp; best = p; } }
        if (!best) return;
        const gx = x(best.at);
        guide.setAttribute('x1', gx); guide.setAttribute('x2', gx); guide.style.display = '';
        let rows = '<div class="uc-tip-t">' + fmtFull(best.at) + '</div>';
        for (const pal of pals) {
          const a = best[pal.aKey], o = best[pal.oKey];
          if (a == null || o == null) continue;
          const over = a - o;
          const verdict = over > 0 ? '<span style="color:#f87171">▲ ' + over + '% over</span>' : (over < 0 ? '<span style="color:#4ade80">▼ ' + (-over) + '% under</span>' : 'on pace');
          rows += '<div class="uc-tip-row"><i style="background:' + pal.line + '"></i><span>' + pal.name + '</span><b>' + a + '%</b> <span class="uc-tip-tg">/ ' + o + '%</span></div>'
            + '<div class="uc-tip-v">' + verdict + '</div>';
        }
        tip.innerHTML = rows;
        tip.style.display = 'block';
        const px = gx / UC.W * host.clientWidth;
        tip.style.left = Math.min(host.clientWidth - tip.offsetWidth - 6, Math.max(6, px + 10)) + 'px';
        tip.style.top = '10px';
      };
      const onLeave = () => { guide.style.display = 'none'; tip.style.display = 'none'; };
      svg.addEventListener('mousemove', onMove);
      svg.addEventListener('mouseleave', onLeave);
      svg.addEventListener('touchstart', (e) => { if (e.touches[0]) onMove(e.touches[0]); }, { passive: true });
      svg.addEventListener('touchmove', (e) => { if (e.touches[0]) onMove(e.touches[0]); }, { passive: true });
    }

    function setupUsageModal() {
      const updated = $('#usage-updated'), bg = $('#usage-bg');
      if (updated) updated.addEventListener('click', openUsageModal);
      if (bg) bg.addEventListener('click', openUsageModal);
      $('#uc-close').addEventListener('click', closeUsageModal);
      $('#usage-modal').addEventListener('click', (e) => { if (e.target.id === 'usage-modal') closeUsageModal(); });
      $('#uc-ranges').addEventListener('click', (e) => { const b = e.target.closest('button[data-range]'); if (b) loadUsageChart(b.dataset.range); });
      $('#uc-metric').addEventListener('click', (e) => {
        const b = e.target.closest('button[data-metric]');
        if (!b) return;
        usageChartShow[b.dataset.metric] = !usageChartShow[b.dataset.metric]; // independent on/off
        renderUsageChart();
      });
    }

    function showApp() {
      $('#login-screen').style.display = 'none';
      $('#app').classList.add('visible');
      connectWS();
      loadProjectConfig();
      loadProjects();
      initTerminal();
      loadUsage();
      // Electron: enable drag region and window controls
      if (window.api) {
        document.querySelector('.topbar').style.webkitAppRegion = 'drag';
        $('#win-controls').style.display = 'flex';
        $('#tg-badge').style.display = '';
        // Poll telegram status via server
        async function updateTgBadge() {
          try {
            const r = await apiFetch('/api/status');
            const d = await r.json();
            const el = $('#tg-badge');
            if (d.botConnected) {
              el.className = 'status-badge connected';
              el.textContent = 'tg: @' + (d.botUsername || 'bot');
            } else {
              el.className = 'status-badge disconnected';
              el.textContent = 'tg: offline';
            }
          } catch {}
        }
        updateTgBadge();
        setInterval(updateTgBadge, 15000);
      }
    }

    // ─── Project Config (mode) ───
    async function loadProjectConfig() {
      try {
        const res = await apiFetch('/api/projects/config');
        const data = await res.json();
        projectMode = data.mode || 'multi';
        projectsDir = data.projectsDir || '';
      } catch { /* ignore, default to multi */ }
    }

    // ─── Projects ───
    async function loadProjects() {
      try {
        const res = await apiFetch('/api/projects');
        const data = await res.json();
        projects = data.projects || [];
        renderProjects();
      } catch (err) {
        toast('Failed to load projects: ' + err.message, 'error');
      }
    }

    function renderProjects() {
      const list = $('#project-list');
      list.innerHTML = '';
      for (const p of projects) {
        const hasTerminal = terminals.some(t => t.project === p.alias);
        const isActive = currentProject === p.alias;
        const item = document.createElement('div');
        item.className = 'sidebar-item' + (isActive ? ' active' : '') + (hasTerminal ? ' has-terminal' : '');
        item.dataset.project = p.alias;
        // Project removal is only offered in multi mode (manually-added
        // projects). In single mode projects are auto-discovered folders and
        // would just reappear, so we don't show a remove control.
        const canRemove = projectMode === 'multi';
        // Heartbeat for projects with registered services: alive (green pulse)
        // if any are running, dim if all stopped, absent if none registered.
        const svcs = services.filter(s => String(s.alias || '').toLowerCase() === p.alias.toLowerCase());
        const runningCount = svcs.filter(s => s.status === 'running').length;
        const heart = svcs.length
          ? '<span class="svc-heart' + (runningCount ? ' live' : '') + '" title="' + (runningCount ? runningCount + ' of ' + svcs.length + ' service(s) running' : svcs.length + ' service(s), all stopped') + '">'
            + '<svg class="svc-ecg" viewBox="0 0 26 12" width="20" height="11" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M1 6 H7 L9 2.5 L12 10 L14.5 4 L16 6 H25"/></svg>'
            + '</span>'
          : '';
        item.innerHTML = '<span class="dot"></span>'
          + '<span class="name">' + escHtml(p.name || p.alias) + '</span>'
          + heart
          + (hasTerminal ? '<button class="close-btn" data-action="close-terminal" data-project="' + escHtml(p.alias) + '" title="Close terminal">&times;</button>' : '')
          + (canRemove ? '<button class="remove-btn" data-action="remove-project" data-project="' + escHtml(p.alias) + '" data-name="' + escHtml(p.name || p.alias) + '" title="Remove project (keeps files)">&#128465;</button>' : '');
        item.addEventListener('click', (e) => {
          const act = e.target.dataset.action;
          if (act === 'close-terminal' || act === 'remove-project') return;
          selectProject(p.alias);
        });
        list.appendChild(item);
      }
    }

    async function selectProject(alias) {
      currentProject = alias;
      filesCurrentDir = '';
      clearInterval(gitRefreshTimer);
      gitRefreshTimer = null;
      const p = projects.find(x => x.alias === alias);
      $('#current-project').textContent = p ? (p.name || p.alias) : alias;
      renderProjects();
      closeSidebar();

      // Subscribe WebSocket to this project
      if (ws && ws.readyState === 1) {
        ws.send(JSON.stringify({ type: 'subscribe', project: alias }));
      }

      // Show tabs
      $('#tab-bar').classList.add('visible');

      // Show launch buttons or terminal depending on whether Claude is running
      const hasTerminal = terminals.some(t => t.project === alias);
      updateTerminalPlaceholder(hasTerminal);

      switchTab('terminal');
      if (hasTerminal && term) {
        term.clear();
        fitTerminal();
        term.focus();
      }
    }

    function updateTerminalPlaceholder(hasTerminal) {
      const ph = $('#terminal-placeholder');
      if (!ph) return;
      if (hasTerminal) {
        ph.style.display = 'none';
      } else {
        ph.style.display = '';
        ph.innerHTML = '<div class="icon">&gt;_</div>'
          + '<p style="font-size:1rem;margin:0 0 18px;">Launch Claude</p>'
          + '<div style="display:flex;flex-direction:column;gap:10px;align-items:center;">'
          + '<button data-action="launch-claude" data-mode="normal" style="padding:10px 28px;border-radius:var(--radius-sm);border:1px solid var(--accent);background:var(--accent);color:#fff;cursor:pointer;font-size:14px;min-width:200px;">Normal Mode</button>'
          + '<button data-action="launch-claude" data-mode="skip" style="padding:10px 28px;border-radius:var(--radius-sm);border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);cursor:pointer;font-size:14px;min-width:200px;">Skip Permissions Mode</button>'
          + '</div>';
      }
    }

    async function launchClaude(mode) {
      if (!currentProject) return;
      const skipPerms = mode === 'skip';
      try {
        const r = await apiFetch('/api/terminals/' + encodeURIComponent(currentProject) + '/create', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ skipPermissions: skipPerms }),
        });
        const data = await r.json();
        if (!data.ok) { showToast(data.error || 'Failed to launch', 'error'); return; }
        updateTerminalPlaceholder(true);
        // Re-subscribe WS so terminal output starts flowing immediately
        if (ws && ws.readyState === 1) {
          ws.send(JSON.stringify({ type: 'subscribe', project: currentProject }));
        }
        if (term) {
          term.clear();
          setTimeout(() => { fitTerminal(); term.focus(); }, 100);
        }
      } catch (err) {
        showToast('Failed to launch Claude: ' + err.message, 'error');
      }
    }

    async function closeTerminal(alias) {
      try {
        await apiFetch('/api/terminals/' + encodeURIComponent(alias) + '/close', { method: 'POST' });
        if (currentProject === alias) {
          updateTerminalPlaceholder(false);
          if (term) term.clear();
        }
        terminals = terminals.filter(t => t.project !== alias);
        renderProjects();
      } catch (err) {
        toast('Failed to close terminal: ' + err.message, 'error');
      }
    }

    // Remove a project reference (files on disk are kept). Stops + deletes its
    // services and closes its terminal server-side.
    async function removeProject(alias, name) {
      if (!confirm('Remove "' + (name || alias) + '" from Crundi?\\n\\nThis only removes the project reference and its services — your files on disk are kept.')) return;
      try {
        const res = await apiFetch('/api/projects/' + encodeURIComponent(alias), { method: 'DELETE' });
        const data = await res.json();
        if (!data.ok) { toast('Failed to remove project: ' + (data.error || 'Unknown error'), 'error'); return; }
        terminals = terminals.filter(t => t.project !== alias);
        if (currentProject === alias) {
          currentProject = null;
          $('#current-project').textContent = 'No project';
          updateTerminalPlaceholder(false);
          if (term) term.clear();
        }
        await loadProjects();
        toast('Removed "' + (name || alias) + '"' + (data.servicesRemoved ? ' and ' + data.servicesRemoved + ' service(s)' : ''), 'success');
      } catch (err) {
        toast('Failed to remove project: ' + err.message, 'error');
      }
    }

    // ─── Terminal ───
    function initTerminal() {
      if (term) return;
      const container = $('#terminal-container');
      term = new Terminal({
        cursorBlink: true,
        fontSize: 14,
        fontFamily: '"Cascadia Code", "Fira Code", "JetBrains Mono", "Consolas", monospace',
        theme: {
          background: '#0a0a0f',
          foreground: '#e8e8f0',
          cursor: '#6366f1',
          cursorAccent: '#0a0a0f',
          selectionBackground: 'rgba(99, 102, 241, 0.3)',
          black: '#1a1a28',
          brightBlack: '#5a5a78',
          red: '#ef4444',
          brightRed: '#f87171',
          green: '#10b981',
          brightGreen: '#34d399',
          yellow: '#f59e0b',
          brightYellow: '#fbbf24',
          blue: '#6366f1',
          brightBlue: '#818cf8',
          magenta: '#a855f7',
          brightMagenta: '#c084fc',
          cyan: '#06b6d4',
          brightCyan: '#22d3ee',
          white: '#e8e8f0',
          brightWhite: '#ffffff',
        },
        allowProposedApi: true,
        scrollback: 10000,
        convertEol: true,
      });

      // Auto-copy selection to clipboard
      term.onSelectionChange(() => {
        const sel = term.getSelection();
        if (sel) {
          navigator.clipboard.writeText(sel).catch(() => {});
        }
      });

      fitAddon = new FitAddon.FitAddon();
      term.loadAddon(fitAddon);
      term.open(container);
      fitTerminal();

      term.onData((data) => {
        if (ws && ws.readyState === 1 && currentProject) {
          ws.send(JSON.stringify({ type: 'input', project: currentProject, data }));
        }
      });

      // Ctrl/Cmd+V into xterm: only intercept IMAGES (upload + paste the saved
      // path). TEXT is left to xterm's own paste handler — sending it here too
      // is what double-pasted text. Return true so xterm always pastes text once.
      term.attachCustomKeyEventHandler((e) => {
        if (e.type === 'keydown' && e.key === 'v' && (e.ctrlKey || e.metaKey) && currentProject) {
          (async () => {
            try {
              if (!navigator.clipboard || !navigator.clipboard.read) return;
              const items = await navigator.clipboard.read();
              for (const item of items) {
                const imageType = item.types.find(t => t.startsWith('image/'));
                if (!imageType) continue;
                const blob = await item.getType(imageType);
                const b64 = arrayBufferToBase64(await blob.arrayBuffer());
                const r = await apiFetch('/api/clipboard/paste-image', {
                  method: 'POST', headers: { 'Content-Type': 'application/json' },
                  body: JSON.stringify({ project: currentProject, data: b64 }),
                });
                const data = await r.json();
                if (data.ok && data.path && ws && ws.readyState === 1) {
                  ws.send(JSON.stringify({ type: 'input', project: currentProject, data: data.path }));
                  toast('Screenshot saved: ' + data.name);
                }
                return;
              }
            } catch { /* no clipboard image / permission denied */ }
          })();
          // Return false so xterm does NOT consume/preventDefault the key — the
          // browser then fires its native paste, which delivers TEXT exactly once.
          // (We only sent text ourselves before, which is what doubled it.)
          return false;
        }
        return true;
      });

      // Clipboard paste — only intercept images, never block text paste
      const termInputEl = document.getElementById('term-input');
      termInputEl.addEventListener('paste', async (e) => {
        if (!currentProject) return;
        // Find a pasted image in items or files (mobile browsers vary). Text
        // paste falls through untouched so it inserts normally.
        let imgFile = null;
        const items = e.clipboardData?.items || [];
        for (const item of items) {
          if (item.type && item.type.startsWith('image/')) { imgFile = item.getAsFile(); break; }
        }
        if (!imgFile && e.clipboardData?.files?.length) {
          for (const f of e.clipboardData.files) { if (f.type && f.type.startsWith('image/')) { imgFile = f; break; } }
        }
        if (imgFile) {
          e.preventDefault();
          await uploadAttachment(imgFile); // chunk-safe base64, cursor-aware insert, → crundi_attachments
          return;
        }
        // In Electron: check for copied file paths when no text is being pasted
        if (!e.clipboardData?.getData('text/plain') && window.api) {
          e.preventDefault();
          try {
            const filePaths = await window.api.getClipboardFilePaths();
            if (filePaths?.length) { insertIntoTermInput(filePaths.join(' ')); return; }
            const image = await window.api.getClipboardImage();
            if (image) { insertIntoTermInput(image.tmpPath); toast('Screenshot from clipboard: ' + image.name); }
          } catch { /* ignore */ }
        }
      });

      // Drag and drop files onto terminal — insert file path into input
      const termWrap = container.parentElement;
      termWrap.addEventListener('dragover', (e) => {
        if (!currentProject) return;
        e.preventDefault();
        e.dataTransfer.dropEffect = 'copy';
        termWrap.style.outline = '2px solid var(--accent)';
        termWrap.style.outlineOffset = '-2px';
      });
      termWrap.addEventListener('dragleave', () => {
        termWrap.style.outline = '';
        termWrap.style.outlineOffset = '';
      });
      termWrap.addEventListener('drop', async (e) => {
        e.preventDefault();
        termWrap.style.outline = '';
        termWrap.style.outlineOffset = '';
        if (!currentProject) return;
        const files = e.dataTransfer?.files;
        if (files && files.length > 0) {
          const paths = [];
          for (const file of files) {
            if (file.type && file.type.startsWith('image/')) {
              await uploadAttachment(file); // uploads to crundi_attachments + inserts path (chunk-safe)
            } else {
              const resolved = window.api?.getPathForFile?.(file);
              paths.push(resolved || file.path || file.name);
            }
          }
          if (paths.length) {
            insertIntoTermInput(paths.join(' '));
            toast(paths.length === 1 ? 'File added' : paths.length + ' files added');
          }
          return;
        }
        // Text drop
        const text = e.dataTransfer?.getData('text/plain');
        if (text) insertIntoTermInput(text);
      });

      // Touch scroll interceptor for mobile
      let touchStartY = 0;
      let touchStartX = 0;
      let touchAccum = 0;
      let touchScrolling = false;
      const scrollThreshold = 12;
      const scrollStartThreshold = 10; // min vertical px before engaging scroll
      container.addEventListener('touchstart', (e) => {
        if (e.touches.length === 1) {
          touchStartY = e.touches[0].clientY;
          touchStartX = e.touches[0].clientX;
          touchAccum = 0;
          touchScrolling = false;
        }
      }, { passive: true });
      container.addEventListener('touchmove', (e) => {
        if (e.touches.length !== 1 || !term) return;
        const dy = touchStartY - e.touches[0].clientY;
        const dx = Math.abs(e.touches[0].clientX - touchStartX);
        // Only engage scroll mode for clearly vertical swipes
        if (!touchScrolling) {
          if (Math.abs(dy) > scrollStartThreshold && Math.abs(dy) > dx) {
            touchScrolling = true;
          } else { return; }
        }
        touchAccum += dy;
        touchStartY = e.touches[0].clientY;
        const lines = Math.trunc(touchAccum / scrollThreshold);
        if (lines !== 0) {
          term.scrollLines(lines);
          touchAccum -= lines * scrollThreshold;
        }
        e.preventDefault();
      }, { passive: false });

      // Scroll-to-bottom floating button
      const scrollBtn = document.getElementById('term-scroll-bottom');
      function updateScrollBtn() {
        if (!term || !scrollBtn) return;
        const buf = term.buffer.active;
        const atBottom = buf.viewportY >= buf.baseY;
        scrollBtn.classList.toggle('visible', !atBottom);
      }
      term.onScroll(updateScrollBtn);
      term.onWriteParsed(updateScrollBtn);

      window.addEventListener('resize', () => {
        clearTimeout(resizeTimer);
        resizeTimer = setTimeout(fitTerminal, 100);
      });
    }

    function termToggleTools() {
      const panel = document.getElementById('term-tool-panel');
      const btn = document.getElementById('term-tool-toggle');
      if (!panel) return;
      panel.classList.toggle('visible');
      if (btn) btn.classList.toggle('active', panel.classList.contains('visible'));
      setTimeout(fitTerminal, 50);
    }

    function termSendKey(key) {
      if (!ws || ws.readyState !== 1 || !currentProject) return;
      const keyMap = {
        'Escape': '\\x1b',
        'Tab': '\\t',
        'ArrowUp': '\\x1b[A',
        'ArrowDown': '\\x1b[B',
        'ArrowRight': '\\x1b[C',
        'ArrowLeft': '\\x1b[D',
        'Home': '\\x1b[H',
        'End': '\\x1b[F',
        'ctrl+c': '\\x03',
        'ctrl+z': '\\x1a',
        'ctrl+a': '\\x01',
        'ctrl+e': '\\x05',
        'ctrl+l': '\\x0c',
        'ctrl+d': '\\x04',
        'Enter': '\\r',
        'Space': ' ',
      };
      const seq = keyMap[key];
      if (seq) ws.send(JSON.stringify({ type: 'input', project: currentProject, data: seq }));
    }

    function termSendInput() {
      const ta = document.getElementById('term-input');
      if (!ta || !ws || ws.readyState !== 1 || !currentProject) return;
      const text = ta.value;
      if (!text) return;
      ws.send(JSON.stringify({ type: 'input', project: currentProject, data: text + '\\r' }));
      ta.value = '';
      ta.style.height = 'auto';
      if (term) term.scrollToBottom();
    }

    // Insert text at the caret in the terminal input (or append if not focused),
    // padding with spaces so a path doesn't fuse onto adjacent words.
    function insertIntoTermInput(text) {
      const ta = document.getElementById('term-input');
      if (!ta) return;
      const focused = document.activeElement === ta && typeof ta.selectionStart === 'number';
      const s = focused ? ta.selectionStart : ta.value.length;
      const e = focused ? ta.selectionEnd : ta.value.length;
      const before = ta.value.slice(0, s), after = ta.value.slice(e);
      const lead = (before && !/\\s$/.test(before)) ? ' ' : '';
      const trail = (!after || !/^\\s/.test(after)) ? ' ' : '';
      const ins = lead + text + trail;
      ta.value = before + ins + after;
      const pos = s + ins.length;
      ta.focus();
      try { ta.setSelectionRange(pos, pos); } catch { /* ignore */ }
      ta.style.height = 'auto'; ta.style.height = Math.min(ta.scrollHeight, 120) + 'px';
    }

    // Attach button → pick a file, upload to crundi_attachments, insert its path.
    async function termAttachFile() {
      if (!currentProject) { toast('Select a project first', 'error'); return; }
      const input = document.getElementById('term-attach-input');
      if (input) input.click();
    }
    function arrayBufferToBase64(buf) {
      // chunked to stay clear of the call-stack/arg limits big images hit
      const bytes = new Uint8Array(buf); let bin = ''; const CH = 0x8000;
      for (let i = 0; i < bytes.length; i += CH) bin += String.fromCharCode.apply(null, bytes.subarray(i, i + CH));
      return btoa(bin);
    }
    async function uploadAttachment(file) {
      if (!file || !currentProject) return;
      const btn = document.querySelector('.term-attach-btn');
      if (btn) btn.classList.add('busy');
      try {
        const name = file.name || ('image.' + ((file.type || 'image/png').split('/')[1] || 'png'));
        const b64 = arrayBufferToBase64(await file.arrayBuffer());
        const r = await apiFetch('/api/attachments/upload', {
          method: 'POST', headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ project: currentProject, name, data: b64 }),
        });
        const d = await r.json();
        if (d.ok && d.path) { insertIntoTermInput(d.path); toast('Attached: ' + d.name); }
        else toast('Upload failed: ' + (d.error || '?'), 'error');
      } catch (err) { toast('Upload failed: ' + err.message, 'error'); }
      finally { if (btn) btn.classList.remove('busy'); }
    }

    // Auto-grow textarea + Ctrl+Enter to send
    document.addEventListener('DOMContentLoaded', () => {
      const ta = document.getElementById('term-input');
      if (!ta) return;
      ta.addEventListener('input', () => {
        ta.style.height = 'auto';
        ta.style.height = Math.min(ta.scrollHeight, 120) + 'px';
      });
      ta.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
          e.preventDefault();
          termSendInput();
        }
      });
      const attachInput = document.getElementById('term-attach-input');
      if (attachInput) attachInput.addEventListener('change', (e) => {
        const file = e.target.files && e.target.files[0];
        if (file) uploadAttachment(file);
        e.target.value = ''; // allow re-selecting the same file
      });
    });

    function fitTerminal() {
      if (!fitAddon || !term) return;
      try {
        fitAddon.fit();
        if (ws && ws.readyState === 1 && currentProject) {
          ws.send(JSON.stringify({ type: 'resize', project: currentProject, cols: term.cols, rows: term.rows }));
        }
      } catch { /* ignore */ }
    }

    function termToggleSelect() {
      const overlay = document.getElementById('term-select-overlay');
      const btn = document.querySelector('.term-select-toggle');
      if (!overlay || !term) return;
      const isActive = overlay.classList.contains('visible');
      if (isActive) {
        overlay.classList.remove('visible');
        if (btn) btn.classList.remove('active');
        if (btn) btn.textContent = 'Select';
        return;
      }
      // Fill overlay with terminal buffer text from current viewport
      const buf = term.buffer.active;
      const lines = [];
      for (let i = 0; i < buf.length; i++) {
        const line = buf.getLine(i);
        if (line) lines.push(line.translateToString(true));
      }
      overlay.textContent = lines.join('\\n');
      overlay.classList.add('visible');
      // Scroll overlay to match terminal viewport position
      const lineH = overlay.scrollHeight / Math.max(lines.length, 1);
      overlay.scrollTop = buf.viewportY * lineH;
      if (btn) btn.classList.add('active');
      if (btn) btn.textContent = 'Done';
    }

    // ─── WebSocket ───
    function connectWS() {
      clearTimeout(reconnectTimer);
      // Detach the previous socket's handlers BEFORE closing it, so replacing a
      // (possibly still-open) socket can't fire a stray onclose that schedules
      // another reconnect — which would loop every few seconds and flap the badge.
      if (ws) { ws.onopen = ws.onmessage = ws.onerror = null; ws.onclose = null; try { ws.close(); } catch {} }

      const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
      const url = proto + '//' + location.host + '/ws?token=' + encodeURIComponent(token);
      ws = new WebSocket(url);

      ws.onopen = () => {
        $('#conn-badge').className = 'status-badge connected';
        $('#conn-badge').textContent = 'connected';
        // Re-subscribe if we had a project selected
        if (currentProject) {
          ws.send(JSON.stringify({ type: 'subscribe', project: currentProject }));
          fitTerminal();
        }
      };

      ws.onmessage = (e) => {
        let msg;
        try { msg = JSON.parse(e.data); } catch { return; }
        if (msg.type === 'output' && msg.project === currentProject && term) {
          term.write(msg.data);
        }
        if (msg.type === 'server-log') {
          appendServerLog(msg);
        }
      };

      ws.onclose = () => {
        $('#conn-badge').className = 'status-badge disconnected';
        $('#conn-badge').textContent = 'disconnected';
        reconnectTimer = setTimeout(connectWS, 3000);
      };

      ws.onerror = () => { /* onclose will fire */ };
    }

    // ─── SSE for state updates ───
    function connectSSE() {
      clearTimeout(sseReconnectTimer);
      if (sse) { try { sse.close(); } catch {} }
      const es = sse = new EventSource('/api/events?token=' + encodeURIComponent(token));
      es.addEventListener('state', (e) => {
        try {
          const state = JSON.parse(e.data);
          terminals = state.terminals || [];
          userTerminals = state.userTerminals || [];
          if (state.services) services = state.services; // keep sidebar heartbeat fresh
          if (state.projects) {
            projects = state.projects;
          }
          renderProjects();
          renderTerminals();
          if (currentProject) {
            const hasTerminal = terminals.some(t => t.project === currentProject);
            updateTerminalPlaceholder(hasTerminal);
          }
        } catch { /* ignore */ }
      });
      es.addEventListener('kanban', (e) => {
        try {
          const d = JSON.parse(e.data);
          if (currentTab === 'kanban' && currentProject && d.project === currentProject.toLowerCase()) {
            loadKanban();
          }
        } catch { /* ignore */ }
      });
      es.addEventListener('secret-requests', (e) => {
        try {
          const d = JSON.parse(e.data);
          secretRequests = d.requests || [];
          updateSecretBadge();
          if (currentTab === 'secrets') renderSecrets();
        } catch { /* ignore */ }
      });
      es.addEventListener('mindmap', () => {
        if (currentTab === 'mindmap') loadMindmap();
      });
      es.addEventListener('usage', (e) => {
        try { renderUsage(JSON.parse(e.data)); } catch { /* ignore */ }
      });
      es.onerror = () => {
        es.onerror = null; // avoid re-entrancy while we reschedule
        try { es.close(); } catch {}
        clearTimeout(sseReconnectTimer);
        sseReconnectTimer = setTimeout(connectSSE, 5000);
      };
    }

    // ─── Reconnect ONLY when actually disconnected ───
    // Mobile PWAs suspend backgrounded tabs and can silently drop the socket; on
    // return we reconnect *only if it's closed*. We never touch a live socket
    // (forcing a reconnect on a healthy connection makes the badge flap).
    function wsHealthy() { return ws && (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING); }
    function sseHealthy() { return sse && sse.readyState !== EventSource.CLOSED; }
    function ensureConnections() {
      if (!token) return false;
      let reconnected = false;
      if (!wsHealthy()) { connectWS(); reconnected = true; }   // re-subscribes on open
      if (!sseHealthy()) { connectSSE(); reconnected = true; } // server re-pushes state + usage
      return reconnected;
    }
    let resumeTimer = null;
    function onResume() {
      if (document.visibilityState === 'hidden') return;
      if (!ensureConnections()) return; // already alive → leave it completely alone
      clearTimeout(resumeTimer);
      resumeTimer = setTimeout(() => { // we actually had to reconnect → refresh the view
        if (!token || !$('#app').classList.contains('visible')) return;
        loadUsage();
        if (currentTab === 'kanban') loadKanban();
        else if (currentTab === 'mindmap') loadMindmap();
        else if (currentTab === 'secrets') loadSecrets();
        else if (currentTab === 'services') loadServices();
      }, 400);
    }
    function setupReconnectHandlers() {
      document.addEventListener('visibilitychange', onResume);
      window.addEventListener('online', onResume); // network returned → reconnect if dropped
      window.addEventListener('pageshow', onResume); // bfcache restore
    }

    // ─── Sidebar ───
    function toggleSidebar() {
      const sb = $('#sidebar');
      const ov = $('#sidebar-overlay');
      sb.classList.toggle('open');
      ov.classList.toggle('visible');
    }

    function closeSidebar() {
      $('#sidebar').classList.remove('open');
      $('#sidebar-overlay').classList.remove('visible');
    }

    // ─── Add Project Modal ───
    function showAddModal() {
      const modal = $('#add-project-modal');
      modal.classList.add('visible');

      if (projectMode === 'single') {
        $('#modal-single-fields').style.display = '';
        $('#modal-multi-fields').style.display = 'none';
        $('#proj-name-single').value = '';
        const sep = projectsDir.includes('/') ? '/' : '\\\\';
        $('#modal-single-hint').textContent = 'Folder: ' + projectsDir + sep + '...';
        setTimeout(() => $('#proj-name-single').focus(), 100);
      } else {
        $('#modal-single-fields').style.display = 'none';
        $('#modal-multi-fields').style.display = '';
        $('#proj-alias').value = '';
        $('#proj-path').value = '';
        $('#proj-name').value = '';
        setTimeout(() => $('#proj-alias').focus(), 100);
      }
    }

    function hideAddModal() {
      $('#add-project-modal').classList.remove('visible');
    }

    async function addProject() {
      if (projectMode === 'single') {
        return addProjectSingle();
      }
      return addProjectMulti();
    }

    async function addProjectSingle() {
      const rawName = $('#proj-name-single').value.trim();
      if (!rawName) { toast('Project name is required', 'error'); return; }
      const alias = rawName.toLowerCase().replace(/[^a-z0-9_-]/g, '-').replace(/-+/g, '-').replace(/^-|-$/g, '');
      if (!alias) { toast('Invalid project name', 'error'); return; }
      try {
        const res = await apiFetch('/api/projects', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ alias, name: rawName, create: true }),
        });
        const data = await res.json();
        if (data.ok) {
          hideAddModal();
          toast('Project created', 'success');
          await loadProjects();
          selectProject(alias);
        } else {
          toast(data.error || 'Failed', 'error');
        }
      } catch (err) {
        toast('Error: ' + err.message, 'error');
      }
    }

    async function addProjectMulti() {
      const alias = $('#proj-alias').value.trim().toLowerCase().replace(/[^a-z0-9_-]/g, '');
      const path = $('#proj-path').value.trim();
      const name = $('#proj-name').value.trim() || alias;
      if (!alias || !path) { toast('Alias and path are required', 'error'); return; }

      // Check if path exists
      let pathExists = true;
      try {
        const checkRes = await apiFetch('/api/projects/check-path', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ path }),
        });
        const checkData = await checkRes.json();
        pathExists = checkData.exists;
      } catch { /* assume exists */ }

      let shouldCreate = false;
      if (!pathExists) {
        if (!confirm('Path does not exist. Create empty folder?')) return;
        shouldCreate = true;
      }

      try {
        const res = await apiFetch('/api/projects', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ alias, path, name, create: shouldCreate }),
        });
        const data = await res.json();
        if (data.ok) {
          hideAddModal();
          toast('Project added', 'success');
          await loadProjects();
          selectProject(alias);
        } else {
          toast(data.error || 'Failed', 'error');
        }
      } catch (err) {
        toast('Error: ' + err.message, 'error');
      }
    }

    // ─── Import ───
    async function checkImport() {
      try {
        const res = await apiFetch('/api/import/check');
        if (!res.ok) return;
        const data = await res.json();
        if (data.available) {
          $('#import-dialog').classList.add('visible');
        }
      } catch { /* ignore */ }
    }

    async function doImport() {
      try {
        const res = await apiFetch('/api/import', { method: 'POST' });
        const data = await res.json();
        if (data.ok) {
          toast('Imported ' + (data.projects || 0) + ' projects', 'success');
          await loadProjects();
        } else {
          toast('Import failed: ' + (data.error || 'Unknown'), 'error');
        }
      } catch (err) {
        toast('Import error: ' + err.message, 'error');
      }
      $('#import-dialog').classList.remove('visible');
    }

    // ─── Tabs ───
    function switchTab(tab) {
      // Leaving the Secrets tab — forget any revealed values for safety.
      if (currentTab === 'secrets' && tab !== 'secrets') {
        for (const k in revealedSecrets) delete revealedSecrets[k];
      }
      currentTab = tab;
      $$('.tab-btn').forEach(b => b.classList.toggle('active', b.dataset.tab === tab));
      $$('.tab-panel').forEach(p => p.classList.toggle('visible', p.dataset.panel === tab));
      if (tab === 'terminal' && term) { fitTerminal(); term.focus(); }
      if (tab === 'services') loadServices();
      if (tab === 'terminals') renderTerminals();
      if (tab === 'browsers') loadBrowsers();
      if (tab === 'git') loadGitInfo();
      if (tab === 'files') loadFiles();
      if (tab === 'kanban') loadKanban();
      if (tab === 'secrets') loadSecrets();
      if (tab === 'mindmap') loadMindmap();
      if (tab === 'info') renderInfo();
      if (tab === 'settings') renderSettings();
    }

    // ─── Git Panel ───
    let gitLastInfo = null;
    let gitRefreshTimer = null;

    async function loadGitInfo() {
      if (!currentProject) return;
      try {
        const res = await apiFetch('/api/git/info?project=' + encodeURIComponent(currentProject));
        const info = await res.json();
        if (!info.ok) {
          gitLastInfo = null;
          $('#git-panel').innerHTML = '<div class="git-empty">Not a git repository</div>';
          return;
        }
        gitLastInfo = info;
        renderGitPanel(info);
      } catch (err) {
        $('#git-panel').innerHTML = '<div class="git-empty">Error: ' + escHtml(err.message) + '</div>';
      }
      // Auto-refresh while git tab is active
      clearInterval(gitRefreshTimer);
      if (currentTab === 'git') {
        gitRefreshTimer = setInterval(loadGitInfo, 8000);
      }
    }

    function renderGitPanel(info) {
      const staged = info.files.filter(f => f.staged);
      const unstaged = info.files.filter(f => !f.staged);

      // Branch header
      const syncParts = [];
      if (info.ahead) syncParts.push('\\u2191' + info.ahead);
      if (info.behind) syncParts.push('\\u2193' + info.behind);
      let html = '<div class="git-branch"><span class="gb-name">' + escHtml(info.branch || '\\u2014') + '</span>'
        + '<span class="gb-sync">' + syncParts.join(' ') + '</span></div>';

      // Files
      html += '<div class="git-files">';
      if (!info.files.length) {
        html += '<div class="git-empty">No changes</div>';
      } else {
        if (staged.length) {
          html += '<div class="git-section-head">Staged <span class="gs-count">(' + staged.length + ')</span>'
            + '<span class="gs-actions"><button data-action="git-unstage-all" title="Unstage all">\\u2212</button></span></div>';
          for (const f of staged) {
            html += renderGitFile(f, true);
          }
        }
        if (unstaged.length) {
          html += '<div class="git-section-head">Changes <span class="gs-count">(' + unstaged.length + ')</span>'
            + '<span class="gs-actions"><button data-action="git-stage-all" title="Stage all">\\uff0b</button></span></div>';
          for (const f of unstaged) {
            html += renderGitFile(f, false);
          }
        }
      }
      html += '</div>';

      // Commit bar
      html += '<div class="git-commit-bar">'
        + '<textarea id="git-commit-msg" placeholder="Commit message\\u2026" rows="2"></textarea>'
        + '<div class="git-commit-btns">'
        + '<button data-action="git-pull" id="git-pull-btn">\\u2193 Pull</button>'
        + '<button data-action="git-push" id="git-push-btn">\\u2191 Push</button>'
        + '<button data-action="git-commit" class="primary">Commit</button>'
        + '</div></div>';

      $('#git-panel').innerHTML = html;
    }

    function renderGitFile(f, isStaged) {
      const code = isStaged ? (f.xy[0] === '?' ? '?' : f.xy[0]) : (f.xy[1] === '?' ? '?' : f.xy[1]);
      const dir = f.file.includes('/') ? f.file.substring(0, f.file.lastIndexOf('/') + 1) : '';
      const name = f.file.includes('/') ? f.file.substring(f.file.lastIndexOf('/') + 1) : f.file;
      const statsHtml = (f.add != null || f.del != null)
        ? '<span class="gf-stats">' + (f.add ? '<span class="gf-add">+' + f.add + '</span>' : '') + (f.del ? '<span class="gf-del">-' + f.del + '</span>' : '') + '</span>'
        : '';
      const ef = escHtml(f.file);
      let actions = '';
      if (isStaged) {
        actions = '<button data-action="git-diff" data-file="' + ef + '" data-cached="1" title="View diff">\\u25D1</button>'
          + '<button data-action="git-unstage" data-file="' + ef + '" title="Unstage">\\u2212</button>';
      } else {
        actions = '<button data-action="git-diff" data-file="' + ef + '" data-cached="0" title="View diff">\\u25D1</button>'
          + '<button data-action="git-stage" data-file="' + ef + '" title="Stage">\\uff0b</button>'
          + '<button data-action="git-discard" data-file="' + ef + '" title="Discard">\\u27F2</button>';
      }
      return '<div class="git-file">'
        + '<span class="gf-status gf-' + code + '">' + code + '</span>'
        + '<span class="gf-name" title="' + ef + '">' + (dir ? '<span class="gf-dir">' + escHtml(dir) + '</span>' : '') + escHtml(name) + '</span>'
        + statsHtml
        + '<span class="gf-actions">' + actions + '</span>'
        + '</div>';
    }

    async function gitStage(file) {
      await apiFetch('/api/git/stage', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project: currentProject, files: [file] }) });
      loadGitInfo();
    }
    async function gitUnstage(file) {
      await apiFetch('/api/git/unstage', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project: currentProject, files: [file] }) });
      loadGitInfo();
    }
    async function gitStageAll() {
      if (!gitLastInfo) return;
      const files = gitLastInfo.files.filter(f => !f.staged).map(f => f.file);
      if (files.length) { await apiFetch('/api/git/stage', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project: currentProject, files }) }); loadGitInfo(); }
    }
    async function gitUnstageAll() {
      if (!gitLastInfo) return;
      const files = gitLastInfo.files.filter(f => f.staged).map(f => f.file);
      if (files.length) { await apiFetch('/api/git/unstage', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project: currentProject, files }) }); loadGitInfo(); }
    }
    async function gitDiscard(file) {
      if (!confirm('Discard changes to ' + file + '? This cannot be undone.')) return;
      const r = await apiFetch('/api/git/discard', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project: currentProject, file }) });
      const d = await r.json();
      if (!d.ok) toast(d.error || 'Discard failed', 'error');
      loadGitInfo();
    }
    async function gitCommit() {
      const msg = ($('#git-commit-msg') || {}).value?.trim();
      if (!msg) { toast('Enter a commit message', 'error'); return; }
      const r = await apiFetch('/api/git/commit', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project: currentProject, message: msg }) });
      const d = await r.json();
      if (d.ok) { const el = $('#git-commit-msg'); if (el) el.value = ''; toast('Committed', 'success'); loadGitInfo(); }
      else toast('Commit failed: ' + (d.error || 'unknown'), 'error');
    }
    async function gitPush() {
      const btn = $('#git-push-btn');
      if (btn) { btn.disabled = true; btn.textContent = '\\u2191 Pushing\\u2026'; }
      try {
        const r = await apiFetch('/api/git/push', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project: currentProject }) });
        const d = await r.json();
        if (d.ok) { toast('Pushed', 'success'); loadGitInfo(); }
        else toast('Push failed: ' + (d.error || 'unknown'), 'error');
      } finally { if (btn) { btn.disabled = false; btn.textContent = '\\u2191 Push'; } }
    }
    async function gitPull() {
      const btn = $('#git-pull-btn');
      if (btn) { btn.disabled = true; btn.textContent = '\\u2193 Pulling\\u2026'; }
      try {
        const r = await apiFetch('/api/git/pull', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project: currentProject }) });
        const d = await r.json();
        if (d.ok) { toast('Pulled', 'success'); loadGitInfo(); }
        else toast('Pull failed: ' + (d.error || 'unknown'), 'error');
      } finally { if (btn) { btn.disabled = false; btn.textContent = '\\u2193 Pull'; } }
    }
    async function gitViewDiff(file, cached) {
      const r = await apiFetch('/api/git/diff?project=' + encodeURIComponent(currentProject) + '&file=' + encodeURIComponent(file) + '&cached=' + (cached ? '1' : '0'));
      const d = await r.json();
      if (!d.ok) { toast('Failed to load diff', 'error'); return; }
      diffCached = !!cached;
      feOpenDiff(file, d.old || '', d.new || '', d.diff || '');
    }

    let diffCurrentFile = '';
    let diffCached = false;

    function feOpenDiff(file, oldContent, newContent, rawDiff) {
      feCurrentProject = '';
      feCurrentFile = '';
      feReadOnly = true;
      diffCurrentFile = file;
      const btn = $('#fe-save-btn');
      if (btn) btn.style.display = 'none';
      $('#file-editor').classList.add('visible');

      const container = $('#fe-container');
      if (feEditorView) { feEditorView.destroy(); feEditorView = null; }

      const oldLines = oldContent.split('\\n');
      const newLines = newContent.split('\\n');
      const diff = simpleDiff(oldLines, newLines);

      // Group consecutive changes into chunks
      const chunks = [];
      let currentChunk = null;
      let lineIdx = 0;
      for (const op of diff) {
        if (op.type !== 'equal') {
          if (!currentChunk) { currentChunk = { startLine: lineIdx, ops: [] }; }
          currentChunk.ops.push(op);
        } else {
          if (currentChunk) { currentChunk.endLine = lineIdx; chunks.push(currentChunk); currentChunk = null; }
        }
        lineIdx++;
      }
      if (currentChunk) { currentChunk.endLine = lineIdx; chunks.push(currentChunk); }

      let currentChunkIdx = 0;

      // Build header
      const hdr = '<div style="display:flex;align-items:center;padding:6px 12px;background:var(--bg-secondary);border-bottom:1px solid var(--border);gap:8px;">'
        + '<span style="font-family:var(--mono);font-size:12px;color:var(--text-primary);flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">' + escHtml(file) + '</span>'
        + '<button id="diff-prev" style="background:none;border:none;color:var(--text-secondary);cursor:pointer;font-size:14px;" title="Previous chunk">\\u2191</button>'
        + '<span id="diff-counter" style="font-size:11px;color:var(--text-muted);font-family:var(--mono);">\\u2014/' + chunks.length + '</span>'
        + '<button id="diff-next" style="background:none;border:none;color:var(--text-secondary);cursor:pointer;font-size:14px;" title="Next chunk">\\u2193</button>'
        + '</div>';

      const isMobile = window.innerWidth <= 768;
      const lineH = 20;
      const lineS = 'display:flex;height:' + lineH + 'px;line-height:' + lineH + 'px;font-family:var(--mono);font-size:12px;';
      const lnS = 'width:' + (isMobile ? '32' : '40') + 'px;text-align:right;padding:0 ' + (isMobile ? '4' : '8') + 'px 0 0;color:var(--text-muted);flex-shrink:0;user-select:none;';
      const codeS = 'flex:1;padding:0 6px;white-space:pre;overflow:hidden;text-overflow:ellipsis;';

      let oldHtml = '', newHtml = '', unifiedHtml = '';
      let oldLn = 1, newLn = 1;
      let chunkIdx = 0;
      lineIdx = 0;
      const chunkLinePositions = [];
      let inChunk = false;

      for (let di = 0; di < diff.length; di++) {
        const op = diff[di];
        // Check if we're entering a chunk — insert stage button
        if (chunkIdx < chunks.length && lineIdx === chunks[chunkIdx].startLine && !inChunk) {
          chunkLinePositions.push(lineIdx);
          const stageBtn = '<div data-chunk="' + chunkIdx + '" style="height:22px;line-height:22px;display:flex;align-items:center;padding:0 6px;background:#1e293b;border-top:1px solid var(--border);border-bottom:1px solid var(--border);">'
            + '<button data-action="diff-stage-chunk" data-cidx="' + chunkIdx + '" style="padding:1px 8px;border-radius:3px;border:1px solid var(--green);background:transparent;color:var(--green);cursor:pointer;font-size:10px;font-weight:600;">Stage</button>'
            + '<span style="margin-left:6px;font-size:10px;color:var(--text-muted);">Chunk ' + (chunkIdx + 1) + '</span></div>';
          if (isMobile) {
            unifiedHtml += stageBtn;
          } else {
            oldHtml += stageBtn;
            newHtml += '<div style="height:22px;line-height:22px;background:#1e293b;border-top:1px solid var(--border);border-bottom:1px solid var(--border);"></div>';
          }
          inChunk = true;
        }

        if (op.type === 'equal') {
          if (inChunk) { inChunk = false; chunkIdx++; }
          if (isMobile) {
            unifiedHtml += '<div style="' + lineS + 'background:var(--bg-primary);"><span style="' + lnS + '">' + newLn + '</span><span style="' + codeS + '">' + escHtml(op.text) + '</span></div>';
          } else {
            oldHtml += '<div style="' + lineS + 'background:var(--bg-primary);"><span style="' + lnS + '">' + oldLn + '</span><span style="' + codeS + '">' + escHtml(op.text) + '</span></div>';
            newHtml += '<div style="' + lineS + 'background:var(--bg-primary);"><span style="' + lnS + '">' + newLn + '</span><span style="' + codeS + '">' + escHtml(op.text) + '</span></div>';
          }
          oldLn++; newLn++;
        } else if (op.type === 'remove' && di + 1 < diff.length && diff[di + 1].type === 'add') {
          const addOp = diff[di + 1];
          const wd = wordDiffHtml(op.text, addOp.text, '#5c2020', '#1a4a2a');
          if (isMobile) {
            unifiedHtml += '<div style="' + lineS + 'background:#2d1215;"><span style="' + lnS + '">-</span><span style="' + codeS + 'color:#f87171;">' + wd.oldHtml + '</span></div>';
            unifiedHtml += '<div style="' + lineS + 'background:#0f291a;"><span style="' + lnS + '">+</span><span style="' + codeS + 'color:#34d399;">' + wd.newHtml + '</span></div>';
          } else {
            oldHtml += '<div style="' + lineS + 'background:#2d1215;"><span style="' + lnS + '">' + oldLn + '</span><span style="' + codeS + 'color:#f87171;">' + wd.oldHtml + '</span></div>';
            newHtml += '<div style="' + lineS + 'background:#0f291a;"><span style="' + lnS + '">' + newLn + '</span><span style="' + codeS + 'color:#34d399;">' + wd.newHtml + '</span></div>';
          }
          oldLn++; newLn++;
          di++; lineIdx++;
        } else if (op.type === 'remove') {
          if (isMobile) {
            unifiedHtml += '<div style="' + lineS + 'background:#2d1215;"><span style="' + lnS + '">-</span><span style="' + codeS + 'color:#f87171;">' + escHtml(op.text) + '</span></div>';
          } else {
            oldHtml += '<div style="' + lineS + 'background:#2d1215;"><span style="' + lnS + '">' + oldLn + '</span><span style="' + codeS + 'color:#f87171;">' + escHtml(op.text) + '</span></div>';
            newHtml += '<div style="' + lineS + 'background:#1a1215;"><span style="' + lnS + '"></span><span style="' + codeS + '"></span></div>';
          }
          oldLn++;
        } else if (op.type === 'add') {
          if (isMobile) {
            unifiedHtml += '<div style="' + lineS + 'background:#0f291a;"><span style="' + lnS + '">+</span><span style="' + codeS + 'color:#34d399;">' + escHtml(op.text) + '</span></div>';
          } else {
            oldHtml += '<div style="' + lineS + 'background:#0f1a15;"><span style="' + lnS + '"></span><span style="' + codeS + '"></span></div>';
            newHtml += '<div style="' + lineS + 'background:#0f291a;"><span style="' + lnS + '">' + newLn + '</span><span style="' + codeS + 'color:#34d399;">' + escHtml(op.text) + '</span></div>';
          }
          newLn++;
        }
        lineIdx++;
      }
      if (inChunk) chunkIdx++;

      if (isMobile) {
        container.innerHTML = hdr
          + '<div id="diff-unified" style="flex:1;overflow:auto;min-height:0;-webkit-overflow-scrolling:touch;background:var(--bg-primary);">'
          + unifiedHtml + '</div>';
      } else {
        container.innerHTML = hdr
          + '<div style="display:flex;flex:1;min-height:0;overflow:hidden;">'
          + '<div id="diff-left" style="flex:1;overflow:auto;border-right:1px solid var(--border);background:var(--bg-primary);">'
          + '<div style="padding:4px 8px;font-size:10px;font-weight:700;text-transform:uppercase;letter-spacing:0.5px;color:var(--text-muted);background:var(--bg-tertiary);border-bottom:1px solid var(--border-subtle);position:sticky;top:0;z-index:1;">Original</div>'
          + oldHtml + '</div>'
          + '<div id="diff-right" style="flex:1;overflow:auto;background:var(--bg-primary);">'
          + '<div style="padding:4px 8px;font-size:10px;font-weight:700;text-transform:uppercase;letter-spacing:0.5px;color:var(--text-muted);background:var(--bg-tertiary);border-bottom:1px solid var(--border-subtle);position:sticky;top:0;z-index:1;">Modified</div>'
          + newHtml + '</div>'
          + '</div>';
      }

      $('#fe-path').textContent = file;

      // Scroll container for chunk navigation
      const scrollEl = isMobile
        ? document.getElementById('diff-unified')
        : document.getElementById('diff-left');

      // Sync scroll (desktop only)
      if (!isMobile) {
        const left = document.getElementById('diff-left');
        const right = document.getElementById('diff-right');
        let syncing = false;
        left.addEventListener('scroll', () => { if (!syncing) { syncing = true; right.scrollTop = left.scrollTop; syncing = false; } });
        right.addEventListener('scroll', () => { if (!syncing) { syncing = true; left.scrollTop = right.scrollTop; syncing = false; } });
      }

      // Chunk navigation
      function scrollToChunk(idx) {
        if (idx < 0 || idx >= chunkLinePositions.length || !scrollEl) return;
        const pos = chunkLinePositions[idx] * lineH + idx * 22 + (isMobile ? 0 : 28);
        scrollEl.scrollTop = pos;
      }
      document.getElementById('diff-prev').onclick = () => {
        if (!chunks.length) return;
        currentChunkIdx = Math.max(0, currentChunkIdx - 1);
        document.getElementById('diff-counter').textContent = (currentChunkIdx + 1) + '/' + chunks.length;
        scrollToChunk(currentChunkIdx);
      };
      document.getElementById('diff-next').onclick = () => {
        if (!chunks.length) return;
        currentChunkIdx = Math.min(chunks.length - 1, currentChunkIdx + 1);
        document.getElementById('diff-counter').textContent = (currentChunkIdx + 1) + '/' + chunks.length;
        scrollToChunk(currentChunkIdx);
      };

      // Store diff + chunk data for stage-chunk action
      container._diffData = { oldLines, newLines, diff, chunks, file };
    }

    async function stageChunk(chunkIdx) {
      const container = $('#fe-container');
      const data = container._diffData;
      if (!data) { toast('No diff data', 'error'); return; }
      const { oldLines, diff, chunks, file } = data;
      const chunk = chunks[chunkIdx];
      if (!chunk) { toast('Invalid chunk', 'error'); return; }

      // Build a unified diff patch for this chunk
      // We need to figure out the old/new line numbers at the chunk start
      let oLn = 1, nLn = 1;
      let idx = 0;
      for (const op of diff) {
        if (idx >= chunk.startLine) break;
        if (op.type === 'equal') { oLn++; nLn++; }
        else if (op.type === 'remove') { oLn++; }
        else if (op.type === 'add') { nLn++; }
        idx++;
      }

      let patchBody = '';
      let removedCount = 0, addedCount = 0, contextBefore = 0, contextAfter = 0;
      // Add up to 3 context lines before
      const ctxStart = Math.max(0, chunk.startLine - 3);
      for (let i = ctxStart; i < chunk.startLine; i++) {
        if (diff[i] && diff[i].type === 'equal') {
          patchBody += ' ' + diff[i].text + '\\n';
          contextBefore++;
        }
      }
      // Add chunk ops
      for (const op of chunk.ops) {
        if (op.type === 'remove') { patchBody += '-' + op.text + '\\n'; removedCount++; }
        else if (op.type === 'add') { patchBody += '+' + op.text + '\\n'; addedCount++; }
      }
      // Add up to 3 context lines after
      for (let i = chunk.endLine; i < Math.min(diff.length, chunk.endLine + 3); i++) {
        if (diff[i] && diff[i].type === 'equal') {
          patchBody += ' ' + diff[i].text + '\\n';
          contextAfter++;
        }
      }

      const startOld = oLn - contextBefore;
      const startNew = nLn - contextBefore;
      const countOld = contextBefore + removedCount + contextAfter;
      const countNew = contextBefore + addedCount + contextAfter;
      const hunkHeader = '@@ -' + startOld + ',' + countOld + ' +' + startNew + ',' + countNew + ' @@\\n';
      const patch = 'diff --git a/' + file + ' b/' + file + '\\n--- a/' + file + '\\n+++ b/' + file + '\\n' + hunkHeader + patchBody;

      try {
        const res = await apiFetch('/api/git/stageHunk', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ project: currentProject, patch }),
        });
        const d = await res.json();
        if (d.ok) {
          toast('Chunk ' + (chunkIdx + 1) + ' staged', 'success');
          // Highlight the staged chunk
          const btn = container.querySelector('[data-cidx="' + chunkIdx + '"]');
          if (btn) { btn.textContent = 'Staged'; btn.disabled = true; btn.style.color = 'var(--text-muted)'; btn.style.borderColor = 'var(--border)'; }
          loadGitInfo();
        } else {
          toast('Stage failed: ' + (d.error || 'unknown'), 'error');
        }
      } catch (err) {
        toast('Error: ' + err.message, 'error');
      }
    }

    // Word-level diff: returns HTML with only changed parts highlighted
    function wordDiffHtml(oldText, newText, removeColor, addColor) {
      // Tokenize into words and whitespace
      const tokenize = s => s.match(/\\S+|\\s+/g) || [''];
      const oldToks = tokenize(oldText);
      const newToks = tokenize(newText);
      const N = oldToks.length, M = newToks.length;
      // LCS on tokens
      const dp = Array.from({ length: N + 1 }, () => new Uint16Array(M + 1));
      for (let i = 1; i <= N; i++)
        for (let j = 1; j <= M; j++)
          dp[i][j] = oldToks[i-1] === newToks[j-1] ? dp[i-1][j-1] + 1 : Math.max(dp[i-1][j], dp[i][j-1]);
      // Backtrack
      const ops = [];
      let i = N, j = M;
      while (i > 0 || j > 0) {
        if (i > 0 && j > 0 && oldToks[i-1] === newToks[j-1]) {
          ops.push({ type: 'eq', old: oldToks[i-1], new: newToks[j-1] }); i--; j--;
        } else if (j > 0 && (i === 0 || dp[i][j-1] >= dp[i-1][j])) {
          ops.push({ type: 'add', old: '', new: newToks[j-1] }); j--;
        } else {
          ops.push({ type: 'del', old: oldToks[i-1], new: '' }); i--;
        }
      }
      ops.reverse();
      // Build HTML for old and new sides
      let oldH = '', newH = '';
      for (const o of ops) {
        if (o.type === 'eq') { oldH += escHtml(o.old); newH += escHtml(o.new); }
        else if (o.type === 'del') { oldH += '<span style="background:' + removeColor + ';border-radius:2px;">' + escHtml(o.old) + '</span>'; }
        else if (o.type === 'add') { newH += '<span style="background:' + addColor + ';border-radius:2px;">' + escHtml(o.new) + '</span>'; }
      }
      return { oldHtml: oldH, newHtml: newH };
    }

    function simpleDiff(oldLines, newLines) {
      // Simple LCS diff
      const N = oldLines.length, M = newLines.length;
      if (N + M > 10000) {
        // Too large, just show raw
        const ops = [];
        for (const l of oldLines) ops.push({ type: 'remove', text: l });
        for (const l of newLines) ops.push({ type: 'add', text: l });
        return ops;
      }
      // Build LCS table
      const dp = Array.from({ length: N + 1 }, () => new Uint16Array(M + 1));
      for (let i = 1; i <= N; i++)
        for (let j = 1; j <= M; j++)
          dp[i][j] = oldLines[i-1] === newLines[j-1] ? dp[i-1][j-1] + 1 : Math.max(dp[i-1][j], dp[i][j-1]);

      // Backtrack
      const ops = [];
      let i = N, j = M;
      while (i > 0 || j > 0) {
        if (i > 0 && j > 0 && oldLines[i-1] === newLines[j-1]) {
          ops.push({ type: 'equal', text: oldLines[i-1] });
          i--; j--;
        } else if (j > 0 && (i === 0 || dp[i][j-1] >= dp[i-1][j])) {
          ops.push({ type: 'add', text: newLines[j-1] });
          j--;
        } else {
          ops.push({ type: 'remove', text: oldLines[i-1] });
          i--;
        }
      }
      ops.reverse();
      return ops;
    }

    // ─── Files Panel ───
    let filesCurrentDir = '';

    async function loadFiles(dir) {
      if (!currentProject) return;
      const relDir = dir || filesCurrentDir || '';
      try {
        const res = await apiFetch('/api/files/list?project=' + encodeURIComponent(currentProject) + '&dir=' + encodeURIComponent(relDir));
        const data = await res.json();
        if (!data.ok) {
          $('#files-panel').innerHTML = '<div class="git-empty">' + escHtml(data.error || 'Failed') + '</div>';
          return;
        }
        filesCurrentDir = data.path || relDir;
        renderFileList(data.entries, filesCurrentDir);
      } catch (err) {
        $('#files-panel').innerHTML = '<div class="git-empty">Error: ' + escHtml(err.message) + '</div>';
      }
    }

    function renderFileList(entries, dirPath) {
      // Breadcrumb + upload button
      const parts = dirPath === '.' ? [] : dirPath.replace(/\\\\/g, '/').split('/').filter(Boolean);
      let bc = '<div class="files-breadcrumb" style="display:flex;align-items:center;gap:6px;"><div style="flex:1;"><a data-action="files-nav" data-dir=".">root</a>';
      let accum = '';
      for (const p of parts) {
        accum += (accum ? '/' : '') + p;
        bc += ' / <a data-action="files-nav" data-dir="' + escHtml(accum) + '">' + escHtml(p) + '</a>';
      }
      bc += '</div><button data-action="files-upload" style="padding:3px 10px;border-radius:4px;border:1px solid var(--accent);background:transparent;color:var(--accent);cursor:pointer;font-size:11px;white-space:nowrap;">\\u2B06 Upload</button></div>';

      // File list
      let html = '<div class="files-list">';
      if (dirPath !== '.' && dirPath !== '') {
        const parent = parts.slice(0, -1).join('/') || '.';
        html += '<div class="file-item dir" data-action="files-nav" data-dir="' + escHtml(parent) + '">'
          + '<span class="fi-icon">\\u2B06</span><span class="fi-name">..</span></div>';
      }
      for (const e of entries) {
        const subPath = (dirPath === '.' ? '' : dirPath + '/') + e.name;
        if (e.type === 'dir') {
          html += '<div class="file-item dir" data-action="files-nav" data-dir="' + escHtml(subPath) + '">'
            + '<span class="fi-icon">\\uD83D\\uDCC1</span><span class="fi-name">' + escHtml(e.name) + '</span></div>';
        } else {
          const size = formatFileSize(e.size);
          html += '<div class="file-item file">'
            + '<span class="fi-icon">' + fileIcon(e.name) + '</span>'
            + '<span class="fi-name" data-action="files-open" data-file="' + escHtml(subPath) + '" style="cursor:pointer;flex:1;">' + escHtml(e.name) + '</span>'
            + '<span class="fi-size">' + size + '</span>'
            + '<button data-action="files-copy-path" data-file="' + escHtml(subPath) + '" title="Copy path" style="background:none;border:none;color:var(--text-muted);cursor:pointer;font-size:14px;padding:4px 6px;min-width:28px;">&#128203;</button>'
            + '<button data-action="files-download" data-file="' + escHtml(subPath) + '" title="Download" style="background:none;border:none;color:var(--text-muted);cursor:pointer;font-size:14px;padding:4px 6px;min-width:28px;">&#8681;</button>'
            + '</div>';
        }
      }
      html += '</div>';
      $('#files-panel').innerHTML = bc + html;
    }

    function filesUpload() {
      const input = document.createElement('input');
      input.type = 'file';
      input.multiple = true;
      input.onchange = async () => {
        for (const file of input.files) {
          const reader = new FileReader();
          reader.onload = async () => {
            const base64 = reader.result.split(',')[1];
            try {
              const res = await apiFetch('/api/files/upload', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ project: currentProject, dir: filesCurrentDir, name: file.name, data: base64 }),
              });
              const d = await res.json();
              if (d.ok) toast('Uploaded ' + file.name, 'success');
              else toast('Upload failed: ' + (d.error || 'unknown'), 'error');
              loadFiles();
            } catch (err) { toast('Error: ' + err.message, 'error'); }
          };
          reader.readAsDataURL(file);
        }
      };
      input.click();
    }

    function filesDownload(relPath) {
      apiFetch('/api/files/download-link', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ project: currentProject, file: relPath }),
      }).then(r => r.json()).then(data => {
        if (!data.ok) { showToast(data.error || 'Failed', 'error'); return; }
        const dlUrl = location.origin + data.url;
        // Try opening in new window
        window.open(dlUrl, '_blank');
        // Also copy link to clipboard as fallback
        navigator.clipboard.writeText(dlUrl).then(
          () => showToast('Download link copied to clipboard'),
          () => showToast('Download started')
        );
      }).catch(e => showToast('Download failed: ' + e.message, 'error'));
    }

    function filesCopyPath(relPath) {
      const project = projects.find(p => p.alias === currentProject);
      if (!project) return;
      // Detect separator from project path (backslash = Windows)
      const isWin = project.path.includes('\\\\');
      const sep = isWin ? '\\\\' : '/';
      const normRel = isWin ? relPath.replace(/\\//g, '\\\\') : relPath;
      const base = project.path.endsWith(sep) ? project.path.slice(0, -1) : project.path;
      navigator.clipboard.writeText(base + sep + normRel).then(
        () => showToast('Path copied'),
        () => showToast('Copy failed', 'error')
      );
    }

    function formatFileSize(bytes) {
      if (!bytes) return '';
      if (bytes < 1024) return bytes + ' B';
      if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB';
      return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
    }

    // ─── File Icons ───
    function fileIcon(name) {
      const ext = name.includes('.') ? name.slice(name.lastIndexOf('.')).toLowerCase() : '';
      const base = name.toLowerCase();
      if (base === 'dockerfile' || base.startsWith('dockerfile.')) return '\\uD83D\\uDC33';
      if (base === '.gitignore' || base === '.gitattributes') return '\\u2310';
      if (base === 'package.json' || base === 'package-lock.json') return '<span style="color:#cb3837">npm</span>';
      if (base === '.env' || base.startsWith('.env.')) return '\\uD83D\\uDD12';
      const icons = {
        '.js': '<span style="color:#f7df1e">JS</span>',
        '.mjs': '<span style="color:#f7df1e">JS</span>',
        '.cjs': '<span style="color:#f7df1e">JS</span>',
        '.ts': '<span style="color:#3178c6">TS</span>',
        '.tsx': '<span style="color:#3178c6">TX</span>',
        '.jsx': '<span style="color:#61dafb">JX</span>',
        '.json': '<span style="color:#a8a8a2">{ }</span>',
        '.html': '<span style="color:#e34c26">\\u25C7</span>',
        '.css': '<span style="color:#264de4">#</span>',
        '.scss': '<span style="color:#cd6799">#</span>',
        '.py': '<span style="color:#3776ab">\\uD83D\\uDC0D</span>',
        '.go': '<span style="color:#00add8">Go</span>',
        '.rs': '<span style="color:#dea584">\\uD83E\\uDD80</span>',
        '.java': '<span style="color:#f89820">\\u2615</span>',
        '.c': '<span style="color:#555">C</span>',
        '.cpp': '<span style="color:#659ad2">C+</span>',
        '.h': '<span style="color:#555">H</span>',
        '.sh': '<span style="color:#4eaa25">$_</span>',
        '.ps1': '<span style="color:#012456">PS</span>',
        '.sql': '<span style="color:#e38c00">DB</span>',
        '.md': '<span style="color:#519aba">M\\u2193</span>',
        '.yaml': '<span style="color:#cb171e">\\u2699</span>',
        '.yml': '<span style="color:#cb171e">\\u2699</span>',
        '.xml': '<span style="color:#e37933">\\u25C7</span>',
        '.svg': '<span style="color:#ffb13b">\\u25C7</span>',
        '.png': '\\uD83D\\uDDBC\\uFE0F', '.jpg': '\\uD83D\\uDDBC\\uFE0F', '.jpeg': '\\uD83D\\uDDBC\\uFE0F', '.gif': '\\uD83D\\uDDBC\\uFE0F', '.webp': '\\uD83D\\uDDBC\\uFE0F',
        '.zip': '\\uD83D\\uDCE6', '.tar': '\\uD83D\\uDCE6', '.gz': '\\uD83D\\uDCE6',
        '.pdf': '\\uD83D\\uDCD5',
        '.lock': '\\uD83D\\uDD12',
        '.log': '\\uD83D\\uDCCB',
        '.vue': '<span style="color:#42b883">V</span>',
        '.svelte': '<span style="color:#ff3e00">S</span>',
      };
      return icons[ext] || '\\uD83D\\uDCC4';
    }

    // ─── File Editor ───
    let feCurrentProject = '';
    let feCurrentFile = '';
    let feOriginal = '';
    let feReadOnly = false;
    let feEditorView = null;

    function feCreateEditor(content, filePath, readOnly) {
      const container = $('#fe-container');
      container.innerHTML = '';
      if (feEditorView) { feEditorView.destroy(); feEditorView = null; }

      if (window.CM) {
        const langExt = window.CM.getLangExtension(filePath || '');
        const extensions = [
          ...window.CM.basicSetup,
          ...langExt,
          ...window.CM.oneDark,
        ];
        if (readOnly) {
          extensions.push(window.CM.EditorState.readOnly.of(true));
        } else {
          extensions.push(window.CM.EditorView.updateListener.of((update) => {
            if (update.docChanged) {
              const btn = $('#fe-save-btn');
              if (btn) btn.disabled = (feEditorView.state.doc.toString() === feOriginal);
            }
          }));
          extensions.push(window.CM.EditorView.domEventHandlers({
            keydown(e) {
              if (e.key === 's' && (e.ctrlKey || e.metaKey)) {
                e.preventDefault(); feSave();
              }
            }
          }));
        }
        feEditorView = new window.CM.EditorView({
          doc: content,
          extensions,
          parent: container,
        });
        if (!readOnly) feEditorView.focus();
      } else {
        // Fallback to textarea
        const ta = document.createElement('textarea');
        ta.value = content;
        ta.readOnly = !!readOnly;
        ta.spellcheck = false;
        ta.style.cssText = 'width:100%;height:100%;resize:none;border:none;background:var(--bg-primary);color:var(--text-primary);font-family:var(--mono);font-size:13px;padding:12px;line-height:1.5;outline:none;';
        container.appendChild(ta);
      }
    }

    function feGetContent() {
      if (feEditorView) return feEditorView.state.doc.toString();
      const ta = $('#fe-container textarea');
      return ta ? ta.value : '';
    }

    async function feOpen(project, file) {
      try {
        const res = await apiFetch('/api/files/read?project=' + encodeURIComponent(project) + '&file=' + encodeURIComponent(file));
        const data = await res.json();
        if (!data.ok) { toast(data.error || 'Cannot open file', 'error'); return; }
        feCurrentProject = project;
        feCurrentFile = file;
        feOriginal = data.content;
        feReadOnly = false;
        $('#fe-path').textContent = file;
        const btn = $('#fe-save-btn');
        if (btn) { btn.disabled = true; btn.style.display = ''; }
        $('#file-editor').classList.add('visible');
        feCreateEditor(data.content, file, false);
      } catch (err) {
        toast('Error: ' + err.message, 'error');
      }
    }

    function feOpenContent(title, content, readOnly) {
      feCurrentProject = '';
      feCurrentFile = '';
      feOriginal = content;
      feReadOnly = readOnly || false;
      $('#fe-path').textContent = title;
      const btn = $('#fe-save-btn');
      if (btn) btn.style.display = readOnly ? 'none' : '';
      $('#file-editor').classList.add('visible');
      feCreateEditor(content, title, !!readOnly);
    }

    function feClose() {
      $('#file-editor').classList.remove('visible');
      if (feEditorView) { feEditorView.destroy(); feEditorView = null; }
    }

    async function feSave() {
      if (feReadOnly || !feCurrentProject || !feCurrentFile) return;
      const content = feGetContent();
      const btn = $('#fe-save-btn');
      try {
        if (btn) { btn.disabled = true; btn.textContent = 'Saving\\u2026'; }
        const res = await apiFetch('/api/files/write', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project: feCurrentProject, file: feCurrentFile, content }) });
        const data = await res.json();
        if (data.ok) {
          feOriginal = content;
          toast('Saved', 'success');
          if (btn) { btn.textContent = 'Saved \\u2713'; setTimeout(() => { btn.textContent = 'Save'; btn.disabled = true; }, 1500); }
        } else {
          toast('Save failed: ' + (data.error || 'unknown'), 'error');
          if (btn) { btn.textContent = 'Save'; btn.disabled = false; }
        }
      } catch (err) {
        toast('Error: ' + err.message, 'error');
        if (btn) { btn.textContent = 'Save'; btn.disabled = false; }
      }
    }

    // Make feClose/feSave available globally for onclick handlers
    window.feClose = feClose;
    window.feSave = feSave;

    // ─── Services ───
    async function loadServices() {
      try {
        const res = await apiFetch('/api/services');
        const data = await res.json();
        services = data.services || [];
        renderServices();
        renderProjects(); // refresh sidebar heartbeats
      } catch (err) {
        toast('Failed to load services: ' + err.message, 'error');
      }
    }

    function renderServices() {
      const panel = $('#services-panel');
      const projectServices = currentProject
        ? services.filter(s => s.alias === currentProject)
        : services;

      const addBtn = '<div style="padding:0 0 8px;text-align:right;">'
        + '<button class="svc-action-btn" data-action="svc-register" style="background:var(--accent);border-color:var(--accent);color:#fff;">+ Register Service</button>'
        + '</div>';

      if (projectServices.length === 0) {
        panel.innerHTML = addBtn + '<div class="services-empty">'
          + '<div class="icon">&#9881;</div>'
          + '<p>No services registered' + (currentProject ? ' for this project' : '') + '</p>'
          + '</div>';
        return;
      }

      panel.innerHTML = addBtn + projectServices.map(s => {
        const statusClass = s.status === 'running' ? 'running' : (s.status === 'error' ? 'error' : 'stopped');
        const tunnelOn = (s.tunnelPort || 0) > 0;
        const tunnelHtml = tunnelOn
          ? '<br><span style="color:var(--text-muted)">tunnel :' + s.tunnelPort + '</span>'
            + (s.tunnelUrl ? ' <a class="tunnel-link" href="' + escHtml(s.tunnelUrl) + '" target="_blank">' + escHtml(s.tunnelUrl) + '</a>' : ' <span style="color:var(--yellow)">(starting…)</span>')
          : '';
        return '<div class="svc-card" data-svc-key="' + escHtml(s.key) + '">'
          + '<div class="svc-header">'
          + '  <span class="svc-name">' + escHtml(s.name || s.key) + '</span>'
          + '  <span class="svc-status ' + statusClass + '">' + escHtml(s.status) + '</span>'
          + '</div>'
          + '<div class="svc-meta">' + escHtml(s.command || '')
          + (s.uptime ? ' &mdash; up ' + escHtml(s.uptime) : '')
          + tunnelHtml + '</div>'
          + '<div class="svc-actions">'
          + (s.status === 'running'
            ? '<button data-action="svc-stop" data-key="' + escHtml(s.key) + '">Stop</button>'
              + '<button data-action="svc-restart" data-key="' + escHtml(s.key) + '">Restart</button>'
            : '<button data-action="svc-start" data-key="' + escHtml(s.key) + '">Start</button>')
          + '<button data-action="svc-logs" data-key="' + escHtml(s.key) + '">Logs</button>'
          + '<button data-action="svc-tunnel" data-key="' + escHtml(s.key) + '" data-port="' + (s.tunnelPort || 0) + '">' + (tunnelOn ? 'Tunnel: on' : 'Tunnel: off') + '</button>'
          + '<button class="danger" data-action="svc-delete" data-key="' + escHtml(s.key) + '">Delete</button>'
          + '</div>'
          + '<div class="svc-logs" id="svc-logs-' + escHtml(s.key) + '"></div>'
          + '</div>';
      }).join('');
    }

    async function svcAction(action, key) {
      try {
        const res = await apiFetch('/api/services/' + encodeURIComponent(key) + '/' + action, { method: 'POST' });
        const data = await res.json();
        if (!data.ok) toast(data.error || 'Failed', 'error');
        else toast(action + ' ' + key, 'success');
        setTimeout(loadServices, 500);
      } catch (err) {
        toast('Error: ' + err.message, 'error');
      }
    }

    async function svcLogs(key) {
      const el = document.getElementById('svc-logs-' + key);
      if (!el) return;
      if (el.classList.contains('visible')) { el.classList.remove('visible'); return; }
      try {
        const res = await apiFetch('/api/services/' + encodeURIComponent(key) + '/logs');
        const data = await res.json();
        el.textContent = (data.logs || []).join('\\n') || '(no logs)';
        el.classList.add('visible');
        el.scrollTop = el.scrollHeight;
      } catch (err) {
        el.textContent = 'Error: ' + err.message;
        el.classList.add('visible');
      }
    }

    async function svcTunnel(key, currentPort) {
      const cur = parseInt(currentPort, 10) || 0;
      const val = await askText({
        title: cur > 0 ? 'Tunnel (currently :' + cur + ')' : 'Enable tunnel',
        label: 'Port to expose via Cloudflare tunnel. Enter 0 (or leave blank) to disable.',
        value: cur > 0 ? String(cur) : '',
      });
      if (val === null) return; // cancelled
      const port = parseInt(val, 10) || 0;
      try {
        const res = await apiFetch('/api/services/' + encodeURIComponent(key) + '/tunnel', {
          method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ port }),
        });
        const d = await res.json();
        if (!d.ok) toast(d.error || 'Failed', 'error');
        else toast(port > 0 ? 'Tunnel enabled on :' + port : 'Tunnel disabled', 'success');
        setTimeout(loadServices, 600);
      } catch (err) { toast('Error: ' + err.message, 'error'); }
    }

    async function svcDelete(key) {
      if (!confirm('Delete service "' + key + '"?')) return;
      try {
        const res = await apiFetch('/api/services/' + encodeURIComponent(key) + '/delete', { method: 'POST' });
        const data = await res.json();
        if (!data.ok) toast(data.error || 'Failed', 'error');
        else toast('Deleted ' + key, 'success');
        setTimeout(loadServices, 500);
      } catch (err) {
        toast('Error: ' + err.message, 'error');
      }
    }

    function showRegisterServiceForm() {
      const panel = $('#services-panel');
      // Check if form already visible
      if (panel.querySelector('.svc-register-form')) return;
      const form = document.createElement('div');
      form.className = 'svc-register-form';
      form.style.cssText = 'padding:12px;background:var(--bg-secondary);border:1px solid var(--border);border-radius:var(--radius-sm);margin:8px 0;display:flex;flex-direction:column;gap:8px;';
      form.innerHTML = '<div style="font-size:12px;font-weight:600;color:var(--text-primary);">Register Service</div>'
        + '<input type="text" id="svc-reg-name" placeholder="Service name" style="padding:6px 8px;border-radius:4px;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font-size:12px;" />'
        + '<input type="text" id="svc-reg-cmd" placeholder="Command (e.g. npm run dev)" style="padding:6px 8px;border-radius:4px;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font-size:12px;font-family:var(--mono);" />'
        + '<input type="text" id="svc-reg-cwd" placeholder="Working directory (optional)" style="padding:6px 8px;border-radius:4px;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font-size:12px;font-family:var(--mono);" />'
        + '<input type="text" id="svc-reg-stop" placeholder="Stop command (optional, e.g. docker compose down)" style="padding:6px 8px;border-radius:4px;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font-size:12px;font-family:var(--mono);" />'
        + '<input type="number" id="svc-reg-tunnel" placeholder="Tunnel port (optional, e.g. 3000) — exposes it via Cloudflare" min="0" style="padding:6px 8px;border-radius:4px;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font-size:12px;font-family:var(--mono);" />'
        + '<div style="display:flex;gap:6px;">'
        + '<button data-action="svc-reg-submit" style="padding:6px 14px;border-radius:4px;border:none;background:var(--accent);color:#fff;cursor:pointer;font-size:12px;">Register</button>'
        + '<button data-action="svc-reg-cancel" style="padding:6px 14px;border-radius:4px;border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-secondary);cursor:pointer;font-size:12px;">Cancel</button>'
        + '</div>';
      // Insert after first element (the toolbar/button area)
      const firstChild = panel.firstElementChild;
      if (firstChild) firstChild.after(form);
      else panel.appendChild(form);
      const nameInput = document.getElementById('svc-reg-name');
      if (nameInput) nameInput.focus();
    }

    async function submitRegisterService() {
      const name = (document.getElementById('svc-reg-name') || {}).value || '';
      const command = (document.getElementById('svc-reg-cmd') || {}).value || '';
      const cwd = (document.getElementById('svc-reg-cwd') || {}).value || '';
      const stopCommand = (document.getElementById('svc-reg-stop') || {}).value || '';
      const tunnelPort = parseInt((document.getElementById('svc-reg-tunnel') || {}).value, 10) || 0;
      if (!name.trim()) { toast('Service name is required', 'error'); return; }
      if (!command.trim()) { toast('Command is required', 'error'); return; }
      try {
        const res = await apiFetch('/api/services', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ alias: currentProject, name: name.trim(), command: command.trim(), cwd: cwd.trim(), stopCommand: stopCommand.trim(), tunnelPort }),
        });
        const data = await res.json();
        if (!data.ok) toast(data.error || 'Failed', 'error');
        else toast('Registered ' + name.trim(), 'success');
        setTimeout(loadServices, 500);
      } catch (err) {
        toast('Error: ' + err.message, 'error');
      }
    }

    function cancelRegisterService() {
      const form = document.querySelector('.svc-register-form');
      if (form) form.remove();
    }

    // ─── Device Profiles ───
    const DEVICE_PROFILES = [
      { name: 'Desktop 1280\\u00d7720', w: 1280, h: 720, dpr: 1, mobile: false, type: 'desktop', ua: '' },
      { name: 'Full HD 1920\\u00d71080', w: 1920, h: 1080, dpr: 1, mobile: false, type: 'desktop', ua: '' },
      { name: 'iPhone SE', w: 375, h: 667, dpr: 2, mobile: true, type: 'apple', ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 13_2_3 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/13.0.3 Mobile/15E148 Safari/604.1' },
      { name: 'iPhone X', w: 375, h: 812, dpr: 3, mobile: true, type: 'apple', ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 13_2_3 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/13.0.3 Mobile/15E148 Safari/604.1' },
      { name: 'iPhone 12 Pro', w: 390, h: 844, dpr: 3, mobile: true, type: 'apple', ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 13_2_3 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/13.0.3 Mobile/15E148 Safari/604.1' },
      { name: 'iPhone 14 Pro', w: 393, h: 852, dpr: 3, mobile: true, type: 'apple', ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/537.51.2 (KHTML, like Gecko) Version/7.0 Mobile/11D257 Safari/9537.53' },
      { name: 'iPhone 15 Pro', w: 393, h: 852, dpr: 3, mobile: true, type: 'apple', ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1' },
      { name: 'iPhone 16 Pro', w: 393, h: 852, dpr: 3, mobile: true, type: 'apple', ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1' },
      { name: 'iPhone 16 Pro Max', w: 430, h: 932, dpr: 3, mobile: true, type: 'apple', ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1' },
      { name: 'iPad Air', w: 820, h: 1180, dpr: 2, mobile: true, type: 'apple', ua: 'Mozilla/5.0 (iPad; CPU OS 13_3 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/87.0.4280.77 Mobile/15E148 Safari/604.1' },
      { name: 'iPad Pro', w: 1024, h: 1366, dpr: 2, mobile: true, type: 'apple', ua: 'Mozilla/5.0 (iPad; CPU OS 11_0 like Mac OS X) AppleWebKit/604.1.34 (KHTML, like Gecko) Version/11.0 Mobile/15A5341f Safari/604.1' },
      { name: 'Pixel 7', w: 412, h: 915, dpr: 2.625, mobile: true, type: 'google', ua: 'Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.120 Mobile Safari/537.36' },
      { name: 'Pixel 9 Pro', w: 430, h: 932, dpr: 2.625, mobile: true, type: 'google', ua: 'Mozilla/5.0 (Linux; Android 15; Pixel 9 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Mobile Safari/537.36' },
      { name: 'Galaxy S24 Ultra', w: 412, h: 915, dpr: 2.625, mobile: true, type: 'samsung', ua: 'Mozilla/5.0 (Linux; Android 14; SM-S928B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36' },
      { name: 'Galaxy Z Fold 5', w: 344, h: 882, dpr: 1, mobile: true, type: 'samsung', ua: 'Mozilla/5.0 (Linux; Android 13; SM-F946B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.5735.131 Mobile Safari/537.36' },
      { name: 'Surface Pro 9', w: 1200, h: 1800, dpr: 2, mobile: true, type: 'microsoft', ua: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36' },
    ];

    // ─── Browsers Panel ───
    async function loadBrowsers() {
      try {
        const alias = currentProject || '';
        const res = await apiFetch('/api/browsers?alias=' + encodeURIComponent(alias));
        const data = await res.json();
        renderBrowsers(data.browsers || []);
      } catch (err) {
        toast('Failed to load browsers: ' + err.message, 'error');
      }
    }

    function renderTerminals() {
      const panel = $('#terminals-panel');
      if (!panel) return;

      // Merge Claude terminals and user terminals for current project
      const claudeTerms = currentProject
        ? terminals.filter(t => t.project === currentProject)
        : terminals;
      const userTerms = currentProject
        ? userTerminals.filter(t => t.alias === currentProject)
        : userTerminals;

      const spawnForm = '<div style="padding:0 0 8px;display:flex;gap:6px;align-items:center;flex-wrap:wrap;">'
        + '<input type="text" id="term-spawn-name" placeholder="Terminal name" style="flex:1;min-width:100px;padding:6px 8px;border-radius:var(--radius-sm);border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font-size:12px;font-family:var(--mono);" />'
        + '<input type="text" id="term-spawn-cmd" placeholder="Command (optional)" style="flex:2;min-width:140px;padding:6px 8px;border-radius:var(--radius-sm);border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font-size:12px;font-family:var(--mono);" />'
        + '<button data-action="term-spawn" style="padding:6px 14px;border-radius:var(--radius-sm);border:1px solid var(--accent);background:var(--accent);color:#fff;cursor:pointer;font-size:12px;white-space:nowrap;">+ Spawn</button>'
        + '</div>';

      let cards = '';

      // Claude terminal cards
      for (const ct of claudeTerms) {
        const statusClass = ct.status === 'running' ? 'running' : 'stopped';
        cards += '<div class="svc-card">'
          + '<div class="svc-header">'
          + '  <span class="svc-name">Claude: ' + escHtml(ct.project) + '</span>'
          + '  <span class="svc-status ' + statusClass + '">' + escHtml(ct.status || 'unknown') + (ct.busy ? ' (busy)' : '') + '</span>'
          + '</div>'
          + '<div class="svc-meta">Claude Code CLI session</div>'
          + '</div>';
      }

      // User terminal cards
      for (const ut of userTerms) {
        const statusClass = ut.status === 'running' ? 'running' : 'stopped';
        const ename = escHtml(ut.name);
        cards += '<div class="svc-card">'
          + '<div class="svc-header">'
          + '  <span class="svc-name">' + ename + '</span>'
          + '  <span class="svc-status ' + statusClass + '">' + escHtml(ut.status) + '</span>'
          + '</div>'
          + (ut.command ? '<div class="svc-meta">' + escHtml(ut.command) + '</div>' : '')
          + '<div class="svc-actions">'
          + '<button data-action="term-view-output" data-name="' + ename + '">Output</button>'
          + (ut.status === 'running' ? '<button class="danger" data-action="term-close-user" data-name="' + ename + '">Close</button>' : '')
          + '</div>'
          + '<div class="svc-logs" id="term-logs-' + ename + '"></div>'
          + '</div>';
      }

      if (!cards) {
        panel.innerHTML = spawnForm + '<div class="services-empty">'
          + '<div class="icon">&gt;_</div>'
          + '<p>No terminals' + (currentProject ? ' for this project' : '') + '</p>'
          + '</div>';
        return;
      }

      panel.innerHTML = spawnForm + cards;
    }

    async function termSpawnUser() {
      const nameEl = document.getElementById('term-spawn-name');
      const cmdEl = document.getElementById('term-spawn-cmd');
      if (!nameEl || !nameEl.value.trim()) { showToast('Enter a terminal name', 'error'); return; }
      try {
        const project = projects.find(p => p.alias === currentProject);
        const r = await apiFetch('/api/terminals/spawn', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ alias: currentProject, name: nameEl.value.trim(), command: cmdEl?.value || undefined, cwd: project?.path }),
        });
        const data = await r.json();
        if (!data.ok) showToast(data.error || 'Failed', 'error');
        else { showToast('Terminal spawned'); nameEl.value = ''; if (cmdEl) cmdEl.value = ''; }
      } catch (e) { showToast(e.message, 'error'); }
    }

    async function termViewOutput(name) {
      const logsEl = document.getElementById('term-logs-' + name);
      if (!logsEl) return;
      if (logsEl.style.display === 'block') { logsEl.style.display = 'none'; return; }
      try {
        const r = await apiFetch('/api/terminals/output?alias=' + encodeURIComponent(currentProject) + '&name=' + encodeURIComponent(name));
        const data = await r.json();
        if (!data.ok) { showToast(data.error, 'error'); return; }
        const lines = data.lines || [];
        logsEl.textContent = lines.join('\\n') || '(no output)';
        logsEl.style.display = 'block';
        logsEl.scrollTop = logsEl.scrollHeight;
      } catch (e) { showToast(e.message, 'error'); }
    }

    async function termCloseUser(name) {
      try {
        const r = await apiFetch('/api/terminals/close', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ alias: currentProject, name }),
        });
        const data = await r.json();
        if (!data.ok) showToast(data.error, 'error');
        else showToast('Terminal closed');
      } catch (e) { showToast(e.message, 'error'); }
    }

    function renderBrowsers(browsers) {
      const panel = $('#browsers-panel');

      // Device selector + open button toolbar
      let deviceOpts = '';
      const groups = { desktop: 'Desktop', apple: 'Apple', google: 'Google', samsung: 'Samsung', microsoft: 'Microsoft' };
      for (const [type, label] of Object.entries(groups)) {
        const devs = DEVICE_PROFILES.filter(d => d.type === type);
        if (devs.length) {
          deviceOpts += '<optgroup label="' + label + '">';
          devs.forEach((d, i) => {
            const idx = DEVICE_PROFILES.indexOf(d);
            deviceOpts += '<option value="' + idx + '">' + escHtml(d.name) + ' (' + d.w + '\\u00d7' + d.h + ')</option>';
          });
          deviceOpts += '</optgroup>';
        }
      }

      const defIdx = DEVICE_PROFILES.findIndex(d => d.name === 'iPhone 16 Pro');
      const toolbar = '<div style="padding:0 0 8px;display:flex;flex-direction:column;gap:6px;">'
        + '<div style="display:flex;gap:6px;align-items:center;flex-wrap:wrap;">'
        + '<select id="browser-device-select" style="flex:1;min-width:140px;padding:6px 8px;border-radius:var(--radius-sm);border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font-size:12px;">'
        + deviceOpts + '</select>'
        + '</div>'
        + '<div style="display:flex;gap:6px;align-items:center;">'
        + '<input type="text" id="browser-new-url" value="http://localhost:3000" placeholder="URL to open" style="flex:1;padding:6px 8px;border-radius:var(--radius-sm);border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);font-size:12px;font-family:var(--mono);" />'
        + '<button data-action="browser-open" style="padding:6px 14px;border-radius:var(--radius-sm);border:1px solid var(--accent);background:var(--accent);color:#fff;cursor:pointer;font-size:12px;white-space:nowrap;">+ Open</button>'
        + '</div>'
        + '</div>';

      if (browsers.length === 0) {
        panel.innerHTML = toolbar + '<div class="services-empty">'
          + '<div class="icon">&#127760;</div>'
          + '<p>No browser instances open</p>'
          + '</div>';
        if (defIdx >= 0) { const sel = $('#browser-device-select'); if (sel) sel.value = String(defIdx); }
        return;
      }

      panel.innerHTML = toolbar + browsers.map(b => {
        const ek = escHtml(b.key);
        return '<div class="svc-card" data-browser-key="' + ek + '">'
          + '<div class="svc-header">'
          + '  <span class="svc-name">' + escHtml(b.name || b.key || 'browser') + '</span>'
          + '  <span class="svc-status running">open</span>'
          + '</div>'
          + '<div class="svc-meta">'
          + '  <div style="display:flex;gap:4px;align-items:center;margin-bottom:4px;">'
          + '    <input type="text" class="browser-url-input" data-bkey="' + ek + '" value="' + escHtml(b.url || '') + '" placeholder="URL" style="flex:1;padding:4px 8px;border-radius:4px;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font-size:11px;font-family:var(--mono);" />'
          + '    <button data-action="browser-navigate" data-key="' + ek + '" style="padding:4px 8px;border-radius:4px;border:1px solid var(--border);background:var(--bg-tertiary);color:var(--text-primary);cursor:pointer;font-size:11px;">Go</button>'
          + '  </div>'
          + '</div>'
          + '<div class="browser-screenshot" id="browser-ss-' + ek + '" style="background:var(--bg-primary);border:1px solid var(--border);border-radius:4px;overflow:hidden;cursor:pointer;" data-action="browser-screenshot" data-key="' + ek + '">'
          + '  <div style="padding:20px;text-align:center;color:var(--text-muted);font-size:11px;">Click to take screenshot</div>'
          + '</div>'
          + '<div class="svc-actions">'
          + '<button data-action="browser-screenshot" data-key="' + ek + '">Screenshot</button>'
          + '<button data-action="browser-close" data-key="' + ek + '">Close</button>'
          + '</div>'
          + '</div>';
      }).join('');

      if (defIdx >= 0) { const sel = $('#browser-device-select'); if (sel) sel.value = String(defIdx); }
    }

    async function showOpenBrowserForm() {
      const urlInput = $('#browser-new-url');
      const url = urlInput ? urlInput.value.trim() : '';
      if (!url) { toast('Enter a URL first', 'error'); return; }
      const name = 'browser-' + Date.now();
      const selIdx = parseInt(($('#browser-device-select') || {}).value || '0');
      const profile = DEVICE_PROFILES[selIdx] || DEVICE_PROFILES[0];
      try {
        const res = await apiFetch('/api/browsers/open', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ alias: currentProject, name, url, width: profile.w, height: profile.h }),
        });
        const data = await res.json();
        if (!data.ok) toast(data.error || 'Failed to open browser', 'error');
        else toast('Browser opened (' + profile.name + ')', 'success');
        setTimeout(loadBrowsers, 500);
      } catch (err) {
        toast('Error: ' + err.message, 'error');
      }
    }

    async function closeBrowserInstance(key) {
      try {
        const res = await apiFetch('/api/browsers/' + encodeURIComponent(key) + '/close', { method: 'POST' });
        const data = await res.json();
        if (!data.ok) toast(data.error || 'Failed', 'error');
        else toast('Browser closed', 'success');
        setTimeout(loadBrowsers, 500);
      } catch (err) {
        toast('Error: ' + err.message, 'error');
      }
    }

    async function browserNavigate(key) {
      const input = document.querySelector('.browser-url-input[data-bkey="' + key + '"]');
      if (!input) return;
      let url = input.value.trim();
      if (!url) return;
      if (!/^https?:\\/\\//i.test(url) && !url.startsWith('about:')) {
        url = (/^localhost(:|$)/i.test(url) ? 'http://' : 'https://') + url;
      }
      try {
        const res = await apiFetch('/api/browsers/' + encodeURIComponent(key) + '/navigate', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ url }),
        });
        const data = await res.json();
        if (!data.ok) toast(data.error || 'Navigate failed', 'error');
      } catch (err) {
        toast('Error: ' + err.message, 'error');
      }
    }

    async function browserScreenshot(key) {
      const container = document.getElementById('browser-ss-' + key);
      if (container) container.innerHTML = '<div style="padding:20px;text-align:center;color:var(--text-muted);font-size:11px;">Loading...</div>';
      try {
        const res = await apiFetch('/api/browsers/' + encodeURIComponent(key) + '/screenshot');
        const data = await res.json();
        if (data.ok && data.data) {
          if (container) container.innerHTML = '<img src="data:image/png;base64,' + data.data + '" style="width:100%;display:block;" />';
        } else {
          if (container) container.innerHTML = '<div style="padding:20px;text-align:center;color:var(--red);font-size:11px;">' + escHtml(data.error || 'Screenshot failed') + '</div>';
        }
      } catch (err) {
        if (container) container.innerHTML = '<div style="padding:20px;text-align:center;color:var(--red);font-size:11px;">Error: ' + escHtml(err.message) + '</div>';
      }
    }

    // ─── User Terminals Panel ───
    async function loadUserTerminals() {
      try {
        const alias = currentProject || '';
        const res = await apiFetch('/api/user-terminals?alias=' + encodeURIComponent(alias));
        const data = await res.json();
        renderUserTerminals(data.terminals || []);
      } catch (err) {
        toast('Failed to load terminals: ' + err.message, 'error');
      }
    }

    function renderUserTerminals(terms) {
      const panel = $('#user-terminals-panel');
      if (terms.length === 0) {
        panel.innerHTML = '<div class="services-empty">'
          + '<div class="icon">&#9002;</div>'
          + '<p>No user terminals open</p>'
          + '<p style="font-size:0.8rem;color:var(--text-muted)">Terminals are spawned via Claude Code MCP tools</p>'
          + '</div>';
        return;
      }
      panel.innerHTML = terms.map(t => {
        const statusClass = t.status === 'running' ? 'running' : 'stopped';
        return '<div class="svc-card">'
          + '<div class="svc-header">'
          + '  <span class="svc-name">' + escHtml(t.name || t.key || 'terminal') + '</span>'
          + '  <span class="svc-status ' + statusClass + '">' + escHtml(t.status || 'unknown') + '</span>'
          + '</div>'
          + (t.cwd ? '<div class="svc-meta">' + escHtml(t.cwd) + '</div>' : '')
          + '</div>';
      }).join('');
    }

    // ─── Info Panel ───
    async function renderInfo() {
      const projectTerminals = terminals.filter(t => t.project === currentProject);
      const projectServices = services.filter(s => s.alias === currentProject);

      // Fetch tunnel info
      let tunnelHtml = '';
      try {
        const tres = await apiFetch('/api/tunnel');
        const tdata = await tres.json();
        const tUrl = tdata.tunnelUrl || '—';
        const tunnels = tdata.tunnels || {};
        const tunnelKeys = Object.keys(tunnels);
        const activeTunnels = tunnelKeys.filter(k => tunnels[k].status === 'active').length;
        tunnelHtml = '<div class="info-section"><h4>Tunnel</h4>'
          + infoRow('URL', tUrl !== '—' ? '<a href="' + escHtml(tUrl) + '" target="_blank">' + escHtml(tUrl) + '</a>' : '—', true)
          + infoRow('Status', activeTunnels > 0 ? 'Connected' : (tunnelKeys.length > 0 ? 'Connecting...' : 'Not configured'))
          + '</div>';
      } catch { /* ignore */ }

      // Project info (if selected)
      let projectHtml = '';
      const p = projects.find(x => x.alias === currentProject);
      if (p) {
        projectHtml = '<div class="info-section"><h4>Project</h4>'
          + infoRow('Alias', p.alias)
          + infoRow('Name', p.name || p.alias)
          + infoRow('Path', p.path || '—')
          + '</div>'
          + '<div class="info-section"><h4>Status</h4>'
          + infoRow('Claude Terminal', projectTerminals.length ? 'Active' : 'Not started')
          + infoRow('Services', projectServices.length + ' registered, '
            + projectServices.filter(s => s.status === 'running').length + ' running')
          + '</div>';
      }

      // Server logs
      let logsHtml = '<div class="info-section"><h4>Server Logs</h4>'
        + '<div id="server-logs-area" style="max-height:300px;overflow-y:auto;background:var(--bg-primary);'
        + 'border:1px solid var(--border);border-radius:var(--radius-sm);padding:8px;'
        + 'font-family:var(--mono);font-size:0.75rem;line-height:1.5;color:var(--text-muted);">'
        + '(loading...)</div></div>';

      $('#info-panel').innerHTML = projectHtml + tunnelHtml + logsHtml;

      // Load server logs
      try {
        const lres = await apiFetch('/api/server-logs');
        const ldata = await lres.json();
        renderServerLogs(ldata.logs || []);
      } catch { /* ignore */ }

      // Subscribe to live server logs via WebSocket
      if (ws && ws.readyState === 1) {
        ws.send(JSON.stringify({ type: 'subscribe-logs' }));
      }
    }

    function renderServerLogs(logs) {
      const area = document.getElementById('server-logs-area');
      if (!area) return;
      area.innerHTML = logs.map(l => {
        const t = new Date(l.time).toLocaleTimeString();
        const cls = l.level === 'error' ? 'color:var(--red)' : (l.level === 'warn' ? 'color:var(--yellow)' : '');
        return '<div style="' + cls + '">[' + escHtml(t) + '] ' + escHtml(l.text) + '</div>';
      }).join('');
      area.scrollTop = area.scrollHeight;
    }

    function appendServerLog(entry) {
      const area = document.getElementById('server-logs-area');
      if (!area) return;
      const t = new Date(entry.time).toLocaleTimeString();
      const cls = entry.level === 'error' ? 'color:var(--red)' : (entry.level === 'warn' ? 'color:var(--yellow)' : '');
      const div = document.createElement('div');
      div.style.cssText = cls;
      div.textContent = '[' + t + '] ' + entry.text;
      area.appendChild(div);
      area.scrollTop = area.scrollHeight;
    }

    function infoRow(label, value, raw) {
      return '<div class="info-row"><span class="label">' + escHtml(label) + '</span>'
        + '<span class="value">' + (raw ? value : escHtml(value)) + '</span></div>';
    }

    // ─── Settings Panel ───
    async function renderSettings() {
      const panel = $('#settings-panel');
      panel.innerHTML = '<div class="info-section"><h4>Loading...</h4></div>';
      try {
        const r = await apiFetch('/api/settings');
        const data = await r.json();
        if (!data.ok) { panel.innerHTML = '<div class="info-section"><h4>Error</h4><p>' + escHtml(data.error) + '</p></div>'; return; }
        const s = data.settings;
        const inputStyle = 'width:100%;padding:8px 10px;border-radius:6px;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font-size:13px;box-sizing:border-box;';
        const monoStyle = inputStyle + 'font-family:var(--mono);';
        const labelStyle = 'display:block;color:var(--text-secondary);font-size:0.78rem;margin-bottom:4px;';
        const hintStyle = 'color:var(--text-muted);font-size:0.7rem;margin-top:2px;';
        const fieldStyle = 'margin-bottom:14px;';

        let html = '<div class="info-section"><h4>Settings</h4>'
          + '<p style="color:var(--text-muted);font-size:0.78rem;margin-bottom:16px;">Changes are saved to <code style="font-family:var(--mono);font-size:0.72rem;background:var(--bg-tertiary);padding:2px 5px;border-radius:3px;">' + escHtml(data.envPath) + '</code></p>'
          + '<div style="' + fieldStyle + '"><label style="' + labelStyle + '">Telegram Bot Token <span style="color:var(--red);">*</span></label>'
          + '<input type="password" id="set-bot-token" style="' + monoStyle + '" value="' + escHtml(s.TELEGRAM_BOT_TOKEN || '') + '" placeholder="123456:ABC-DEF..." />'
          + '<p style="' + hintStyle + '">From @BotFather. Used for login + notifications.</p></div>'
          + '<div style="' + fieldStyle + '"><label style="' + labelStyle + '">Allowed Username <span style="color:var(--red);">*</span></label>'
          + '<input type="text" id="set-username" style="' + inputStyle + '" value="' + escHtml(s.ALLOWED_USERNAME || '') + '" placeholder="your-telegram-username" />'
          + '<p style="' + hintStyle + '">Only this Telegram user can access Crundi.</p></div>'
          + '<div style="' + fieldStyle + '"><label style="' + labelStyle + '">Telegram Chat ID</label>'
          + '<input type="text" id="set-chat-id" style="' + monoStyle + '" value="' + escHtml(data.chatId || '') + '" placeholder="(auto-set when you send /start to the bot)" />'
          + '<p style="' + hintStyle + '">Required for notifications. Auto-discovered when you send /start, or enter manually.</p></div>'
          + '<div style="' + fieldStyle + '"><label style="' + labelStyle + '">Projects Directory</label>'
          + '<input type="text" id="set-projects-dir" style="' + monoStyle + '" value="' + escHtml(s.PROJECTS_DIR || '') + '" placeholder="C:\\\\Projects" />'
          + '<p style="' + hintStyle + '">Single mode: subfolders auto-discovered. Leave empty for multi mode (add projects individually).</p></div>'
          + '<div style="' + fieldStyle + '"><label style="' + labelStyle + '">Web Port</label>'
          + '<input type="text" id="set-web-port" style="' + monoStyle + '" value="' + escHtml(s.WEB_PORT || '') + '" placeholder="0 (auto)" />'
          + '<p style="' + hintStyle + '">Port for the web UI. 0 = auto-assign.</p></div>'
          + '<div style="' + fieldStyle + '"><label style="' + labelStyle + '">Cloudflare Tunnel Token</label>'
          + '<input type="password" id="set-tunnel-token" style="' + monoStyle + '" value="' + escHtml(s.CLOUDFLARE_TUNNEL_TOKEN || '') + '" placeholder="(optional)" />'
          + '<p style="' + hintStyle + '">For a persistent custom domain. Leave empty for random quick tunnel.</p></div>'
          + '<div style="' + fieldStyle + '"><label style="' + labelStyle + '">Cloudflare Tunnel URL</label>'
          + '<input type="text" id="set-tunnel-url" style="' + monoStyle + '" value="' + escHtml(s.CLOUDFLARE_TUNNEL_URL || '') + '" placeholder="https://crundi.example.com" />'
          + '<p style="' + hintStyle + '">Your public tunnel URL (used with named tunnel token above).</p></div>'
          + '<div style="' + fieldStyle + '"><label style="' + labelStyle + '">Data Directory</label>'
          + '<input type="text" id="set-data-dir" style="' + monoStyle + '" value="' + escHtml(s.DATA_DIR || '') + '" placeholder="(default: platform app dir)" />'
          + '<p style="' + hintStyle + '">Override data storage location. Leave empty for default.</p></div>'
          + '<div style="display:flex;gap:8px;align-items:center;margin-top:8px;">'
          + '<button data-action="settings-save" style="padding:8px 20px;border-radius:6px;border:none;background:var(--accent);color:#fff;cursor:pointer;font-size:13px;font-weight:600;">Save</button>'
          + '<span id="settings-status" style="font-size:0.78rem;color:var(--text-muted);"></span>'
          + '</div></div>';

        panel.innerHTML = html;
      } catch (err) {
        panel.innerHTML = '<div class="info-section"><h4>Error</h4><p>' + escHtml(err.message) + '</p></div>';
      }
    }

    async function saveSettings() {
      const settings = {
        TELEGRAM_BOT_TOKEN: ($('#set-bot-token') || {}).value || '',
        ALLOWED_USERNAME: ($('#set-username') || {}).value || '',
        PROJECTS_DIR: ($('#set-projects-dir') || {}).value || '',
        WEB_PORT: ($('#set-web-port') || {}).value || '',
        CLOUDFLARE_TUNNEL_TOKEN: ($('#set-tunnel-token') || {}).value || '',
        CLOUDFLARE_TUNNEL_URL: ($('#set-tunnel-url') || {}).value || '',
        DATA_DIR: ($('#set-data-dir') || {}).value || '',
      };
      if (!settings.TELEGRAM_BOT_TOKEN || !settings.ALLOWED_USERNAME) {
        toast('Bot Token and Allowed Username are required', 'error');
        return;
      }
      const chatId = ($('#set-chat-id') || {}).value || '';
      const status = $('#settings-status');
      if (status) status.textContent = 'Saving...';
      try {
        const r = await apiFetch('/api/settings', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ settings, chatId }),
        });
        const data = await r.json();
        if (data.ok) {
          toast('Settings saved. Restart required for changes to take effect.');
          if (status) status.textContent = 'Saved — restart required';
          if (status) status.style.color = 'var(--yellow)';
        } else {
          toast(data.error || 'Failed to save', 'error');
          if (status) status.textContent = 'Error: ' + (data.error || 'unknown');
        }
      } catch (err) {
        toast('Failed to save: ' + err.message, 'error');
        if (status) status.textContent = 'Error';
      }
    }

    // ─── Event Delegation ───
    document.addEventListener('click', (e) => {
      // Tab clicks
      if (e.target.dataset.tab) { switchTab(e.target.dataset.tab); return; }
      const actionEl = e.target.closest('[data-action]');
      const action = e.target.dataset.action || actionEl?.dataset.action;
      if (!action) return;
      const d = actionEl ? actionEl.dataset : e.target.dataset;
      switch (action) {
        case 'toggle-sidebar': toggleSidebar(); break;
        case 'close-sidebar': closeSidebar(); break;
        case 'add-project': closeSidebar(); showAddModal(); break;
        case 'modal-cancel': hideAddModal(); break;
        case 'modal-add': addProject(); break;
        case 'import-yes': doImport(); break;
        case 'import-skip': $('#import-dialog').classList.remove('visible'); break;
        case 'close-terminal': {
          const project = d.project;
          if (project) { e.stopPropagation(); closeTerminal(project); }
          break;
        }
        case 'remove-project': {
          const project = d.project;
          if (project) { e.stopPropagation(); removeProject(project, d.name); }
          break;
        }
        case 'svc-start': svcAction('start', d.key); break;
        case 'svc-stop': svcAction('stop', d.key); break;
        case 'svc-restart': svcAction('restart', d.key); break;
        case 'svc-logs': svcLogs(d.key); break;
        case 'svc-tunnel': svcTunnel(d.key, d.port); break;
        case 'svc-delete': svcDelete(d.key); break;
        case 'svc-register': showRegisterServiceForm(); break;
        case 'svc-reg-submit': submitRegisterService(); break;
        case 'svc-reg-cancel': cancelRegisterService(); break;
        case 'settings-save': saveSettings(); break;
        case 'term-spawn': termSpawnUser(); break;
        case 'term-view-output': termViewOutput(d.name); break;
        case 'term-close-user': termCloseUser(d.name); break;
        case 'browser-open': showOpenBrowserForm(); break;
        case 'browser-close': closeBrowserInstance(d.key); break;
        case 'browser-navigate': browserNavigate(d.key); break;
        case 'browser-screenshot': browserScreenshot(d.key); break;
        // Git actions
        case 'diff-stage-chunk': stageChunk(parseInt(d.cidx)); break;
        case 'git-stage': gitStage(d.file); break;
        case 'git-unstage': gitUnstage(d.file); break;
        case 'git-stage-all': gitStageAll(); break;
        case 'git-unstage-all': gitUnstageAll(); break;
        case 'git-discard': gitDiscard(d.file); break;
        case 'git-commit': gitCommit(); break;
        case 'git-push': gitPush(); break;
        case 'git-pull': gitPull(); break;
        case 'git-diff': gitViewDiff(d.file, d.cached === '1'); break;
        // Files actions
        case 'files-nav': loadFiles(d.dir); break;
        case 'files-open': feOpen(currentProject, d.file); break;
        case 'files-upload': filesUpload(); break;
        case 'files-download': filesDownload(d.file); break;
        case 'files-copy-path': filesCopyPath(d.file); break;
        case 'term-select': termToggleSelect(); break;
        case 'term-scroll-bottom': if (term) term.scrollToBottom(); break;
        case 'term-send': termSendInput(); break;
        case 'term-attach': termAttachFile(); break;
        case 'term-tool-toggle': termToggleTools(); break;
        case 'term-key': termSendKey(d.key); break;
        case 'launch-claude': launchClaude(d.mode); break;
      }
    });

    // Enter key in modal
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') {
        hideAddModal();
        feClose();
        $('#import-dialog').classList.remove('visible');
      }
      // Ctrl+S in file editor
      if (e.key === 's' && (e.ctrlKey || e.metaKey) && $('#file-editor').classList.contains('visible')) {
        e.preventDefault(); feSave();
      }
      if (e.key === 'Enter' && $('#add-project-modal').classList.contains('visible')) {
        addProject();
      }
      // Enter in browser URL bar
      if (e.key === 'Enter' && e.target.classList.contains('browser-url-input')) {
        browserNavigate(e.target.dataset.bkey);
      }
    });

    // ─── Kanban ───
    const KANBAN_STATUS_LABELS = { backlog: 'Backlog', todo: 'To Do', in_progress: 'In Progress', done: 'Done' };

    async function loadKanban() {
      const panel = $('#kanban-panel');
      if (!currentProject) {
        panel.innerHTML = '<div class="kanban-empty">Select a project to use its Kanban board.</div>';
        return;
      }
      try {
        const inc = kanbanView === 'trash' ? '&includeDeleted=1' : '';
        const res = await apiFetch('/api/kanban?project=' + encodeURIComponent(currentProject) + inc);
        const data = await res.json();
        if (!data.ok) { panel.innerHTML = '<div class="kanban-empty">' + escHtml(data.error || 'Failed to load') + '</div>'; return; }
        kanbanBoard = data.board;
        renderKanban();
      } catch (err) {
        panel.innerHTML = '<div class="kanban-empty">Failed to load board: ' + escHtml(err.message) + '</div>';
      }
    }

    function renderKanban() {
      const panel = $('#kanban-panel');
      const b = kanbanBoard;
      if (!b) return;
      let h = '<div class="kanban-toolbar">'
        + '<button class="kanban-btn primary" data-kact="add-task">+ Add task</button>'
        + '<div class="spacer"></div>'
        + '<button class="kanban-btn ' + (kanbanView === 'board' ? 'toggled' : '') + '" data-kact="view-board">Board</button>'
        + '<button class="kanban-btn ' + (kanbanView === 'trash' ? 'toggled' : '') + '" data-kact="view-trash">Trash (' + b.deletedTasks.length + ')</button>'
        + '<button class="kanban-btn ' + (kanbanView === 'history' ? 'toggled' : '') + '" data-kact="view-history">History</button>'
        + '</div>';
      if (kanbanView === 'history') h += renderKanbanHistory();
      else if (kanbanView === 'trash') h += renderKanbanTrash();
      else h += renderKanbanBoard();
      panel.innerHTML = h;
      if (kanbanView === 'board') attachKanbanDrag();
    }

    function renderKanbanBoard() {
      let h = '<div class="kanban-board">';
      for (const st of kanbanBoard.statuses) {
        const tasks = kanbanBoard.tasks.filter(t => t.status === st);
        h += '<div class="kanban-col" data-status="' + st + '">'
          + '<div class="kanban-col-head"><span>' + KANBAN_STATUS_LABELS[st] + '</span><span class="count">' + tasks.length + '</span></div>'
          + '<div class="kanban-col-body" data-status="' + st + '">'
          + tasks.map(renderKanbanCard).join('')
          + '</div></div>';
      }
      // Safety net: surface any task whose status isn't a known column so it can
      // never silently disappear (move it back via the card's status dropdown).
      const orphans = kanbanBoard.tasks.filter(t => !kanbanBoard.statuses.includes(t.status));
      if (orphans.length) {
        h += '<div class="kanban-col" data-status="backlog" style="border-color:var(--yellow)">'
          + '<div class="kanban-col-head"><span style="color:var(--yellow)">Other</span><span class="count">' + orphans.length + '</span></div>'
          + '<div class="kanban-col-body" data-status="backlog">'
          + orphans.map(renderKanbanCard).join('')
          + '</div></div>';
      }
      return h + '</div>';
    }

    function renderKanbanCard(t) {
      const todos = t.todos || [];
      const done = todos.filter(td => td.done).length;
      let h = '<div class="kanban-card" data-task="' + t.id + '">';
      const mmCount = (t.mindmapNodes || []).length;
      h += '<div class="card-title">' + escHtml(t.title)
        + (mmCount ? ' <span class="card-mm" data-kact="goto-mindmap" title="' + mmCount + ' linked mindmap idea(s)">🧠 ' + mmCount + '</span>' : '')
        + '</div>';
      if (t.description) h += '<div class="card-desc">' + escHtml(t.description) + '</div>';
      if (todos.length) {
        h += '<div class="kanban-todos">';
        for (const td of todos) {
          h += '<div class="kanban-todo">'
            + '<input type="checkbox" data-ktodo-toggle="' + t.id + '|' + td.id + '"' + (td.done ? ' checked' : '') + '>'
            + '<span class="todo-text' + (td.done ? ' done' : '') + '">' + escHtml(td.text) + '</span>'
            + '<button class="todo-del" data-ktodo-del="' + t.id + '|' + td.id + '" title="Delete todo">&times;</button>'
            + '</div>';
        }
        h += '</div><div class="kanban-progress">' + done + '/' + todos.length + ' done</div>';
      }
      h += '<div class="kanban-todo-add"><input type="text" placeholder="+ add todo" data-ktodo-input="' + t.id + '"></div>';
      let opts = '';
      for (const st of kanbanBoard.statuses) {
        opts += '<option value="' + st + '"' + (st === t.status ? ' selected' : '') + '>' + KANBAN_STATUS_LABELS[st] + '</option>';
      }
      h += '<div class="card-actions">'
        + '<select data-kmove="' + t.id + '" title="Move to column">' + opts + '</select>'
        + '<button data-kact="brainstorm" data-task="' + t.id + '" title="Brainstorm this task in the Mindmap">🧠 Brainstorm</button>'
        + '<button data-kact="edit-task" data-task="' + t.id + '">Edit</button>'
        + '<button data-kact="del-task" data-task="' + t.id + '">Delete</button>'
        + '</div></div>';
      return h;
    }

    function renderKanbanTrash() {
      const b = kanbanBoard;
      // Deleted todos live inside (possibly non-deleted) tasks.
      const deletedTodos = [];
      for (const t of b.tasks.concat(b.deletedTasks)) {
        for (const td of (t.todos || [])) {
          if (td.deleted) deletedTodos.push({ task: t, todo: td });
        }
      }
      let h = '<div class="kanban-history">';
      h += '<div class="secrets-section-title">Deleted tasks</div>';
      if (!b.deletedTasks.length) h += '<div class="kanban-empty">No deleted tasks.</div>';
      for (const t of b.deletedTasks) {
        h += '<div class="kanban-card deleted"><div class="card-title">' + escHtml(t.title) + '</div>'
          + '<div class="card-actions"><button data-kact="restore-task" data-task="' + t.id + '">Restore</button></div></div>';
      }
      h += '<div class="secrets-section-title" style="margin-top:16px">Deleted todos</div>';
      if (!deletedTodos.length) h += '<div class="kanban-empty">No deleted todos.</div>';
      for (const { task, todo } of deletedTodos) {
        h += '<div class="hist-row">' + escHtml(todo.text) + ' <span class="ts">in ' + escHtml(task.title) + '</span>'
          + ' <button class="kanban-btn" data-kact="restore-todo" data-task="' + task.id + '" data-todo="' + todo.id + '">Restore</button></div>';
      }
      return h + '</div>';
    }

    function renderKanbanHistory() {
      const hist = (kanbanBoard.history || []).slice().reverse();
      let h = '<div class="kanban-history">';
      if (!hist.length) h += '<div class="kanban-empty">No history yet.</div>';
      for (const e of hist) {
        const t = new Date(e.ts).toLocaleString();
        h += '<div class="hist-row"><span class="ts">' + escHtml(t) + '</span>' + escHtml(e.message) + '</div>';
      }
      return h + '</div>';
    }

    async function kanbanPost(payload) {
      payload.project = currentProject;
      try {
        const res = await apiFetch('/api/kanban', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) });
        const d = await res.json();
        if (!d.ok) toast(d.error || 'Action failed', 'error');
        return d;
      } catch (err) { toast('Action failed: ' + err.message, 'error'); return { ok: false }; }
    }

    // ─── Unified drag controller (mouse drag + mobile long-press) ───
    // h: { onStart?(), onMove(x,y), onEnd(commit) }. Ignores drags that begin on
    // interactive controls (buttons/inputs/etc.) so those keep working.
    function makeDraggable(el, h) {
      const THRESH = 6, HOLD = 320;
      let armed = false, dragging = false, sx = 0, sy = 0, holdTimer = null, clone = null, cdx = 0, cdy = 0;
      const interactive = (t) => t && t.closest && t.closest('button, input, select, textarea, a, label');
      function begin(x, y) {
        const r = el.getBoundingClientRect();
        clone = el.cloneNode(true);
        clone.classList.add('drag-clone');
        Object.assign(clone.style, { position: 'fixed', left: r.left + 'px', top: r.top + 'px', width: r.width + 'px', height: r.height + 'px', margin: '0', pointerEvents: 'none', zIndex: '9999', opacity: '0.92', transform: 'rotate(1.5deg)' });
        cdx = r.left - x; cdy = r.top - y;
        document.body.appendChild(clone);
        dragging = true;
        el.classList.add('dragging');
        document.body.classList.add('dragging-active');
        h.onStart && h.onStart();
        move(x, y);
      }
      function move(x, y) { if (clone) { clone.style.left = (x + cdx) + 'px'; clone.style.top = (y + cdy) + 'px'; } h.onMove && h.onMove(x, y); }
      function finish(commit) {
        if (clone) { clone.remove(); clone = null; }
        el.classList.remove('dragging');
        document.body.classList.remove('dragging-active');
        if (holdTimer) { clearTimeout(holdTimer); holdTimer = null; }
        const was = dragging; dragging = false; armed = false;
        document.removeEventListener('mousemove', onMM); document.removeEventListener('mouseup', onMU);
        if (was) h.onEnd && h.onEnd(commit);
      }
      function onMD(e) { if (e.button !== 0 || interactive(e.target)) return; armed = true; sx = e.clientX; sy = e.clientY; document.addEventListener('mousemove', onMM); document.addEventListener('mouseup', onMU); }
      function onMM(e) { if (dragging) { e.preventDefault(); move(e.clientX, e.clientY); } else if (armed && (Math.abs(e.clientX - sx) > THRESH || Math.abs(e.clientY - sy) > THRESH)) begin(e.clientX, e.clientY); }
      function onMU() { if (dragging) finish(true); else { armed = false; document.removeEventListener('mousemove', onMM); document.removeEventListener('mouseup', onMU); } }
      function onTS(e) { if (e.touches.length !== 1 || interactive(e.target)) return; const t = e.touches[0]; armed = true; sx = t.clientX; sy = t.clientY; holdTimer = setTimeout(() => { holdTimer = null; if (armed) begin(sx, sy); }, HOLD); }
      function onTM(e) { const t = e.touches[0]; if (dragging) { e.preventDefault(); move(t.clientX, t.clientY); } else if (armed && (Math.abs(t.clientX - sx) > 10 || Math.abs(t.clientY - sy) > 10)) { if (holdTimer) { clearTimeout(holdTimer); holdTimer = null; } armed = false; } }
      function onTE(e) { if (dragging) { e.preventDefault(); finish(true); } else { if (holdTimer) { clearTimeout(holdTimer); holdTimer = null; } armed = false; } }
      el.addEventListener('mousedown', onMD);
      el.addEventListener('touchstart', onTS, { passive: true });
      el.addEventListener('touchmove', onTM, { passive: false });
      el.addEventListener('touchend', onTE);
      el.addEventListener('touchcancel', onTE);
    }

    // ─── Kanban drag (reorder within + across columns) ───
    let _kbDropLine = null;
    function kanbanDropIndex(body, y, draggedId) {
      const cards = [...body.querySelectorAll('.kanban-card')].filter(c => c.dataset.task !== draggedId);
      for (let i = 0; i < cards.length; i++) {
        const r = cards[i].getBoundingClientRect();
        if (y < r.top + r.height / 2) return i;
      }
      return cards.length;
    }
    function showKbDropLine(body, idx, draggedId) {
      if (!_kbDropLine) { _kbDropLine = document.createElement('div'); _kbDropLine.className = 'kb-drop-line'; }
      const cards = [...body.querySelectorAll('.kanban-card')].filter(c => c.dataset.task !== draggedId);
      if (idx >= cards.length) body.appendChild(_kbDropLine);
      else body.insertBefore(_kbDropLine, cards[idx]);
    }
    function clearKbDropLine() { if (_kbDropLine && _kbDropLine.parentNode) _kbDropLine.parentNode.removeChild(_kbDropLine); }
    function kanbanDragHandlers(taskId) {
      let target = null;
      return {
        onMove: (x, y) => {
          clearKbDropLine();
          const elAt = document.elementFromPoint(x, y);
          const col = elAt && elAt.closest ? elAt.closest('.kanban-col') : null;
          if (!col) { target = null; return; }
          const body = col.querySelector('.kanban-col-body');
          const idx = kanbanDropIndex(body, y, taskId);
          showKbDropLine(body, idx, taskId);
          target = { status: col.dataset.status, index: idx };
        },
        onEnd: async (commit) => {
          clearKbDropLine();
          if (commit && target) { await kanbanPost({ action: 'moveTask', taskId, status: target.status, index: target.index }); loadKanban(); }
          target = null;
        },
      };
    }
    function attachKanbanDrag() {
      $('#kanban-panel').querySelectorAll('.kanban-card').forEach(card => makeDraggable(card, kanbanDragHandlers(card.dataset.task)));
    }

    // ─── Mindmap drag (reparent: drop on a node = become its child; drop on empty = root) ───
    function clearMmDropTargets() {
      document.querySelectorAll('.mm-node.mm-drop-target, .mm-node.mm-insert-above, .mm-node.mm-insert-below')
        .forEach(n => n.classList.remove('mm-drop-target', 'mm-insert-above', 'mm-insert-below'));
    }
    function mindmapDragHandlers(nodeId) {
      let target = null;
      return {
        onMove: (x, y) => {
          clearMmDropTargets();
          const elAt = document.elementFromPoint(x, y);
          const over = elAt && elAt.closest ? elAt.closest('.mm-node') : null;
          if (!over || over.dataset.node === nodeId) { target = { parentId: null }; return; } // empty → root
          const overId = over.dataset.node;
          const r = over.getBoundingClientRect();
          const rel = (y - r.top) / r.height;
          if (rel < 0.30 || rel > 0.70) {
            // drop above/below a node → reorder as its sibling
            const overNode = mindmapNodes.find(n => n.id === overId);
            const parentId = (overNode && overNode.parentId) || null;
            const sibs = mindmapNodes.filter(n => (n.parentId || null) === parentId && n.id !== nodeId);
            const pos = sibs.findIndex(n => n.id === overId);
            const before = rel < 0.30;
            target = { parentId, index: before ? pos : pos + 1 };
            over.classList.add(before ? 'mm-insert-above' : 'mm-insert-below');
          } else {
            // drop onto a node → make it a child (append)
            over.classList.add('mm-drop-target');
            target = { parentId: overId };
          }
        },
        onEnd: async (commit) => {
          clearMmDropTargets();
          lastMmDrag = Date.now(); // suppress the click that follows a drag
          if (commit && target) { await mindmapPost({ action: 'moveNode', id: nodeId, parentId: target.parentId, index: target.index }); loadMindmap(); }
          target = null;
        },
      };
    }
    function attachMindmapDrag(inner) {
      inner.querySelectorAll('.mm-node:not(.mm-ghost)').forEach(node => makeDraggable(node, mindmapDragHandlers(node.dataset.node)));
    }

    function setupKanbanHandlers() {
      const panel = $('#kanban-panel');
      panel.addEventListener('click', async (e) => {
        const delBtn = e.target.closest('[data-ktodo-del]');
        if (delBtn) {
          const [taskId, todoId] = delBtn.dataset.ktodoDel.split('|');
          await kanbanPost({ action: 'deleteTodo', taskId, todoId }); loadKanban(); return;
        }
        const btn = e.target.closest('[data-kact]');
        if (!btn) return;
        const act = btn.dataset.kact;
        const taskId = btn.dataset.task;
        if (act === 'view-board') { kanbanView = 'board'; loadKanban(); }
        else if (act === 'view-trash') { kanbanView = 'trash'; loadKanban(); }
        else if (act === 'view-history') { kanbanView = 'history'; loadKanban(); }
        else if (act === 'add-task') {
          const title = await askText({ title: 'Add task', label: 'Task title' });
          if (title && title.trim()) { await kanbanPost({ action: 'addTask', title: title.trim() }); loadKanban(); }
        } else if (act === 'edit-task') {
          const task = (kanbanBoard.tasks || []).find(t => t.id === taskId);
          if (!task) return;
          const title = await askText({ title: 'Edit task', label: 'Task title', value: task.title });
          if (title === null) return;
          const description = await askText({ title: 'Edit task', label: 'Description', value: task.description || '', multiline: true });
          if (description === null) return;
          await kanbanPost({ action: 'updateTask', taskId, title: title.trim(), description }); loadKanban();
        } else if (act === 'del-task') {
          await kanbanPost({ action: 'deleteTask', taskId }); loadKanban();
        } else if (act === 'restore-task') {
          await kanbanPost({ action: 'restoreTask', taskId }); loadKanban();
        } else if (act === 'restore-todo') {
          await kanbanPost({ action: 'restoreTodo', taskId, todoId: btn.dataset.todo }); loadKanban();
        } else if (act === 'brainstorm') {
          const task = (kanbanBoard.tasks || []).find(t => t.id === taskId);
          if (!task) return;
          const r = await mindmapPost({ action: 'addNode', text: task.title, project: currentProject, taskId });
          if (r.ok && r.node) {
            // Bring the task's todos (subtasks) over as child idea nodes.
            const todos = (task.todos || []).filter(td => !td.deleted);
            for (const td of todos) {
              await mindmapPost({ action: 'addNode', text: td.text, parentId: r.node.id, project: currentProject, taskId, todoId: td.id });
            }
            toast('Added to Mindmap' + (todos.length ? ' with ' + todos.length + ' subtask(s)' : ''), 'success');
            document.querySelector('.tab-btn[data-tab="mindmap"]').click();
          }
        } else if (act === 'goto-mindmap') {
          document.querySelector('.tab-btn[data-tab="mindmap"]').click();
        }
      });
      panel.addEventListener('change', async (e) => {
        const cb = e.target.closest('[data-ktodo-toggle]');
        if (cb) {
          const [taskId, todoId] = cb.dataset.ktodoToggle.split('|');
          await kanbanPost({ action: 'updateTodo', taskId, todoId, done: cb.checked }); loadKanban();
          return;
        }
        const sel = e.target.closest('[data-kmove]');
        if (sel) {
          await kanbanPost({ action: 'moveTask', taskId: sel.dataset.kmove, status: sel.value }); loadKanban();
        }
      });
      panel.addEventListener('keydown', async (e) => {
        const inp = e.target.closest('[data-ktodo-input]');
        if (!inp || e.key !== 'Enter') return;
        e.preventDefault();
        const text = inp.value.trim();
        if (!text) return;
        await kanbanPost({ action: 'addTodo', taskId: inp.dataset.ktodoInput, text }); loadKanban();
      });
    }

    // ─── Secrets ───
    async function loadSecrets() {
      try {
        const res = await apiFetch('/api/secrets');
        const d = await res.json();
        if (d.ok) { secretsList = d.secrets || []; secretRequests = d.requests || []; }
        updateSecretBadge();
        renderSecrets();
      } catch (err) {
        $('#secrets-panel').innerHTML = '<div class="secrets-empty">Failed to load: ' + escHtml(err.message) + '</div>';
      }
    }

    function updateSecretBadge() {
      const el = $('#secret-badge');
      if (!el) return;
      if (secretRequests.length) { el.style.display = ''; el.textContent = String(secretRequests.length); }
      else el.style.display = 'none';
    }

    function renderSecrets() {
      const panel = $('#secrets-panel');
      let h = '';
      if (secretRequests.length) {
        h += '<div class="secrets-requests"><div class="secrets-section-title">Pending access requests</div>';
        for (const r of secretRequests) {
          h += '<div class="secret-request"><div class="req-info">'
            + '<div class="req-name">' + escHtml(r.secretName) + '</div>'
            + '<div class="req-meta">' + (r.project ? 'project: ' + escHtml(r.project) + ' · ' : '') + (r.reason ? escHtml(r.reason) : 'Claude requested access') + '</div>'
            + '</div><div class="sec-actions">'
            + '<button class="kanban-btn primary" data-sreq-approve="' + r.id + '" data-sname="' + escHtml(r.secretName) + '">Approve</button>'
            + '<button class="kanban-btn" data-sreq-deny="' + r.id + '">Deny</button>'
            + '</div></div>';
        }
        h += '</div>';
      }
      h += '<div class="secrets-add">'
        + '<div class="secrets-section-title">Add a secret</div>'
        + '<input id="sec-add-name" placeholder="Name (e.g. AWS Access Key)" autocomplete="off">'
        + '<textarea id="sec-add-desc" placeholder="Description — Claude searches names &amp; descriptions"></textarea>'
        + '<textarea id="sec-add-value" placeholder="Secret value"></textarea>'
        + '<input id="sec-add-pin" inputmode="numeric" maxlength="6" placeholder="6-digit PIN to encrypt this secret" autocomplete="off">'
        + '<div class="hint">The PIN is never stored. If you lose it, this secret can only be deleted &amp; recreated.</div>'
        + '<button class="kanban-btn primary" data-sact="add" style="align-self:flex-start">Save secret</button>'
        + '</div>';
      h += '<div><div class="secrets-section-title">Secrets (' + secretsList.length + ')</div><div class="secret-list">';
      if (!secretsList.length) h += '<div class="secrets-empty">No secrets yet.</div>';
      for (const s of secretsList) {
        const revealed = revealedSecrets[s.id];
        const isRevealed = revealed !== undefined;
        h += '<div class="secret-item"><div class="sec-info">'
          + '<div class="sec-name">' + escHtml(s.name) + '</div>'
          + (s.description ? '<div class="sec-desc">' + escHtml(s.description) + '</div>' : '')
          + (isRevealed ? '<div class="sec-value">' + escHtml(revealed) + '</div>' : '')
          + '</div><div class="sec-actions">'
          + (isRevealed
              ? '<button data-sact="copy" data-sid="' + s.id + '">Copy</button><button data-sact="hide" data-sid="' + s.id + '">Hide</button>'
              : '<button data-sact="reveal" data-sid="' + s.id + '" data-sname="' + escHtml(s.name) + '">Reveal</button>')
          + '<button class="danger" data-sact="delete" data-sid="' + s.id + '" data-sname="' + escHtml(s.name) + '">Delete</button>'
          + '</div></div>';
      }
      h += '</div></div>';
      panel.innerHTML = h;
    }

    async function secretsPost(payload) {
      const res = await apiFetch('/api/secrets', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) });
      return res.json();
    }

    function setupSecretsHandlers() {
      const panel = $('#secrets-panel');
      panel.addEventListener('click', async (e) => {
        const approveBtn = e.target.closest('[data-sreq-approve]');
        if (approveBtn) {
          const reqId = approveBtn.dataset.sreqApprove;
          openPinModal({
            title: 'Approve access',
            subtitle: 'Enter the PIN for "' + approveBtn.dataset.sname + '" to release it to Claude.',
            submitLabel: 'Approve',
            onSubmit: async (pin) => {
              const d = await secretsPost({ action: 'approve', reqId, pin });
              if (d.ok) { toast('Approved', 'success'); loadSecrets(); }
              return d;
            },
          });
          return;
        }
        const denyBtn = e.target.closest('[data-sreq-deny]');
        if (denyBtn) {
          await secretsPost({ action: 'deny', reqId: denyBtn.dataset.sreqDeny });
          loadSecrets();
          return;
        }
        const btn = e.target.closest('[data-sact]');
        if (!btn) return;
        const act = btn.dataset.sact;
        const sid = btn.dataset.sid;
        if (act === 'add') {
          const name = $('#sec-add-name').value.trim();
          const description = $('#sec-add-desc').value;
          const value = $('#sec-add-value').value;
          const pin = $('#sec-add-pin').value.trim();
          if (!name) { toast('Name is required', 'error'); return; }
          if (!value) { toast('Value is required', 'error'); return; }
          if (!/^\\d{6}$/.test(pin)) { toast('PIN must be exactly 6 digits', 'error'); return; }
          const d = await secretsPost({ action: 'add', name, description, value, pin });
          if (d.ok) { toast('Secret saved', 'success'); loadSecrets(); }
          else toast(d.error || 'Failed to save', 'error');
        } else if (act === 'reveal') {
          openPinModal({
            title: 'Reveal secret',
            subtitle: 'Enter the PIN for "' + btn.dataset.sname + '".',
            submitLabel: 'Reveal',
            onSubmit: async (pin) => {
              const d = await secretsPost({ action: 'reveal', id: sid, pin });
              if (d.ok) { revealedSecrets[sid] = d.value; renderSecrets(); }
              return d;
            },
          });
        } else if (act === 'hide') {
          delete revealedSecrets[sid]; renderSecrets();
        } else if (act === 'copy') {
          try { await navigator.clipboard.writeText(revealedSecrets[sid] || ''); toast('Copied', 'success'); }
          catch { toast('Copy failed', 'error'); }
        } else if (act === 'delete') {
          if (!confirm('Delete secret "' + btn.dataset.sname + '"? This cannot be undone.')) return;
          const d = await secretsPost({ action: 'delete', id: sid });
          if (d.ok) { delete revealedSecrets[sid]; toast('Deleted', 'success'); loadSecrets(); }
          else toast(d.error || 'Failed to delete', 'error');
        }
      });
    }

    // ─── Mindmap (global) ───
    const MM_COL_W = 235, MM_ROW_H = 90, MM_PAD = 24, MM_NODE_W = 180;

    async function loadMindmap() {
      try {
        const res = await apiFetch('/api/mindmap');
        const d = await res.json();
        mindmapNodes = (d.ok && d.mindmap && d.mindmap.nodes) ? d.mindmap.nodes : [];
        mmAnimate = true; // a fresh load → play the entrance animation
        renderMindmap();
      } catch (err) {
        $('#mindmap-panel').innerHTML = '<div class="kanban-empty">Failed to load mindmap: ' + escHtml(err.message) + '</div>';
      }
    }

    async function mindmapPost(payload) {
      try {
        const res = await apiFetch('/api/mindmap', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload) });
        const d = await res.json();
        if (!d.ok) toast(d.error || 'Action failed', 'error');
        return d;
      } catch (err) { toast('Action failed: ' + err.message, 'error'); return { ok: false }; }
    }

    function renderMindmap() {
      const anim = mmAnimate; mmAnimate = false; // entrance plays once per fresh load
      const panel = $('#mindmap-panel');
      let h = '<div class="kanban-toolbar">'
        + '<button class="kanban-btn primary" data-mact="add-root">+ Add idea</button>'
        + '<input type="text" class="mm-search" id="mm-search" placeholder="Filter ideas…" autocomplete="off" value="' + escHtml(mindmapSearch) + '">'
        + '<div class="spacer"></div>'
        + '<span style="font-size:0.74rem;color:var(--text-muted)">Tap a node for details · Ctrl+scroll / pinch to zoom · green = linked</span>'
        + '</div>';
      if (!mindmapNodes.length) {
        h += '<div class="kanban-empty">No ideas yet. Add one, or hit “Brainstorm” on a Kanban card to extend a task here.</div>';
        panel.innerHTML = h;
        return;
      }
      h += renderMindmapProjectBar(anim);
      h += '<div class="mindmap-canvas' + (anim ? ' mm-anim' : '') + '" id="mindmap-canvas"><div class="mindmap-inner" id="mindmap-inner"><div class="mm-scale" id="mm-scale"></div></div></div>';
      panel.innerHTML = h;
      layoutMindmap($('#mm-scale'));
      const search = $('#mm-search');
      if (search) {
        search.addEventListener('input', () => setMindmapSearch(search.value));
        if (mindmapSearch) { search.focus(); search.setSelectionRange(search.value.length, search.value.length); }
      }
    }

    // Project filter chips. All projects shown by default; toggle any off to
    // hide its nodes. Shows a "filtered" indicator when a filter is active.
    function renderMindmapProjectBar(anim) {
      const keys = [...new Set(mindmapNodes.map(mmNodeProjectKey))];
      // Show the filter when there's anything to filter by: 2+ groups, or even a
      // single real project (so it's discoverable). Hide only when everything is
      // General (one '' group → no projects to filter).
      const realKeys = keys.filter(k => k !== '');
      if (realKeys.length === 0) return '';
      keys.sort((a, b) => (a === '' ? 1 : b === '' ? -1 : a.localeCompare(b))); // Unlinked last
      const projName = (k) => k === '' ? 'General' : ((projects.find(p => p.alias === k) || {}).name || k);
      const hiddenCount = keys.filter(k => mindmapHiddenProjects.has(k)).length;
      let chips = keys.map(k => {
        const off = mindmapHiddenProjects.has(k);
        return '<button class="mm-proj-chip' + (off ? ' off' : '') + '" data-mmproj="' + escHtml(k) + '">'
          + (off ? '' : '<span class="dot"></span>') + escHtml(projName(k)) + '</button>';
      }).join('');
      const indicator = hiddenCount
        ? '<span class="mm-proj-flag">🔽 filtered (' + hiddenCount + ' hidden)</span>'
          + '<button class="mm-proj-chip reset" data-mmproj-all="1">Show all</button>'
        : '';
      return '<div class="mm-proj-bar' + (anim ? ' mm-anim' : '') + '"><span class="mm-proj-label">Projects:</span>' + chips + indicator + '</div>';
    }

    function layoutMindmap(inner) {
      // Responsive spacing: on touch/narrow screens node actions are always
      // shown (no hover), so rows need more vertical room to avoid overlap.
      const isMobile = window.innerWidth <= 768;
      const COL_W = isMobile ? 200 : MM_COL_W;
      const NODE_W = isMobile ? 168 : MM_NODE_W;
      const GAP = isMobile ? 22 : 16;   // vertical gap between stacked nodes
      const MIN_H = isMobile ? 60 : 48; // fallback height before measuring
      // Work only with nodes whose project (and every ancestor's project) is
      // enabled — hiding a project hides its whole subtree, no orphaned children.
      const nodes = mindmapNodes.filter(mmNodePassesProject);
      const byId = {}; nodes.forEach(n => { byId[n.id] = n; });
      const children = {};
      const roots = [];
      for (const n of nodes) {
        const hasParent = n.parentId && byId[n.parentId];
        if (hasParent) { (children[n.parentId] = children[n.parentId] || []).push(n); }
        else roots.push(n);
      }
      // A node is visible only if no ancestor is collapsed.
      const isVisible = (n) => {
        let p = n.parentId && byId[n.parentId];
        while (p) { if (mindmapCollapsed[p.id]) return false; p = p.parentId && byId[p.parentId]; }
        return true;
      };
      const depthOf = (n) => {
        let d = 0, p = n;
        while (p.parentId && byId[p.parentId]) { d++; p = byId[p.parentId]; }
        return d;
      };

      const px = (x) => MM_PAD + x * COL_W;

      // --- pass 1: build compact markup for VISIBLE nodes (left from depth;
      // top deferred until we've measured real heights). ---
      const trunc = (s, m) => { s = String(s || ''); return s.length > m ? s.slice(0, m - 1) + '…' : s; };
      const pos = {}; // id → {x(depth), y(center-px)}
      let maxDepth = 0;
      const visNodes = nodes.filter(isVisible);
      visNodes.forEach(n => { pos[n.id] = { x: depthOf(n), y: 0 }; maxDepth = Math.max(maxDepth, pos[n.id].x); });

      // A persistent dashed "add idea" ghost under every visible, expanded node —
      // one tap adds a child linked/scoped to it. Appended after real children.
      const phantomFor = {};
      const phantoms = [];
      for (const n of visNodes) {
        if (mindmapCollapsed[n.id]) continue;
        const ph = { id: '__add__' + n.id, parentId: n.id, ghost: true };
        phantomFor[n.id] = ph; phantoms.push(ph);
        pos[ph.id] = { x: pos[n.id].x + 1, y: 0 };
        maxDepth = Math.max(maxDepth, pos[ph.id].x);
      }
      const hasRealKids = (id) => !mindmapCollapsed[id] && children[id] && children[id].length;
      // For packing: a parent's ghost appends after its real children (drives a
      // row below them); a leaf's ghost sits BESIDE it (placed later) so the
      // leaf still reserves its own height and siblings never overlap.
      const kidsOf = (node) => {
        const real = hasRealKids(node.id) ? children[node.id] : [];
        return (phantomFor[node.id] && real.length) ? real.concat(phantomFor[node.id]) : real;
      };

      const ghostHtml = (ph) => '<div class="mm-node mm-ghost" style="left:' + px(pos[ph.id].x) + 'px" data-node="' + ph.id + '" data-addparent="' + ph.parentId + '" title="Add a child idea here">'
        + '<span class="mm-ghost-plus">+</span><span class="mm-ghost-label">add idea</span></div>';

      const nodeHtml = (n) => {
        const isRoot = !(n.parentId && byId[n.parentId]);
        const info = n.linkedTaskInfo;
        const linked = !!(n.linkedTask);
        const hasKids = !!(children[n.id] && children[n.id].length);
        const collapsed = !!mindmapCollapsed[n.id];
        let chip = '';
        if (linked) {
          if (info && !info.missing) {
            if (info.kind === 'todo') chip = '<span class="mm-chip link">🔗 ' + (info.done ? '☑' : '☐') + ' ' + escHtml(trunc(info.text || info.taskTitle || 'subtask', 20)) + '</span>';
            else chip = '<span class="mm-chip link">🔗 ' + escHtml(trunc(info.title || info.project || 'task', 20)) + '</span>';
          } else {
            chip = '<span class="mm-chip link missing">🔗 removed</span>';
          }
        } else if (n.project) {
          // scoped to a project without a task link
          const nm = (projects.find(p => p.alias === n.project) || {}).name || n.project;
          chip = '<span class="mm-chip scope">📁 ' + escHtml(trunc(nm, 18)) + '</span>';
        } else if (n.effectiveProject) {
          // in a project's scope via an ancestor (no link/scope of its own)
          const nm = (projects.find(p => p.alias === n.effectiveProject) || {}).name || n.effectiveProject;
          chip = '<span class="mm-chip inherited" title="In ' + escHtml(nm) + ' (inherited from parent)">↳ ' + escHtml(trunc(nm, 16)) + '</span>';
        }
        const nNotes = (n.notes && n.notes.length) || 0;
        const noteChip = nNotes ? '<span class="mm-chip note">📝 ' + nNotes + (nNotes === 1 ? ' note' : ' notes') + '</span>' : '';
        return '<div class="mm-node' + (isRoot ? ' root' : '') + (linked ? ' linked' : (n.project ? ' scoped' : '')) + '" style="left:' + px(pos[n.id].x) + 'px" data-node="' + n.id + '">'
          + '<div class="mm-text">' + escHtml(n.text) + '</div>'
          + '<div class="mm-meta">' + chip + noteChip + '</div>'
          + (hasKids ? '<button class="mm-collapse" data-mact="toggle" data-node="' + n.id + '" title="Collapse/expand">' + (collapsed ? '+' : '−') + '</button>' : '')
          + '</div>';
      };
      inner.innerHTML = visNodes.map(nodeHtml).join('') + phantoms.map(ghostHtml).join('');

      // measure rendered heights (real nodes + ghosts)
      const nodeEls = {};
      inner.querySelectorAll('.mm-node').forEach(el => { nodeEls[el.dataset.node] = el; });
      const heights = {};
      const allNodes = visNodes.concat(phantoms);
      allNodes.forEach(n => { const el = nodeEls[n.id]; heights[n.id] = (el && el.offsetHeight) || MIN_H; });

      // --- pass 2: pack vertical centers using real heights (post-order) ---
      let cursor = MM_PAD;
      const placeY = (node) => {
        const kids = kidsOf(node);
        if (kids.length) {
          const cs = kids.map(placeY);
          pos[node.id].y = (cs[0] + cs[cs.length - 1]) / 2;
        } else {
          pos[node.id].y = cursor + heights[node.id] / 2;
          cursor += heights[node.id] + GAP;
        }
        return pos[node.id].y;
      };
      roots.forEach(placeY);
      // Leaf ghosts weren't packed (so the leaf reserves its own height) — sit
      // them beside their parent at the same row.
      for (const n of visNodes) {
        const ph = phantomFor[n.id];
        if (ph && !hasRealKids(n.id)) pos[ph.id].y = pos[n.id].y;
      }
      const contentH = Math.max(cursor + MM_PAD, 120);
      const width = MM_PAD + maxDepth * COL_W + NODE_W + MM_PAD;

      // apply vertical positions
      allNodes.forEach(n => { if (nodeEls[n.id]) nodeEls[n.id].style.top = pos[n.id].y + 'px'; });

      // edges: solid to real children, dashed to the "add idea" ghosts
      const edge = (n, k, dash) => {
        const x1 = px(pos[n.id].x) + NODE_W, y1 = pos[n.id].y;
        const x2 = px(pos[k.id].x), y2 = pos[k.id].y;
        const mid = (x1 + x2) / 2;
        return '<path d="M' + x1 + ',' + y1 + ' C' + mid + ',' + y1 + ' ' + mid + ',' + y2 + ' ' + x2 + ',' + y2 + '" fill="none" stroke="var(--border)" stroke-width="2"' + dash + '/>';
      };
      let paths = '';
      for (const n of visNodes) {
        if (hasRealKids(n.id)) for (const k of children[n.id]) { if (pos[k.id]) paths += edge(n, k, ''); }
      }
      for (const ph of phantoms) {
        const n = byId[ph.parentId];
        if (n && pos[n.id] && pos[ph.id]) paths += edge(n, ph, ' stroke-dasharray="5 5" opacity="0.45"');
      }
      inner.insertAdjacentHTML('afterbegin', '<svg class="mindmap-edges" width="' + width + '" height="' + contentH + '">' + paths + '</svg>');

      mindmapBaseW = width; mindmapBaseH = contentH;
      inner.style.width = width + 'px';
      inner.style.height = contentH + 'px';

      // initial zoom: fit everything vertically on first render
      const canvas = $('#mindmap-canvas');
      if (!mindmapZoomInit && canvas && canvas.clientHeight > 0) {
        const avail = canvas.clientHeight - 16;
        mindmapZoom = (contentH > avail) ? Math.max(0.25, avail / contentH) : 1;
        mindmapZoomInit = true;
      }
      applyMindmapZoom();
      attachMindmapDrag(inner);
      attachMindmapZoom(canvas, inner);
      applyMindmapSearch();
    }

    // Apply current zoom: scale the content layer, size the scroll wrapper to match.
    function applyMindmapZoom() {
      const scale = $('#mm-scale'), inner = $('#mindmap-inner');
      if (!scale || !inner) return;
      scale.style.transform = 'scale(' + mindmapZoom + ')';
      inner.style.width = (mindmapBaseW * mindmapZoom) + 'px';
      inner.style.height = (mindmapBaseH * mindmapZoom) + 'px';
    }

    function setMindmapZoom(z, cx, cy) {
      const canvas = $('#mindmap-canvas');
      const prev = mindmapZoom;
      mindmapZoom = Math.min(3, Math.max(0.15, z));
      if (canvas && cx != null) {
        // keep the point under the cursor stationary
        const rect = canvas.getBoundingClientRect();
        const ox = cx - rect.left + canvas.scrollLeft;
        const oy = cy - rect.top + canvas.scrollTop;
        const ratio = mindmapZoom / prev;
        applyMindmapZoom();
        canvas.scrollLeft = ox * ratio - (cx - rect.left);
        canvas.scrollTop = oy * ratio - (cy - rect.top);
      } else {
        applyMindmapZoom();
      }
    }

    let _mmZoomBound = null;
    function attachMindmapZoom(canvas, inner) {
      if (!canvas || canvas === _mmZoomBound) return;
      _mmZoomBound = canvas;
      canvas.addEventListener('wheel', (e) => {
        if (!e.ctrlKey) return;          // Ctrl+scroll to zoom
        e.preventDefault();
        const factor = e.deltaY < 0 ? 1.12 : 1 / 1.12;
        setMindmapZoom(mindmapZoom * factor, e.clientX, e.clientY);
      }, { passive: false });
      // pinch zoom
      let pinchStart = 0, pinchZoom0 = 1;
      const dist = (t) => Math.hypot(t[0].clientX - t[1].clientX, t[0].clientY - t[1].clientY);
      canvas.addEventListener('touchstart', (e) => {
        if (e.touches.length === 2) { pinchStart = dist(e.touches); pinchZoom0 = mindmapZoom; }
      }, { passive: true });
      canvas.addEventListener('touchmove', (e) => {
        if (e.touches.length === 2 && pinchStart) {
          e.preventDefault();
          const mx = (e.touches[0].clientX + e.touches[1].clientX) / 2;
          const my = (e.touches[0].clientY + e.touches[1].clientY) / 2;
          setMindmapZoom(pinchZoom0 * (dist(e.touches) / pinchStart), mx, my);
        }
      }, { passive: false });
      canvas.addEventListener('touchend', (e) => { if (e.touches.length < 2) pinchStart = 0; });
    }

    // ─── Mindmap search/filter (highlight matches, dim the rest) ───
    function setMindmapSearch(q) {
      const had = !!mindmapSearch;
      mindmapSearch = q || '';
      const has = !!mindmapSearch.trim();
      if (has && !had) {
        // expand everything so matches in collapsed subtrees become visible
        mmSearchSavedCollapsed = { ...mindmapCollapsed };
        for (const k of Object.keys(mindmapCollapsed)) delete mindmapCollapsed[k];
        renderMindmap();
      } else if (!has && had) {
        if (mmSearchSavedCollapsed) { Object.assign(mindmapCollapsed, mmSearchSavedCollapsed); mmSearchSavedCollapsed = null; }
        renderMindmap();
      } else {
        applyMindmapSearch();
      }
    }
    function nodeMatches(n, q) {
      if ((n.text || '').toLowerCase().includes(q)) return true;
      if ((n.notes || []).join(' ').toLowerCase().includes(q)) return true;
      const i = n.linkedTaskInfo;
      if (i && ((i.title || '') + ' ' + (i.text || '') + ' ' + (i.taskTitle || '') + ' ' + (i.project || '')).toLowerCase().includes(q)) return true;
      return false;
    }
    function applyMindmapSearch() {
      const inner = $('#mm-scale'); if (!inner) return;
      const q = mindmapSearch.trim().toLowerCase();
      inner.querySelectorAll('.mm-node:not(.mm-ghost)').forEach(el => {
        if (!q) { el.classList.remove('mm-dim', 'mm-hit'); return; }
        const n = mindmapNodes.find(x => x.id === el.dataset.node);
        const hit = n && nodeMatches(n, q);
        el.classList.toggle('mm-hit', !!hit);
        el.classList.toggle('mm-dim', !hit);
      });
      // hide the "add idea" ghosts while filtering, to keep results clean
      inner.querySelectorAll('.mm-ghost').forEach(el => { el.style.display = q ? 'none' : ''; });
    }

    // ─── Link picker (browse / search / filter tasks & subtasks) ───
    let linkPickNodeId = null;
    let linkPickBoard = null;
    function openLinkPicker(nodeId) {
      linkPickNodeId = nodeId;
      const node = mindmapNodes.find(n => n.id === nodeId);
      const parent = node && node.parentId ? mindmapNodes.find(n => n.id === node.parentId) : null;
      const locked = parent ? (parent.effectiveProject || '') : ''; // child strictly inherits parent's project
      const sel = $('#lm-project');
      let opts = (projects || []).map(p => '<option value="' + escHtml(p.alias) + '">' + escHtml(p.name || p.alias) + '</option>');
      if (locked && !(projects || []).some(p => p.alias === locked)) opts.unshift('<option value="' + escHtml(locked) + '">' + escHtml(locked) + '</option>');
      sel.innerHTML = opts.join('');
      const note = $('#lm-locked');
      if (locked) {
        sel.value = locked; sel.disabled = true;
        const nm = (projects.find(p => p.alias === locked) || {}).name || locked;
        note.textContent = '🔒 Scoped to “' + nm + '” (inherited from parent) — can only link tasks in this project.';
        note.style.display = '';
      } else {
        sel.disabled = false;
        if (currentProject) sel.value = currentProject;
        note.style.display = 'none';
      }
      $('#lm-search').value = '';
      updateScopeBtn();
      $('#link-modal').classList.add('visible');
      loadLinkBoard();
      setTimeout(() => $('#lm-search').focus(), 50);
    }
    function updateScopeBtn() {
      const sel = $('#lm-project'), btn = $('#lm-scope-project');
      if (!sel || !btn) return;
      const nm = (projects.find(p => p.alias === sel.value) || {}).name || sel.value;
      btn.textContent = '📁 Scope to “' + nm + '” (no task)';
    }
    async function loadLinkBoard() {
      updateScopeBtn();
      const proj = $('#lm-project').value;
      $('#lm-list').innerHTML = '<div class="lm-empty">Loading…</div>';
      try {
        const res = await apiFetch('/api/kanban?project=' + encodeURIComponent(proj));
        const d = await res.json();
        linkPickBoard = (d.ok && d.board) ? d.board : { tasks: [] };
      } catch { linkPickBoard = { tasks: [] }; }
      renderLinkList();
    }
    function renderLinkList() {
      const proj = $('#lm-project').value;
      const q = $('#lm-search').value.trim().toLowerCase();
      const tasks = (linkPickBoard && linkPickBoard.tasks) || [];
      let h = '';
      for (const t of tasks) {
        const tMatch = !q || (t.title || '').toLowerCase().includes(q);
        const todos = (t.todos || []).filter(td => !td.deleted);
        const matchTodos = todos.filter(td => !q || (td.text || '').toLowerCase().includes(q));
        if (!tMatch && matchTodos.length === 0) continue;
        h += '<div class="lm-item" data-project="' + escHtml(proj) + '" data-task="' + t.id + '">' + escHtml(t.title) + ' <span class="st">[' + (t.status || '').replace('_', ' ') + ']</span></div>';
        for (const td of (q ? matchTodos : todos)) {
          h += '<div class="lm-item todo" data-project="' + escHtml(proj) + '" data-task="' + t.id + '" data-todo="' + td.id + '">' + (td.done ? '☑' : '☐') + ' ' + escHtml(td.text) + '</div>';
        }
      }
      $('#lm-list').innerHTML = h || '<div class="lm-empty">No matching tasks or subtasks.</div>';
    }

    function setupMindmapHandlers() {
      const panel = $('#mindmap-panel');
      panel.addEventListener('click', async (e) => {
        // project filter chips
        const projBtn = e.target.closest('[data-mmproj]');
        if (projBtn) {
          const k = projBtn.dataset.mmproj;
          if (mindmapHiddenProjects.has(k)) mindmapHiddenProjects.delete(k); else mindmapHiddenProjects.add(k);
          renderMindmap();
          return;
        }
        if (e.target.closest('[data-mmproj-all]')) { mindmapHiddenProjects.clear(); renderMindmap(); return; }
        // dashed "add idea" ghost → add a child to its parent
        const ghost = e.target.closest('.mm-ghost');
        if (ghost) {
          const pid = ghost.dataset.addparent;
          const text = await askText({ title: 'Add idea', label: 'New idea' });
          if (text && text.trim()) { await mindmapPost({ action: 'addNode', text: text.trim(), parentId: pid }); loadMindmap(); }
          return;
        }
        const btn = e.target.closest('[data-mact]');
        if (btn) {
          const act = btn.dataset.mact;
          const id = btn.dataset.node;
          if (act === 'add-root') {
            const text = await askText({ title: 'Add idea', label: 'New idea' });
            if (text && text.trim()) { await mindmapPost({ action: 'addNode', text: text.trim() }); loadMindmap(); }
          } else if (act === 'toggle') {
            mindmapCollapsed[id] = !mindmapCollapsed[id];
            renderMindmap();
          }
          return;
        }
        // Click on a node card → open its detail modal (ignore the click that
        // immediately follows a drag-reorder).
        const nodeEl = e.target.closest('.mm-node');
        if (nodeEl && Date.now() - lastMmDrag > 300) openMmDetail(nodeEl.dataset.node);
      });
      setupMmDetailModal();
    }

    // Action helpers shared by the detail modal.
    async function mmAddChild(id) {
      const text = await askText({ title: 'Add child idea', label: 'Child idea' });
      if (text && text.trim()) { await mindmapPost({ action: 'addNode', text: text.trim(), parentId: id }); loadMindmap(); }
    }
    async function mmToggleLink(id) {
      const node = mindmapNodes.find(n => n.id === id);
      // "Unlink" clears both a task link and a project-only scope.
      if (node && (node.linkedTask || node.project)) { await mindmapPost({ action: 'unlinkNode', id }); loadMindmap(); }
      else openLinkPicker(id);
    }
    async function mmDelete(id) {
      if (!confirm('Delete this idea and all its sub-ideas?')) return;
      await mindmapPost({ action: 'deleteNode', id }); loadMindmap();
    }

    // ─── Mindmap node detail modal ───
    function mmLinkHtml(n) {
      const info = n.linkedTaskInfo;
      if (!n.linkedTask) {
        if (n.project) {
          const nm = (projects.find(p => p.alias === n.project) || {}).name || n.project;
          return '📁 Scoped to project “' + escHtml(nm) + '” <span class="st">(no task linked)</span>';
        }
        return '';
      }
      if (info && !info.missing) {
        if (info.kind === 'todo') return '🔗 Subtask ' + (info.done ? '☑' : '☐') + ' ' + escHtml(info.text || '') + ' <span class="st">· in ' + escHtml(info.taskTitle || '') + '</span>';
        return '🔗 Kanban: ' + escHtml(info.project || '') + ' · ' + escHtml(info.title || '') + ' <span class="st">[' + escHtml((info.status || '').replace('_', ' ')) + (info.deleted ? ', deleted' : '') + ']</span>';
      }
      return '🔗 Linked item no longer exists';
    }
    function openMmDetail(id) {
      const n = mindmapNodes.find(x => x.id === id);
      if (!n) return;
      mmDetailId = id;
      $('#mmd-title').textContent = n.text || 'Idea';
      const linkEl = $('#mmd-link');
      linkEl.innerHTML = mmLinkHtml(n);
      linkEl.className = 'mmd-link' + (n.linkedTask && (!n.linkedTaskInfo || n.linkedTaskInfo.missing) ? ' missing' : '');
      $('#mmd-notes').innerHTML = (n.notes || []).map(t => '<li>' + escHtml(t) + '</li>').join('');
      $('#mmd-link-btn').textContent = (n.linkedTask || n.project) ? 'Unlink' : 'Link / scope';
      mmDetailSetEditing(false);
      $('#mm-detail-modal').classList.add('visible');
    }
    function closeMmDetail() { $('#mm-detail-modal').classList.remove('visible'); mmDetailId = null; }
    function addMmNoteRow(value) {
      const row = document.createElement('div');
      row.className = 'mmd-note-row';
      const ta = document.createElement('textarea');
      ta.value = value || ''; ta.placeholder = 'Note…';
      const autosize = () => { ta.style.height = 'auto'; ta.style.height = ta.scrollHeight + 'px'; };
      ta.addEventListener('input', autosize);
      const del = document.createElement('button');
      del.type = 'button'; del.className = 'mmd-note-del'; del.textContent = '✕'; del.title = 'Remove note';
      del.addEventListener('click', () => row.remove());
      row.appendChild(ta); row.appendChild(del);
      $('#mmd-notes-edit').appendChild(row);
      setTimeout(autosize, 0);
      return ta;
    }
    function mmDetailSetEditing(on) {
      $('#mmd-view').style.display = on ? 'none' : '';
      $('#mmd-edit').style.display = on ? '' : 'none';
      $('#mmd-edit-btn').style.display = on ? 'none' : '';
      $('#mmd-save-btn').style.display = on ? '' : 'none';
      if (on) {
        const n = mindmapNodes.find(x => x.id === mmDetailId);
        $('#mmd-text-input').value = n ? (n.text || '') : '';
        $('#mmd-notes-edit').innerHTML = '';
        ((n && n.notes) || []).forEach(t => addMmNoteRow(t));
        setTimeout(() => $('#mmd-text-input').focus(), 30);
      }
    }
    let _mmDetailBound = false;
    function setupMmDetailModal() {
      if (_mmDetailBound) return; _mmDetailBound = true;
      $('#mmd-close-btn').addEventListener('click', closeMmDetail);
      $('#mm-detail-modal').addEventListener('click', (e) => { if (e.target.id === 'mm-detail-modal') closeMmDetail(); });
      $('#mmd-edit-btn').addEventListener('click', () => mmDetailSetEditing(true));
      $('#mmd-add-note').addEventListener('click', () => addMmNoteRow('').focus());
      $('#mmd-save-btn').addEventListener('click', async () => {
        const id = mmDetailId; if (!id) return;
        const text = $('#mmd-text-input').value.trim();
        if (!text) { toast('Idea text is required', 'error'); return; }
        const notes = [...$('#mmd-notes-edit').querySelectorAll('textarea')].map(t => t.value.trim()).filter(Boolean);
        await mindmapPost({ action: 'updateNode', id, text, notes });
        closeMmDetail(); loadMindmap();
      });
      $('#mmd-addchild-btn').addEventListener('click', () => { const id = mmDetailId; closeMmDetail(); mmAddChild(id); });
      $('#mmd-link-btn').addEventListener('click', () => { const id = mmDetailId; closeMmDetail(); mmToggleLink(id); });
      $('#mmd-del-btn').addEventListener('click', () => { const id = mmDetailId; closeMmDetail(); mmDelete(id); });
    }

    // ─── Text input modal (window.prompt replacement; Electron has no prompt) ───
    let imResolve = null;
    function askText({ title = 'Input', label = '', value = '', multiline = false, okLabel = 'OK' } = {}) {
      return new Promise((resolve) => {
        imResolve = resolve;
        $('#im-title').textContent = title;
        const lbl = $('#im-label');
        lbl.textContent = label || '';
        lbl.style.display = label ? '' : 'none';
        const inp = $('#im-input'), ta = $('#im-textarea');
        inp.style.display = multiline ? 'none' : '';
        ta.style.display = multiline ? '' : 'none';
        const field = multiline ? ta : inp;
        field.value = value || '';
        $('#im-ok').textContent = okLabel;
        $('#input-modal').classList.add('visible');
        setTimeout(() => { field.focus(); if (field.select) field.select(); }, 50);
      });
    }
    function imSubmit() {
      const ml = $('#im-textarea').style.display !== 'none';
      const val = (ml ? $('#im-textarea') : $('#im-input')).value;
      $('#input-modal').classList.remove('visible');
      const r = imResolve; imResolve = null; if (r) r(val);
    }
    function imCancel() {
      $('#input-modal').classList.remove('visible');
      const r = imResolve; imResolve = null; if (r) r(null);
    }

    // ─── PIN modal ───
    function openPinModal({ title, subtitle, submitLabel = 'Unlock', onSubmit }) {
      pinOnSubmit = onSubmit;
      $('#pin-title').textContent = title || 'Enter PIN';
      $('#pin-subtitle').textContent = subtitle || '';
      $('#pin-error').textContent = '';
      $('#pin-submit-btn').textContent = submitLabel;
      const inp = $('#pin-input');
      inp.value = '';
      $('#pin-modal').classList.add('visible');
      setTimeout(() => inp.focus(), 50);
    }
    function closePinModal() {
      $('#pin-modal').classList.remove('visible');
      pinOnSubmit = null;
    }
    async function submitPin() {
      if (!pinOnSubmit) return;
      const pin = $('#pin-input').value.trim();
      if (!/^\\d{6}$/.test(pin)) { $('#pin-error').textContent = 'Enter a 6-digit PIN'; return; }
      $('#pin-submit-btn').disabled = true;
      let r;
      try { r = await pinOnSubmit(pin); } catch (err) { r = { ok: false, error: err.message }; }
      $('#pin-submit-btn').disabled = false;
      if (r && r.ok) { closePinModal(); }
      else { $('#pin-error').textContent = (r && r.error) || 'Failed'; const i = $('#pin-input'); i.select(); i.focus(); }
    }

    // Wire panel + modal handlers once.
    setupKanbanHandlers();
    setupSecretsHandlers();
    setupMindmapHandlers();
    setupUsageModal();
    setupReconnectHandlers();
    setInterval(tickResets, 30000); // advance the time-to-reset lines on wall-clock
    setInterval(updateUsageBadge, 30000); // refresh "updated Xm ago"
    setInterval(pollStoredUsage, 90000);  // periodic check against the stored sample
    $('#pin-submit-btn').addEventListener('click', submitPin);
    $('#pin-cancel-btn').addEventListener('click', closePinModal);
    $('#pin-input').addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); submitPin(); } });
    $('#im-ok').addEventListener('click', imSubmit);
    $('#im-cancel').addEventListener('click', imCancel);
    $('#im-input').addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); imSubmit(); } });
    $('#im-textarea').addEventListener('keydown', (e) => { if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); imSubmit(); } });
    // Link picker
    $('#lm-project').addEventListener('change', loadLinkBoard);
    $('#lm-search').addEventListener('input', renderLinkList);
    $('#lm-cancel').addEventListener('click', () => $('#link-modal').classList.remove('visible'));
    $('#lm-scope-project').addEventListener('click', async () => {
      const proj = $('#lm-project').value;
      if (!proj) return;
      const d = await mindmapPost({ action: 'scopeProject', id: linkPickNodeId, project: proj });
      if (d && d.ok) { $('#link-modal').classList.remove('visible'); loadMindmap(); }
    });
    $('#lm-list').addEventListener('click', async (e) => {
      const it = e.target.closest('.lm-item');
      if (!it) return;
      const d = await mindmapPost({ action: 'linkNode', id: linkPickNodeId, project: it.dataset.project, taskId: it.dataset.task, todoId: it.dataset.todo || undefined });
      if (d && d.ok) { $('#link-modal').classList.remove('visible'); loadMindmap(); }
    });

    // ─── Util ───
    function escHtml(s) {
      return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
    }

    // ─── Telegram WebApp auth (opened inside Telegram) ───
    async function tryTelegramWebAppAuth() {
      const tg = window.Telegram?.WebApp;
      if (!tg || !tg.initData) return false;
      try {
        const res = await fetch('/api/auth/webapp', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ initData: tg.initData }),
        });
        const data = await res.json();
        if (data.ok && data.token) {
          token = data.token;
          localStorage.setItem('crundi_token', token);
          tg.ready();
          tg.expand();
          return true;
        }
      } catch { /* ignore */ }
      return false;
    }

    // ─── Local auth (Electron / localhost) ───
    async function tryLocalAuth() {
      const params = new URLSearchParams(location.search);
      const key = params.get('key');
      if (!key) return false;
      try {
        const res = await fetch('/api/auth/local', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ key }),
        });
        const data = await res.json();
        if (data.ok && data.token) {
          token = data.token;
          localStorage.setItem('crundi_token', token);
          // Clean key from URL
          history.replaceState(null, '', location.pathname);
          return true;
        }
      } catch { /* ignore */ }
      return false;
    }

    // ─── Telegram Login redirect flow ───
    // After the widget's data-auth-url redirect, the server bounces back with
    // the session token in the URL fragment (#token=...). Pull it out, persist
    // it, and clean the URL so it isn't left in history.
    function consumeRedirectToken() {
      const m = location.hash.match(/(?:^#|&)token=([a-f0-9]+)/);
      if (m) {
        token = m[1];
        localStorage.setItem('crundi_token', token);
        history.replaceState(null, '', location.pathname);
        return true;
      }
      const params = new URLSearchParams(location.search);
      const err = params.get('auth_error');
      if (err) {
        history.replaceState(null, '', location.pathname);
        toast('Login failed: ' + err, 'error');
      }
      return false;
    }

    // ─── Init ───
    async function init() {
      if (consumeRedirectToken() && await checkAuth()) {
        showApp(); connectSSE(); checkImport();
        return;
      }
      if (token && await checkAuth()) {
        showApp(); connectSSE(); checkImport();
      } else if (await tryTelegramWebAppAuth()) {
        showApp(); connectSSE(); checkImport();
      } else if (await tryLocalAuth()) {
        showApp(); connectSSE(); checkImport();
      } else {
        token = null;
        localStorage.removeItem('crundi_token');
        $('#login-screen').style.display = '';
        injectTelegramWidget();
      }
    }

    init();

    // ─── PWA service worker ───
    // Skip under Electron (window.api present) — only useful for the browser PWA.
    if ('serviceWorker' in navigator && !window.api) {
      window.addEventListener('load', () => {
        navigator.serviceWorker.register('/sw.js').catch(() => { /* non-fatal */ });
      });
    }
  })();
  <\/script>
</body>
</html>`;
}
