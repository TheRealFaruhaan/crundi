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

    /* Flat inline icons — sized to surrounding text, inherit color. */
    .ic { width: 1em; height: 1em; display: inline-block; vertical-align: -0.125em; flex-shrink: 0; pointer-events: none; }

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
      transition: width 0.22s cubic-bezier(0.4, 0, 0.2, 1);
    }
    .sidebar-header {
      display: flex; align-items: center; justify-content: space-between; gap: 8px;
      padding: 12px 16px;
      font-size: 0.75rem;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      color: var(--text-muted);
      border-bottom: 1px solid var(--border-subtle);
    }
    .sidebar-collapse {
      display: none; align-items: center; justify-content: center;
      width: 24px; height: 24px; flex-shrink: 0;
      background: none; border: none; color: var(--text-muted); cursor: pointer;
      border-radius: var(--radius-sm); transition: color 0.12s, background 0.12s;
    }
    .sidebar-collapse:hover { color: var(--text-primary); background: var(--bg-hover); }
    .sidebar-collapse .ic { transition: transform 0.22s cubic-bezier(0.4, 0, 0.2, 1); }
    /* Collapse control is desktop-only (mobile uses the overlay drawer). */
    @media (min-width: 769px) { .sidebar-collapse { display: inline-flex; } }
    /* ── Project initial-avatar (shown only in the collapsed rail) ── */
    .sidebar-item .proj-initials {
      display: none; width: 38px; height: 38px; border-radius: 50%;
      align-items: center; justify-content: center; flex-shrink: 0;
      background: var(--bg-tertiary); border: 1px solid var(--border);
      color: var(--text-secondary); font-size: 0.74rem; font-weight: 700;
      letter-spacing: 0.03em; font-family: var(--mono);
      transition: border-color 0.14s, color 0.14s, box-shadow 0.14s, background 0.14s;
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
    .sidebar-item.dragging { opacity: 0.4; }
    /* drop indicator while reordering projects */
    .proj-drop-line { height: 3px; margin: 1px 6px; border-radius: 2px; background: var(--accent); pointer-events: none; }
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
    /* Terminal-state indicator (precedence computed in JS): purple = a terminal
       needs input, amber = a terminal is working, green = running/idle. */
    .sidebar-item.ts-running .dot { background: var(--green); }
    .sidebar-item.ts-working .dot { background: var(--yellow); box-shadow: 0 0 0 3px var(--yellow-dim, rgba(245,180,60,0.18)); }
    .sidebar-item.ts-input .dot { background: var(--accent); box-shadow: 0 0 0 3px var(--accent-dim); }
    .sidebar-item .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .sidebar-item .svc-heart { flex-shrink: 0; display: inline-flex; align-items: center; color: var(--text-muted); }
    .sidebar-item .svc-heart .svc-ecg { display: block; }
    .sidebar-item .svc-heart.live { color: var(--green); }
    .sidebar-item .sched-mark { flex-shrink: 0; display: inline-flex; align-items: center; color: var(--text-muted); }
    .sidebar-item.active .sched-mark { color: var(--accent-hover); }
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
    .sidebar-footer .add-plus { font-weight: 700; }

    /* ─── Collapsed sidebar rail (desktop) ───
       A narrow rail of circular initial-avatars; each project's services ECG
       "vital sign" tucks beneath its avatar. */
    @media (min-width: 769px) {
      .sidebar.collapsed { width: 60px; }
      .sidebar.collapsed .sidebar-header { justify-content: center; padding: 12px 0; }
      .sidebar.collapsed .sh-title { display: none; }
      .sidebar.collapsed .sidebar-collapse .ic { transform: rotate(180deg); }
      .sidebar.collapsed .sidebar-list { padding: 8px 0; }
      .sidebar.collapsed .sidebar-item {
        flex-direction: column; gap: 5px; padding: 8px 0; justify-content: center;
      }
      .sidebar.collapsed .sidebar-item .dot,
      .sidebar.collapsed .sidebar-item .name,
      .sidebar.collapsed .sidebar-item .remove-btn { display: none; }
      .sidebar.collapsed .sidebar-item.active { background: transparent; }
      .sidebar.collapsed .sidebar-item.active::before { display: none; }
      .sidebar.collapsed .sidebar-item .proj-initials { display: flex; }
      /* active = indigo ring + glow; the avatar IS the selection indicator now */
      .sidebar.collapsed .sidebar-item.active .proj-initials {
        border-color: var(--accent); color: #fff;
        background: var(--accent-grad, var(--accent));
        box-shadow: 0 0 0 1px var(--accent), 0 0 12px -2px var(--accent);
      }
      .sidebar.collapsed .sidebar-item:hover .proj-initials { border-color: var(--accent); color: var(--text-primary); }
      .sidebar.collapsed .sidebar-item.active:hover .proj-initials { color: #fff; }
      /* Collapsed-rail border reflects terminal state (overrides active ring). */
      .sidebar.collapsed .sidebar-item.ts-running .proj-initials { border-color: var(--green); color: var(--text-primary); }
      .sidebar.collapsed .sidebar-item.ts-working .proj-initials { border-color: var(--yellow); color: var(--text-primary); box-shadow: 0 0 10px -2px var(--yellow); }
      .sidebar.collapsed .sidebar-item.ts-input .proj-initials { border-color: var(--accent); color: #fff; box-shadow: 0 0 12px -2px var(--accent); }
      /* services heartbeat sits below the circle as a vital sign */
      .sidebar.collapsed .sidebar-item .svc-heart { margin: 0; }
      .sidebar.collapsed .sidebar-item .svc-heart .svc-ecg { width: 22px; height: 9px; }
      .sidebar.collapsed .sidebar-footer { padding: 8px 6px; }
      .sidebar.collapsed .sidebar-footer .add-label { display: none; }
      .sidebar.collapsed .sidebar-footer button { padding: 8px 0; font-size: 1.05rem; }
    }

    /* ─── Terminal Area ─── */
    .terminal-area {
      flex: 1;
      display: flex;
      flex-direction: column;
      overflow: hidden;
      position: relative;
    }
    .terminal-wrap { position: relative; overflow: hidden; }

    /* Multi-terminal grid: columns side-by-side on desktop (min-width +
       horizontal scroll), stacked on mobile (min-height + vertical scroll).
       With few terminals each flexes to fill the available space. */
    .term-grid {
      flex: 1;
      display: flex;
      flex-direction: row;
      gap: 6px;
      padding: 6px;
      overflow-x: auto;
      overflow-y: hidden;
      min-height: 0;
    }
    .term-cell {
      flex: 1 1 0;
      min-width: 340px;
      display: flex;
      flex-direction: column;
      min-height: 0;
      background: var(--bg-primary);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      overflow: hidden;
    }
    .term-cell.focused { border-color: var(--accent); box-shadow: 0 0 0 1px var(--accent); }
    .term-cell.term-drop-hover { outline: 2px dashed var(--accent); outline-offset: -2px; }
    .file-item[data-drag-ref], .git-file[data-drag-ref], .kanban-todo[data-drag-ref], .media-card[data-drag-ref] { cursor: grab; }
    body.wb-drag-armed .term-cell[data-tid] { outline: 2px dashed var(--accent); outline-offset: -2px; }
    .term-head {
      display: flex; align-items: center; gap: 4px;
      padding: 3px 4px 3px 2px;
      background: var(--bg-secondary);
      border-bottom: 1px solid var(--border);
      flex-shrink: 0; user-select: none;
    }
    .term-drag { cursor: grab; color: var(--text-muted); font-size: 12px; padding: 0 3px; line-height: 1; letter-spacing: -2px; }
    .term-title {
      font-size: 12px; font-weight: 600; color: var(--text-primary);
      padding: 2px 6px; border-radius: var(--radius-sm); cursor: text;
      white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 160px;
    }
    .term-title:hover { background: var(--bg-tertiary); }
    .term-title-input {
      font-size: 12px; font-weight: 600; font-family: inherit;
      padding: 2px 6px; border-radius: var(--radius-sm);
      border: 1px solid var(--accent); background: var(--bg-primary); color: var(--text-primary);
      max-width: 160px; outline: none;
    }
    .term-status-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--green); flex-shrink: 0; }
    .term-status-dot.exited { background: var(--text-muted); }
    .term-status-dot.working { background: var(--yellow); }
    .term-status-dot.input { background: var(--accent); box-shadow: 0 0 0 3px var(--accent-dim); }
    .term-agent-badge { flex-shrink: 0; font-size: 0.6rem; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em; padding: 1px 7px; border-radius: 999px; }
    .term-agent-badge.working { background: var(--yellow-dim, rgba(245,180,60,0.16)); color: var(--yellow); }
    .term-agent-badge.input { background: var(--accent-dim); color: var(--accent-hover); }
    .term-head-spacer { flex: 1; }
    .term-font-btn, .term-head-btn {
      border: 1px solid var(--border); background: var(--bg-primary);
      color: var(--text-secondary); cursor: pointer; border-radius: var(--radius-sm);
      font-size: 11px; line-height: 1; padding: 3px 6px; flex-shrink: 0;
    }
    .term-font-btn:hover, .term-head-btn:hover { color: var(--text-primary); border-color: var(--accent); }
    .term-head-btn.term-close { font-size: 14px; padding: 1px 7px; }
    .term-head-btn.term-close:hover { color: var(--red); border-color: var(--red); }
    .term-body { position: relative; flex: 1; padding: 4px; overflow: hidden; min-height: 0; }
    .term-mount { height: 100%; }
    .term-body .xterm { height: 100%; }
    .term-launch {
      display: flex; flex-direction: column; align-items: center; justify-content: center;
      height: 100%; gap: 10px; color: var(--text-muted);
    }
    .term-launch .icon { font-size: 2.4rem; opacity: 0.3; }
    .term-launch button { min-width: 190px; padding: 9px 24px; border-radius: var(--radius-sm); cursor: pointer; font-size: 14px; }
    .term-launch .btn-normal { border: 1px solid var(--accent); background: var(--accent); color: #fff; }
    .term-launch .btn-skip { border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-primary); }
    .term-launch .btn-shell { border: 1px solid var(--border); background: transparent; color: var(--text-secondary); }
    .term-launch .btn-shell:hover { color: var(--text-primary); border-color: var(--accent); }
    /* Agent button group (Claude today; more agents can be added the same way). */
    .term-launch .term-agent-group {
      display: flex; flex-direction: column; align-items: center; gap: 10px;
      border: 1px solid var(--border); border-radius: var(--radius);
      padding: 16px 20px; margin-top: 4px;
    }
    .term-launch .term-agent-label {
      font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.08em;
      color: var(--text-muted); font-weight: 700;
    }
    /* "+ new terminal" lives in the tab bar (next to the Terminal tab) to save
       space instead of floating over the terminals. */
    .tab-add-term {
      align-self: center; flex-shrink: 0; margin-right: 6px;
      width: 24px; height: 24px; border-radius: var(--radius-sm);
      border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-secondary);
      cursor: pointer; font-size: 17px; line-height: 1;
      display: flex; align-items: center; justify-content: center;
    }
    .tab-add-term:hover { background: var(--accent); color: #fff; border-color: var(--accent); }
    .term-drop-line { width: 3px; align-self: stretch; background: var(--accent); border-radius: 2px; flex-shrink: 0; }

    /* "+" dropdown: choose what to add to the workbench. */
    .wb-add-menu {
      position: fixed; z-index: 600; min-width: 150px;
      background: var(--bg-secondary); border: 1px solid var(--border);
      border-radius: var(--radius); padding: 5px;
      box-shadow: 0 6px 22px rgba(0,0,0,0.55); flex-direction: column; gap: 2px;
    }
    .wb-add-menu.visible { display: flex; }
    .wb-add-menu button {
      display: flex; align-items: center; gap: 9px; width: 100%;
      padding: 8px 11px; border: none; background: none; cursor: pointer;
      color: var(--text-primary); font-size: 0.86rem; border-radius: var(--radius-sm);
      text-align: left;
    }
    .wb-add-menu button:hover { background: var(--accent); color: #fff; }
    .wb-add-menu button .ic { font-size: 1rem; color: var(--accent-hover); }
    .wb-add-menu button:hover .ic { color: #fff; }
    /* ─── Interactive browser panel ─── */
    .browser-panel { display: flex; flex-direction: column; height: 100%; min-height: 0; overflow: hidden; }
    .brz-bar { display: flex; align-items: center; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--border); background: var(--bg-secondary); flex: 0 0 auto; }
    .brz-nav { display: inline-flex; align-items: center; justify-content: center; width: 30px; height: 30px; flex: 0 0 auto; border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-secondary); border-radius: var(--radius-sm); cursor: pointer; padding: 0; }
    .brz-nav:hover:not(:disabled) { border-color: var(--accent); color: var(--text-primary); }
    .brz-nav:disabled { opacity: 0.35; cursor: default; }
    .brz-nav .ic { width: 15px; height: 15px; }
    .brz-url { flex: 1 1 auto; min-width: 0; height: 30px; padding: 0 12px; border: 1px solid var(--border); background: var(--bg-primary); color: var(--text-primary); border-radius: 999px; font-size: 0.8rem; font-family: var(--mono); outline: none; }
    .brz-url:focus { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-dim); }
    .brz-device { height: 30px; max-width: 168px; border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-secondary); border-radius: var(--radius-sm); font-size: 0.74rem; cursor: pointer; padding: 0 6px; flex: 0 0 auto; }
    .brz-device:hover { border-color: var(--accent); }
    .brz-stage { flex: 1 1 auto; min-height: 0; position: relative; background-color: var(--bg-primary); background-image: radial-gradient(var(--border) 1px, transparent 1px); background-size: 16px 16px; overflow: hidden; }
    .brz-hint { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; color: var(--text-muted); font-size: 0.8rem; pointer-events: none; }
    .browser-panel.brz-loading .brz-nav[data-brz="reload"] .ic { animation: brz-spin 0.8s linear infinite; }
    @keyframes brz-spin { to { transform: rotate(360deg); } }
    .brz-unavail { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; color: var(--text-muted); text-align: center; padding: 24px; line-height: 1.5; }
    .brz-unavail .ic { width: 38px; height: 38px; opacity: 0.45; }
    .brz-unavail b { color: var(--text-secondary); }
    /* "Layout" submenu flyout (desktop only) */
    .wb-sub { position: relative; }
    .wb-sub-caret { margin-left: auto; opacity: 0.55; }
    .wb-subitems {
      display: none; position: absolute; left: 100%; top: -5px; margin-left: 4px; min-width: 178px;
      background: var(--bg-secondary); border: 1px solid var(--border); border-radius: var(--radius);
      padding: 5px; box-shadow: 0 6px 22px rgba(0,0,0,0.55); flex-direction: column; gap: 2px;
    }
    .wb-sub:hover .wb-subitems, .wb-subitems:hover { display: flex; }
    @media (max-width: 768px) { .wb-sub { display: none; } }
    .wb-subitems button .lay-ic { width: 22px; height: 16px; flex-shrink: 0; color: var(--accent-hover); }
    .wb-subitems button:hover .lay-ic { color: #fff; }

    /* ─── Mosaic layout (desktop) ─── */
    /* Overflow-x:auto so that once panes hit their min-width the row scrolls
       horizontally instead of squashing further. */
    .term-grid.mosaic { display: flex; overflow-x: auto; overflow-y: hidden; padding: 6px; }
    /* The root node (leaf OR split) always grows to fill the grid — so a single
       panel takes the full width + height, never compact. */
    .term-grid.mosaic > .mosaic-leaf,
    .term-grid.mosaic > .mosaic-split { flex: 1 1 0; }
    /* Nested splits get an inline flex from their proportion; align-items:stretch
       fills the cross axis. */
    .mosaic-split { display: flex; flex: 1 1 0; min-width: 0; min-height: 0; }
    .mosaic-split.row { flex-direction: row; }
    .mosaic-split.col { flex-direction: column; }
    .mosaic-leaf { position: relative; display: flex; min-width: 0; min-height: 0; overflow: hidden; }
    .mosaic-leaf > .term-cell { flex: 1 1 0; min-width: 0; min-height: 0; }
    /* Desktop: every pane has a min-width floor; splits size to their content so
       the grid scrolls when the floors exceed the viewport. (Mobile overrides
       these with full-width snap columns.) */
    .term-grid.mosaic:not(.mobile) .mosaic-leaf { min-width: 340px; }
    .term-grid.mosaic:not(.mobile) .mosaic-split { min-width: min-content; }
    .mosaic-gutter { flex: 0 0 7px; position: relative; z-index: 3; touch-action: none; }
    .mosaic-split.row > .mosaic-gutter { cursor: col-resize; }
    .mosaic-split.col > .mosaic-gutter { cursor: row-resize; }
    .mosaic-gutter::after { content: ''; position: absolute; background: var(--border); transition: background 0.12s; }
    .mosaic-split.row > .mosaic-gutter::after { top: 0; bottom: 0; left: 3px; width: 1px; }
    .mosaic-split.col > .mosaic-gutter::after { left: 0; right: 0; top: 3px; height: 1px; }
    .mosaic-gutter:hover::after, body.mosaic-resizing .mosaic-gutter::after { background: var(--accent); }
    .mosaic-empty {
      flex: 1 1 0; position: relative; margin: 3px; display: flex; flex-direction: column; gap: 10px;
      align-items: center; justify-content: center; border: 2px dashed var(--border);
      border-radius: var(--radius); color: var(--text-muted); font-size: 0.82rem;
    }
    .mosaic-empty.drop-hover { border-color: var(--accent); color: var(--accent-hover); background: var(--accent-dim); }
    .mosaic-empty .me-acts { display: flex; gap: 6px; }
    .mosaic-empty .me-acts button {
      display: inline-flex; align-items: center; gap: 4px; padding: 5px 9px; cursor: pointer;
      border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-secondary);
      border-radius: var(--radius-sm); font-size: 0.72rem;
    }
    .mosaic-empty .me-acts button:hover { border-color: var(--accent); color: var(--text-primary); }
    .mosaic-empty .me-acts button .ic { width: 13px; height: 13px; }
    /* per-leaf split controls (hover) */
    .leaf-ctrls { position: absolute; top: 3px; left: 50%; transform: translateX(-50%); z-index: 6; display: flex; gap: 3px; opacity: 0; transition: opacity 0.12s; }
    .mosaic-leaf:hover .leaf-ctrls { opacity: 1; }
    .leaf-ctrls button {
      display: inline-flex; align-items: center; justify-content: center; width: 22px; height: 18px;
      padding: 0; cursor: pointer; border: 1px solid var(--border); background: var(--bg-secondary);
      color: var(--text-secondary); border-radius: 5px;
    }
    .leaf-ctrls button:hover { border-color: var(--accent); color: var(--accent-hover); }
    .leaf-ctrls button .lay-ic { width: 14px; height: 11px; }
    body.mosaic-cell-drag .mosaic-leaf { outline: 1px dashed var(--border); outline-offset: -2px; }
    body.mosaic-cell-drag .mosaic-leaf.drop-hover { outline: 2px solid var(--accent); }
    /* ── Mobile mosaic: horizontal scroll of fixed full-width columns that snap;
       only the rows (vertical splits) inside a column are resizable. ── */
    /* Force horizontal layout — override the flat-stack flex-direction:column the
       mobile media query sets on .term-grid (else a column grid collapses panes). */
    .term-grid.mosaic.mobile { flex-direction: row; overflow: hidden; }
    /* The single root node (leaf OR split) always fills the grid — so a
       non-split column takes the full width AND height. */
    .term-grid.mosaic.mobile > .mosaic-leaf,
    .term-grid.mosaic.mobile > .mosaic-split { flex: 1 1 auto !important; min-width: 0; min-height: 0; }
    /* A top-level ROW split is the horizontal snap-scroller of full-width columns. */
    .term-grid.mosaic.mobile > .mosaic-split.row { overflow-x: auto; scroll-snap-type: x mandatory; -webkit-overflow-scrolling: touch; }
    .term-grid.mosaic.mobile > .mosaic-split.row > .mosaic-leaf,
    .term-grid.mosaic.mobile > .mosaic-split.row > .mosaic-split {
      flex: 0 0 100% !important; min-width: 100%; scroll-snap-align: start;
    }
    /* No horizontal (between-column) resizing anywhere on mobile — only rows. */
    .term-grid.mosaic.mobile .mosaic-split.row > .mosaic-gutter { display: none; }
    /* Pane split controls have no hover on touch — always show them. */
    .term-grid.mosaic.mobile .leaf-ctrls { opacity: 1; }

    /* Workbench panel cells (Files / Git / Kanban / Mindmap embedded in the grid).
       The real tab-panel node is relocated into .wb-cell-body while the Workbench
       tab is active, then parked back to its tab slot when you leave. */
    .term-cell.wb-cell .term-body { padding: 0; overflow: hidden; }
    .wb-cell-body { overflow: hidden; }
    .wb-cell-body > .tab-panel.wb-embedded {
      display: flex !important; flex-direction: column;
      position: absolute; inset: 0; overflow: auto; -webkit-overflow-scrolling: touch;
    }
    .wb-head-ic { display: inline-flex; align-items: center; color: var(--accent-hover); font-size: 13px; }

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
      display: inline-flex; align-items: center; gap: 6px;
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
    .tab-btn .ic { width: 15px; height: 15px; flex-shrink: 0; }
    /* Mobile: icon-only tabs to fit the bar; labels hidden. */
    @media (max-width: 768px) {
      .tab-btn { padding: 10px 12px; gap: 0; }
      .tab-btn .tab-label { display: none; }
      .tab-btn .ic { width: 17px; height: 17px; }
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
    .services-empty .icon { display: inline-flex; opacity: 0.4; }
    .services-empty .icon .ic { width: 40px; height: 40px; }
    /* Toolbar: title + primary register button (matches the kanban/mindmap toolbars). */
    .svc-toolbar { display: flex; align-items: center; gap: 8px; padding-bottom: 12px; }
    .svc-toolbar .svc-title { font-size: 0.82rem; font-weight: 600; color: var(--text-secondary); }
    .svc-toolbar .spacer { flex: 1; }
    .svc-add-btn {
      display: inline-flex; align-items: center; gap: 6px; cursor: pointer;
      padding: 7px 13px; border-radius: var(--radius-sm);
      border: 1px solid var(--accent); background: var(--accent); color: #fff; font-size: 0.8rem; font-weight: 500;
    }
    .svc-add-btn:hover { background: var(--accent-hover); border-color: var(--accent-hover); }
    .svc-add-btn .ic { width: 15px; height: 15px; }
    .svc-card {
      background: var(--bg-card);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 14px 16px;
      display: flex;
      flex-direction: column;
      gap: 10px;
    }
    .svc-card .svc-header { display: flex; align-items: center; gap: 9px; }
    .svc-card .svc-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; background: var(--text-muted); }
    .svc-card .svc-dot.running { background: var(--green); box-shadow: 0 0 0 3px var(--green-dim); }
    .svc-card .svc-dot.error { background: var(--red); box-shadow: 0 0 0 3px var(--red-dim); }
    .svc-card .svc-name { font-weight: 600; font-size: 0.9rem; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .svc-card .svc-status {
      font-size: 0.68rem; padding: 2px 9px; border-radius: 999px; font-family: var(--mono);
      text-transform: uppercase; letter-spacing: 0.04em;
    }
    .svc-card .svc-status.running { background: var(--green-dim); color: var(--green); }
    .svc-card .svc-status.stopped { background: var(--bg-tertiary); color: var(--text-muted); }
    .svc-card .svc-status.error { background: var(--red-dim); color: var(--red); }
    .svc-card .svc-meta {
      font-size: 0.76rem; color: var(--text-muted); font-family: var(--mono);
      overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    }
    .svc-card .svc-meta .svc-up { color: var(--text-secondary); }
    /* Action buttons — one consistent button style across the card. The bare
       .svc-actions button selector also covers panels that reuse this row
       (Terminals, Browsers) without needing the explicit class. */
    .svc-card .svc-actions, .svc-actions { display: flex; gap: 6px; flex-wrap: wrap; }
    .svc-btn, .svc-actions button {
      display: inline-flex; align-items: center; gap: 5px; cursor: pointer;
      padding: 5px 10px; border-radius: var(--radius-sm);
      border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-secondary);
      font-size: 0.76rem; white-space: nowrap;
    }
    .svc-actions button:hover { border-color: var(--accent); color: var(--text-primary); }
    .svc-actions button.danger:hover { border-color: var(--red); color: var(--red); background: var(--red-dim); }
    .svc-btn .ic, .svc-actions button .ic { width: 13px; height: 13px; }
    .svc-btn:hover { border-color: var(--accent); color: var(--text-primary); }
    .svc-btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
    .svc-btn.primary:hover { background: var(--accent-hover); }
    .svc-btn.danger:hover { border-color: var(--red); color: var(--red); background: var(--red-dim); }
    .svc-btn.on { border-color: var(--green); color: var(--green); }
    .svc-btn[disabled] { opacity: 0.5; cursor: not-allowed; }
    /* Tunnel controls grouped on their own row. */
    .svc-tunnel { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; padding-top: 9px; border-top: 1px dashed var(--border-subtle); }
    .svc-tunnel .svc-tunnel-ic { display: inline-flex; color: var(--text-muted); }
    .svc-tunnel .svc-tunnel-ic .ic { width: 14px; height: 14px; }
    .svc-tunnel .svc-tunnel-badge { font-size: 0.72rem; font-family: var(--mono); color: var(--text-muted); }
    .svc-tunnel .tunnel-link { color: var(--accent); text-decoration: none; }
    .svc-tunnel .tunnel-link:hover { text-decoration: underline; }
    /* Register-service form — matches the card + input system. */
    .svc-register-form { padding: 14px; background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius); margin: 0 0 12px; display: flex; flex-direction: column; gap: 9px; }
    .svc-register-form .srf-title { font-size: 0.82rem; font-weight: 600; color: var(--text-primary); }
    .svc-reg-input { padding: 8px 10px; border-radius: var(--radius-sm); border: 1px solid var(--border); background: var(--bg-primary); color: var(--text-primary); font-size: 0.82rem; font-family: var(--mono); }
    .svc-reg-input::placeholder { color: var(--text-muted); }
    .svc-reg-input:focus { border-color: var(--accent); outline: none; }
    .svc-register-form .srf-actions { display: flex; gap: 6px; margin-top: 2px; }

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
    /* Search + breadcrumb toolbar */
    .files-toolbar { flex-shrink: 0; border-bottom: 1px solid var(--border-subtle); }
    .files-search {
      width: 100%; padding: 8px 12px; border: none; border-bottom: 1px solid var(--border-subtle);
      background: var(--bg-primary); color: var(--text-primary);
      font-size: 12px; font-family: var(--mono); outline: none;
    }
    .files-search:focus { background: var(--bg-secondary); }
    .files-bc { display: flex; align-items: center; gap: 8px; padding: 6px 8px 6px 12px; }
    .files-crumbs { flex: 1; min-width: 0; font-size: 12px; color: var(--text-muted); overflow-x: auto; white-space: nowrap; display: flex; align-items: center; gap: 5px; }
    .files-crumbs a { color: var(--accent); cursor: pointer; text-decoration: none; }
    .files-crumbs a:hover { text-decoration: underline; }
    .files-crumbs .crumb-sep { color: var(--text-muted); opacity: 0.6; }
    .files-bc-actions { display: flex; gap: 4px; flex-shrink: 0; }
    .files-act {
      background: none; border: 1px solid var(--border); color: var(--text-secondary);
      border-radius: var(--radius-sm); cursor: pointer; padding: 4px 7px;
      display: inline-flex; align-items: center; justify-content: center; font-size: 13px;
    }
    .files-act:hover { color: var(--text-primary); border-color: var(--accent); }
    .files-results { flex: 1; overflow-y: auto; }
    .files-search-note { padding: 6px 12px; font-size: 11px; color: var(--text-muted); border-bottom: 1px solid var(--border-subtle); }
    .file-item .fi-dir { color: var(--text-muted); }
    .files-list { overflow-y: auto; }
    .file-item { display: flex; align-items: center; gap: 8px; padding: 4px 12px; font-size: 12px; font-family: var(--mono); cursor: pointer; }
    .file-item:hover { background: var(--bg-hover); }
    .file-item .fi-icon { width: 16px; text-align: center; flex-shrink: 0; white-space: nowrap; font-size: 11px; line-height: 1; }
    .file-item .fi-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .file-item .fi-size { color: var(--text-muted); font-size: 11px; flex-shrink: 0; }
    .file-item.dir .fi-icon { color: var(--yellow); }
    .file-item.file .fi-icon { color: var(--text-muted); }

    /* ─── File Editor Modal ─── */
    .fe-editor { position: fixed; inset: 0; z-index: 2000; display: none; }
    .fe-editor.visible { display: block; }
    .fe-window { display: flex; flex-direction: column; background: var(--bg-primary); overflow: hidden; }
    .fe-arrange, .fe-snap-hint { display: none; }
    /* Mobile: full-screen modal (dimmed); only one window at a time. */
    @media (max-width: 768px) {
      .fe-editor.visible { background: rgba(0,0,0,0.6); backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); }
      .fe-window { position: absolute; inset: 0; width: 100%; height: 100%; }
      .fe-expand { display: none; } /* expand/snap are desktop-only */
    }
    /* Desktop: NON-modal floating, draggable + resizable windows — no backdrop,
       clicks in the empty area pass through so you can keep working behind them. */
    @media (min-width: 769px) {
      .fe-editor { pointer-events: none; }
      .fe-window {
        pointer-events: auto; position: fixed;
        width: min(900px, 80vw); height: min(72vh, 820px);
        min-width: 360px; min-height: 200px;
        border: 1px solid var(--border); border-radius: var(--radius);
        box-shadow: 0 24px 70px -12px rgba(0,0,0,0.75);
      }
      .fe-window.maximized { top: 48px !important; left: 6px !important; width: calc(100vw - 12px) !important; height: calc(100vh - 54px) !important; }
      .fe-window.maximized .fe-resize { display: none; }
      .fe-header { cursor: move; }
      /* Custom resize handles (the editor content covers a native corner grip). */
      .fe-resize { position: absolute; z-index: 6; }
      .fe-resize-e { top: 0; right: -2px; width: 8px; height: 100%; cursor: ew-resize; }
      .fe-resize-s { left: 0; bottom: -2px; height: 8px; width: 100%; cursor: ns-resize; }
      .fe-resize-se { right: -2px; bottom: -2px; width: 16px; height: 16px; cursor: nwse-resize; }
      .fe-resize-se::after { content: ''; position: absolute; right: 3px; bottom: 3px; width: 7px; height: 7px; border-right: 2px solid var(--text-muted); border-bottom: 2px solid var(--text-muted); border-bottom-right-radius: 3px; }
      .fe-window:hover .fe-resize-se::after { border-color: var(--accent); }
      /* Arrange toolbar (top-center) — only while dragging a window (+ brief linger). */
      .fe-arrange {
        pointer-events: auto; position: fixed; top: 54px; left: 50%; transform: translateX(-50%);
        z-index: 2400; align-items: center; gap: 6px; padding: 5px 8px;
        background: var(--bg-secondary); border: 1px solid var(--border); border-radius: 999px;
        box-shadow: 0 6px 22px rgba(0,0,0,0.5);
      }
      .fe-editor.show-arrange .fe-arrange { display: flex; }
      .fe-arrange-label { font-size: 0.72rem; color: var(--text-muted); padding: 0 4px; }
      .fe-arrange button { border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-secondary); border-radius: 999px; padding: 4px 11px; font-size: 0.74rem; cursor: pointer; }
      .fe-arrange button:hover { border-color: var(--accent); color: #fff; background: var(--accent); }
      /* Snap target preview while dragging to an edge. */
      .fe-snap-hint { position: fixed; z-index: 2390; background: var(--accent-dim); border: 2px solid var(--accent); border-radius: var(--radius); pointer-events: none; transition: all 0.08s ease; }
      .fe-editor.snapping .fe-snap-hint { display: block; }
    }
    .fe-header { padding: 8px 12px; background: var(--bg-secondary); border-bottom: 1px solid var(--border); display: flex; align-items: center; gap: 8px; flex-shrink: 0; user-select: none; }
    .fe-header .fe-expand { padding: 4px 9px; }
    .fe-header .fe-path { flex: 1; font-size: 12px; font-family: var(--mono); color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .fe-header button { padding: 4px 12px; border-radius: var(--radius-sm); border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-primary); cursor: pointer; font-size: 12px; }
    .fe-header button:hover { background: var(--accent); border-color: var(--accent); }
    .fe-header button.save { background: var(--green); border-color: var(--green); color: #fff; }
    .fe-content { flex: 1; overflow: hidden; display: flex; flex-direction: column; }
    .fe-content .cm-editor { height: 100%; }
    .fe-content .cm-scroller { overflow: auto; }
    /* Compiled/binary viewers (images, PDFs) */
    .fe-viewer-img { flex: 1; overflow: auto; display: flex; align-items: center; justify-content: center; background: repeating-conic-gradient(var(--bg-secondary) 0% 25%, var(--bg-primary) 0% 50%) 50% / 22px 22px; padding: 16px; }
    .fe-viewer-img img { max-width: 100%; max-height: 100%; object-fit: contain; box-shadow: 0 4px 18px rgba(0,0,0,0.5); }
    .fe-viewer-frame { flex: 1; width: 100%; border: none; background: #fff; }
    .fe-loading { flex: 1; display: flex; align-items: center; justify-content: center; color: var(--text-muted); font-size: 0.9rem; }

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
        font-size: 0; padding: 6px; gap: 0 !important; border-radius: 50%;
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
    /* Segmented preference control (settings) — refined pill toggle. */
    .seg-pref { display: inline-flex; gap: 3px; padding: 3px; background: var(--bg-tertiary); border: 1px solid var(--border); border-radius: 8px; }
    .seg-pref button {
      appearance: none; border: none; background: transparent; cursor: pointer;
      font: inherit; font-size: 0.76rem; font-weight: 500; letter-spacing: 0.01em;
      color: var(--text-secondary); padding: 6px 14px; border-radius: 6px;
      transition: background 0.16s ease, color 0.16s ease, box-shadow 0.16s ease;
    }
    .seg-pref button:hover:not(.active) { color: var(--text-primary); background: var(--accent-dim); }
    .seg-pref button.active { background: var(--accent); color: #fff; box-shadow: 0 1px 7px rgba(99, 102, 241, 0.4); }
    .seg-pref button kbd {
      font-family: var(--mono); font-size: 0.92em; font-weight: 600;
      padding: 1px 5px; border-radius: 4px; background: rgba(255, 255, 255, 0.08);
      border: 1px solid var(--border); color: inherit;
    }
    .seg-pref button.active kbd { background: rgba(255, 255, 255, 0.18); border-color: rgba(255, 255, 255, 0.25); }
    /* Notification matrix — a control-panel grid of tri-state "lamps". Each event
       is a row; the active mode lights up in its column colour (green=Always,
       indigo=When Away, dim=Never) so the whole board reads at a glance. */
    .ntf-matrix { border: 1px solid var(--border); border-radius: 10px; overflow: hidden; background: var(--bg-secondary); }
    .ntf-row { display: grid; grid-template-columns: minmax(0, 1fr) repeat(3, 62px); align-items: stretch; border-top: 1px solid var(--border); }
    .ntf-matrix > .ntf-row:first-child { border-top: none; }
    .ntf-row.head { background: var(--bg-tertiary); }
    .ntf-row.event { animation: ntfRise 0.4s cubic-bezier(0.2, 0.7, 0.2, 1) both; }
    .ntf-row.event:hover { background: rgba(255, 255, 255, 0.025); }
    @keyframes ntfRise { from { opacity: 0; transform: translateY(4px); } to { opacity: 1; transform: none; } }
    .ntf-h { font-family: var(--mono); font-size: 0.56rem; letter-spacing: 0.09em; text-transform: uppercase; color: var(--text-muted); text-align: center; padding: 11px 2px 9px; align-self: center; }
    .ntf-h.lbl { text-align: left; padding-left: 14px; letter-spacing: 0.12em; }
    .ntf-h:not(.lbl), .ntf-cell { border-left: 1px solid var(--border); }
    .ntf-cat { font-size: 0.6rem; font-weight: 700; letter-spacing: 0.13em; text-transform: uppercase; color: var(--accent); background: var(--bg-tertiary); padding: 6px 14px; border-top: 1px solid var(--border); }
    .ntf-lbl { padding: 11px 14px; font-size: 0.82rem; color: var(--text-primary); line-height: 1.3; display: flex; align-items: center; }
    .ntf-cell { display: flex; align-items: center; justify-content: center; padding: 9px 0; cursor: pointer; }
    .ntf-lamp { width: 15px; height: 15px; border-radius: 50%; border: 2px solid var(--border); background: transparent; transition: background 0.18s ease, border-color 0.18s ease, box-shadow 0.18s ease, transform 0.18s ease; }
    .ntf-cell:hover .ntf-lamp { border-color: var(--text-secondary); transform: scale(1.14); }
    .ntf-cell.on .ntf-lamp { border-color: transparent; }
    .ntf-cell.on[data-val="always"] .ntf-lamp { background: var(--green); box-shadow: 0 0 9px rgba(16, 185, 129, 0.55); }
    .ntf-cell.on[data-val="away"] .ntf-lamp { background: var(--accent); box-shadow: 0 0 9px rgba(99, 102, 241, 0.6); }
    .ntf-cell.on[data-val="never"] .ntf-lamp { background: var(--text-muted); }
    @media (max-width: 768px) { .ntf-row { grid-template-columns: minmax(0, 1fr) repeat(3, 50px); } }
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
    /* Mobile radial quick-tools overlay (long-press a terminal). */
    .term-radial { position: fixed; inset: 0; z-index: 2000; pointer-events: none; }
    .term-radial .rcenter {
      position: fixed; width: 36px; height: 36px; border-radius: 50%;
      border: 2px dashed var(--text-muted); background: rgba(10,10,15,0.35);
    }
    .term-radial .rslot {
      position: fixed; width: 48px; height: 48px; border-radius: 50%;
      display: flex; align-items: center; justify-content: center;
      background: var(--bg-secondary); border: 1px solid var(--border);
      color: var(--text-primary); font-size: 13px; font-weight: 700; font-family: var(--mono);
      box-shadow: 0 3px 12px rgba(0,0,0,0.55); transition: transform 0.08s ease, background 0.08s ease;
    }
    .term-radial .rslot.active {
      background: var(--accent); color: #fff; border-color: var(--accent); transform: scale(1.22);
    }
    /* Mobile: stack terminals vertically (like the kanban board) with a min
       height and vertical scroll; a lone terminal still fills the space. */
    @media (max-width: 768px) {
      .term-grid { flex-direction: column; overflow-x: hidden; overflow-y: auto; }
      /* flex-basis 0 (not auto) so cell height is driven by available space + the
         min-height floor, NOT by xterm content — otherwise bumping the font size
         makes the terminal taller and grows the whole container. */
      .term-cell { min-width: 0; min-height: 320px; flex: 1 0 0; }
      .term-drop-line { width: auto; height: 3px; }
      .term-title, .term-title-input { max-width: 110px; }
    }
    .term-input-bar {
      display: flex; gap: 6px; padding: 6px 8px;
      background: var(--bg-secondary); border-top: 1px solid var(--border);
      align-items: flex-end;
    }
    .term-input {
      flex: 1; resize: none; padding: 7px 10px; box-sizing: border-box;
      min-height: 34px; max-height: 120px; overflow-y: auto;
      border-radius: var(--radius-sm); border: 1px solid var(--border);
      background: var(--bg-primary); color: var(--text-primary);
      font-family: var(--mono); font-size: 13px; line-height: 1.4;
      outline: none;
    }
    .term-input:focus { border-color: var(--accent); }
    /* All bottom controls share one square footprint, bottom-aligned with the
       input (which grows upward as you type). */
    .term-send-btn, .term-attach-btn {
      align-self: flex-end; width: 34px; height: 34px; padding: 0; flex-shrink: 0;
      border-radius: var(--radius-sm); cursor: pointer;
      display: flex; align-items: center; justify-content: center;
    }
    .term-send-btn .ic, .term-attach-btn .ic { width: 16px; height: 16px; }
    .term-send-btn { border: 1px solid var(--accent); background: var(--accent); color: #fff; }
    .term-send-btn:hover { background: var(--accent-hover); }
    .term-attach-btn {
      border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-secondary);
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
      border-radius: var(--radius); padding: 10px;
      box-shadow: 0 4px 16px rgba(0,0,0,0.5);
      flex-direction: row; align-items: flex-start; justify-content: space-between;
      gap: 14px; flex-wrap: wrap;
    }
    .term-tool-panel.visible { display: flex; }
    .ttg { display: flex; gap: 5px; }
    .ttg-left { flex-direction: column; }
    .ttg-mid { display: grid; grid-template-columns: 1fr 1fr; gap: 5px; align-content: flex-start; }
    .ttg-mid .span2 { grid-column: 1 / 3; }
    .ttg-nav {
      display: grid; grid-template-columns: repeat(3, minmax(40px, 1fr));
      grid-auto-rows: 34px; gap: 5px; margin-left: auto;
    }
    .ttg-nav [data-key="Home"] { grid-area: 1 / 1 / 2 / 3; }
    .ttg-nav [data-key="End"] { grid-area: 1 / 3 / 2 / 4; }
    .ttg-nav [data-key="ArrowUp"] { grid-area: 2 / 2 / 3 / 3; }
    .ttg-nav [data-key="ArrowLeft"] { grid-area: 3 / 1 / 4 / 2; }
    .ttg-nav [data-key="Enter"] { grid-area: 3 / 2 / 4 / 3; }
    .ttg-nav [data-key="ArrowRight"] { grid-area: 3 / 3 / 4 / 4; }
    .ttg-nav [data-key="ArrowDown"] { grid-area: 4 / 2 / 5 / 3; }
    .term-tool-btn {
      min-width: 40px; height: 34px; border-radius: var(--radius-sm);
      border: 1px solid var(--border); background: var(--bg-primary);
      color: var(--text-primary); cursor: pointer;
      font-family: var(--mono); font-size: 12px; font-weight: 600;
      display: flex; align-items: center; justify-content: center;
      padding: 0 10px; white-space: nowrap;
    }
    .term-tool-btn:active { background: var(--accent); color: #fff; border-color: var(--accent); }
    #mobile-layout-toggle.active { background: var(--accent); color: #fff; border-color: var(--accent); }
    /* The horizontal-layout toggle lives in the bottom tools menu, mobile only. */
    .ttg-mobile-only { display: none; }
    /* Mobile: compact the three groups (keys / ctrl grid / nav d-pad) so they
       stay on one row instead of the nav cluster wrapping to a second line. */
    @media (max-width: 768px) {
      .term-tool-panel { gap: 8px; padding: 8px; flex-wrap: wrap; }
      .ttg { gap: 4px; }
      .ttg-mid { gap: 4px; }
      .ttg-nav { grid-template-columns: repeat(3, minmax(32px, 1fr)); grid-auto-rows: 32px; gap: 4px; }
      .term-tool-btn { min-width: 32px; height: 32px; padding: 0 5px; font-size: 11px; }
      .ttg-mobile-only { display: flex; width: 100%; }
      .ttg-mobile-only .term-tool-btn { width: 100%; }
    }

    /* ─── Kanban ─── */
    .kanban-panel { padding: 0; container-type: inline-size; }
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
    /* When the kanban PANEL itself is narrow (e.g. a small Workbench cell on
       desktop, not just a small viewport), stack columns vertically with a
       vertical scroll — same as the mobile layout. */
    @container (max-width: 700px) {
      .kanban-board { flex-direction: column; align-items: stretch; gap: 10px; overflow-x: hidden; overflow-y: auto; }
      .kanban-col { max-width: 100%; min-width: 0; max-height: none; }
      .kanban-col-body { max-height: none; }
    }
    /* When the panel is narrow, condense the toolbar to one tight row so the
       board / mindmap canvas keeps the vertical space: smaller buttons, the
       Media label folds to icon-only, the usage hint drops, and the mindmap
       filter input + project bar tighten. Container-scoped to the panel width. */
    @container (max-width: 480px) {
      .kanban-panel .kanban-toolbar,
      .mindmap-panel .kanban-toolbar { padding: 6px 8px; gap: 5px; }
      .kanban-panel .kanban-toolbar .spacer,
      .mindmap-panel .kanban-toolbar .spacer { display: none; }
      .kanban-panel .kanban-btn,
      .mindmap-panel .kanban-btn { padding: 5px 8px; font-size: 0.74rem; border-radius: 6px; gap: 0; }
      .kanban-panel .kanban-btn .tb-lbl,
      .mindmap-panel .kanban-btn .tb-lbl { display: none; }
      .mindmap-panel .mm-hint { display: none; }
      .mindmap-panel .mm-search { flex: 1 1 80px; min-width: 0; padding: 5px 8px; font-size: 0.76rem; }
      .mindmap-panel .mm-proj-bar { padding: 5px 8px; gap: 5px; }
      .mindmap-panel .mm-proj-label { display: none; }
      .mindmap-panel .mm-proj-chip { font-size: 0.72rem; padding: 2px 8px; }
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
    .kanban-todo .todo-edit, .kanban-todo .todo-del { background: none; border: none; color: var(--text-muted); cursor: pointer; font-size: 0.85rem; padding: 0 3px; line-height: 1; flex-shrink: 0; }
    .kanban-todo .todo-edit:hover { color: var(--accent-hover); }
    .kanban-todo .todo-del:hover { color: var(--red); }
    .kanban-todo-add { display: flex; gap: 4px; margin-top: 6px; }
    .kanban-todo-add input {
      flex: 1; background: var(--bg-primary); border: 1px solid var(--border); color: var(--text-primary);
      border-radius: 4px; padding: 3px 6px; font-size: 0.78rem;
    }
    .kanban-todo-add .ktodo-add-btn {
      flex-shrink: 0; padding: 0 11px; border-radius: 4px; cursor: pointer; font-size: 0.85rem;
      border: 1px solid var(--accent); background: var(--accent-dim); color: var(--accent-hover);
    }
    .kanban-todo-add .ktodo-add-btn:hover { background: var(--accent); color: #fff; }
    .kanban-progress { font-size: 0.72rem; color: var(--text-muted); margin-top: 6px; }

    /* ─── Schedule tab ─── */
    .schedule-panel { padding: 0; }
    .sched-list { padding: 10px 12px; display: flex; flex-direction: column; gap: 8px; overflow-y: auto; }
    .sched-item { display: flex; align-items: flex-start; gap: 10px; background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 10px 12px; }
    .sched-item.off { opacity: 0.55; }
    .sched-power { flex-shrink: 0; width: 30px; height: 18px; border-radius: 10px; border: 1px solid var(--border); background: var(--bg-tertiary); position: relative; cursor: pointer; margin-top: 2px; transition: background 0.15s, border-color 0.15s; }
    .sched-power::after { content: ''; position: absolute; top: 1px; left: 1px; width: 14px; height: 14px; border-radius: 50%; background: var(--text-muted); transition: transform 0.15s, background 0.15s; }
    .sched-power.on { background: var(--accent-dim); border-color: var(--accent); }
    .sched-power.on::after { transform: translateX(12px); background: var(--accent); }
    .sched-body { flex: 1; min-width: 0; cursor: pointer; }
    .sched-name { font-weight: 600; font-size: 0.9rem; word-break: break-word; }
    .sched-name .sched-proj { font-size: 0.66rem; font-weight: 600; color: var(--accent-hover); background: var(--accent-dim); border-radius: 8px; padding: 0 6px; margin-left: 4px; }
    .sched-sum { font-size: 0.8rem; color: var(--text-secondary); margin-top: 2px; word-break: break-word; }
    .sched-conds { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 6px; }
    .sched-cond { font-size: 0.68rem; color: var(--text-secondary); background: var(--bg-tertiary); border: 1px solid var(--border-subtle); border-radius: 6px; padding: 1px 7px; }
    .sched-last { font-size: 0.66rem; color: var(--text-muted); margin-top: 6px; }
    .sched-rowacts { display: flex; gap: 2px; flex-shrink: 0; }

    /* Schedule add/edit modal */
    #sched-modal { display: none; position: fixed; inset: 0; z-index: 2100; background: rgba(0,0,0,0.55); align-items: center; justify-content: center; }
    #sched-modal.visible { display: flex; }
    .sched-modal-card { background: var(--bg-secondary); border: 1px solid var(--border); border-radius: var(--radius); width: min(560px, 94vw); max-height: 90vh; display: flex; flex-direction: column; box-shadow: 0 24px 70px -12px rgba(0,0,0,0.7); }
    .sm-head { display: flex; align-items: center; justify-content: space-between; padding: 14px 16px; border-bottom: 1px solid var(--border); }
    .sm-head h3 { font-size: 1.05rem; }
    .sm-x { background: none; border: none; color: var(--text-muted); font-size: 1.2rem; cursor: pointer; line-height: 1; }
    .sm-x:hover { color: var(--text-primary); }
    .sm-bodyscroll { padding: 14px 16px; overflow-y: auto; }
    .sm-label { display: block; font-size: 0.72rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); margin: 12px 0 5px; }
    .sm-label:first-child { margin-top: 0; }
    .sm-hint { text-transform: none; letter-spacing: 0; color: var(--text-muted); opacity: 0.8; }
    .sm-input { width: 100%; padding: 7px 10px; border-radius: var(--radius-sm); border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-primary); font-size: 0.86rem; outline: none; }
    .sm-input:focus { border-color: var(--accent); }
    textarea.sm-input { resize: vertical; font-family: var(--mono); }
    .sm-row2 { display: flex; gap: 10px; }
    .sm-row2 > div { flex: 1; }
    .sm-seg { display: flex; gap: 6px; flex-wrap: wrap; }
    .sm-segb { padding: 6px 12px; border-radius: var(--radius-sm); border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-secondary); cursor: pointer; font-size: 0.82rem; }
    .sm-segb.on { background: var(--accent-dim); border-color: var(--accent); color: var(--accent-hover); }
    .sm-segb[disabled] { opacity: 0.8; cursor: default; }
    .sm-cond { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; background: var(--bg-tertiary); border: 1px solid var(--border-subtle); border-radius: var(--radius-sm); padding: 7px 9px; margin-bottom: 6px; }
    .sm-cond .sm-condt { font-size: 0.74rem; font-weight: 600; color: var(--text-secondary); }
    .sm-cond .sm-input { width: auto; flex: 0 1 auto; }
    .sm-cond select.sm-input { min-width: 90px; }
    .sm-condx { margin-left: auto; background: none; border: none; color: var(--text-muted); cursor: pointer; font-size: 1rem; line-height: 1; }
    .sm-condx:hover { color: var(--red); }
    .sm-addcond { display: flex; gap: 6px; flex-wrap: wrap; margin-top: 4px; }
    .sm-foot { display: flex; justify-content: flex-end; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--border); }
    .sm-note { font-size: 0.74rem; color: var(--text-muted); line-height: 1.45; margin-top: 6px; }
    .sm-note code { font-family: var(--mono); background: var(--bg-tertiary); border: 1px solid var(--border-subtle); border-radius: 4px; padding: 0 5px; color: var(--accent-hover); }
    .sm-when { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-top: 6px; }
    .sm-when .sm-input { width: auto; }
    .sm-days { display: flex; gap: 4px; flex-wrap: wrap; }
    .sm-day { width: 34px; height: 32px; border-radius: var(--radius-sm); border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-secondary); cursor: pointer; font-size: 0.76rem; font-weight: 600; }
    .sm-day.on { background: var(--accent-dim); border-color: var(--accent); color: var(--accent-hover); }
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
    /* Update-available dot on the Settings tab + the Updates settings section. */
    .update-badge { width: 8px; height: 8px; border-radius: 50%; background: var(--green); box-shadow: 0 0 0 3px rgba(16,185,129,0.18); }
    .sw { position: relative; width: 42px; height: 24px; flex-shrink: 0; padding: 0; border: 1px solid var(--border); border-radius: 12px; background: var(--bg-tertiary); cursor: pointer; transition: background 0.16s, border-color 0.16s; }
    .sw .sw-knob { position: absolute; top: 2px; left: 2px; width: 18px; height: 18px; border-radius: 50%; background: var(--text-muted); transition: transform 0.18s cubic-bezier(0.2,0.7,0.2,1), background 0.18s; }
    .sw.on { background: var(--accent); border-color: var(--accent); }
    .sw.on .sw-knob { transform: translateX(18px); background: #fff; }
    .upd-status { display: inline-flex; align-items: center; gap: 7px; font-size: 0.82rem; color: var(--text-secondary); }
    .upd-status .upd-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--green); }
    .upd-status.avail { color: var(--accent-hover); }
    .upd-status.avail .upd-dot { background: var(--accent); box-shadow: 0 0 0 3px var(--accent-dim); }
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
    /* ─── Media library ─── */
    /* Visibility/flex come from .tab-panel / .tab-panel.visible — do NOT force
       display here or the panel stays in the layout while hidden and steals
       height from the workbench. */
    .media-panel { padding: 0; }
    .media-body { flex: 1; overflow: auto; padding: 12px; }
    .media-toolbar { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; padding: 10px 12px; border-bottom: 1px solid var(--border); }
    .media-toolbar .spacer { flex: 1; }
    /* Search field — cohesive with the mono/dark system + accent focus ring. */
    .media-search {
      flex: 1 1 150px; max-width: 280px; min-width: 130px;
      background: var(--bg-primary); border: 1px solid var(--border); color: var(--text-primary);
      border-radius: var(--radius-sm); padding: 6px 10px; font-size: 0.8rem; font-family: var(--mono);
    }
    .media-search::placeholder { color: var(--text-muted); }
    .media-search:focus { border-color: var(--accent); outline: none; }
    .media-chip {
      display: inline-flex; align-items: center; gap: 5px; font-size: 0.78rem; cursor: pointer;
      padding: 5px 11px; border-radius: 999px; border: 1px solid var(--border);
      background: var(--bg-tertiary); color: var(--text-secondary);
    }
    .media-chip:hover { border-color: var(--accent); color: var(--text-primary); }
    .media-chip.on { background: var(--accent); border-color: var(--accent); color: #fff; }
    .media-chip .ic { width: 14px; height: 14px; }
    .media-upload-btn {
      display: inline-flex; align-items: center; gap: 6px; font-size: 0.8rem; cursor: pointer;
      padding: 7px 13px; border-radius: var(--radius-sm); border: 1px solid var(--accent);
      background: var(--accent); color: #fff;
    }
    .media-upload-btn:hover { background: var(--accent-hover); border-color: var(--accent-hover); }
    .media-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 12px; }
    .media-empty { color: var(--text-muted); text-align: center; padding: 40px 12px; font-size: 0.88rem; }
    .media-card {
      position: relative; border: 1px solid var(--border); border-radius: var(--radius);
      background: var(--bg-card); overflow: hidden; display: flex; flex-direction: column;
    }
    .media-card.dropping { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-dim); }
    .media-thumb {
      position: relative; height: 110px; cursor: pointer; background:
        repeating-conic-gradient(var(--bg-secondary) 0% 25%, var(--bg-primary) 0% 50%) 50% / 18px 18px;
      display: flex; align-items: center; justify-content: center; overflow: hidden;
    }
    .media-thumb img { width: 100%; height: 100%; object-fit: cover; }
    .media-thumb .media-glyph { color: var(--text-muted); display: flex; flex-direction: column; align-items: center; gap: 6px; }
    .media-thumb .media-glyph .ic { width: 34px; height: 34px; }
    .media-thumb .media-glyph .ext { font-size: 0.66rem; text-transform: uppercase; letter-spacing: 0.06em; }
    .media-kindbadge {
      position: absolute; top: 6px; left: 6px; background: rgba(0,0,0,0.55); color: #fff;
      border-radius: 6px; padding: 2px 5px; display: inline-flex; align-items: center;
    }
    .media-kindbadge .ic { width: 13px; height: 13px; }
    .media-info { padding: 7px 9px; display: flex; flex-direction: column; gap: 3px; }
    .media-name { font-size: 0.76rem; font-weight: 600; word-break: break-word; line-height: 1.25;
      display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
    .media-sub { font-size: 0.66rem; color: var(--text-muted); display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
    .media-link-tag { display: inline-flex; align-items: center; gap: 3px; color: var(--green); cursor: pointer; }
    .media-link-tag:hover:not(.deleted) { text-decoration: underline; }
    .media-link-tag.deleted { color: var(--text-muted); text-decoration: line-through; cursor: default; }
    .media-actions { display: flex; gap: 4px; padding: 0 9px 9px; }
    .media-actions button {
      flex: 1; display: inline-flex; align-items: center; justify-content: center; gap: 4px;
      font-size: 0.68rem; padding: 5px; border-radius: var(--radius-sm); cursor: pointer;
      border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-secondary);
    }
    .media-actions button:hover { border-color: var(--accent); color: var(--text-primary); }
    .media-actions button.danger:hover { border-color: var(--red); color: var(--red); }
    .media-actions button .ic { width: 13px; height: 13px; }
    /* Lightbox */
    .media-lightbox {
      display: none; position: fixed; inset: 0; z-index: 2200; background: rgba(0,0,0,0.86);
      align-items: center; justify-content: center; flex-direction: column; padding: 24px;
    }
    .media-lightbox.visible { display: flex; }
    .media-lightbox .ml-head { position: absolute; top: 0; left: 0; right: 0; display: flex; align-items: center; gap: 10px; padding: 12px 16px; color: #fff; }
    .media-lightbox .ml-title { font-size: 0.86rem; font-weight: 600; word-break: break-word; }
    .media-lightbox .ml-head .spacer { flex: 1; }
    .media-lightbox .ml-btn { background: rgba(255,255,255,0.12); color: #fff; border: none; border-radius: var(--radius-sm); padding: 7px 10px; cursor: pointer; display: inline-flex; align-items: center; gap: 5px; font-size: 0.78rem; }
    .media-lightbox .ml-btn:hover { background: rgba(255,255,255,0.25); }
    .media-lightbox .ml-stage { max-width: 100%; max-height: 100%; display: flex; align-items: center; justify-content: center; }
    .media-lightbox .ml-stage img { max-width: 90vw; max-height: 82vh; object-fit: contain; box-shadow: 0 8px 30px rgba(0,0,0,0.6); }
    .media-lightbox .ml-stage iframe { width: 90vw; height: 82vh; border: none; background: #fff; border-radius: 6px; }
    .media-lightbox .ml-stage video, .media-lightbox .ml-stage audio { max-width: 90vw; max-height: 82vh; }
    /* Inline media strip on kanban cards / mindmap detail */
    .card-media { display: flex; gap: 5px; flex-wrap: wrap; align-items: center; margin: 6px 0 2px; }
    .media-mini {
      width: 38px; height: 38px; border-radius: 7px; overflow: hidden; cursor: pointer;
      border: 1px solid var(--border); background: var(--bg-tertiary);
      display: flex; align-items: center; justify-content: center; color: var(--text-muted); flex-shrink: 0;
    }
    .media-mini:hover { border-color: var(--accent); }
    .media-mini img { width: 100%; height: 100%; object-fit: cover; }
    .media-mini .ic { width: 18px; height: 18px; }
    .card-media-add, .todo-media {
      display: inline-flex; align-items: center; gap: 2px; cursor: pointer;
      border: 1px dashed var(--border); background: transparent; color: var(--text-muted);
      border-radius: 7px; padding: 0 6px; height: 24px; font-size: 0.66rem;
    }
    .card-media-add { width: 38px; height: 38px; justify-content: center; border-radius: 7px; }
    .card-media-add:hover, .todo-media:hover { border-color: var(--accent); color: var(--accent-hover); }
    .card-media-add .ic, .todo-media .ic { width: 14px; height: 14px; }
    .todo-media .cnt { font-weight: 700; color: var(--accent-hover); }
    /* Media browser inside the modal */
    #media-modal .media-modal-body { display: flex; flex-direction: column; min-height: 0; flex: 1; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
    #media-modal .media-toolbar { border-bottom: 1px solid var(--border); }
    #media-modal .media-body { max-height: 58vh; }
    /* highlight pulse used by jump-to-source */
    .jump-flash { animation: jumpFlash 2.2s ease-out; }
    @keyframes jumpFlash {
      0%, 60% { box-shadow: 0 0 0 3px var(--accent), 0 0 18px 4px var(--accent-dim); }
      100% { box-shadow: 0 0 0 0 transparent; }
    }
    .mindmap-panel { padding: 0; container-type: inline-size; }
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
    /* Shared by the mindmap detail modal AND the media modal's action row. */
    .mmd-actions { display: flex; gap: 8px; margin-top: 14px; flex-wrap: wrap; }
    .mmd-actions button { flex: 1; min-width: 72px; border-radius: var(--radius-sm); padding: 8px; font-size: 0.82rem; cursor: pointer; border: 1px solid var(--border); background: var(--bg-tertiary); color: var(--text-primary); }
    .mmd-actions button:hover { border-color: var(--accent); color: var(--text-primary); }
    .mmd-actions button.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
    .mmd-actions button.danger { color: var(--red); }
    .mmd-actions button.danger:hover { border-color: var(--red); background: rgba(255,80,80,0.12); }

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
    .topbar .status-badge.usage-updated { background: rgba(148, 163, 184, 0.15); color: var(--text-secondary); cursor: pointer; }
    .topbar .status-badge.usage-updated:hover { background: rgba(148, 163, 184, 0.22); color: var(--text-primary); }
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
    #add-project-modal, #import-dialog, .input-modal, .pin-modal { backdrop-filter: blur(3px); -webkit-backdrop-filter: blur(3px); }

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
      <button class="hamburger" data-action="toggle-sidebar"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="18" x2="21" y2="18"/></svg></button>
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
        <div class="sidebar-header">
          <span class="sh-title">Projects</span>
          <button class="sidebar-collapse" data-action="toggle-sidebar-collapse" title="Collapse panel"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="11 17 6 12 11 7"/><polyline points="18 17 13 12 18 7"/></svg></button>
        </div>
        <div class="sidebar-list" id="project-list"></div>
        <div class="sidebar-footer">
          <button data-action="add-project" title="Add project"><span class="add-plus">+</span><span class="add-label"> Add project</span></button>
        </div>
      </div>
      <div class="sidebar-overlay" id="sidebar-overlay" data-action="close-sidebar"></div>
      <div class="terminal-area">
        <div class="tab-bar" id="tab-bar">
          <button class="tab-btn active" data-tab="workbench" title="Workbench"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/></svg><span class="tab-label">Workbench</span></button>
          <button class="tab-add-term" id="tab-add-term" data-action="wb-add-menu" title="Add to workbench" style="display:none;">+</button>
          <button class="tab-btn" data-tab="git" title="Git"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/></svg><span class="tab-label">Git</span></button>
          <button class="tab-btn" data-tab="files" title="Files"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/></svg><span class="tab-label">Files</span></button>
          <button class="tab-btn" data-tab="kanban" title="Kanban"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/><line x1="15" y1="3" x2="15" y2="21"/></svg><span class="tab-label">Kanban</span></button>
          <button class="tab-btn" data-tab="mindmap" title="Mindmap"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/><line x1="8.59" y1="13.51" x2="15.42" y2="17.49"/><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"/></svg><span class="tab-label">Mindmap</span></button>
          <button class="tab-btn" data-tab="media" title="Media"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="7" y="3" width="14" height="14" rx="2"/><circle cx="11" cy="7.5" r="1.3"/><polyline points="21 13 17 9.5 9 17"/><path d="M3 7v12a2 2 0 0 0 2 2h12"/></svg><span class="tab-label">Media</span></button>
          <button class="tab-btn" data-tab="schedule" title="Schedule"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><polyline points="12 7 12 12 16 14"/></svg><span class="tab-label">Schedule</span></button>
          <button class="tab-btn" data-tab="services" title="Services"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2" y="3" width="20" height="6" rx="1.5"/><rect x="2" y="13" width="20" height="6" rx="1.5"/><line x1="6" y1="6" x2="6.01" y2="6"/><line x1="6" y1="16" x2="6.01" y2="16"/></svg><span class="tab-label">Services</span></button>
          <button class="tab-btn" data-tab="terminals" title="Terminals"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/></svg><span class="tab-label">Terminals</span></button>
          <button class="tab-btn" data-tab="browsers" title="Browsers"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg><span class="tab-label">Browsers</span></button>
          <button class="tab-btn" data-tab="info" title="Info"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><line x1="12" y1="11" x2="12" y2="16"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg><span class="tab-label">Info</span></button>
          <button class="tab-btn" data-tab="secrets" title="Secrets"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg><span class="tab-label">Secrets</span> <span class="secret-badge" id="secret-badge" style="display:none"></span></button>
          <button class="tab-btn" data-tab="settings" title="Settings"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg><span class="tab-label">Settings</span> <span class="update-badge" id="update-badge" style="display:none" title="Update available"></span></button>
        </div>
        <div class="wb-add-menu" id="wb-add-menu" style="display:none;">
          <button data-action="wb-add" data-kind="terminal"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/></svg> Terminal</button>
          <button data-action="wb-add" data-kind="files"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/></svg> Files</button>
          <button data-action="wb-add" data-kind="git"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/></svg> Git</button>
          <button data-action="wb-add" data-kind="kanban"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/><line x1="15" y1="3" x2="15" y2="21"/></svg> Kanban</button>
          <button data-action="wb-add" data-kind="mindmap"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/><line x1="8.59" y1="13.51" x2="15.42" y2="17.49"/><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"/></svg> Mindmap</button>
          <button data-action="wb-add" data-kind="media"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="7" y="3" width="14" height="14" rx="2"/><circle cx="11" cy="7.5" r="1.3"/><polyline points="21 13 17 9.5 9 17"/><path d="M3 7v12a2 2 0 0 0 2 2h12"/></svg> Media</button>
          <button data-action="wb-add" data-kind="browser"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/></svg> Browser</button>
          <div class="wb-sub" id="wb-layout-sub">
            <button type="button"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="12" y1="3" x2="12" y2="21"/></svg> Layout <span class="wb-sub-caret">&#9656;</span></button>
            <div class="wb-subitems">
              <button data-action="wb-layout" data-mlayout="cols2"><svg class="lay-ic" viewBox="0 0 24 18" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="1" y="1" width="10" height="16" rx="1.5"/><rect x="13" y="1" width="10" height="16" rx="1.5"/></svg> 2 columns</button>
              <button data-action="wb-layout" data-mlayout="cols3"><svg class="lay-ic" viewBox="0 0 24 18" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="1" y="1" width="6.3" height="16" rx="1.2"/><rect x="8.85" y="1" width="6.3" height="16" rx="1.2"/><rect x="16.7" y="1" width="6.3" height="16" rx="1.2"/></svg> 3 columns</button>
              <button data-action="wb-layout" data-mlayout="cross4"><svg class="lay-ic" viewBox="0 0 24 18" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="1" y="1" width="10" height="7.5" rx="1.2"/><rect x="13" y="1" width="10" height="7.5" rx="1.2"/><rect x="1" y="9.5" width="10" height="7.5" rx="1.2"/><rect x="13" y="9.5" width="10" height="7.5" rx="1.2"/></svg> 2&#215;2 cross</button>
              <button data-action="wb-layout" data-mlayout="left-right2"><svg class="lay-ic" viewBox="0 0 24 18" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="1" y="1" width="10" height="16" rx="1.2"/><rect x="13" y="1" width="10" height="7.5" rx="1.2"/><rect x="13" y="9.5" width="10" height="7.5" rx="1.2"/></svg> Left + right split</button>
              <button data-action="wb-layout" data-mlayout="top-bottom2"><svg class="lay-ic" viewBox="0 0 24 18" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="1" y="1" width="22" height="7.5" rx="1.2"/><rect x="1" y="9.5" width="10" height="7.5" rx="1.2"/><rect x="13" y="9.5" width="10" height="7.5" rx="1.2"/></svg> Top + bottom split</button>
            </div>
          </div>
        </div>
        <div class="terminal-wrap tab-panel visible" data-panel="workbench">
          <div class="terminal-placeholder" id="terminal-placeholder">
            <div class="icon">&gt;_</div>
            <p>Select a project to open the workbench</p>
            <p class="hint">Or add a new project from the sidebar</p>
          </div>
          <div class="term-grid" id="term-grid" style="display:none;"></div>
          <div class="term-tool-panel" id="term-tool-panel">
            <div class="ttg ttg-left">
              <button class="term-tool-btn" data-action="term-key" data-key="Escape">Esc</button>
              <button class="term-tool-btn" data-action="term-key" data-key="Tab">Tab</button>
              <button class="term-tool-btn" data-action="term-key" data-key="shift+tab" title="Shift+Tab">&#x21e7;Tab</button>
            </div>
            <div class="ttg ttg-mid">
              <button class="term-tool-btn" data-action="term-key" data-key="ctrl+c">Ctrl+C</button>
              <button class="term-tool-btn" data-action="term-key" data-key="ctrl+z">Ctrl+Z</button>
              <button class="term-tool-btn" data-action="term-key" data-key="ctrl+a">Ctrl+A</button>
              <button class="term-tool-btn" data-action="term-key" data-key="ctrl+e">Ctrl+E</button>
              <button class="term-tool-btn" data-action="term-key" data-key="ctrl+d">Ctrl+D</button>
              <button class="term-tool-btn" data-action="term-key" data-key="ctrl+l">Ctrl+L</button>
              <button class="term-tool-btn span2" data-action="term-key" data-key="Space">Space</button>
            </div>
            <div class="ttg ttg-nav">
              <button class="term-tool-btn" data-action="term-key" data-key="Home">Home</button>
              <button class="term-tool-btn" data-action="term-key" data-key="End">End</button>
              <button class="term-tool-btn" data-action="term-key" data-key="ArrowUp">&#9650;</button>
              <button class="term-tool-btn" data-action="term-key" data-key="ArrowLeft">&#9664;</button>
              <button class="term-tool-btn" data-action="term-key" data-key="Enter">Enter</button>
              <button class="term-tool-btn" data-action="term-key" data-key="ArrowRight">&#9654;</button>
              <button class="term-tool-btn" data-action="term-key" data-key="ArrowDown">&#9660;</button>
            </div>
            <div class="ttg ttg-mobile-only">
              <button class="term-tool-btn span2" id="mobile-layout-toggle" data-action="toggle-mobile-layout" title="Toggle horizontal column layout"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" style="width:14px;height:14px;vertical-align:-2px"><rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/><line x1="15" y1="3" x2="15" y2="21"/></svg> Layout</button>
            </div>
          </div>
          <div class="term-input-bar" id="term-input-bar">
            <button class="term-tool-toggle" id="term-tool-toggle" data-action="term-tool-toggle" title="Keyboard tools"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg></button>
            <textarea id="term-input" class="term-input" rows="1" placeholder="Type here... (Ctrl+Enter or Send button)"></textarea>
            <button class="term-attach-btn" data-action="term-attach" title="Attach a file (uploads to crundi_attachments)"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg></button>
            <input type="file" id="term-attach-input" style="display:none">
            <button class="term-send-btn" data-action="term-send" title="Send to terminal" aria-label="Send"><svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/></svg></button>
          </div>
        </div>
        <div class="git-panel tab-panel" id="git-panel" data-panel="git"></div>
        <div class="files-panel tab-panel" id="files-panel" data-panel="files"></div>
        <div class="kanban-panel tab-panel" id="kanban-panel" data-panel="kanban"></div>
        <div class="secrets-panel tab-panel" id="secrets-panel" data-panel="secrets"></div>
        <div class="mindmap-panel tab-panel" id="mindmap-panel" data-panel="mindmap"></div>
        <div class="media-panel tab-panel" id="media-panel" data-panel="media"></div>
        <div class="browser-panel tab-panel" id="browser-panel" data-panel="browser"></div>
        <div class="schedule-panel tab-panel" id="schedule-panel" data-panel="schedule"></div>
        <div class="services-panel tab-panel" id="services-panel" data-panel="services"></div>
        <div class="services-panel tab-panel" id="terminals-panel" data-panel="terminals"></div>
        <div class="services-panel tab-panel" id="browsers-panel" data-panel="browsers"></div>
        <div class="info-panel tab-panel" id="info-panel" data-panel="info"></div>
        <div class="info-panel tab-panel" id="settings-panel" data-panel="settings"></div>
      </div>
    </div>
  </div>

  <!-- ─── Schedule add/edit modal ─── -->
  <div id="sched-modal"><div class="sched-modal-card" id="sched-modal-card"></div></div>

  <!-- ─── File Editor Modal ─── -->
  <!-- Floating editor/diff windows are created dynamically inside this overlay.
       Desktop: multiple draggable / resizable windows with snap + tile. Mobile:
       a single full-screen window. -->
  <div class="fe-editor" id="file-editor">
    <div class="fe-arrange" id="fe-arrange">
      <span class="fe-arrange-label">Arrange</span>
      <button data-fe-arrange="tile" title="Tile windows side by side">Tile</button>
      <button data-fe-arrange="grid" title="Tile windows in a grid">Grid</button>
      <button data-fe-arrange="cascade" title="Cascade windows">Cascade</button>
    </div>
    <div class="fe-snap-hint" id="fe-snap-hint"></div>
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
        <div class="card-media" id="mmd-media"></div>
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
        <button id="mmd-media-btn">Media</button>
        <button id="mmd-del-btn" class="danger">Delete</button>
        <button id="mmd-close-btn">Close</button>
      </div>
    </div>
  </div>

  <!-- ─── Media lightbox (large preview) ─── -->
  <div class="media-lightbox" id="media-lightbox">
    <div class="ml-head">
      <span class="ml-title" id="ml-title"></span>
      <div class="spacer"></div>
      <button class="ml-btn" id="ml-download">Download</button>
      <button class="ml-btn" id="ml-close">Close</button>
    </div>
    <div class="ml-stage" id="ml-stage"></div>
  </div>
  <!-- hidden input used for media uploads -->
  <input type="file" id="media-file-input" multiple style="display:none">

  <!-- ─── Media library modal (used by Kanban / Mindmap "Media" buttons) ─── -->
  <div class="input-modal" id="media-modal">
    <div class="im-box" style="max-width:780px;width:96%;display:flex;flex-direction:column;max-height:88vh">
      <h3 id="media-modal-title" style="margin-bottom:10px">Media</h3>
      <div class="media-modal-body" id="media-modal-body"></div>
      <div class="mmd-actions" style="margin-top:12px"><button id="media-modal-close">Close</button></div>
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
    // Multi-terminal state: each live terminal gets an xterm view; the unified
    // input box / tool buttons act on whichever terminal is focused. Pending
    // cells are client-only placeholders that show the launch buttons.
    const termViews = new Map();   // termId → { term, fit, mount, cellEl, scrollBtn }
    let focusedTermId = null;
    let pendingCells = [];         // local ids of un-launched cells for the current project
    let pendingProject = null;     // which project pendingCells belong to
    let lastTermSig = '';          // signature of the last-rendered server terminal set (skip redundant grid rebuilds)
    let wbCells = [];              // client-only workbench panel cells: { id, kind:'files'|'git', dir }
    let wbStateProject = null;     // which project wbCells/wbOrder belong to
    let wbOrder = [];             // unified display order of cell keys (live:/pend:/panel:) for the current project
    // Per-project workbench cell state (open panels, ordering, un-launched
    // placeholders), persisted so switching projects — or reloading — no longer
    // wipes the layout. The mosaic *tree* (splits/sizes) is stored separately in
    // crundi_wb_mosaic and references these cells by key, so both must survive.
    let wbByProject = {};
    try { wbByProject = JSON.parse(localStorage.getItem('crundi_wb_cells') || '{}') || {}; } catch { wbByProject = {}; }
    function persistWbState() {
      if (wbStateProject != null) wbByProject[wbStateProject] = { cells: wbCells, order: wbOrder, pending: pendingCells };
      try { localStorage.setItem('crundi_wb_cells', JSON.stringify(wbByProject)); } catch { /* ignore */ }
    }
    // Save the outgoing project's workbench state and restore the incoming one.
    // Replaces the old "reset to empty on project change" behaviour.
    function syncWbStateProject() {
      if (wbStateProject === currentProject && pendingProject === currentProject) return;
      if (wbStateProject != null && wbStateProject !== currentProject) {
        wbByProject[wbStateProject] = { cells: wbCells, order: wbOrder, pending: pendingCells };
      }
      const s = wbByProject[currentProject] || {};
      wbCells = Array.isArray(s.cells) ? s.cells.slice() : [];
      wbOrder = Array.isArray(s.order) ? s.order.slice() : [];
      pendingCells = Array.isArray(s.pending) ? s.pending.slice() : [];
      wbStateProject = currentProject; pendingProject = currentProject;
      persistWbState();
    }
    const termFont = JSON.parse(localStorage.getItem('crundi_term_font') || '{}'); // termId → px
    // Per-event Telegram notification policy ('always' | 'away' | 'never'),
    // mirrored from the server in renderSettings and applied instantly on change.
    const NOTIFY_DEFAULTS_CLIENT = {
      finished: 'away', needsInput: 'away',
      kanbanTask: 'never', kanbanSubtask: 'never',
      scheduleRun: 'away',
      serviceDown: 'always', serviceUp: 'away',
      mindmapAdd: 'never', mindmapDelete: 'never',
      browserLaunch: 'never', browserStop: 'never',
      secretRequest: 'always',
    };
    let notifyPrefs = { ...NOTIFY_DEFAULTS_CLIENT };

    // ─── Auto-update (desktop app only; driven by Electron main via window.api) ───
    let updateState = { enabled: true, available: false, downloading: false, downloaded: false, percent: 0, version: '', current: '', launch: false };
    // Apply state IN PLACE (never re-render the whole Settings panel — that would
    // reset the user's scroll position on every status push / toggle).
    function applyUpdateState(s) {
      if (s) updateState = Object.assign(updateState, s);
      const b = document.getElementById('update-badge');
      if (b) b.style.display = updateState.available ? '' : 'none';
      const swU = document.querySelector('#settings-panel .sw[data-action="update-toggle"]');
      if (swU) { swU.classList.toggle('on', !!updateState.enabled); swU.setAttribute('aria-checked', updateState.enabled ? 'true' : 'false'); }
      const swS = document.querySelector('#settings-panel .sw[data-action="startup-toggle"]');
      if (swS) { swS.classList.toggle('on', !!updateState.launch); swS.setAttribute('aria-checked', updateState.launch ? 'true' : 'false'); }
      const row = document.getElementById('upd-statusrow');
      if (row) row.innerHTML = updStatusRowHtml();
    }
    function updStatusRowHtml() {
      const us = updateState;
      const status = us.downloading
        ? '<div class="upd-status avail"><span class="upd-dot"></span>Downloading\\u2026 ' + (us.percent || 0) + '%</div>'
        : us.available
          ? '<div class="upd-status avail"><span class="upd-dot"></span>Update available \\u2014 v' + escHtml(us.version || '') + (us.downloaded ? ' (ready)' : '') + '</div>'
          : '<div class="upd-status"><span class="upd-dot"></span>Up to date' + (us.current ? ' \\u2014 v' + escHtml(us.current) : '') + '</div>';
      const action = us.available
        ? '<button class="kanban-btn primary"' + (us.downloading ? ' disabled' : '') + ' data-action="update-install">' + (us.downloaded ? 'Restart \\u0026 install' : us.downloading ? 'Downloading\\u2026' : 'Download \\u0026 install') + '</button>'
        : '<button class="kanban-btn" data-action="update-check">Check now</button>';
      return status + action;
    }
    function buildUpdatesSection() {
      const us = updateState;
      const sw = (on, act, title) => '<button type="button" class="sw' + (on ? ' on' : '') + '" data-action="' + act + '" role="switch" aria-checked="' + (on ? 'true' : 'false') + '" title="' + title + '"><span class="sw-knob"></span></button>';
      const row = (title, sub, control) => '<div style="display:flex;align-items:center;justify-content:space-between;gap:12px;margin-bottom:14px;">'
        + '<div><div style="font-size:0.86rem;color:var(--text-primary);">' + title + '</div>'
        + '<div style="font-size:0.72rem;color:var(--text-muted);margin-top:2px;">' + sub + '</div></div>' + control + '</div>';
      return '<div class="info-section"><h4>Desktop App</h4>'
        + row('Launch at startup', 'Start Crundi in the tray when Windows starts.', sw(us.launch, 'startup-toggle', 'Launch at startup'))
        + row('Automatic updates', 'Check on launch and once a day. Always asks before installing.', sw(us.enabled, 'update-toggle', 'Automatic updates'))
        + '<div id="upd-statusrow" style="display:flex;align-items:center;justify-content:space-between;gap:12px;">' + updStatusRowHtml() + '</div></div>';
    }
    async function initUpdateUi() {
      if (!window.api || !window.api.getUpdateState) return;
      try { applyUpdateState(await window.api.getUpdateState()); } catch { /* ignore */ }
      if (window.api.onUpdateStatus) { try { window.api.onUpdateStatus(applyUpdateState); } catch { /* ignore */ } }
    }
    // Which key inserts a newline (vs. submitting) in the terminal: 'shift' or 'ctrl'.
    let termNewlineKey = localStorage.getItem('crundi_term_newline_key') === 'ctrl' ? 'ctrl' : 'shift';
    function setTermNewlineKey(v) {
      termNewlineKey = v === 'ctrl' ? 'ctrl' : 'shift';
      try { localStorage.setItem('crundi_term_newline_key', termNewlineKey); } catch { /* ignore */ }
    }
    let currentProject = null;
    let currentTab = 'workbench';
    let projects = [];
    let terminals = []; // { id, project, title, order, status }
    let userTerminals = []; // { name, status, alias, command }
    let services = [];
    let scheduledProjects = []; // aliases with at least one enabled schedule
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

    // ─── Flat icon set (stroke icons, inherit currentColor) ───
    const ICON_PATHS = {
      menu: '<line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="18" x2="21" y2="18"/>',
      folder: '<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
      file: '<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/>',
      'arrow-up': '<line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/>',
      upload: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="17 8 12 3 7 8"/><line x1="12" y1="3" x2="12" y2="15"/>',
      download: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/>',
      copy: '<rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>',
      trash: '<polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"/><path d="M10 11v6M14 11v6"/><path d="M9 6V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>',
      paperclip: '<path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/>',
      settings: '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>',
      globe: '<circle cx="12" cy="12" r="10"/><line x1="2" y1="12" x2="22" y2="12"/><path d="M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>',
      terminal: '<polyline points="4 17 10 11 4 5"/><line x1="12" y1="19" x2="20" y2="19"/>',
      'rotate-ccw': '<polyline points="1 4 1 10 7 10"/><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10"/>',
      pencil: '<path d="M12 20h9"/><path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4z"/>',
      check: '<polyline points="20 6 9 17 4 12"/>',
      lock: '<rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
      image: '<rect x="3" y="3" width="18" height="18" rx="2" ry="2"/><circle cx="8.5" cy="8.5" r="1.5"/><polyline points="21 15 16 10 5 21"/>',
      archive: '<polyline points="21 8 21 21 3 21 3 8"/><rect x="1" y="3" width="22" height="5"/><line x1="10" y1="12" x2="14" y2="12"/>',
      'file-text': '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/>',
      list: '<line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/>',
      docker: '<path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/>',
      'git-branch': '<line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/>',
      refresh: '<polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/>',
      plus: '<line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>',
      kanban: '<rect x="3" y="3" width="18" height="18" rx="2"/><line x1="9" y1="3" x2="9" y2="21"/><line x1="15" y1="3" x2="15" y2="21"/>',
      'chevrons-left': '<polyline points="11 17 6 12 11 7"/><polyline points="18 17 13 12 18 7"/>',
      'chevron-left': '<polyline points="15 18 9 12 15 6"/>',
      'chevron-right': '<polyline points="9 18 15 12 9 6"/>',
      'bar-chart': '<line x1="6" y1="20" x2="6" y2="14"/><line x1="12" y1="20" x2="12" y2="9"/><line x1="18" y1="20" x2="18" y2="4"/>',
      clock: '<circle cx="12" cy="12" r="9"/><polyline points="12 7 12 12 16 14"/>',
      mindmap: '<circle cx="18" cy="5" r="3"/><circle cx="6" cy="12" r="3"/><circle cx="18" cy="19" r="3"/><line x1="8.59" y1="13.51" x2="15.42" y2="17.49"/><line x1="15.41" y1="6.51" x2="8.59" y2="10.49"/>',
      film: '<rect x="2" y="2" width="20" height="20" rx="2.18"/><line x1="7" y1="2" x2="7" y2="22"/><line x1="17" y1="2" x2="17" y2="22"/><line x1="2" y1="12" x2="22" y2="12"/><line x1="2" y1="7" x2="7" y2="7"/><line x1="2" y1="17" x2="7" y2="17"/><line x1="17" y1="17" x2="22" y2="17"/><line x1="17" y1="7" x2="22" y2="7"/>',
      music: '<path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>',
      'external-link': '<path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"/><polyline points="15 3 21 3 21 9"/><line x1="10" y1="14" x2="21" y2="3"/>',
      x: '<line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>',
      images: '<rect x="7" y="3" width="14" height="14" rx="2"/><circle cx="11" cy="7.5" r="1.3"/><polyline points="21 13 17 9.5 9 17"/><path d="M3 7v12a2 2 0 0 0 2 2h12"/>',
      info: '<circle cx="12" cy="12" r="10"/><line x1="12" y1="11" x2="12" y2="16"/><line x1="12" y1="8" x2="12.01" y2="8"/>',
      server: '<rect x="2" y="3" width="20" height="6" rx="1.5"/><rect x="2" y="13" width="20" height="6" rx="1.5"/><line x1="6" y1="6" x2="6.01" y2="6"/><line x1="6" y1="16" x2="6.01" y2="16"/>',
      send: '<line x1="22" y1="2" x2="11" y2="13"/><polygon points="22 2 15 22 11 13 2 9 22 2"/>',
      maximize: '<path d="M8 3H5a2 2 0 0 0-2 2v3"/><path d="M16 3h3a2 2 0 0 1 2 2v3"/><path d="M21 16v3a2 2 0 0 1-2 2h-3"/><path d="M3 16v3a2 2 0 0 0 2 2h3"/>',
      play: '<polygon points="6 4 20 12 6 20 6 4"/>',
      stop: '<rect x="5" y="5" width="14" height="14" rx="2"/>',
    };
    function ic(name) {
      const p = ICON_PATHS[name]; if (!p) return '';
      return '<svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' + p + '</svg>';
    }

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
      el.innerHTML = ic('bar-chart') + ' ' + escHtml(fmtAgo(usageUpdatedAt));
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
      label.innerHTML = ic('clock') + ' ' + escHtml(fmtRemaining(resetsAtISO));
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

    // Periodic refresh that works even when SSE is down/backgrounded. Hits the
    // LIVE endpoint, which is server-cached for 60s — so this 90s poll causes at
    // most one real Anthropic call per minute (same rate as the SSE interval, no
    // extra rate-limit risk, never forced).
    async function pollStoredUsage() {
      try {
        const res = await apiFetch('/api/usage');
        const d = await res.json();
        if (d && d.ok) renderUsage(d);
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
      setupTerminalArea();
      updateMobileLayoutBtn();
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

    // Initials for the collapsed-rail avatar. Split on space / _ / . / - and take
    // the first letter of the first two parts; a single word uses its first two
    // letters (Dhivo -> DH). Always 2 letters when possible, gracefully handles
    // leading specials (".name"/"_name" -> NA), one-letter names, and symbol-only
    // names (-> "?").
    function projectInitials(name) {
      const raw = String(name || '');
      const parts = raw.split(/[\\s_.\\-]+/).filter(Boolean);
      let ini = '';
      if (parts.length >= 2) ini = (parts[0][0] || '') + (parts[1][0] || '');
      else if (parts.length === 1) ini = parts[0].slice(0, 2);
      ini = ini.replace(/[^A-Za-z0-9]/g, '');
      if (ini.length < 2) {
        const an = raw.replace(/[^A-Za-z0-9]/g, '');
        if (an) ini = an.slice(0, 2);
      }
      return (ini || '?').toUpperCase();
    }

    function renderProjects() {
      const list = $('#project-list');
      list.innerHTML = '';
      for (const p of projects) {
        // Terminal-state precedence for this project's dot/border:
        // needs-input (purple) > working (amber) > running/idle (green) > none.
        const projTerms = terminals.filter(t => t.project === p.alias && t.status === 'running');
        let ts = '';
        if (projTerms.some(t => t.agentState === 'needs-input')) ts = 'ts-input';
        else if (projTerms.some(t => t.agentState === 'working')) ts = 'ts-working';
        else if (projTerms.length) ts = 'ts-running';
        const isActive = currentProject === p.alias;
        const item = document.createElement('div');
        item.className = 'sidebar-item' + (isActive ? ' active' : '') + (ts ? ' ' + ts : '');
        item.dataset.project = p.alias;
        item.title = p.name || p.alias; // full name on hover (esp. when collapsed)
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
        // Clock mark when the project has upcoming (enabled) scheduled tasks.
        const hasSchedule = scheduledProjects.includes(p.alias.toLowerCase());
        const sched = hasSchedule
          ? '<span class="sched-mark" title="Has scheduled tasks">'
            + '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><polyline points="12 7 12 12 16 14"/></svg>'
            + '</span>'
          : '';
        // Per-terminal close lives on each terminal's header now; the project
        // row only shows a "has terminals" dot + the remove-project control.
        item.innerHTML = '<span class="proj-initials" aria-hidden="true">' + escHtml(projectInitials(p.name || p.alias)) + '</span>'
          + '<span class="dot"></span>'
          + '<span class="name">' + escHtml(p.name || p.alias) + '</span>'
          + sched
          + heart
          + (canRemove ? '<button class="remove-btn" data-action="remove-project" data-project="' + escHtml(p.alias) + '" data-name="' + escHtml(p.name || p.alias) + '" title="Remove project (keeps files)">' + ic('trash') + '</button>' : '');
        item.addEventListener('click', (e) => {
          const act = e.target.dataset.action;
          if (act === 'remove-project') return;
          // Ignore the click that trails a drag-reorder.
          if (Date.now() - _projDragEndAt < 300) return;
          selectProject(p.alias);
        });
        makeDraggable(item, projectDragHandlers(p.alias));
        list.appendChild(item);
      }
    }

    // ─── Sidebar project reordering (drag to rearrange; persisted server-side) ──
    let _projDropLine = null, _projDragEndAt = 0;
    function projectReorderTarget(y, draggedAlias) {
      const list = $('#project-list'); if (!list) return null;
      const items = [...list.children].filter(c => c.dataset && c.dataset.project && c.dataset.project !== draggedAlias && c !== _projDropLine);
      for (let i = 0; i < items.length; i++) {
        const r = items[i].getBoundingClientRect();
        if (y < r.top + r.height / 2) return { index: i, before: items[i] };
      }
      return { index: items.length, before: null };
    }
    function projectDragHandlers(alias) {
      let target = null;
      return {
        onMove: (x, y) => {
          const list = $('#project-list'); if (!list) return;
          if (!_projDropLine) { _projDropLine = document.createElement('div'); _projDropLine.className = 'proj-drop-line'; }
          target = projectReorderTarget(y, alias);
          if (!target) { if (_projDropLine.parentNode) _projDropLine.remove(); return; }
          if (target.before) list.insertBefore(_projDropLine, target.before);
          else list.appendChild(_projDropLine);
        },
        onEnd: async (commit) => {
          if (_projDropLine && _projDropLine.parentNode) _projDropLine.remove();
          _projDragEndAt = Date.now();
          if (commit && target) {
            const order = projects.map(p => p.alias).filter(a => a !== alias);
            order.splice(Math.min(target.index, order.length), 0, alias);
            projects.sort((a, b) => order.indexOf(a.alias) - order.indexOf(b.alias));
            renderProjects();
            try { await apiFetch('/api/projects/reorder', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ order }) }); } catch { /* ignore */ }
          }
          target = null;
        },
      };
    }

    async function selectProject(alias) {
      currentProject = alias;
      filesState = { path: '', root: '', parent: null, inside: true, entries: [], crumbs: [] };
      filesSearchQuery = ''; filesSearchResults = null;
      clearInterval(gitRefreshTimer);
      gitRefreshTimer = null;
      const p = projects.find(x => x.alias === alias);
      $('#current-project').textContent = p ? (p.name || p.alias) : alias;
      // Switching project: save the outgoing workbench state and restore this
      // project's (open panels, ordering, placeholders) instead of wiping it.
      syncWbStateProject();
      focusedTermId = null;
      renderProjects();
      closeSidebar();
      $('#tab-bar').classList.add('visible');
      switchTab('workbench');
      renderTermGrid();
    }

    // Launch a terminal into a pending cell: create a server terminal, then swap
    // the placeholder for the live terminal (SSE state will reconcile shortly).
    // mode: 'shell' (plain shell), 'normal' / 'skip' (Claude). Other agents later.
    async function launchTerminal(mode, localId) {
      if (!currentProject) return;
      const skipPerms = mode === 'skip';
      const shellOnly = mode === 'shell';
      try {
        const r = await apiFetch('/api/terminals/create', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ project: currentProject, skipPermissions: skipPerms, shell: shellOnly }),
        });
        const data = await r.json();
        if (!data.ok) { toast(data.error || 'Failed to launch', 'error'); return; }
        if (localId) pendingCells = pendingCells.filter(x => x !== localId);
        if (!terminals.some(t => t.id === data.id)) {
          terminals.push({ id: data.id, project: currentProject, title: data.title || 'Terminal', order: data.order || 0, status: 'running' });
        }
        focusedTermId = data.id;
        renderTermGrid();
        renderProjects();
      } catch (err) {
        toast('Failed to launch terminal: ' + err.message, 'error');
      }
    }

    async function closeTerminal(id) {
      try {
        await apiFetch('/api/terminals/' + encodeURIComponent(id) + '/close', { method: 'POST' });
        terminals = terminals.filter(t => t.id !== id);
        if (focusedTermId === id) focusedTermId = null;
        delete termFont[id];
        renderTermGrid();
        renderProjects();
      } catch (err) {
        toast('Failed to close terminal: ' + err.message, 'error');
      }
    }

    // Discard an un-launched placeholder cell.
    function closePendingCell(localId) {
      pendingCells = pendingCells.filter(x => x !== localId);
      persistWbState();
      renderTermGrid();
    }

    // Add another un-launched terminal cell for this project.
    function addTerminalCell() {
      if (!currentProject) { toast('Select a project first', 'error'); return; }
      if (currentTab !== 'workbench') { switchTab('workbench'); }
      syncWbStateProject();
      pendingCells.push(genLocalId());
      persistWbState();
      renderTermGrid();
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
          focusedTermId = null;
          renderTermGrid();
        }
        await loadProjects();
        toast('Removed "' + (name || alias) + '"' + (data.servicesRemoved ? ' and ' + data.servicesRemoved + ' service(s)' : ''), 'success');
      } catch (err) {
        toast('Failed to remove project: ' + err.message, 'error');
      }
    }

    // ─── Terminal grid (multiple Claude terminals per project) ───
    const TERM_THEME = {
      background: '#0a0a0f', foreground: '#e8e8f0', cursor: '#6366f1', cursorAccent: '#0a0a0f',
      selectionBackground: 'rgba(99, 102, 241, 0.3)',
      black: '#1a1a28', brightBlack: '#5a5a78', red: '#ef4444', brightRed: '#f87171',
      green: '#10b981', brightGreen: '#34d399', yellow: '#f59e0b', brightYellow: '#fbbf24',
      blue: '#6366f1', brightBlue: '#818cf8', magenta: '#a855f7', brightMagenta: '#c084fc',
      cyan: '#06b6d4', brightCyan: '#22d3ee', white: '#e8e8f0', brightWhite: '#ffffff',
    };
    const TERM_FONT_FAMILY = '"Cascadia Code", "Fira Code", "JetBrains Mono", "Consolas", monospace';

    function genLocalId() { let s = 'p'; for (let i = 0; i < 12; i++) s += Math.floor(Math.random() * 16).toString(16); return s; }
    function isMobileTerm() { return window.matchMedia('(max-width: 768px)').matches; }

    function liveTermsForProject() {
      if (!currentProject) return [];
      const k = currentProject.toLowerCase();
      return terminals.filter(t => String(t.project).toLowerCase() === k).sort((a, b) => (a.order || 0) - (b.order || 0));
    }

    // Reconcile the grid DOM against the desired set of cells (live terminals +
    // un-launched placeholders). Existing xterm views are preserved; cells are
    // re-appended in order (moving a cell within its parent doesn't disturb the
    // embedded terminal).
    function renderTermGrid() {
      const grid = $('#term-grid'); const ph = $('#terminal-placeholder');
      const addBtn = $('#tab-add-term'); const bar = $('#term-input-bar');
      if (!grid) return;
      if (!currentProject) {
        grid.style.display = 'none';
        if (ph) { ph.style.display = ''; ph.innerHTML = '<div class="icon">&gt;_</div><p>Select a project to open the workbench</p><p class="hint">Or add a new project from the sidebar</p>'; }
        if (addBtn) addBtn.style.display = 'none';
        if (bar) bar.style.display = 'none';
        return;
      }
      if (ph) ph.style.display = 'none';
      grid.style.display = '';
      if (addBtn) addBtn.style.display = '';
      if (bar) bar.style.display = '';

      syncWbStateProject();
      const live = liveTermsForProject();
      // "Terminal by default opens in one column": seed a single placeholder when
      // a project has no cells of any kind yet.
      if (!live.length && !pendingCells.length && !wbCells.length) pendingCells = [genLocalId()];

      // Unordered set of every cell the grid should contain.
      const items = [
        ...live.map(t => ({ key: 'live:' + t.id, type: 'live', t })),
        ...pendingCells.map(lid => ({ key: 'pend:' + lid, type: 'pending', localId: lid })),
        ...wbCells.map(c => ({ key: 'panel:' + c.id, type: 'panel', cell: c })),
      ];
      // Apply the unified display order (wbOrder); new cells append in natural order.
      const byKey = new Map(items.map(d => [d.key, d]));
      const ordered = [];
      for (const k of wbOrder) if (byKey.has(k)) { ordered.push(byKey.get(k)); byKey.delete(k); }
      for (const d of items) if (byKey.has(d.key)) ordered.push(d);
      wbOrder = ordered.map(d => d.key);
      const desired = ordered;
      wbLastKeys = desired.map(d => d.key);
      persistWbState(); // capture latest panels/order/placeholders for this project

      // Reconcile the set of cell elements (search the WHOLE grid, since in mosaic
      // mode cells live nested inside leaves — not just top-level children).
      const wanted = new Set(desired.map(d => d.key));
      grid.querySelectorAll('[data-cellkey]').forEach(ch => { const k = ch.dataset.cellkey; if (k && !wanted.has(k)) destroyCell(ch); });
      const elByKey = {};
      const newLive = [];
      for (const d of desired) {
        let el = grid.querySelector('[data-cellkey="' + d.key + '"]');
        if (!el) { el = buildCellEl(d); if (d.type === 'live') newLive.push([d, el]); }
        else if (d.type === 'live') updateCellHead(el, d.t);
        elByKey[d.key] = el;
      }

      // Arrange: mosaic on desktop (always); on mobile only when the user opts in
      // via the bottom-tools toggle (otherwise the classic flat vertical stack).
      if (mosaicActive()) {
        grid.classList.add('mosaic');
        grid.classList.toggle('mobile', isMobileTerm()); // mobile = fixed full-width snap columns, only rows resize
        arrangeMosaic(grid, desired, elByKey);
      } else {
        grid.classList.remove('mosaic', 'mobile');
        // replaceChildren moves the cells in order and drops any leftover mosaic
        // skeleton from a previous desktop render.
        grid.replaceChildren(...desired.map(d => elByKey[d.key]));
      }

      // Mount xterm for newly built live cells AFTER they're attached to the DOM.
      for (const [d, el] of newLive) mountXterm(d.t, el);

      if (!focusedTermId || !live.some(t => t.id === focusedTermId)) focusedTermId = live[0] ? live[0].id : null;
      updateFocusStyles();
      embedWbPanels();
      setTimeout(fitAllTerms, 30);
    }
    let wbLastKeys = [];
    function currentDesiredKeys() { return wbLastKeys.slice(); }

    // Build the mosaic DOM from the (synced) tree and relocate each cell element
    // into its leaf. Empty leaves render as drop targets. Cells are MOVED (not
    // rebuilt), so running terminals survive.
    function arrangeMosaic(grid, desired, elByKey) {
      const tree = mosaicSync(desired.map(d => d.key));
      const leafMap = [];
      const skeleton = buildMosaicDom(tree, leafMap);
      for (const { node, dom } of leafMap) {
        if (node.key && elByKey[node.key]) {
          dom.appendChild(elByKey[node.key]);
          dom.insertAdjacentHTML('beforeend', leafCtrlsHtml(node._id));
        } else {
          dom.innerHTML = emptyLeafHtml(node._id);
        }
      }
      grid.replaceChildren(skeleton); // old skeleton dropped; cells already moved into new one
    }
    function buildMosaicDom(node, leafMap) {
      if (node.t === 'leaf') {
        const d = document.createElement('div');
        d.className = 'mosaic-leaf'; d.dataset.leafId = node._id;
        leafMap.push({ node, dom: d });
        return d;
      }
      const s = document.createElement('div');
      s.className = 'mosaic-split ' + node.dir; s.dataset.splitId = node._id;
      node.kids.forEach((kid, i) => {
        if (i > 0) {
          const g = document.createElement('div');
          g.className = 'mosaic-gutter'; g.dataset.splitId = node._id; g.dataset.gutter = i;
          s.appendChild(g);
        }
        const kEl = buildMosaicDom(kid, leafMap);
        const size = (node.sizes && node.sizes[i] != null) ? node.sizes[i] : (100 / node.kids.length);
        kEl.style.flex = size + ' 1 0';
        s.appendChild(kEl);
      });
      return s;
    }
    function leafCtrlsHtml(id) {
      const sr = '<svg class="lay-ic" viewBox="0 0 24 18" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="1" y="1" width="22" height="16" rx="1.5"/><line x1="12" y1="1" x2="12" y2="17"/></svg>';
      const sd = '<svg class="lay-ic" viewBox="0 0 24 18" fill="none" stroke="currentColor" stroke-width="1.8"><rect x="1" y="1" width="22" height="16" rx="1.5"/><line x1="1" y1="9" x2="23" y2="9"/></svg>';
      return '<div class="leaf-ctrls">'
        + '<button data-action="leaf-split" data-leaf="' + id + '|row" title="Split right">' + sr + '</button>'
        + '<button data-action="leaf-split" data-leaf="' + id + '|col" title="Split down">' + sd + '</button>'
        + '</div>';
    }
    function emptyLeafHtml(id) {
      return '<div class="mosaic-empty" data-leaf-drop="' + id + '">'
        + '<div class="me-msg">Drop a panel here</div>'
        + '<div class="me-acts">'
        + '<button data-action="leaf-split" data-leaf="' + id + '|row" title="Split right">' + ic('plus') + 'Split &#9656;</button>'
        + '<button data-action="leaf-split" data-leaf="' + id + '|col" title="Split down">' + ic('plus') + 'Split &#9662;</button>'
        + '<button data-action="leaf-remove" data-leaf="' + id + '" title="Remove pane">' + ic('x') + '</button>'
        + '</div></div>';
    }

    function buildCellEl(d) {
      const el = document.createElement('div');
      el.className = 'term-cell';
      el.dataset.cellkey = d.key;
      const head = document.createElement('div'); head.className = 'term-head';
      const body = document.createElement('div'); body.className = 'term-body';
      if (d.type === 'pending') {
        el.dataset.lid = d.localId;
        head.innerHTML = '<span class="term-drag" style="opacity:.25">\\u22ee\\u22ee</span>'
          + '<span class="term-title">New terminal</span>'
          + '<span class="term-head-spacer"></span>'
          + '<button class="term-head-btn term-close" data-action="term-close-pending" data-lid="' + d.localId + '" title="Remove">\\u00d7</button>';
        body.innerHTML = '<div class="term-launch">'
          + '<div class="icon">&gt;_</div>'
          + '<button class="btn-shell" data-action="launch-terminal" data-mode="shell" data-lid="' + d.localId + '">Empty Shell</button>'
          + '<div class="term-agent-group">'
          + '<div class="term-agent-label">Claude</div>'
          + '<button class="btn-normal" data-action="launch-terminal" data-mode="normal" data-lid="' + d.localId + '">Normal Mode</button>'
          + '<button class="btn-skip" data-action="launch-terminal" data-mode="skip" data-lid="' + d.localId + '">Skip Permissions Mode</button>'
          + '</div>'
          + '</div>';
      } else if (d.type === 'panel') {
        el.classList.add('wb-cell');
        el.dataset.wbid = d.cell.id;
        head.innerHTML = headHtmlPanel(d.cell);
        body.classList.add('wb-cell-body');
        body.dataset.wb = d.cell.id;
        // The real tab-panel node is moved in by embedWbPanels().
      } else {
        el.dataset.tid = d.t.id;
        head.innerHTML = headHtmlLive(d.t);
        body.innerHTML = '<div class="term-mount"></div>'
          + '<button class="term-select-toggle" data-action="term-select" data-tid="' + d.t.id + '" title="Toggle text selection">Select</button>'
          + '<button class="term-scroll-bottom" data-action="term-scroll-bottom" data-tid="' + d.t.id + '" title="Scroll to bottom">\\u2193</button>'
          + '<div class="term-select-overlay"></div>';
      }
      el.appendChild(head);
      el.appendChild(body);
      if (d.type === 'live') {
        // Clicking a cell focuses it (so the unified input targets it).
        el.addEventListener('mousedown', () => focusTerm(d.t.id), true);
        el.addEventListener('touchstart', () => focusTerm(d.t.id), { passive: true, capture: true });
        // Native drop target: dropping a dragged path/ref here sends it to THIS
        // terminal (stopPropagation so the wrap-level handler doesn't also fire).
        el.addEventListener('dragover', (e) => { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; el.classList.add('term-drop-hover'); });
        el.addEventListener('dragleave', (e) => { if (!el.contains(e.relatedTarget)) el.classList.remove('term-drop-hover'); });
        el.addEventListener('drop', (e) => {
          const txt = e.dataTransfer && e.dataTransfer.getData('text/plain');
          el.classList.remove('term-drop-hover');
          if (txt) { e.preventDefault(); e.stopPropagation(); insertRefToTarget({ kind: 'term', id: d.t.id }, txt); }
        });
      }
      if (d.type === 'live' || d.type === 'panel') {
        // Drag the header to reorder (reuses the kanban drag controller).
        makeDraggable(el, Object.assign({ handle: head }, termDragHandlers(d.key)));
      }
      return el;
    }

    const WB_KIND_META = {
      files: { icon: 'folder', label: 'Files' },
      git: { icon: 'git-branch', label: 'Git' },
      kanban: { icon: 'kanban', label: 'Kanban' },
      mindmap: { icon: 'mindmap', label: 'Mindmap' },
      media: { icon: 'images', label: 'Media' },
      browser: { icon: 'globe', label: 'Browser' },
    };
    function headHtmlPanel(cell) {
      const m = WB_KIND_META[cell.kind] || { icon: 'file', label: cell.kind };
      return '<span class="term-drag" title="Drag to reorder">\\u22ee\\u22ee</span>'
        + '<span class="wb-head-ic">' + ic(m.icon) + '</span>'
        + '<span class="term-title" style="cursor:default;">' + escHtml(m.label) + '</span>'
        + '<span class="term-head-spacer"></span>'
        + '<button class="term-font-btn" data-action="wb-refresh" data-wbid="' + cell.id + '" title="Refresh">' + ic('refresh') + '</button>'
        + '<button class="term-head-btn term-close" data-action="wb-close" data-wbid="' + cell.id + '" title="Close panel">\\u00d7</button>';
    }

    function headHtmlLive(t) {
      const exited = t.status === 'exited';
      const as = exited ? '' : (t.agentState || 'idle');
      const dotCls = exited ? ' exited' : (as === 'working' ? ' working' : as === 'needs-input' ? ' input' : '');
      const badge = (!exited && as === 'working') ? '<span class="term-agent-badge working">working</span>'
        : (!exited && as === 'needs-input') ? '<span class="term-agent-badge input">needs input</span>' : '';
      return '<span class="term-drag" title="Drag to reorder">\\u22ee\\u22ee</span>'
        + '<span class="term-status-dot' + dotCls + '" title="' + (exited ? 'exited' : as) + '"></span>'
        + '<span class="term-title" data-action="term-rename" data-tid="' + t.id + '" title="Click to rename">' + escHtml(t.title || 'Terminal') + '</span>'
        + badge
        + '<span class="term-head-spacer"></span>'
        + '<button class="term-font-btn" data-action="term-font" data-dir="-1" data-tid="' + t.id + '" title="Smaller text">A-</button>'
        + '<button class="term-font-btn" data-action="term-font-reset" data-tid="' + t.id + '" title="Reset text size">' + ic('rotate-ccw') + '</button>'
        + '<button class="term-font-btn" data-action="term-font" data-dir="1" data-tid="' + t.id + '" title="Larger text">A+</button>'
        + '<button class="term-head-btn term-close" data-action="term-close" data-tid="' + t.id + '" title="Close terminal">\\u00d7</button>';
    }

    function updateCellHead(el, t) {
      const titleEl = el.querySelector('.term-title');
      if (titleEl && titleEl.tagName !== 'INPUT' && titleEl.textContent !== (t.title || 'Terminal')) titleEl.textContent = t.title || 'Terminal';
      const exited = t.status === 'exited';
      const as = exited ? '' : (t.agentState || 'idle');
      const dot = el.querySelector('.term-status-dot');
      if (dot) {
        dot.classList.toggle('exited', exited);
        dot.classList.toggle('working', as === 'working');
        dot.classList.toggle('input', as === 'needs-input');
        dot.title = exited ? 'exited' : as;
      }
      // Agent-state badge (working / needs input) next to the title.
      let badge = el.querySelector('.term-agent-badge');
      const want = (!exited && as === 'working') ? ['working', 'working']
        : (!exited && as === 'needs-input') ? ['input', 'needs input'] : null;
      if (!want) { if (badge) badge.remove(); }
      else {
        if (!badge && titleEl) { badge = document.createElement('span'); titleEl.after(badge); }
        if (badge) { badge.className = 'term-agent-badge ' + want[0]; badge.textContent = want[1]; }
      }
    }

    // Refresh just the status dot / badge of each live grid cell in place. Used on
    // every state push so an agent status change (working → done) never triggers a
    // full renderTermGrid() — a rebuild re-parents the cells and would blur the
    // terminal you're currently typing in.
    function updateLiveCellHeads() {
      const grid = document.getElementById('term-grid');
      if (!grid) return;
      for (const t of terminals) {
        const el = grid.querySelector('[data-cellkey="live:' + t.id + '"]');
        if (el) updateCellHead(el, t);
      }
    }

    function mountXterm(t, cellEl) {
      if (termViews.has(t.id)) return;
      const mount = cellEl.querySelector('.term-mount');
      if (!mount) return;
      const xterm = new Terminal({
        cursorBlink: true,
        fontSize: termFont[t.id] || 14,
        fontFamily: TERM_FONT_FAMILY,
        theme: TERM_THEME,
        allowProposedApi: true,
        scrollback: 10000,
        convertEol: true,
      });
      xterm.onSelectionChange(() => { const sel = xterm.getSelection(); if (sel) navigator.clipboard.writeText(sel).catch(() => {}); });
      const fit = new FitAddon.FitAddon();
      xterm.loadAddon(fit);
      xterm.open(mount);
      xterm.onData((data) => { if (ws && ws.readyState === 1) ws.send(JSON.stringify({ type: 'input', id: t.id, data })); });
      if (xterm.textarea) xterm.textarea.addEventListener('focus', () => { focusTerm(t.id); scrollFocusIntoView(cellEl); });

      // Ctrl/Cmd+V image paste → upload + paste path; text left to xterm (paste once).
      xterm.attachCustomKeyEventHandler((e) => {
        // Configurable newline key (Settings -> Terminal): Shift+Enter (default)
        // or Ctrl/Cmd+Enter inserts a newline instead of submitting. Terminals
        // send the SAME byte (CR, 0x0d) for Enter and Shift+Enter, so xterm would
        // just submit. We instead send ESC+CR (0x1b 0x0d) -- the sequence Claude
        // Code (and terminal Shift+Enter keybindings) treat as "insert newline".
        // Read live so the toggle applies to already-open terminals.
        if (e.type === 'keydown' && e.key === 'Enter' && !e.altKey) {
          const wantNewline = termNewlineKey === 'ctrl'
            ? ((e.ctrlKey || e.metaKey) && !e.shiftKey)
            : (e.shiftKey && !e.ctrlKey && !e.metaKey);
          if (wantNewline) {
            e.preventDefault();
            if (ws && ws.readyState === 1) ws.send(JSON.stringify({ type: 'input', id: t.id, data: '\\x1b\\r' }));
            return false;
          }
        }
        if (e.type === 'keydown' && e.key === 'v' && (e.ctrlKey || e.metaKey) && currentProject) {
          (async () => {
            try {
              if (!navigator.clipboard || !navigator.clipboard.read) return;
              const items = await navigator.clipboard.read();
              for (const item of items) {
                const imageType = item.types.find(tp => tp.startsWith('image/'));
                if (!imageType) continue;
                const blob = await item.getType(imageType);
                const b64 = arrayBufferToBase64(await blob.arrayBuffer());
                const r = await apiFetch('/api/clipboard/paste-image', {
                  method: 'POST', headers: { 'Content-Type': 'application/json' },
                  body: JSON.stringify({ project: currentProject, data: b64 }),
                });
                const data = await r.json();
                if (data.ok && data.path && ws && ws.readyState === 1) {
                  ws.send(JSON.stringify({ type: 'input', id: t.id, data: data.path }));
                  toast('Screenshot saved: ' + data.name);
                }
                return;
              }
            } catch { /* no clipboard image / permission denied */ }
          })();
          return false;
        }
        return true;
      });

      // Touch scroll interceptor + long-press radial quick-tools (mobile).
      let tsy = 0, tsx = 0, acc = 0, scrolling = false, sx = 0, sy = 0, lpTimer = null, radialActive = false;
      const TH = 12, START = 10, LP_MS = 380, LP_MOVE = 12;
      mount.addEventListener('touchstart', (e) => {
        if (e.touches.length !== 1) { clearTimeout(lpTimer); lpTimer = null; return; }
        sx = tsx = e.touches[0].clientX; sy = tsy = e.touches[0].clientY; acc = 0; scrolling = false; radialActive = false;
        clearTimeout(lpTimer);
        if (isMobileTerm()) lpTimer = setTimeout(() => { radialActive = true; focusTerm(t.id); openRadial(sx, sy, t.id); }, LP_MS);
      }, { passive: true });
      mount.addEventListener('touchmove', (e) => {
        if (e.touches.length !== 1) return;
        const cx = e.touches[0].clientX, cy = e.touches[0].clientY;
        if (radialActive) { updateRadial(cx, cy); e.preventDefault(); return; }
        // A pending long-press is cancelled once the finger travels (it's a scroll).
        if (lpTimer && Math.hypot(cx - sx, cy - sy) > LP_MOVE) { clearTimeout(lpTimer); lpTimer = null; }
        const dy = tsy - cy; const dx = Math.abs(cx - tsx);
        if (!scrolling) { if (Math.abs(dy) > START && Math.abs(dy) > dx) scrolling = true; else return; }
        acc += dy; tsy = cy;
        const lines = Math.trunc(acc / TH);
        if (lines !== 0) { xterm.scrollLines(lines); acc -= lines * TH; }
        e.preventDefault();
      }, { passive: false });
      mount.addEventListener('touchend', (e) => {
        clearTimeout(lpTimer); lpTimer = null;
        if (radialActive) { finishRadial(); radialActive = false; e.preventDefault(); }
      });
      mount.addEventListener('touchcancel', () => { clearTimeout(lpTimer); lpTimer = null; if (radialActive) { closeRadial(); radialActive = false; } });

      // Scroll-to-bottom button.
      const scrollBtn = cellEl.querySelector('.term-scroll-bottom');
      const updateScrollBtn = () => { if (!scrollBtn) return; const buf = xterm.buffer.active; scrollBtn.classList.toggle('visible', buf.viewportY < buf.baseY); };
      xterm.onScroll(updateScrollBtn);
      xterm.onWriteParsed(updateScrollBtn);

      const view = { term: xterm, fit, mount, cellEl, scrollBtn };
      termViews.set(t.id, view);
      // Auto-refit whenever this terminal's drawable area changes size — covers
      // the input box auto-growing, a sibling terminal being added, the tools
      // panel opening, orientation changes, etc. (debounced to coalesce bursts).
      if (window.ResizeObserver) {
        const ro = new ResizeObserver(() => { clearTimeout(view._roTimer); view._roTimer = setTimeout(() => fitTerm(t.id), 60); });
        ro.observe(mount);
        view.ro = ro;
      }
      if (ws && ws.readyState === 1) ws.send(JSON.stringify({ type: 'subscribe', id: t.id }));
      setTimeout(() => fitTerm(t.id), 30);
    }

    function destroyCell(cellEl) {
      const tid = cellEl.dataset.tid;
      if (tid && termViews.has(tid)) {
        const v = termViews.get(tid);
        try { if (v.ro) v.ro.disconnect(); } catch { /* ignore */ }
        try { if (ws && ws.readyState === 1) ws.send(JSON.stringify({ type: 'unsubscribe', id: tid })); } catch { /* ignore */ }
        try { v.term.dispose(); } catch { /* ignore */ }
        termViews.delete(tid);
      }
      // Panel cell: park any relocated tab-panel back to its tab slot first so we
      // don't delete the real panel node along with the cell.
      const hosted = cellEl.querySelector('.wb-cell-body > .tab-panel');
      if (hosted) parkPanelNode(hosted);
      cellEl.remove();
    }

    // ─── Workbench panel cells (Files / Git / Kanban / Mindmap) ───
    function parkPanelNode(node) {
      const area = document.querySelector('.terminal-area');
      if (!node || !area) return;
      node.classList.remove('wb-embedded', 'visible');
      area.appendChild(node);
    }
    function parkWbPanels() {
      document.querySelectorAll('.wb-cell-body > .tab-panel').forEach(parkPanelNode);
    }
    function wbLoad(kind) {
      if (kind === 'files') loadFiles();
      else if (kind === 'git') loadGitInfo();
      else if (kind === 'kanban') loadKanban();
      else if (kind === 'mindmap') loadMindmap();
      else if (kind === 'media') loadMedia();
      else if (kind === 'browser') initBrowserPanel();
    }
    // Move each workbench cell's panel node into its body (if not already there)
    // and refresh it. Only loads when it actually (re)mounts to avoid churn.
    function embedWbPanels() {
      // Only relocate while the Workbench tab is active — otherwise we'd yank a
      // panel node out of the tab the user is currently viewing.
      if (currentTab !== 'workbench') return;
      for (const c of wbCells) {
        const body = document.querySelector('.wb-cell-body[data-wb="' + c.id + '"]');
        const node = document.getElementById(c.kind + '-panel');
        if (!body || !node) continue;
        if (node.parentNode !== body) {
          node.classList.add('wb-embedded');
          body.appendChild(node);
          wbLoad(c.kind);
        }
      }
    }
    function addWbPanel(kind) {
      if (!currentProject) { toast('Select a project first', 'error'); return; }
      if (!WB_KIND_META[kind]) return;
      if (currentTab !== 'workbench') switchTab('workbench');
      syncWbStateProject();
      const existing = wbCells.find(c => c.kind === kind);
      if (existing) { toast(WB_KIND_META[kind].label + ' is already in the workbench', ''); return; }
      wbCells.push({ id: genLocalId(), kind });
      persistWbState();
      renderTermGrid();
    }
    function closeWbCell(wbid) {
      const cell = wbCells.find(c => c.id === wbid);
      if (cell) {
        if (cell.kind === 'browser') brzPost('close');
        const node = document.getElementById(cell.kind + '-panel');
        const body = document.querySelector('.wb-cell-body[data-wb="' + wbid + '"]');
        if (node && body && node.parentNode === body) parkPanelNode(node);
      }
      wbCells = wbCells.filter(c => c.id !== wbid);
      wbOrder = wbOrder.filter(k => k !== 'panel:' + wbid);
      persistWbState();
      renderTermGrid();
    }
    function refreshWbCell(wbid) {
      const cell = wbCells.find(c => c.id === wbid);
      if (cell) wbLoad(cell.kind);
    }

    // ─── Interactive browser panel (desktop app only) ───────────────────────────
    // A real Chromium WebContentsView is managed in the Electron main process and
    // overlaid on this panel's stage (see app/main.js). In the desktop app the web
    // UI is the top-level page, so the preload's window.api.wbrowser is available
    // and we call it directly (rect / navigation / device-emulation), and receive
    // page state via api.wbrowser.onState. Picking a device constrains the view
    // (scaled + centered + touch); "Responsive" fills the panel.
    const BRZ_API = () => (window.api && window.api.wbrowser) || null;
    const BRZ_DESKTOP = !!BRZ_API();
    const BRZ_ID = 'wb';
    const BRZ_START = 'https://www.google.com';
    const BRZ_DEVICES = [
      { name: 'Responsive', responsive: true },
      { name: 'iPhone SE', w: 375, h: 667, dpr: 2, mobile: true, ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1' },
      { name: 'iPhone 12 Pro', w: 390, h: 844, dpr: 3, mobile: true, ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 16_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/16.0 Mobile/15E148 Safari/604.1' },
      { name: 'iPhone 15 Pro', w: 393, h: 852, dpr: 3, mobile: true, ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1' },
      { name: 'iPhone 16 Pro', w: 393, h: 852, dpr: 3, mobile: true, ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1' },
      { name: 'iPhone 16 Pro Max', w: 430, h: 932, dpr: 3, mobile: true, ua: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1' },
      { name: 'iPad Mini', w: 768, h: 1024, dpr: 2, mobile: true, ua: 'Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1' },
      { name: 'iPad Air', w: 820, h: 1180, dpr: 2, mobile: true, ua: 'Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1' },
      { name: 'iPad Pro', w: 1024, h: 1366, dpr: 2, mobile: true, ua: 'Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1' },
      { name: 'Pixel 5', w: 393, h: 851, dpr: 2.75, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 11; Pixel 5) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.91 Mobile Safari/537.36' },
      { name: 'Pixel 7', w: 412, h: 915, dpr: 2.625, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 13; Pixel 7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36' },
      { name: 'Pixel 8 Pro', w: 430, h: 932, dpr: 2.625, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 14; Pixel 8 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36' },
      { name: 'Galaxy S22', w: 360, h: 800, dpr: 3, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 13; SM-S901B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36' },
      { name: 'Galaxy S24 Ultra', w: 412, h: 915, dpr: 2.625, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 14; SM-S928B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36' },
      { name: 'Galaxy Z Fold 5', w: 344, h: 882, dpr: 2.8, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 13; SM-F946B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Mobile Safari/537.36' },
      { name: 'Galaxy Z Flip 5', w: 264, h: 844, dpr: 2.8, mobile: true, ua: 'Mozilla/5.0 (Linux; Android 13; SM-F731B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/114.0.0.0 Mobile Safari/537.36' },
      { name: 'Surface Pro 9', w: 1200, h: 1800, dpr: 2, mobile: true, ua: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36' },
      { name: 'Desktop 1280', w: 1280, h: 800, dpr: 1, mobile: false, ua: '' },
      { name: 'Full HD 1920', w: 1920, h: 1080, dpr: 1, mobile: false, ua: '' },
    ];
    let brzUrl = BRZ_START, brzDevice = 0, brzLastEmu = '', brzSyncTimer = null;

    function brzPost(action, extra) {
      const w = BRZ_API(); extra = extra || {};
      if (w) {
        if (action === 'open') w.open(BRZ_ID, extra.url);
        else if (action === 'sync') w.sync(BRZ_ID, extra.rect, extra.visible);
        else if (action === 'navigate') w.navigate(BRZ_ID, extra.url);
        else if (action === 'nav') w.nav(BRZ_ID, extra.dir);
        else if (action === 'emulate') w.emulate(BRZ_ID, extra.config, extra.scale);
        else if (action === 'devtools') w.devtools(BRZ_ID);
        else if (action === 'close') w.close(BRZ_ID);
        return;
      }
      // Fallback: if ever embedded in a shell parent, post to it instead.
      try { if (window.parent && window.parent !== window) window.parent.postMessage(Object.assign({ ns: 'crundi-browser', id: BRZ_ID, action }, extra), '*'); } catch { /* ignore */ }
    }
    function brzStage() { return document.getElementById('brz-stage'); }
    function brzModalOpen() { return !!document.querySelector('.input-modal.visible, .lightbox.visible, #image-lightbox.visible, .modal.visible'); }
    // A native WebContentsView floats above the DOM, so it must hide whenever
    // something should appear over the panel: an open dropdown, a drag, a resize.
    function brzOverlayActive() {
      const menu = document.getElementById('wb-add-menu');
      if (menu && menu.classList.contains('visible')) return true;
      if (document.body.classList.contains('mosaic-cell-drag') || document.body.classList.contains('mosaic-resizing')) return true;
      return false;
    }
    function brzVisibleNow() {
      if (!BRZ_DESKTOP || currentTab !== 'workbench' || brzModalOpen() || brzOverlayActive()) return false;
      const st = brzStage(); if (!st || !st.closest('.wb-cell-body')) return false;
      const r = st.getBoundingClientRect();
      if (r.width < 2 || r.height < 2) return false;
      // A native webview floats above the page, so hide it if a floating editor
      // window overlaps the stage (otherwise it would cover the editor).
      for (const w of document.querySelectorAll('.fe-window')) {
        const wr = w.getBoundingClientRect();
        if (wr.left < r.right && wr.right > r.left && wr.top < r.bottom && wr.bottom > r.top) return false;
      }
      return true;
    }
    function brzComputeRect() {
      const st = brzStage(); if (!st) return null;
      const r = st.getBoundingClientRect();
      const dev = BRZ_DEVICES[brzDevice];
      if (!dev || dev.responsive) {
        return { left: r.left, top: r.top, width: Math.round(r.width), height: Math.round(r.height), scale: 1, config: null };
      }
      const pad = 10;
      const availW = Math.max(40, r.width - pad * 2), availH = Math.max(40, r.height - pad * 2);
      const scale = Math.min(availW / dev.w, availH / dev.h); // fit both dims (may zoom in for small devices)
      const bw = Math.round(dev.w * scale), bh = Math.round(dev.h * scale);
      return {
        left: r.left + (r.width - bw) / 2, top: r.top + (r.height - bh) / 2, width: bw, height: bh, scale,
        config: { width: dev.w, height: dev.h, deviceScaleFactor: dev.dpr || 0, mobile: !!dev.mobile, ua: dev.ua || '' },
      };
    }
    function brzSync() {
      if (!BRZ_DESKTOP) return;
      if (!brzVisibleNow()) { brzPost('sync', { rect: null, visible: false }); return; }
      const c = brzComputeRect(); if (!c) { brzPost('sync', { rect: null, visible: false }); return; }
      // The web UI is the top-level page, so getBoundingClientRect coords map
      // directly to the window content (WebContentsView setBounds) — no offset.
      // scale rides along so the view zooms to fit on resize (no reload needed).
      brzPost('sync', { rect: { x: c.left, y: c.top, width: c.width, height: c.height }, visible: true });
      // Re-apply emulation when the device OR the fit-scale changes. Main rescales
      // the image for a scale-only change and reloads only when the device itself
      // changes (so the site re-serves the right mobile/desktop markup).
      const key = JSON.stringify(c.config) + '|' + (c.scale ? c.scale.toFixed(3) : '');
      if (key !== brzLastEmu) { brzLastEmu = key; brzPost('emulate', { config: c.config, scale: c.scale }); }
    }
    function brzGo(v) {
      v = (v || '').trim(); if (!v) return;
      let url = v;
      const hasScheme = /^https?:\\/\\//i.test(v) || v.indexOf('about:') === 0;
      const looksDomain = v.indexOf(' ') < 0 && v.indexOf('.') > 0;
      if (!hasScheme && !looksDomain) url = 'https://www.google.com/search?q=' + encodeURIComponent(v);
      else if (!hasScheme) url = 'https://' + v;
      brzUrl = url;
      brzPost('navigate', { url });
      const inp = document.querySelector('#browser-panel .brz-url'); if (inp) inp.blur();
    }
    function brzDeviceOptions() {
      return BRZ_DEVICES.map((d, i) => '<option value="' + i + '">' + escHtml(d.responsive ? 'Responsive (fill)' : (d.name + '  ' + d.w + '×' + d.h)) + '</option>').join('');
    }
    function initBrowserPanel() {
      const node = document.getElementById('browser-panel');
      if (!node) return;
      if (!BRZ_DESKTOP) {
        node.innerHTML = '<div class="brz-unavail">' + ic('globe') + '<div>The interactive browser runs a real Chromium view on the host, so it is available in the <b>Crundi desktop app</b> only \\u2014 not over the web.</div></div>';
        return;
      }
      if (!node.querySelector('.brz-bar')) {
        node.innerHTML = ''
          + '<div class="brz-bar">'
          + '<button class="brz-nav" data-brz="back" title="Back" disabled>' + ic('chevron-left') + '</button>'
          + '<button class="brz-nav" data-brz="forward" title="Forward" disabled>' + ic('chevron-right') + '</button>'
          + '<button class="brz-nav" data-brz="reload" title="Reload">' + ic('refresh') + '</button>'
          + '<input class="brz-url" type="text" spellcheck="false" autocomplete="off" placeholder="Enter URL or search\\u2026">'
          + '<select class="brz-device" title="Emulate device">' + brzDeviceOptions() + '</select>'
          + '<button class="brz-nav" data-brz="devtools" title="Toggle DevTools">' + ic('terminal') + '</button>'
          + '</div>'
          + '<div class="brz-stage" id="brz-stage"><div class="brz-hint">Loading\\u2026</div></div>';
        const inp = node.querySelector('.brz-url');
        inp.addEventListener('keydown', (e) => { if (e.key === 'Enter') brzGo(inp.value); });
        node.querySelector('.brz-device').addEventListener('change', (e) => { brzDevice = Number(e.target.value) || 0; brzLastEmu = '__force__'; brzSync(); });
        node.addEventListener('click', (e) => {
          const b = e.target.closest('[data-brz]'); if (!b) return;
          const a = b.dataset.brz;
          if (a === 'reload') brzPost('nav', { dir: 'reload' });
          else if (a === 'back') brzPost('nav', { dir: 'back' });
          else if (a === 'forward') brzPost('nav', { dir: 'forward' });
          else if (a === 'devtools') brzPost('devtools');
        });
        const sel = node.querySelector('.brz-device'); if (sel) sel.value = String(brzDevice);
      }
      brzPost('open', { url: brzUrl || BRZ_START });
      brzLastEmu = '__force__';
      if (!brzSyncTimer) brzSyncTimer = setInterval(brzSync, 200);
      brzSync();
    }
    // Reflect page state (url/title/loading/history or a diagnostic) in the toolbar.
    function brzApplyState(d) {
      if (!d || (d.id && d.id !== BRZ_ID)) return;
      const node = document.getElementById('browser-panel'); if (!node) return;
      if (d.diag) { const hint = node.querySelector('.brz-hint'); if (hint) { hint.style.display = 'flex'; hint.textContent = d.diag; } return; }
      const inp = node.querySelector('.brz-url');
      if (inp && document.activeElement !== inp && d.url) { inp.value = d.url; brzUrl = d.url; }
      const back = node.querySelector('[data-brz="back"]'); if (back) back.disabled = !d.canGoBack;
      const fwd = node.querySelector('[data-brz="forward"]'); if (fwd) fwd.disabled = !d.canGoForward;
      node.classList.toggle('brz-loading', !!d.loading);
      const hint = node.querySelector('.brz-hint'); if (hint && d.url && d.url !== 'about:blank') hint.style.display = 'none';
    }
    // Desktop: receive state directly from the main process via the preload.
    if (BRZ_API() && BRZ_API().onState) { try { BRZ_API().onState(brzApplyState); } catch { /* ignore */ } }
    // Fallback: state relayed from a shell parent (if ever embedded in an iframe).
    window.addEventListener('message', (ev) => {
      const d = ev.data; if (!d || d.ns !== 'crundi-browser-evt') return;
      brzApplyState(d.type === 'diag' ? { id: d.id, diag: d.text } : d);
    });

    // ─── Mosaic workbench layout (desktop only; always-on; persisted per project) ─
    // Tree node: { t:'split', dir:'row'|'col', sizes:[%..], kids:[node..] }
    //          | { t:'leaf', key:<cellKey>|null }
    let wbMosaicByProject = {};
    try { wbMosaicByProject = JSON.parse(localStorage.getItem('crundi_wb_mosaic') || '{}') || {}; } catch { wbMosaicByProject = {}; }
    function saveMosaic() { try { localStorage.setItem('crundi_wb_mosaic', JSON.stringify(wbMosaicByProject)); } catch { /* ignore */ } }
    let _mosSeq = 0;
    function genMosId() { return 'm' + Date.now().toString(36) + (_mosSeq++).toString(36); }
    function mosaicDesktop() { return !isMobileTerm(); }
    // Global mobile preference: opt into the mosaic on phone-width screens (fixed
    // full-width snap columns, only rows resizable). Off by default → flat stack.
    let mobileMosaic = false;
    try { mobileMosaic = localStorage.getItem('crundi_mobile_mosaic') === '1'; } catch { mobileMosaic = false; }
    // Mosaic is used on desktop always, and on mobile only when opted in.
    function mosaicActive() { return !isMobileTerm() || mobileMosaic; }
    function toggleMobileMosaic() {
      mobileMosaic = !mobileMosaic;
      try { localStorage.setItem('crundi_mobile_mosaic', mobileMosaic ? '1' : '0'); } catch { /* ignore */ }
      updateMobileLayoutBtn();
      renderTermGrid();
    }
    function updateMobileLayoutBtn() {
      const b = document.getElementById('mobile-layout-toggle');
      if (b) b.classList.toggle('active', mobileMosaic);
    }
    function currentMosaic() { return currentProject ? (wbMosaicByProject[currentProject] || null) : null; }
    function setMosaic(tree) { if (currentProject) { wbMosaicByProject[currentProject] = tree; saveMosaic(); } }
    function mosLeaf(key) { return { t: 'leaf', _id: genMosId(), key: key || null }; }
    function mosSplit(dir, kids) { return { t: 'split', _id: genMosId(), dir, kids, sizes: kids.map(() => 100 / kids.length) }; }
    function mosaicLeaves(node, out = []) {
      if (!node) return out;
      if (node.t === 'leaf') out.push(node); else node.kids.forEach(k => mosaicLeaves(k, out));
      return out;
    }
    function mosaicKeySet(node) { return new Set(mosaicLeaves(node).filter(l => l.key).map(l => l.key)); }
    function mosaicHasKey(node, key) { return mosaicLeaves(node).some(l => l.key === key); }
    function mosaicLeafById(node, id) { return mosaicLeaves(node).find(l => l._id === id) || null; }
    function mosaicFind(root, id, parent = null, index = -1) {
      if (!root) return null;
      if (root._id === id) return { node: root, parent, index };
      if (root.t === 'split') for (let i = 0; i < root.kids.length; i++) { const r = mosaicFind(root.kids[i], id, root, i); if (r) return r; }
      return null;
    }
    function mosaicDefault(keys) {
      if (!keys.length) return mosLeaf(null);
      if (keys.length === 1) return mosLeaf(keys[0]);
      return mosSplit('row', keys.map(k => mosLeaf(k)));
    }
    // Keep the tree in sync with the live cell-key set: drop gone cells (collapse),
    // add new cells into an empty leaf or append a column.
    function mosaicSync(keys) {
      let tree = currentMosaic();
      if (!tree) { tree = mosaicDefault(keys); setMosaic(tree); return tree; }
      for (const k of [...mosaicKeySet(tree)]) if (!keys.includes(k)) tree = mosaicRemoveKey(tree, k);
      for (const k of keys) if (!mosaicHasKey(tree, k)) tree = mosaicAddKey(tree, k);
      setMosaic(tree);
      return tree;
    }
    function mosaicAddKey(tree, key) {
      const empty = mosaicLeaves(tree).find(l => !l.key);
      if (empty) { empty.key = key; return tree; }
      if (tree.t === 'leaf') return mosSplit('row', [tree, mosLeaf(key)]);
      tree.kids.push(mosLeaf(key));
      tree.sizes = tree.kids.map(() => 100 / tree.kids.length);
      return tree;
    }
    function mosaicRemoveKey(tree, key) {
      const leaf = mosaicLeaves(tree).find(l => l.key === key);
      return leaf ? mosaicCollapseLeaf(tree, leaf._id) : tree;
    }
    // Remove a leaf, collapsing its parent if it drops to a single child.
    function mosaicCollapseLeaf(tree, leafId) {
      const f = mosaicFind(tree, leafId);
      if (!f) return tree;
      if (!f.parent) return mosLeaf(null); // root leaf -> empty
      const parent = f.parent;
      parent.kids.splice(f.index, 1);
      parent.sizes = parent.kids.map(() => 100 / parent.kids.length);
      if (parent.kids.length === 1) {
        const only = parent.kids[0];
        const pf = mosaicFind(tree, parent._id);
        if (!pf.parent) return only;            // parent was root
        pf.parent.kids[pf.index] = only;        // hoist the single child
      }
      return tree;
    }
    // Split a leaf: keep its cell, add an empty leaf beside it (drop target).
    function mosaicSplitLeaf(tree, leafId, dir) {
      const f = mosaicFind(tree, leafId);
      if (!f || f.node.t !== 'leaf') return tree;
      const leaf = f.node, parent = f.parent, newLeaf = mosLeaf(null);
      if (parent && parent.dir === dir) {
        parent.kids.splice(f.index + 1, 0, newLeaf);
        parent.sizes = parent.kids.map(() => 100 / parent.kids.length);
        return tree;
      }
      const split = mosSplit(dir, [leaf, newLeaf]);
      if (!parent) return split;
      parent.kids[f.index] = split;
      return tree;
    }
    // Drag a cell onto a leaf: swap if occupied, else move (collapse source slot).
    function mosaicAssignToLeaf(tree, leafId, key) {
      const target = mosaicLeafById(tree, leafId);
      if (!target) return tree;
      const source = mosaicLeaves(tree).find(l => l.key === key);
      if (source && source._id === target._id) return tree;
      if (!source) { target.key = key; return tree; }
      if (target.key) { const tmp = target.key; target.key = source.key; source.key = tmp; return tree; }
      target.key = key; source.key = null;
      return mosaicCollapseLeaf(tree, source._id);
    }
    // Double-click a pane → expand it to fill its parent split (siblings drop to
    // their min-width floor); double-click again → even split.
    function mosaicMaximizeLeaf(tree, leafId) {
      const f = mosaicFind(tree, leafId);
      if (!f || !f.parent) return tree; // a lone root pane is already full
      const parent = f.parent, n = parent.kids.length;
      const big = 100 - (n - 1) * 8;
      const alreadyMax = parent.sizes[f.index] >= big - 0.5;
      parent.sizes = parent.kids.map((_, i) => alreadyMax ? 100 / n : (i === f.index ? big : 8));
      return tree;
    }
    // Double-click a gutter → reset that split to an even distribution.
    function mosaicEvenSplit(tree, splitId) {
      const f = mosaicFind(tree, splitId);
      if (f && f.node.t === 'split') f.node.sizes = f.node.kids.map(() => 100 / f.node.kids.length);
      return tree;
    }
    function mosaicApplyPreset(preset) {
      if (!mosaicDesktop()) { toast('Layouts are available on desktop only', ''); return; }
      const keys = currentDesiredKeys();
      const L = (i) => mosLeaf(keys[i] || null);
      let tree;
      switch (preset) {
        case 'cols2': tree = mosSplit('row', [L(0), L(1)]); break;
        case 'cols3': tree = mosSplit('row', [L(0), L(1), L(2)]); break;
        case 'cross4': tree = mosSplit('row', [mosSplit('col', [L(0), L(2)]), mosSplit('col', [L(1), L(3)])]); break;
        case 'left-right2': tree = mosSplit('row', [L(0), mosSplit('col', [L(1), L(2)])]); break;
        case 'top-bottom2': tree = mosSplit('col', [L(0), mosSplit('row', [L(1), L(2)])]); break;
        default: return;
      }
      setMosaic(tree);
      // place any cells beyond the preset's slots
      let t = currentMosaic();
      for (const k of keys) if (!mosaicHasKey(t, k)) t = mosaicAddKey(t, k);
      setMosaic(t);
      renderTermGrid();
    }
    // The "+" dropdown by the Workbench tab.
    function toggleWbAddMenu() {
      const menu = $('#wb-add-menu'); const btn = $('#tab-add-term');
      if (!menu || !btn) return;
      if (menu.classList.contains('visible')) { hideWbAddMenu(); return; }
      const r = btn.getBoundingClientRect();
      menu.style.top = (r.bottom + 4) + 'px';
      menu.style.left = Math.max(6, Math.min(r.left, window.innerWidth - 166)) + 'px';
      menu.style.display = 'flex';
      menu.classList.add('visible');
      if (typeof brzSync === 'function') brzSync(); // hide the browser view behind the dropdown
      setTimeout(() => document.addEventListener('click', wbAddMenuOutside, true), 0);
    }
    function hideWbAddMenu() {
      const menu = $('#wb-add-menu'); if (!menu) return;
      const wasVisible = menu.classList.contains('visible');
      menu.classList.remove('visible'); menu.style.display = 'none';
      if (wasVisible && typeof brzSync === 'function') setTimeout(brzSync, 0); // restore the browser view
      document.removeEventListener('click', wbAddMenuOutside, true);
    }
    function wbAddMenuOutside(e) {
      const menu = $('#wb-add-menu');
      if (menu && !menu.contains(e.target) && e.target.id !== 'tab-add-term') hideWbAddMenu();
    }

    function focusTerm(id) {
      if (!id) return;
      if (focusedTermId !== id) { focusedTermId = id; updateFocusStyles(); }
    }
    function updateFocusStyles() {
      const grid = $('#term-grid'); if (!grid) return;
      // Cells may be nested inside mosaic leaves, so query deep (not just direct
      // children) for the terminal cells.
      grid.querySelectorAll('.term-cell[data-tid]').forEach(ch => ch.classList.toggle('focused', ch.dataset.tid === focusedTermId));
    }

    function fitTerm(id) {
      const v = termViews.get(id); if (!v) return;
      try { v.fit.fit(); if (ws && ws.readyState === 1) ws.send(JSON.stringify({ type: 'resize', id, cols: v.term.cols, rows: v.term.rows })); } catch { /* ignore */ }
    }
    function fitAllTerms() { for (const id of termViews.keys()) fitTerm(id); }

    function setTermFont(id, dir) {
      const v = termViews.get(id); if (!v) return;
      const cur = termFont[id] || 14;
      const next = Math.max(8, Math.min(28, cur + (dir < 0 ? -1 : 1)));
      termFont[id] = next;
      try { v.term.options.fontSize = next; } catch { /* ignore */ }
      localStorage.setItem('crundi_term_font', JSON.stringify(termFont));
      fitTerm(id);
    }

    function resetTermFont(id) {
      const v = termViews.get(id); if (!v) return;
      delete termFont[id];
      try { v.term.options.fontSize = 14; } catch { /* ignore */ }
      localStorage.setItem('crundi_term_font', JSON.stringify(termFont));
      fitTerm(id);
    }

    function renameTerminal(id) {
      const grid = $('#term-grid'); if (!grid) return;
      const el = grid.querySelector('.term-cell[data-tid="' + id + '"]');
      if (!el) return;
      const titleEl = el.querySelector('.term-title');
      if (!titleEl || titleEl.tagName === 'INPUT') return;
      const cur = (terminals.find(t => t.id === id) || {}).title || 'Terminal';
      const input = document.createElement('input');
      input.className = 'term-title-input';
      input.value = cur;
      titleEl.replaceWith(input);
      input.focus(); input.select();
      let done = false;
      const commit = async (save) => {
        if (done) return; done = true;
        const val = (input.value || '').trim() || 'Terminal';
        const span = document.createElement('span');
        span.className = 'term-title';
        span.dataset.action = 'term-rename';
        span.dataset.tid = id;
        span.title = 'Click to rename';
        span.textContent = save ? val : cur;
        input.replaceWith(span);
        if (save && val !== cur) {
          const tt = terminals.find(t => t.id === id); if (tt) tt.title = val;
          try { await apiFetch('/api/terminals/' + encodeURIComponent(id) + '/rename', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ title: val }) }); } catch { /* ignore */ }
        }
      };
      input.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); commit(true); } else if (e.key === 'Escape') { e.preventDefault(); commit(false); } });
      input.addEventListener('blur', () => commit(true));
    }

    // Drag-to-reorder terminals (horizontal on desktop, vertical on mobile).
    let _termDropLine = null;
    // Reorder among ALL grid cells (terminals + panels), identified by cellkey.
    function termReorderTarget(x, y, draggedKey) {
      const grid = $('#term-grid'); if (!grid) return null;
      const cells = [...grid.children].filter(c => c.dataset.cellkey && c.dataset.cellkey !== draggedKey && c !== _termDropLine);
      const mobile = isMobileTerm();
      for (let i = 0; i < cells.length; i++) {
        const r = cells[i].getBoundingClientRect();
        const mid = mobile ? r.top + r.height / 2 : r.left + r.width / 2;
        if ((mobile ? y : x) < mid) return { index: i, before: cells[i] };
      }
      return { index: cells.length, before: null };
    }
    function termDragHandlers(key) {
      let target = null;
      let leafEl = null; // mosaic drop-target leaf
      const mosaicMode = () => currentMosaic() && mosaicDesktop();
      // On mobile mosaic, header-drag does nothing (drag-to-place is desktop-only,
      // and the flat reorder must not run against the mosaic skeleton).
      const mobileMosaicOn = () => mosaicActive() && isMobileTerm();
      const clearLeaf = () => { if (leafEl) { leafEl.classList.remove('drop-hover'); leafEl = null; } };
      return {
        onStart: () => { if (mosaicMode()) { document.body.classList.add('mosaic-cell-drag'); if (typeof brzSync === 'function') brzSync(); } },
        onMove: (x, y) => {
          if (mobileMosaicOn()) return;
          // Mosaic: highlight the leaf under the pointer (drop = place cell there).
          if (mosaicMode()) {
            clearLeaf();
            const under = document.elementFromPoint(x, y);
            const lf = under && under.closest('.mosaic-leaf');
            if (lf && lf.dataset.leafId) { leafEl = lf; lf.classList.add('drop-hover'); }
            return;
          }
          const grid = $('#term-grid'); if (!grid) return;
          if (!_termDropLine) { _termDropLine = document.createElement('div'); _termDropLine.className = 'term-drop-line'; }
          target = termReorderTarget(x, y, key);
          if (!target) { if (_termDropLine.parentNode) _termDropLine.remove(); return; }
          if (target.before) grid.insertBefore(_termDropLine, target.before);
          else grid.appendChild(_termDropLine);
        },
        onEnd: async (commit) => {
          if (mobileMosaicOn()) return;
          if (mosaicMode()) {
            document.body.classList.remove('mosaic-cell-drag');
            if (typeof brzSync === 'function') setTimeout(brzSync, 0);
            const lid = leafEl && leafEl.dataset.leafId;
            clearLeaf();
            if (commit && lid) { setMosaic(mosaicAssignToLeaf(currentMosaic(), lid, key)); renderTermGrid(); }
            target = null;
            return;
          }
          if (_termDropLine && _termDropLine.parentNode) _termDropLine.remove();
          if (commit && target) {
            // Reorder the unified key list, then persist terminal-relative order.
            const keys = wbOrder.slice();
            const from = keys.indexOf(key);
            if (from !== -1) {
              keys.splice(from, 1);
              keys.splice(Math.min(target.index, keys.length), 0, key);
              wbOrder = keys;
              const termOrder = keys.filter(k => k.startsWith('live:')).map(k => k.slice(5));
              termOrder.forEach((tid, i) => { const tt = terminals.find(t => t.id === tid); if (tt) tt.order = i; });
              renderTermGrid();
              if (termOrder.length) {
                try { await apiFetch('/api/terminals/reorder', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project: currentProject, order: termOrder }) }); } catch { /* ignore */ }
              }
            }
          }
          target = null;
        },
      };
    }

    // One-time wiring for the terminal area: unified-input paste (images), file
    // drag-drop onto the grid, and a debounced refit on window resize.
    // Drag a mosaic gutter to resize the two adjacent panes. Uses pointer events
    // so it works with both mouse and touch (touch-action:none on the gutter
    // stops the page from scrolling while dragging a row divider on mobile).
    function setupMosaicResize() {
      const grid = document.getElementById('term-grid');
      if (!grid || grid._mosResize) return; grid._mosResize = true;
      // Double-click: a gutter → even split; a pane header / empty pane → expand
      // that pane to fill its split (toggle).
      grid.addEventListener('dblclick', (e) => {
        if (!currentMosaic() || !mosaicActive()) return;
        const g = e.target.closest('.mosaic-gutter');
        if (g) { setMosaic(mosaicEvenSplit(currentMosaic(), g.dataset.splitId)); renderTermGrid(); return; }
        const leaf = e.target.closest('.mosaic-leaf'); if (!leaf) return;
        // Only via the header or an empty pane — never from inside a terminal
        // (where double-click selects a word).
        if (!e.target.closest('.term-head') && !e.target.closest('.mosaic-empty')) return;
        setMosaic(mosaicMaximizeLeaf(currentMosaic(), leaf.dataset.leafId)); renderTermGrid();
      });
      grid.addEventListener('pointerdown', (e) => {
        const g = e.target.closest('.mosaic-gutter'); if (!g) return;
        const split = g.parentNode; const node = mosaicFind(currentMosaic(), g.dataset.splitId);
        if (!node) return;
        e.preventDefault();
        const gi = parseInt(g.dataset.gutter, 10);
        const kids = [...split.children].filter(c => !c.classList.contains('mosaic-gutter'));
        const a = kids[gi - 1], b = kids[gi];
        const horiz = node.node.dir === 'row';
        const ra = a.getBoundingClientRect(), rb = b.getBoundingClientRect();
        const totalPx = horiz ? (ra.width + rb.width) : (ra.height + rb.height);
        const sizes = node.node.sizes;
        const sA0 = sizes[gi - 1], sB0 = sizes[gi], totalS = sA0 + sB0;
        const start = horiz ? e.clientX : e.clientY;
        document.body.classList.add('mosaic-resizing');
        if (typeof brzSync === 'function') brzSync();
        const mv = (ev) => {
          const cur = horiz ? ev.clientX : ev.clientY;
          const frac = totalPx ? (cur - start) / totalPx : 0;
          let nA = sA0 + frac * totalS;
          nA = Math.max(totalS * 0.1, Math.min(totalS * 0.9, nA));
          const nB = totalS - nA;
          sizes[gi - 1] = nA; sizes[gi] = nB;
          a.style.flex = nA + ' 1 0'; b.style.flex = nB + ' 1 0';
        };
        const up = () => {
          document.removeEventListener('pointermove', mv); document.removeEventListener('pointerup', up);
          document.body.classList.remove('mosaic-resizing');
          if (typeof brzSync === 'function') setTimeout(brzSync, 0);
          saveMosaic(); fitAllTerms();
        };
        document.addEventListener('pointermove', mv); document.addEventListener('pointerup', up);
      });
    }

    function setupTerminalArea() {
      setupMosaicResize();
      const ta = document.getElementById('term-input');
      if (ta) {
        ta.addEventListener('paste', async (e) => {
          if (!currentProject) return;
          let imgFile = null;
          const items = e.clipboardData?.items || [];
          for (const item of items) { if (item.type && item.type.startsWith('image/')) { imgFile = item.getAsFile(); break; } }
          if (!imgFile && e.clipboardData?.files?.length) { for (const f of e.clipboardData.files) { if (f.type && f.type.startsWith('image/')) { imgFile = f; break; } } }
          if (imgFile) { e.preventDefault(); await uploadAttachment(imgFile); return; }
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
      }

      const wrap = document.querySelector('.terminal-wrap');
      if (wrap) {
        wrap.addEventListener('dragover', (e) => {
          if (!currentProject) return;
          e.preventDefault(); e.dataTransfer.dropEffect = 'copy';
          wrap.style.outline = '2px solid var(--accent)'; wrap.style.outlineOffset = '-2px';
        });
        wrap.addEventListener('dragleave', () => { wrap.style.outline = ''; wrap.style.outlineOffset = ''; });
        wrap.addEventListener('drop', async (e) => {
          e.preventDefault(); wrap.style.outline = ''; wrap.style.outlineOffset = '';
          if (!currentProject) return;
          const files = e.dataTransfer?.files;
          if (files && files.length > 0) {
            const paths = [];
            for (const file of files) {
              if (file.type && file.type.startsWith('image/')) await uploadAttachment(file);
              else { const resolved = window.api?.getPathForFile?.(file); paths.push(resolved || file.path || file.name); }
            }
            if (paths.length) { insertIntoTermInput(paths.join(' ')); toast(paths.length === 1 ? 'File added' : paths.length + ' files added'); }
            return;
          }
          const text = e.dataTransfer?.getData('text/plain');
          if (text) insertIntoTermInput(text);
        });
      }

      // Refit on resize; if we cross the desktop/mobile breakpoint, re-render the
      // grid so it switches between the mosaic layout and the mobile flat stack.
      let _wasMobile = isMobileTerm();
      window.addEventListener('resize', () => {
        clearTimeout(resizeTimer);
        resizeTimer = setTimeout(() => {
          const m = isMobileTerm();
          if (m !== _wasMobile) { _wasMobile = m; if (currentProject && currentTab === 'workbench') renderTermGrid(); }
          fitAllTerms();
        }, 100);
      });

      // Mobile: when the on-screen keyboard opens it shrinks the visual viewport.
      // Bind #app to that height so the input bar + focused terminal stay visible
      // above the keyboard (no manual scrolling). Desktop keeps the CSS 100dvh.
      const vv = window.visualViewport;
      if (vv) {
        const syncViewport = () => {
          const app = document.getElementById('app');
          if (!app) return;
          if (window.matchMedia('(max-width: 768px)').matches) {
            app.style.height = vv.height + 'px';
            clearTimeout(resizeTimer); resizeTimer = setTimeout(fitAllTerms, 80);
          } else if (app.style.height) {
            app.style.height = '';
          }
        };
        vv.addEventListener('resize', syncViewport);
        vv.addEventListener('scroll', syncViewport);
      }
    }

    // Bring a freshly-focused field/terminal into view above the keyboard.
    function scrollFocusIntoView(el) {
      if (!el || !window.matchMedia('(max-width: 768px)').matches) return;
      setTimeout(() => { try { el.scrollIntoView({ block: 'nearest', behavior: 'smooth' }); } catch { /* ignore */ } }, 250);
    }

    function termToggleTools() {
      const panel = document.getElementById('term-tool-panel');
      const btn = document.getElementById('term-tool-toggle');
      if (!panel) return;
      panel.classList.toggle('visible');
      if (btn) btn.classList.toggle('active', panel.classList.contains('visible'));
      setTimeout(fitAllTerms, 50);
    }

    function termSendKey(key) {
      if (!ws || ws.readyState !== 1 || !focusedTermId) return;
      const keyMap = {
        'Escape': '\\x1b',
        'Tab': '\\t',
        'shift+tab': '\\x1b[Z',
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
      if (seq) ws.send(JSON.stringify({ type: 'input', id: focusedTermId, data: seq }));
    }

    // ─── Mobile radial quick-tools overlay (long-press a terminal) ───
    // Center is empty; cardinal slots are the arrow keys, diagonals are Esc / Tab
    // / Ctrl+C / Enter. Release over a slot sends that key; release in the center
    // (or off any slot) cancels.
    let _radial = null;
    const RADIAL_SLOTS = [
      { key: 'ArrowUp', html: '\\u25B2', ang: -90 },
      { key: 'ArrowDown', html: '\\u25BC', ang: 90 },
      { key: 'ArrowLeft', html: '\\u25C0', ang: 180 },
      { key: 'ArrowRight', html: '\\u25B6', ang: 0 },
      { key: 'Escape', html: 'Esc', ang: -135 },
      { key: 'Tab', html: 'Tab', ang: -45 },
      { key: 'ctrl+c', html: '^C', ang: 135 },
      { key: 'Enter', html: '\\u23ce', ang: 45 },
    ];
    function openRadial(x, y, termId) {
      closeRadial();
      const R = 80;
      const cx = Math.max(R + 26, Math.min(x, window.innerWidth - R - 26));
      const cy = Math.max(R + 26, Math.min(y, window.innerHeight - R - 26));
      const el = document.createElement('div'); el.className = 'term-radial';
      const center = document.createElement('div'); center.className = 'rcenter';
      center.style.left = (cx - 18) + 'px'; center.style.top = (cy - 18) + 'px';
      el.appendChild(center);
      const slots = RADIAL_SLOTS.map(s => {
        const a = s.ang * Math.PI / 180;
        const bx = cx + R * Math.cos(a), by = cy + R * Math.sin(a);
        const b = document.createElement('div'); b.className = 'rslot'; b.innerHTML = s.html;
        b.style.left = (bx - 24) + 'px'; b.style.top = (by - 24) + 'px';
        el.appendChild(b);
        return { key: s.key, el: b, x: bx, y: by };
      });
      document.body.appendChild(el);
      _radial = { el, slots, cx, cy, termId, active: null };
    }
    function updateRadial(x, y) {
      if (!_radial) return;
      const dx = x - _radial.cx, dy = y - _radial.cy;
      let active = null;
      if (Math.hypot(dx, dy) >= 30) {
        const ang = Math.atan2(dy, dx);
        let best = Infinity;
        for (const s of _radial.slots) {
          const sa = Math.atan2(s.y - _radial.cy, s.x - _radial.cx);
          let d = Math.abs(ang - sa); if (d > Math.PI) d = 2 * Math.PI - d;
          if (d < best) { best = d; active = s; }
        }
      }
      _radial.active = active;
      for (const s of _radial.slots) s.el.classList.toggle('active', s === active);
    }
    function finishRadial() {
      if (!_radial) return;
      const a = _radial.active, tid = _radial.termId;
      closeRadial();
      if (a && tid) { focusedTermId = tid; termSendKey(a.key); }
    }
    function closeRadial() { if (_radial) { _radial.el.remove(); _radial = null; } }

    function termSendInput() {
      const ta = document.getElementById('term-input');
      if (!ta || !ws || ws.readyState !== 1 || !focusedTermId) return;
      const text = ta.value;
      if (!text) return;
      const tid = focusedTermId;
      // Send the text first, then the Enter as a SEPARATE write. For a large or
      // multi-line chunk the agent TUI treats the burst as a paste and would
      // otherwise swallow a trailing CR into the pasted buffer instead of
      // submitting — so the carriage return must arrive on its own.
      ws.send(JSON.stringify({ type: 'input', id: tid, data: text }));
      setTimeout(() => {
        if (ws && ws.readyState === 1) ws.send(JSON.stringify({ type: 'input', id: tid, data: '\\r' }));
      }, 60);
      ta.value = '';
      ta.style.height = 'auto';
      const v = termViews.get(focusedTermId);
      if (v) v.term.scrollToBottom();
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

    // ─── Drag a workbench-panel item onto a terminal / the input box ───
    // Returns where a screen point lands: a specific terminal, the input, or null.
    function wbDropTargetAt(x, y) {
      const el = document.elementFromPoint(x, y);
      if (!el || !el.closest) return null;
      if (el.closest('#term-input')) return { kind: 'input' };
      const cell = el.closest('.term-cell[data-tid]');
      if (cell) return { kind: 'term', id: cell.dataset.tid };
      return null;
    }
    function insertRefToTarget(target, ref) {
      if (!ref || !target) return;
      if (target.kind === 'term' && ws && ws.readyState === 1) {
        // Trailing space so the next token doesn't fuse onto the ref, then focus
        // the terminal so the user can keep typing right away.
        ws.send(JSON.stringify({ type: 'input', id: target.id, data: ref + ' ' }));
        focusedTermId = target.id;
        const v = termViews.get(target.id);
        if (v && v.term) { try { v.term.focus(); } catch { /* ignore */ } }
        updateFocusStyles();
        toast('Sent to terminal');
      } else {
        insertIntoTermInput(ref); // already pads with spaces + focuses the input
      }
    }
    // Pointer-based drag for panel rows (Files / Git / Kanban subtasks) so they
    // can be dropped onto a terminal/input on BOTH desktop AND mobile. (Native
    // HTML5 drag doesn't fire on touch, which is why this previously only worked
    // on PC.) Each row carries data-drag-ref / -kind / -abs.
    function dragRefHandlers(el) {
      let target = null;
      return {
        onMove: (x, y) => { target = wbDropTargetAt(x, y); document.body.classList.toggle('wb-drag-armed', !!target); },
        onEnd: (commit) => {
          document.body.classList.remove('wb-drag-armed');
          if (commit && target) {
            let v = el.dataset.dragRef || '';
            if (el.dataset.dragAbs === '1') v = absProjectPath(v);
            insertRefToTarget(target, formatDragRef(el.dataset.dragKind, v));
          }
          target = null;
        },
      };
    }
    // Attach the pointer-drag to every [data-drag-ref] row inside a container.
    function wireDragRefs(container) {
      (container || document).querySelectorAll('[data-drag-ref]').forEach(el => {
        if (el._dragRefWired) return;
        el._dragRefWired = true;
        makeDraggable(el, dragRefHandlers(el));
      });
    }

    // Tag a dragged reference with its type so the agent knows what it received,
    // e.g. "[File Path: C:\\proj\\x.js]", "[Kanban Task ID: ab12]".
    const DRAG_LABELS = {
      file: 'File Path', folder: 'Folder Path',
      'kanban-task': 'Kanban Task ID', 'kanban-subtask': 'Kanban Subtask ID',
      mindmap: 'Mindmap Idea ID', media: 'Media Path',
    };
    function formatDragRef(kind, value) {
      const label = DRAG_LABELS[kind];
      return label ? '[' + label + ': ' + value + ']' : value;
    }
    // Native HTML5 drag from Files/Git rows + Kanban subtasks (elements carry
    // data-drag-ref; data-drag-abs="1" = resolve to an absolute path; data-drag-kind
    // selects the type label).
    document.addEventListener('dragstart', (e) => {
      const item = e.target.closest && e.target.closest('[data-drag-ref]');
      if (!item || !e.dataTransfer) return;
      let val = item.dataset.dragRef;
      if (item.dataset.dragAbs === '1') val = absProjectPath(val);
      e.dataTransfer.setData('text/plain', formatDragRef(item.dataset.dragKind, val));
      e.dataTransfer.effectAllowed = 'copy';
    });

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
      // Shorter placeholder on phone-width screens (no Ctrl+Enter there).
      const setInputPh = () => { ta.placeholder = window.innerWidth <= 768 ? 'Type here... (Press Send)' : 'Type here... (Ctrl+Enter or Send button)'; };
      setInputPh();
      window.addEventListener('resize', setInputPh);
      ta.addEventListener('input', () => {
        ta.style.height = 'auto';
        ta.style.height = Math.min(ta.scrollHeight, 120) + 'px';
      });
      ta.addEventListener('focus', () => scrollFocusIntoView(document.getElementById('term-input-bar')));
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

    // Back-compat alias: a few callers still say "fit the terminal".
    function fitTerminal() { fitAllTerms(); }

    // Per-terminal selection overlay (mobile: lets you select/copy text without
    // the touch-scroll interceptor fighting the selection).
    function termToggleSelect(id) {
      const v = termViews.get(id); if (!v) return;
      const overlay = v.cellEl.querySelector('.term-select-overlay');
      const btn = v.cellEl.querySelector('.term-select-toggle');
      if (!overlay) return;
      const isActive = overlay.classList.contains('visible');
      if (isActive) {
        overlay.classList.remove('visible');
        if (btn) { btn.classList.remove('active'); btn.textContent = 'Select'; }
        return;
      }
      const buf = v.term.buffer.active;
      const lines = [];
      for (let i = 0; i < buf.length; i++) {
        const line = buf.getLine(i);
        if (line) lines.push(line.translateToString(true));
      }
      overlay.textContent = lines.join('\\n');
      overlay.classList.add('visible');
      const lineH = overlay.scrollHeight / Math.max(lines.length, 1);
      overlay.scrollTop = buf.viewportY * lineH;
      if (btn) { btn.classList.add('active'); btn.textContent = 'Done'; }
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
        // Re-subscribe every mounted terminal. Reset each first so the scrollback
        // the server re-sends on subscribe replaces (not duplicates) the buffer.
        for (const [id, v] of termViews) {
          try { v.term.reset(); } catch { /* ignore */ }
          ws.send(JSON.stringify({ type: 'subscribe', id }));
        }
        fitAllTerms();
        reportPresence(true); // re-assert presence on a fresh socket
      };

      ws.onmessage = (e) => {
        let msg;
        try { msg = JSON.parse(e.data); } catch { return; }
        if (msg.type === 'output' && msg.id) {
          const v = termViews.get(msg.id);
          if (v) v.term.write(msg.data);
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

    // ─── Presence ───
    // Tell the server whether the user is actively focused on this window (the
    // browser tab or the Electron app). When present, the server skips the
    // agent-done / needs-input Telegram ping — the user is already here.
    // hasFocus() is true only while THIS tab/window holds OS focus, so a tab in
    // the background, or a window left open on a LOCKED PC (lock drops focus),
    // both report not-present and still get the Telegram alert.
    let _present = null;
    function reportPresence(force) {
      const active = document.hasFocus() && document.visibilityState === 'visible';
      if (!force && active === _present) return;
      _present = active;
      if (ws && ws.readyState === 1) ws.send(JSON.stringify({ type: 'presence', active }));
    }
    window.addEventListener('focus', () => reportPresence());
    window.addEventListener('blur', () => reportPresence());
    document.addEventListener('visibilitychange', () => reportPresence());
    // Heartbeat refreshes the server's presence TTL while focused, so a silently
    // dropped socket's presence lapses (server stops muting) instead of sticking.
    setInterval(() => { if (_present) reportPresence(true); }, 30000);

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
          if (state.scheduled) scheduledProjects = state.scheduled; // projects with upcoming schedules
          if (state.projects) {
            projects = state.projects;
          }
          renderProjects();
          renderTerminals();
          // Status dots/badges update in place every push (cheap, no focus loss).
          updateLiveCellHeads();
          // Only rebuild the grid when the terminal SET/layout actually changes —
          // NOT for agent-status changes (working↔done), which are excluded from
          // the signature so a rebuild can't re-parent cells and steal typing focus.
          const termSig = JSON.stringify(terminals.map(t => [t.id, t.project, t.title, t.order, t.status]));
          if (termSig !== lastTermSig) { lastTermSig = termSig; renderTermGrid(); }
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
      es.addEventListener('schedule', () => { if (currentTab === 'schedule') loadSchedules(); });
      es.addEventListener('mindmap', () => {
        if (currentTab === 'mindmap') loadMindmap();
      });
      es.addEventListener('media', () => {
        // Refresh any open media browser (tab/panel or modal) and embedded views.
        refreshAllMediaViews();
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
      if (token) loadUsage(); // always refresh usage on resume (cheap, server-cached, no force)
      if (!ensureConnections()) return; // connections already alive → don't touch them
      clearTimeout(resumeTimer);
      resumeTimer = setTimeout(() => { // we actually had to reconnect → refresh the active view
        if (!token || !$('#app').classList.contains('visible')) return;
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

    // Desktop: collapse the projects panel to a compact initial-avatar rail.
    function toggleSidebarCollapse() {
      const sb = $('#sidebar'); if (!sb) return;
      const collapsed = sb.classList.toggle('collapsed');
      const btn = $('.sidebar-collapse');
      if (btn) btn.title = collapsed ? 'Expand panel' : 'Collapse panel';
      try { localStorage.setItem('crundi_sidebar_collapsed', collapsed ? '1' : '0'); } catch { /* ignore */ }
      // Terminals refit once the panel finishes its width transition.
      setTimeout(fitAllTerms, 240);
    }
    function applyStoredSidebarCollapsed() {
      const sb = $('#sidebar'); if (!sb) return;
      let collapsed = false;
      try { collapsed = localStorage.getItem('crundi_sidebar_collapsed') === '1'; } catch { /* ignore */ }
      sb.classList.toggle('collapsed', collapsed);
      const btn = $('.sidebar-collapse');
      if (btn) btn.title = collapsed ? 'Expand panel' : 'Collapse panel';
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
      // Return any workbench-embedded panel nodes to their tab slots before we
      // toggle tab visibility (so the destination tab can show its panel). When
      // re-entering the Workbench tab, renderTermGrid() re-embeds them.
      parkWbPanels();
      currentTab = tab;
      $$('.tab-btn').forEach(b => b.classList.toggle('active', b.dataset.tab === tab));
      $$('.tab-panel').forEach(p => p.classList.toggle('visible', p.dataset.panel === tab));
      if (tab === 'workbench') { renderTermGrid(); setTimeout(() => { fitAllTerms(); const v = focusedTermId && termViews.get(focusedTermId); if (v) v.term.focus(); }, 30); }
      if (tab === 'services') loadServices();
      if (tab === 'terminals') renderTerminals();
      if (tab === 'browsers') loadBrowsers();
      if (tab === 'git') loadGitInfo();
      if (tab === 'files') loadFiles();
      if (tab === 'kanban') loadKanban();
      if (tab === 'secrets') loadSecrets();
      if (tab === 'mindmap') loadMindmap();
      if (tab === 'media') loadMedia();
      if (tab === 'schedule') loadSchedules();
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
      $('#git-panel').innerHTML = gitPanelHtml(info);
      wireDragRefs($('#git-panel')); // drag changed files onto a terminal/input
    }

    // Pure HTML builder for the git panel — used by the Git tab and by embedded
    // Git cells in the workbench. Uses classes (not ids) so multiple instances
    // can coexist; callers scope queries to their container.
    function gitPanelHtml(info) {
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
        + '<textarea class="git-commit-msg" placeholder="Commit message\\u2026" rows="2"></textarea>'
        + '<div class="git-commit-btns">'
        + '<button data-action="git-pull" class="git-pull-btn">\\u2193 Pull</button>'
        + '<button data-action="git-push" class="git-push-btn">\\u2191 Push</button>'
        + '<button data-action="git-commit" class="primary">Commit</button>'
        + '</div></div>';

      return html;
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
          + '<button data-action="git-discard" data-file="' + ef + '" title="Discard">' + ic('rotate-ccw') + '</button>';
      }
      return '<div class="git-file" data-drag-ref="' + ef + '" data-drag-abs="1" data-drag-kind="file" title="Drag onto a terminal to insert its path">'
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
      const msg = ($('#git-panel .git-commit-msg') || {}).value?.trim();
      if (!msg) { toast('Enter a commit message', 'error'); return; }
      const r = await apiFetch('/api/git/commit', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project: currentProject, message: msg }) });
      const d = await r.json();
      if (d.ok) { const el = $('#git-panel .git-commit-msg'); if (el) el.value = ''; toast('Committed', 'success'); loadGitInfo(); }
      else toast('Commit failed: ' + (d.error || 'unknown'), 'error');
    }
    async function gitPush() {
      const btn = $('#git-panel .git-push-btn');
      if (btn) { btn.disabled = true; btn.textContent = '\\u2191 Pushing\\u2026'; }
      try {
        const r = await apiFetch('/api/git/push', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project: currentProject }) });
        const d = await r.json();
        if (d.ok) { toast('Pushed', 'success'); loadGitInfo(); }
        else toast('Push failed: ' + (d.error || 'unknown'), 'error');
      } finally { if (btn) { btn.disabled = false; btn.textContent = '\\u2191 Push'; } }
    }
    async function gitPull() {
      const btn = $('#git-panel .git-pull-btn');
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

    let diffCached = false;

    function feOpenDiff(file, oldContent, newContent, rawDiff) {
      const w = feNewWindow({ kind: 'diff' });
      Object.assign(w._fe, { project: currentProject, file: '', readOnly: true, kind: 'diff' });
      const sb = w.querySelector('.fe-save'); if (sb) sb.style.display = 'none';
      w.querySelector('.fe-path').textContent = file;
      const container = w.querySelector('.fe-content');

      // Split on any line ending so CRLF (working tree) vs LF (git's normalized
      // index/HEAD blob) doesn't make every line compare unequal — otherwise the
      // LCS finds no common lines and the whole file shows as deleted + re-added.
      const oldLines = oldContent.split(/\\r\\n|\\r|\\n/);
      const newLines = newContent.split(/\\r\\n|\\r|\\n/);
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
        + '<button class="diff-prev" style="background:none;border:none;color:var(--text-secondary);cursor:pointer;font-size:14px;" title="Previous chunk">\\u2191</button>'
        + '<span class="diff-counter" style="font-size:11px;color:var(--text-muted);font-family:var(--mono);">\\u2014/' + chunks.length + '</span>'
        + '<button class="diff-next" style="background:none;border:none;color:var(--text-secondary);cursor:pointer;font-size:14px;" title="Next chunk">\\u2193</button>'
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
          + '<div class="diff-unified" style="flex:1;overflow:auto;min-height:0;-webkit-overflow-scrolling:touch;background:var(--bg-primary);">'
          + unifiedHtml + '</div>';
      } else {
        container.innerHTML = hdr
          + '<div style="display:flex;flex:1;min-height:0;overflow:hidden;">'
          + '<div class="diff-left" style="flex:1;overflow:auto;border-right:1px solid var(--border);background:var(--bg-primary);">'
          + '<div style="padding:4px 8px;font-size:10px;font-weight:700;text-transform:uppercase;letter-spacing:0.5px;color:var(--text-muted);background:var(--bg-tertiary);border-bottom:1px solid var(--border-subtle);position:sticky;top:0;z-index:1;">Original</div>'
          + oldHtml + '</div>'
          + '<div class="diff-right" style="flex:1;overflow:auto;background:var(--bg-primary);">'
          + '<div style="padding:4px 8px;font-size:10px;font-weight:700;text-transform:uppercase;letter-spacing:0.5px;color:var(--text-muted);background:var(--bg-tertiary);border-bottom:1px solid var(--border-subtle);position:sticky;top:0;z-index:1;">Modified</div>'
          + newHtml + '</div>'
          + '</div>';
      }

      // Scroll container for chunk navigation (scoped to this window)
      const scrollEl = isMobile
        ? container.querySelector('.diff-unified')
        : container.querySelector('.diff-left');

      // Sync scroll (desktop only)
      if (!isMobile) {
        const left = container.querySelector('.diff-left');
        const right = container.querySelector('.diff-right');
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
      container.querySelector('.diff-prev').onclick = () => {
        if (!chunks.length) return;
        currentChunkIdx = Math.max(0, currentChunkIdx - 1);
        container.querySelector('.diff-counter').textContent = (currentChunkIdx + 1) + '/' + chunks.length;
        scrollToChunk(currentChunkIdx);
      };
      container.querySelector('.diff-next').onclick = () => {
        if (!chunks.length) return;
        currentChunkIdx = Math.min(chunks.length - 1, currentChunkIdx + 1);
        container.querySelector('.diff-counter').textContent = (currentChunkIdx + 1) + '/' + chunks.length;
        scrollToChunk(currentChunkIdx);
      };

      // Store diff + chunk data for stage-chunk action
      container._diffData = { oldLines, newLines, diff, chunks, file };
    }

    async function stageChunk(win, chunkIdx) {
      const container = win && win.querySelector ? win.querySelector('.fe-content') : null;
      const data = container && container._diffData;
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

    // Line diff via Myers' O(ND) algorithm. Fast for similar files (cost scales
    // with the number of differences, not file size), and — unlike the old LCS
    // table with its 10k-line cap — it never falls back to "whole file removed +
    // re-added" on large files. Common prefix/suffix are trimmed first.
    function myersMiddle(a, b) {
      const N = a.length, M = b.length;
      const ops = [];
      if (N === 0 && M === 0) return ops;
      if (N === 0) { for (const l of b) ops.push({ type: 'add', text: l }); return ops; }
      if (M === 0) { for (const l of a) ops.push({ type: 'remove', text: l }); return ops; }
      const max = N + M;
      const v = {}; v[1] = 0;
      const trace = [];
      let reached = -1;
      for (let d = 0; d <= max; d++) {
        trace.push(Object.assign({}, v));
        for (let k = -d; k <= d; k += 2) {
          let x;
          if (k === -d || (k !== d && (v[k - 1] ?? -1) < (v[k + 1] ?? -1))) x = (v[k + 1] ?? 0);
          else x = (v[k - 1] ?? 0) + 1;
          let y = x - k;
          while (x < N && y < M && a[x] === b[y]) { x++; y++; }
          v[k] = x;
          if (x >= N && y >= M) { reached = d; break; }
        }
        if (reached !== -1) break;
      }
      let x = N, y = M;
      for (let d = reached; d > 0; d--) {
        const vp = trace[d];
        const k = x - y;
        let pk;
        if (k === -d || (k !== d && (vp[k - 1] ?? -1) < (vp[k + 1] ?? -1))) pk = k + 1;
        else pk = k - 1;
        const px = (vp[pk] ?? 0), py = px - pk;
        while (x > px && y > py) { ops.push({ type: 'equal', text: a[x - 1] }); x--; y--; }
        if (x === px) { ops.push({ type: 'add', text: b[y - 1] }); y--; }
        else { ops.push({ type: 'remove', text: a[x - 1] }); x--; }
      }
      while (x > 0 && y > 0) { ops.push({ type: 'equal', text: a[x - 1] }); x--; y--; }
      while (y > 0) { ops.push({ type: 'add', text: b[y - 1] }); y--; }
      while (x > 0) { ops.push({ type: 'remove', text: a[x - 1] }); x--; }
      ops.reverse();
      return ops;
    }
    function simpleDiff(oldLines, newLines) {
      const a = oldLines, b = newLines;
      let start = 0;
      while (start < a.length && start < b.length && a[start] === b[start]) start++;
      let ea = a.length, eb = b.length;
      while (ea > start && eb > start && a[ea - 1] === b[eb - 1]) { ea--; eb--; }
      const ops = [];
      for (let i = 0; i < start; i++) ops.push({ type: 'equal', text: a[i] });
      for (const o of myersMiddle(a.slice(start, ea), b.slice(start, eb))) ops.push(o);
      for (let i = ea; i < a.length; i++) ops.push({ type: 'equal', text: a[i] });
      return ops;
    }

    // ─── Files Panel ───
    // Files browsing uses absolute paths (server resolves them), which lets the
    // browser go above the project root. The server also returns each entry's
    // absolute path + breadcrumb segments, so the client does no path math.
    let filesState = { path: '', root: '', parent: null, inside: true, entries: [], crumbs: [] };
    let filesSearchQuery = '';
    let filesSearchResults = null; // null = not searching
    let _filesSearchTimer = null;

    async function loadFiles(dir) {
      if (!currentProject) return;
      const target = (dir !== undefined && dir !== null) ? dir : (filesState.path || '');
      try {
        const res = await apiFetch('/api/files/list?project=' + encodeURIComponent(currentProject) + '&dir=' + encodeURIComponent(target));
        const data = await res.json();
        if (!data.ok) { $('#files-panel').innerHTML = '<div class="git-empty">' + escHtml(data.error || 'Failed') + '</div>'; return; }
        filesState = { path: data.path, root: data.root, parent: data.parent, inside: data.inside, entries: data.entries || [], crumbs: data.crumbs || [] };
        filesSearchQuery = ''; filesSearchResults = null;
        renderFilesPanel();
      } catch (err) {
        $('#files-panel').innerHTML = '<div class="git-empty">Error: ' + escHtml(err.message) + '</div>';
      }
    }

    // Back-compat name (some callers say renderFileList).
    function renderFileList() { renderFilesPanel(); }

    // Rebuild the whole panel (toolbar + results). The search input is recreated
    // here, so this is only called on navigation — typing in search updates just
    // the results container so the input keeps focus.
    function renderFilesPanel() {
      const panel = $('#files-panel'); if (!panel) return;
      let h = '<div class="files-toolbar">';
      if (filesState.inside) {
        h += '<input id="files-search" class="files-search" type="text" autocomplete="off" placeholder="Search files & folders\\u2026" value="' + escHtml(filesSearchQuery) + '">';
      }
      h += filesBreadcrumbHtml() + '</div><div class="files-results" id="files-results"></div>';
      panel.innerHTML = h;
      const si = panel.querySelector('#files-search');
      if (si) {
        si.addEventListener('input', () => {
          filesSearchQuery = si.value;
          clearTimeout(_filesSearchTimer);
          _filesSearchTimer = setTimeout(runFilesSearch, 220);
        });
        si.addEventListener('keydown', (e) => { if (e.key === 'Escape') { si.value = ''; filesSearchQuery = ''; runFilesSearch(); } });
        if (filesSearchQuery) si.focus();
      }
      renderFilesResults();
    }

    function filesBreadcrumbHtml() {
      let crumbs = '';
      (filesState.crumbs || []).forEach((c, i) => {
        crumbs += (i ? '<span class="crumb-sep">/</span>' : '') + '<a data-action="files-nav" data-dir="' + escHtml(c.path) + '">' + escHtml(c.name) + '</a>';
      });
      let actions = '';
      if (filesState.parent) actions += '<button class="files-act" data-action="files-nav" data-dir="' + escHtml(filesState.parent) + '" title="Up to parent folder">' + ic('arrow-up') + '</button>';
      if (filesState.inside) actions += '<button class="files-act" data-action="files-upload" title="Upload to this folder">' + ic('upload') + '</button>';
      return '<div class="files-bc"><div class="files-crumbs">' + (crumbs || '<span>—</span>') + '</div><div class="files-bc-actions">' + actions + '</div></div>';
    }

    async function runFilesSearch() {
      const q = (filesSearchQuery || '').trim();
      if (!q) { filesSearchResults = null; renderFilesResults(); return; }
      try {
        const res = await apiFetch('/api/files/search?project=' + encodeURIComponent(currentProject) + '&q=' + encodeURIComponent(q));
        const data = await res.json();
        // Ignore stale responses (user kept typing / cleared).
        if ((filesSearchQuery || '').trim() !== q) return;
        filesSearchResults = data.ok ? { entries: data.entries || [], truncated: data.truncated, gitignoreApplied: data.gitignoreApplied } : { entries: [], error: data.error };
      } catch (err) {
        filesSearchResults = { entries: [], error: err.message };
      }
      renderFilesResults();
    }

    function renderFilesResults() {
      const box = $('#files-results'); if (!box) return;
      box.innerHTML = filesSearchResults ? searchResultsHtml(filesSearchResults) : fileListHtml(filesState);
      wireDragRefs(box); // pointer-drag rows onto a terminal/input (works on touch)
    }

    const _fbtn = (action, p, title, icon, color) =>
      '<button data-action="' + action + '" data-file="' + escHtml(p) + '" title="' + title + '" style="background:none;border:none;color:' + (color || 'var(--text-muted)') + ';cursor:pointer;padding:4px 6px;min-width:28px;display:inline-flex;align-items:center;justify-content:center;font-size:14px;">' + ic(icon) + '</button>';

    // Directory listing (entries carry absolute .path from the server).
    function fileListHtml(state) {
      let html = '<div class="files-list">';
      if (state.parent) {
        html += '<div class="file-item dir" data-action="files-nav" data-dir="' + escHtml(state.parent) + '">'
          + '<span class="fi-icon">' + ic('arrow-up') + '</span><span class="fi-name">..</span></div>';
      }
      for (const e of state.entries) {
        if (e.type === 'dir') {
          html += '<div class="file-item dir" data-drag-ref="' + escHtml(e.path) + '" data-drag-kind="folder" title="Drag onto a terminal to insert its path">'
            + '<span class="fi-icon" data-action="files-nav" data-dir="' + escHtml(e.path) + '" style="cursor:pointer;">' + ic('folder') + '</span>'
            + '<span class="fi-name" data-action="files-nav" data-dir="' + escHtml(e.path) + '" style="cursor:pointer;flex:1;">' + escHtml(e.name) + '</span>'
            + _fbtn('files-copy-path', e.path, 'Copy path', 'copy')
            + _fbtn('files-delete', e.path, 'Delete', 'trash', 'var(--red)')
            + '</div>';
        } else {
          html += '<div class="file-item file" data-drag-ref="' + escHtml(e.path) + '" data-drag-kind="file" title="Drag onto a terminal to insert its path">'
            + '<span class="fi-icon">' + fileIcon(e.name) + '</span>'
            + '<span class="fi-name" data-action="files-open" data-file="' + escHtml(e.path) + '" style="cursor:pointer;flex:1;">' + escHtml(e.name) + '</span>'
            + '<span class="fi-size">' + formatFileSize(e.size) + '</span>'
            + _fbtn('files-copy-path', e.path, 'Copy path', 'copy')
            + _fbtn('files-download', e.path, 'Download', 'download')
            + _fbtn('files-delete', e.path, 'Delete', 'trash', 'var(--red)')
            + '</div>';
        }
      }
      html += '</div>';
      return html;
    }

    // Flat search results (each entry carries absolute .path + project-relative .rel).
    function searchResultsHtml(r) {
      if (r.error) return '<div class="git-empty">Search error: ' + escHtml(r.error) + '</div>';
      const note = r.entries.length
        ? '<div class="files-search-note">' + r.entries.length + (r.truncated ? '+' : '') + ' match' + (r.entries.length === 1 ? '' : 'es')
            + (r.gitignoreApplied ? ' &middot; gitignored files excluded' : '') + '</div>'
        : '<div class="git-empty">No matches' + (r.gitignoreApplied ? ' (gitignored files are excluded)' : '') + '</div>';
      let html = note + '<div class="files-list">';
      for (const e of r.entries) {
        const isDir = e.type === 'dir';
        const dir = e.rel.includes('/') ? e.rel.slice(0, e.rel.lastIndexOf('/') + 1) : '';
        const nameCell = '<span class="fi-name" style="flex:1;cursor:pointer;" data-action="' + (isDir ? 'files-nav" data-dir="' : 'files-open" data-file="') + escHtml(e.path) + '">'
          + (dir ? '<span class="fi-dir">' + escHtml(dir) + '</span>' : '') + escHtml(e.name) + '</span>';
        html += '<div class="file-item ' + (isDir ? 'dir' : 'file') + '" data-drag-ref="' + escHtml(e.path) + '" data-drag-kind="' + (isDir ? 'folder' : 'file') + '" title="Drag onto a terminal to insert its path">'
          + '<span class="fi-icon">' + (isDir ? ic('folder') : fileIcon(e.name)) + '</span>'
          + nameCell
          + _fbtn('files-copy-path', e.path, 'Copy path', 'copy')
          + (isDir ? '' : _fbtn('files-download', e.path, 'Download', 'download'))
          + _fbtn('files-delete', e.path, 'Delete', 'trash', 'var(--red)')
          + '</div>';
      }
      html += '</div>';
      return html;
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
                body: JSON.stringify({ project: currentProject, dir: filesState.path, name: file.name, data: base64 }),
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
        if (!data.ok) { toast(data.error || 'Failed', 'error'); return; }
        const dlUrl = location.origin + data.url;
        // Try opening in new window
        window.open(dlUrl, '_blank');
        // Also copy link to clipboard as fallback
        navigator.clipboard.writeText(dlUrl).then(
          () => toast('Download link copied to clipboard'),
          () => toast('Download started')
        );
      }).catch(e => toast('Download failed: ' + e.message, 'error'));
    }

    // Build the absolute on-disk path for a project-relative path, matching the
    // project's OS path separator.
    function absProjectPath(relPath) {
      const project = projects.find(p => p.alias === currentProject);
      if (!project) return relPath;
      const isWin = project.path.includes('\\\\');
      const sep = isWin ? '\\\\' : '/';
      const normRel = isWin ? relPath.replace(/\\//g, '\\\\') : relPath;
      const base = project.path.endsWith(sep) ? project.path.slice(0, -1) : project.path;
      return base + sep + normRel;
    }
    function filesCopyPath(p) {
      // Files-panel rows carry absolute paths already; only resolve if relative.
      const abs = /^([a-zA-Z]:[\\\\/]|[\\\\/])/.test(p) ? p : absProjectPath(p);
      navigator.clipboard.writeText(abs).then(
        () => toast('Path copied'),
        () => toast('Copy failed', 'error')
      );
    }

    async function filesDelete(relPath) {
      const name = relPath.split('/').pop();
      if (!confirm('Delete "' + name + '"?\\n\\nThis permanently removes it from disk and cannot be undone.')) return;
      try {
        const res = await apiFetch('/api/files/delete', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ project: currentProject, file: relPath }),
        });
        const data = await res.json();
        if (!data.ok) { toast('Delete failed: ' + (data.error || 'unknown'), 'error'); return; }
        toast('Deleted ' + name, 'success');
        loadFiles();
      } catch (err) { toast('Delete failed: ' + err.message, 'error'); }
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
      if (base === 'dockerfile' || base.startsWith('dockerfile.')) return '<span style="color:#2496ed">' + ic('docker') + '</span>';
      if (base === '.gitignore' || base === '.gitattributes') return '<span style="color:#f1502f">' + ic('file') + '</span>';
      if (base === 'package.json' || base === 'package-lock.json') return '<span style="color:#cb3837">npm</span>';
      if (base === '.env' || base.startsWith('.env.')) return '<span style="color:#ecd53f">' + ic('lock') + '</span>';
      const icons = {
        '.js': '<span style="color:#f7df1e">JS</span>',
        '.mjs': '<span style="color:#f7df1e">JS</span>',
        '.cjs': '<span style="color:#f7df1e">JS</span>',
        '.ts': '<span style="color:#3178c6">TS</span>',
        '.tsx': '<span style="color:#3178c6">TX</span>',
        '.jsx': '<span style="color:#61dafb">JX</span>',
        '.json': '<span style="color:#a8a8a2">{}</span>',
        '.html': '<span style="color:#e34c26">\\u25C7</span>',
        '.css': '<span style="color:#264de4">#</span>',
        '.scss': '<span style="color:#cd6799">#</span>',
        '.py': '<span style="color:#3776ab">PY</span>',
        '.go': '<span style="color:#00add8">Go</span>',
        '.rs': '<span style="color:#dea584">RS</span>',
        '.java': '<span style="color:#f89820">JV</span>',
        '.c': '<span style="color:#555">C</span>',
        '.cpp': '<span style="color:#659ad2">C+</span>',
        '.h': '<span style="color:#555">H</span>',
        '.sh': '<span style="color:#4eaa25">$_</span>',
        '.ps1': '<span style="color:#012456">PS</span>',
        '.sql': '<span style="color:#e38c00">DB</span>',
        '.md': '<span style="color:#519aba">M\\u2193</span>',
        '.yaml': '<span style="color:#cb171e">YML</span>',
        '.yml': '<span style="color:#cb171e">YML</span>',
        '.xml': '<span style="color:#e37933">XML</span>',
        '.svg': '<span style="color:#ffb13b">' + ic('image') + '</span>',
        '.png': ic('image'), '.jpg': ic('image'), '.jpeg': ic('image'), '.gif': ic('image'), '.webp': ic('image'),
        '.zip': ic('archive'), '.tar': ic('archive'), '.gz': ic('archive'),
        '.pdf': '<span style="color:#e5252a">' + ic('file-text') + '</span>',
        '.lock': '<span style="color:#ecd53f">' + ic('lock') + '</span>',
        '.log': ic('list'),
        '.vue': '<span style="color:#42b883">V</span>',
        '.svelte': '<span style="color:#ff3e00">S</span>',
      };
      return icons[ext] || ic('file');
    }

    // ─── File Editor (multi-window manager) ───
    // Desktop: multiple floating, draggable, resizable windows with edge-snap and
    // tile/grid/cascade. Mobile: a single full-screen window (opening replaces).
    // Per-window state lives on el._fe = { project, file, original, readOnly, view, kind }.
    const FE_IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'ico', 'svg', 'avif'];
    function feMobile() { return window.innerWidth <= 768; }
    let feZ = 2100, feFront = null, feCascade = 0;
    function feOverlay() { return document.getElementById('file-editor'); }
    function feWins() { return [...document.querySelectorAll('.fe-window')]; }
    function feSyncOverlay() {
      const o = feOverlay(); if (!o) return;
      const has = feWins().length > 0;
      o.classList.toggle('visible', has);
      o.classList.toggle('has-windows', has);
      if (!has) feFront = null;
    }
    function feBringFront(win) { if (win) { win.style.zIndex = String(++feZ); feFront = win; } }
    function feFocusedWin() { return (feFront && feFront.isConnected) ? feFront : (feWins().slice(-1)[0] || null); }

    function feNewWindow(opts = {}) {
      if (feMobile()) {
        let w = feWins()[0];
        if (!w) w = feBuildWindow(opts); else feResetWinContent(w);
        feSyncOverlay();
        return w;
      }
      const w = feBuildWindow(opts);
      feSyncOverlay();
      return w;
    }
    function feBuildWindow(opts) {
      const o = feOverlay();
      const w = document.createElement('div');
      w.className = 'fe-window'; w.dataset.fewin = genLocalId();
      w._fe = { project: '', file: '', original: '', readOnly: false, view: null, kind: opts.kind || 'edit' };
      w.innerHTML = '<div class="fe-header">'
        + '<span class="fe-path"></span>'
        + '<button class="fe-expand" title="Expand / restore">' + ic('maximize') + '</button>'
        + '<button class="save fe-save" style="display:none">Save</button>'
        + '<button class="fe-close">Close</button>'
        + '</div><div class="fe-content"></div>'
        + '<div class="fe-resize fe-resize-e"></div>'
        + '<div class="fe-resize fe-resize-s"></div>'
        + '<div class="fe-resize fe-resize-se"></div>';
      o.appendChild(w);
      if (!feMobile()) {
        const step = 28, n = (feCascade++ % 6);
        w.style.top = (54 + n * step) + 'px';
        w.style.left = (Math.max(40, window.innerWidth * 0.18) + n * step) + 'px';
        // Pin starting size as explicit px so resize math has a baseline.
        const r = w.getBoundingClientRect();
        w.style.width = r.width + 'px'; w.style.height = r.height + 'px';
      }
      w.querySelector('.fe-close').addEventListener('click', () => feCloseWin(w));
      w.querySelector('.fe-save').addEventListener('click', () => feSaveWin(w));
      w.querySelector('.fe-expand').addEventListener('click', () => feExpandWin(w));
      w.addEventListener('mousedown', () => feBringFront(w), true);
      feWireDrag(w);
      feWireResize(w);
      feWireArrangeBar();
      feBringFront(w);
      return w;
    }
    function feResetWinContent(w) {
      if (w._fe && w._fe.view) { try { w._fe.view.destroy(); } catch { /* ignore */ } w._fe.view = null; }
      const c = w.querySelector('.fe-content'); if (c) { c._diffData = null; c.innerHTML = ''; }
    }
    function feCloseWin(w) {
      if (!w) return;
      if (w._fe && w._fe.view) { try { w._fe.view.destroy(); } catch { /* ignore */ } }
      w.remove();
      feSyncOverlay();
    }
    function feExpandWin(w) { if (w) { w.classList.toggle('maximized'); feBringFront(w); } }

    function feMountEditor(w, content, filePath, readOnly) {
      const container = w.querySelector('.fe-content');
      container.innerHTML = '';
      if (w._fe.view) { try { w._fe.view.destroy(); } catch { /* ignore */ } w._fe.view = null; }
      if (window.CM) {
        const extensions = [...window.CM.basicSetup, ...window.CM.getLangExtension(filePath || ''), ...window.CM.oneDark];
        if (readOnly) {
          extensions.push(window.CM.EditorState.readOnly.of(true));
        } else {
          extensions.push(window.CM.EditorView.updateListener.of((update) => {
            if (update.docChanged) { const b = w.querySelector('.fe-save'); if (b) b.disabled = (w._fe.view.state.doc.toString() === w._fe.original); }
          }));
          extensions.push(window.CM.EditorView.domEventHandlers({ keydown(e) { if (e.key === 's' && (e.ctrlKey || e.metaKey)) { e.preventDefault(); feSaveWin(w); } } }));
        }
        w._fe.view = new window.CM.EditorView({ doc: content, extensions, parent: container });
        if (!readOnly) w._fe.view.focus();
      } else {
        const ta = document.createElement('textarea');
        ta.value = content; ta.readOnly = !!readOnly; ta.spellcheck = false;
        ta.style.cssText = 'width:100%;height:100%;resize:none;border:none;background:var(--bg-primary);color:var(--text-primary);font-family:var(--mono);font-size:13px;padding:12px;line-height:1.5;outline:none;';
        container.appendChild(ta);
      }
    }
    function feWinContent(w) {
      if (w._fe.view) return w._fe.view.state.doc.toString();
      const ta = w.querySelector('.fe-content textarea'); return ta ? ta.value : '';
    }

    async function feOpen(project, file) {
      const ext = (file.includes('.') ? file.slice(file.lastIndexOf('.') + 1) : '').toLowerCase();
      if (FE_IMAGE_EXTS.includes(ext) || ext === 'pdf') return feOpenViewer(project, file, ext);
      // Already open on desktop → focus it instead of duplicating.
      if (!feMobile()) {
        const ex = feWins().find(w => w._fe && w._fe.kind === 'edit' && w._fe.project === project && w._fe.file === file);
        if (ex) { feBringFront(ex); return; }
      }
      try {
        const res = await apiFetch('/api/files/read?project=' + encodeURIComponent(project) + '&file=' + encodeURIComponent(file));
        const data = await res.json();
        if (!data.ok) { toast(data.error || 'Cannot open file', 'error'); return; }
        const w = feNewWindow({ kind: 'edit' });
        Object.assign(w._fe, { project, file, original: data.content, readOnly: false, kind: 'edit' });
        w.querySelector('.fe-path').textContent = file;
        const b = w.querySelector('.fe-save'); if (b) { b.disabled = true; b.style.display = ''; }
        feMountEditor(w, data.content, file, false);
      } catch (err) { toast('Error: ' + err.message, 'error'); }
    }

    // Read-only viewer for images / PDFs (served inline via a short-lived token).
    async function feOpenViewer(project, file, ext) {
      const w = feNewWindow({ kind: 'view' });
      Object.assign(w._fe, { project: '', file: '', readOnly: true, kind: 'view' });
      w.querySelector('.fe-path').textContent = file;
      const b = w.querySelector('.fe-save'); if (b) b.style.display = 'none';
      const c = w.querySelector('.fe-content');
      c.innerHTML = '<div class="fe-loading">Loading\\u2026</div>';
      try {
        const r = await apiFetch('/api/files/download-link', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project, file }) });
        const d = await r.json();
        if (!d.ok) { c.innerHTML = '<div class="fe-loading">' + escHtml(d.error || 'Cannot open file') + '</div>'; return; }
        const u = location.origin + d.url + '?inline=1';
        c.innerHTML = ext === 'pdf'
          ? '<iframe class="fe-viewer-frame" src="' + u + '"></iframe>'
          : '<div class="fe-viewer-img"><img src="' + u + '" alt="' + escHtml(file) + '"></div>';
      } catch (err) { c.innerHTML = '<div class="fe-loading">Error: ' + escHtml(err.message) + '</div>'; }
    }

    function feOpenContent(title, content, readOnly) {
      const w = feNewWindow({ kind: 'content' });
      Object.assign(w._fe, { project: '', file: '', original: content, readOnly: !!readOnly, kind: 'content' });
      w.querySelector('.fe-path').textContent = title;
      const b = w.querySelector('.fe-save'); if (b) b.style.display = readOnly ? 'none' : '';
      feMountEditor(w, content, title, !!readOnly);
    }

    async function feSaveWin(w) {
      if (!w || !w._fe || w._fe.readOnly || !w._fe.project || !w._fe.file) return;
      const content = feWinContent(w);
      const b = w.querySelector('.fe-save');
      try {
        if (b) { b.disabled = true; b.textContent = 'Saving\\u2026'; }
        const res = await apiFetch('/api/files/write', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ project: w._fe.project, file: w._fe.file, content }) });
        const data = await res.json();
        if (data.ok) { w._fe.original = content; toast('Saved', 'success'); if (b) { b.textContent = 'Saved \\u2713'; setTimeout(() => { b.textContent = 'Save'; b.disabled = true; }, 1500); } }
        else { toast('Save failed: ' + (data.error || 'unknown'), 'error'); if (b) { b.textContent = 'Save'; b.disabled = false; } }
      } catch (err) { toast('Error: ' + err.message, 'error'); if (b) { b.textContent = 'Save'; b.disabled = false; } }
    }

    // Legacy/global helpers act on the focused window (Escape, Ctrl+S).
    function feClose() { const w = feFocusedWin(); if (w) feCloseWin(w); }
    function feSave() { const w = feFocusedWin(); if (w) feSaveWin(w); }
    function feAnyOpen() { return feWins().length > 0; }
    window.feClose = feClose; window.feSave = feSave;

    // ── Drag + edge-snap (desktop) ──
    // Editor windows stay below this Y (px). The web UI runs in an iframe under a
    // 32px OS title bar (an app-region drag region); keeping windows — and their
    // header, hence the drag pointer — clear of the top avoids Windows hijacking
    // the gesture into a full-app move. Matches --topbar-height (48px).
    const FE_TOP = 48;
    function feSnapRect(zone) {
      const vw = window.innerWidth, vh = window.innerHeight, top = FE_TOP, h = vh - FE_TOP - 4, w2 = vw / 2, hh = (h / 2) - 2;
      const R = { left: { left: 4, top, width: w2 - 6, height: h }, right: { left: w2 + 2, top, width: w2 - 6, height: h },
        max: { left: 4, top, width: vw - 8, height: h },
        tl: { left: 4, top, width: w2 - 6, height: hh }, tr: { left: w2 + 2, top, width: w2 - 6, height: hh },
        bl: { left: 4, top: top + h / 2 + 2, width: w2 - 6, height: hh }, br: { left: w2 + 2, top: top + h / 2 + 2, width: w2 - 6, height: hh } };
      return R[zone] || null;
    }
    function feZoneAt(x, y) {
      const vw = window.innerWidth, vh = window.innerHeight, E = 36;
      const l = x < E, r = x > vw - E, t = y < E, b = y > vh - E;
      if (t && l) return 'tl'; if (t && r) return 'tr'; if (b && l) return 'bl'; if (b && r) return 'br';
      if (l) return 'left'; if (r) return 'right'; if (t) return 'max';
      return null;
    }
    function feApplyRect(w, rect) {
      w.classList.remove('maximized');
      w.style.left = rect.left + 'px'; w.style.top = rect.top + 'px';
      w.style.width = rect.width + 'px'; w.style.height = rect.height + 'px';
    }
    // NOTE: drag/resize use Pointer Events + setPointerCapture (not mouse events
    // on document). In the desktop app the webapp runs in an iframe sitting below
    // a 32px OS title bar that is an app-region drag region. With plain mouse
    // listeners, dragging a window's header up past the iframe's top edge
    // leaks the gesture to that title bar and Windows starts moving the WHOLE app.
    // Capturing the pointer on the header keeps every move/up bound to our element
    // for the entire gesture, so it never reaches the OS drag region.
    function feWireDrag(w) {
      const head = w.querySelector('.fe-header');
      let sx = 0, sy = 0, ol = 0, ot = 0, dragging = false, zone = null, pid = null;
      head.addEventListener('pointerdown', (e) => {
        if (e.target.closest('button') || feMobile() || e.button !== 0) return;
        feBringFront(w);
        feShowArrange(); // reveal the Tile/Grid/Cascade bar while moving windows
        const r = w.getBoundingClientRect();
        ol = r.left; ot = r.top; sx = e.clientX; sy = e.clientY; dragging = true; pid = e.pointerId;
        w.classList.remove('maximized');
        try { head.setPointerCapture(pid); } catch { /* ignore */ }
        head.addEventListener('pointermove', mv); head.addEventListener('pointerup', up); head.addEventListener('pointercancel', up);
        e.preventDefault();
      });
      function mv(e) {
        if (!dragging) return;
        w.style.left = (ol + e.clientX - sx) + 'px';
        w.style.top = Math.max(FE_TOP, ot + e.clientY - sy) + 'px';
        zone = feZoneAt(e.clientX, e.clientY);
        const o = feOverlay(), hint = document.getElementById('fe-snap-hint');
        if (zone) { const rc = feSnapRect(zone); o.classList.add('snapping'); Object.assign(hint.style, { left: rc.left + 'px', top: rc.top + 'px', width: rc.width + 'px', height: rc.height + 'px' }); }
        else o.classList.remove('snapping');
      }
      function up() {
        dragging = false;
        try { head.releasePointerCapture(pid); } catch { /* ignore */ }
        head.removeEventListener('pointermove', mv); head.removeEventListener('pointerup', up); head.removeEventListener('pointercancel', up);
        feOverlay().classList.remove('snapping');
        if (zone) { feApplyRect(w, feSnapRect(zone)); zone = null; }
        feHideArrangeSoon(); // keep the bar briefly so it stays clickable after a drag
      }
    }

    // Custom resize handles (right edge, bottom edge, SE corner). Native CSS
    // resize is covered by the editor content, so we drive it ourselves. Same
    // pointer-capture rationale as feWireDrag (avoid leaking to the OS title bar).
    function feWireResize(w) {
      w.querySelectorAll('.fe-resize').forEach(handle => {
        const dir = handle.classList.contains('fe-resize-e') ? 'e' : handle.classList.contains('fe-resize-s') ? 's' : 'se';
        handle.addEventListener('pointerdown', (e) => {
          if (feMobile() || e.button !== 0) return;
          e.preventDefault(); e.stopPropagation();
          feBringFront(w); w.classList.remove('maximized');
          const r = w.getBoundingClientRect();
          const sx = e.clientX, sy = e.clientY, sw = r.width, sh = r.height, pid = e.pointerId;
          w.style.left = r.left + 'px'; w.style.top = r.top + 'px'; // pin origin
          const MINW = 360, MINH = 200;
          try { handle.setPointerCapture(pid); } catch { /* ignore */ }
          const mv = (ev) => {
            if (dir === 'e' || dir === 'se') w.style.width = Math.max(MINW, sw + ev.clientX - sx) + 'px';
            if (dir === 's' || dir === 'se') w.style.height = Math.max(MINH, sh + ev.clientY - sy) + 'px';
          };
          const up = () => {
            try { handle.releasePointerCapture(pid); } catch { /* ignore */ }
            handle.removeEventListener('pointermove', mv); handle.removeEventListener('pointerup', up); handle.removeEventListener('pointercancel', up);
          };
          handle.addEventListener('pointermove', mv); handle.addEventListener('pointerup', up); handle.addEventListener('pointercancel', up);
        });
      });
    }

    // Arrange bar visibility — shown only while dragging a window, with a short
    // linger afterwards so its buttons stay clickable.
    let feArrangeTimer = null;
    function feShowArrange() { const o = feOverlay(); if (o) o.classList.add('show-arrange'); clearTimeout(feArrangeTimer); }
    function feHideArrangeSoon(ms = 3500) { clearTimeout(feArrangeTimer); feArrangeTimer = setTimeout(() => { const o = feOverlay(); if (o) o.classList.remove('show-arrange'); }, ms); }
    let feArrangeBarWired = false;
    function feWireArrangeBar() {
      if (feArrangeBarWired) return; const bar = document.getElementById('fe-arrange'); if (!bar) return;
      feArrangeBarWired = true;
      bar.addEventListener('mouseenter', feShowArrange);
      bar.addEventListener('mouseleave', () => feHideArrangeSoon(1200));
    }

    // ── Arrange: tile / grid / cascade ──
    function feArrange(mode) {
      const wins = feWins(); if (!wins.length || feMobile()) return;
      feShowArrange(); feHideArrangeSoon(); // keep the bar up briefly after a click
      const vw = window.innerWidth, vh = window.innerHeight, top = FE_TOP, gap = 6, h = vh - FE_TOP - 4;
      wins.forEach(w => w.classList.remove('maximized'));
      if (mode === 'cascade') {
        const dw = Math.min(900, vw * 0.7), dh = Math.min(vh * 0.7, 760);
        wins.forEach((w, i) => { const off = 28 * (i % 8); feApplyRect(w, { left: 40 + off, top: FE_TOP + off, width: dw, height: dh }); });
      } else if (mode === 'tile') {
        const n = wins.length, cw = (vw - gap) / n;
        wins.forEach((w, i) => feApplyRect(w, { left: gap / 2 + i * cw, top, width: cw - gap, height: h }));
      } else if (mode === 'grid') {
        const n = wins.length, cols = Math.ceil(Math.sqrt(n)), rows = Math.ceil(n / cols);
        const cw = (vw - gap) / cols, ch = (h - gap * (rows - 1)) / rows;
        wins.forEach((w, i) => { const c = i % cols, r = Math.floor(i / cols); feApplyRect(w, { left: gap / 2 + c * cw, top: top + r * (ch + gap), width: cw - gap, height: ch }); });
      }
    }

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

      const toolbar = '<div class="svc-toolbar">'
        + '<span class="svc-title">Services' + (currentProject ? '' : ' \\u00b7 all projects') + '</span>'
        + '<div class="spacer"></div>'
        + '<button class="svc-add-btn" data-action="svc-register">' + ic('plus') + 'Register service</button>'
        + '</div>';

      if (projectServices.length === 0) {
        panel.innerHTML = toolbar + '<div class="services-empty">'
          + '<div class="icon">' + ic('server') + '</div>'
          + '<p>No services registered' + (currentProject ? ' for this project' : '') + '</p>'
          + '</div>';
        return;
      }

      panel.innerHTML = toolbar + projectServices.map(s => {
        const k = escHtml(s.key);
        const running = s.status === 'running';
        const statusClass = running ? 'running' : (s.status === 'error' ? 'error' : 'stopped');
        // Tunnel port and on/off are independent. The runtime badge reflects the
        // actual tunnel status — a stopped service shows "(idle)", not "(starting…)".
        const tPort = s.tunnelPort || 0;
        const tEnabled = !!s.tunnelEnabled;
        const tStatus = s.tunnelStatus;
        let tunnelRow = '';
        if (tPort > 0 || tEnabled) {
          let badge;
          if (tEnabled && s.tunnelUrl && tStatus === 'active') badge = '<a class="tunnel-link" href="' + escHtml(s.tunnelUrl) + '" target="_blank">' + escHtml(s.tunnelUrl) + '</a>';
          else if (tEnabled && tStatus === 'connecting') badge = '<span class="svc-tunnel-badge" style="color:var(--yellow)">starting…</span>';
          else if (tEnabled && tStatus === 'error') badge = '<span class="svc-tunnel-badge" style="color:var(--red)">error</span>';
          else if (tEnabled) badge = '<span class="svc-tunnel-badge">idle</span>';
          else badge = '<span class="svc-tunnel-badge">off</span>';
          tunnelRow = '<div class="svc-tunnel">'
            + '<span class="svc-tunnel-ic">' + ic('globe') + '</span>'
            + '<button class="svc-btn" data-action="svc-tunnel-port" data-key="' + k + '" data-port="' + tPort + '">' + (tPort > 0 ? 'Port ' + tPort : 'Set port') + '</button>'
            + '<button class="svc-btn' + (tEnabled ? ' on' : '') + '" data-action="svc-tunnel-toggle" data-key="' + k + '" data-port="' + tPort + '" data-enabled="' + (tEnabled ? '1' : '0') + '"' + (tPort > 0 ? '' : ' title="Set a tunnel port first"') + '>' + ic('globe') + (tEnabled ? 'On' : 'Off') + '</button>'
            + badge
            + '</div>';
        }
        return '<div class="svc-card" data-svc-key="' + k + '">'
          + '<div class="svc-header">'
          + '<span class="svc-dot ' + statusClass + '"></span>'
          + '<span class="svc-name">' + escHtml(s.name || s.key) + '</span>'
          + '<span class="svc-status ' + statusClass + '">' + escHtml(s.status) + '</span>'
          + '</div>'
          + '<div class="svc-meta">' + escHtml(s.command || '')
          + (s.uptime ? ' <span class="svc-up">&middot; up ' + escHtml(s.uptime) + '</span>' : '') + '</div>'
          + '<div class="svc-actions">'
          + (running
            ? '<button class="svc-btn" data-action="svc-stop" data-key="' + k + '">' + ic('stop') + 'Stop</button>'
              + '<button class="svc-btn" data-action="svc-restart" data-key="' + k + '">' + ic('refresh') + 'Restart</button>'
            : '<button class="svc-btn primary" data-action="svc-start" data-key="' + k + '">' + ic('play') + 'Start</button>')
          + '<button class="svc-btn" data-action="svc-logs" data-key="' + k + '">' + ic('file-text') + 'Logs</button>'
          + '<button class="svc-btn" data-action="svc-tunnel-port" data-key="' + k + '" data-port="' + tPort + '"' + (tPort > 0 || tEnabled ? ' style="display:none"' : '') + '>' + ic('globe') + 'Tunnel</button>'
          + '<button class="svc-btn danger" data-action="svc-delete" data-key="' + k + '">' + ic('trash') + 'Delete</button>'
          + '</div>'
          + tunnelRow
          + '<div class="svc-logs" id="svc-logs-' + k + '"></div>'
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

    async function svcTunnelPost(key, body, okMsg) {
      try {
        const res = await apiFetch('/api/services/' + encodeURIComponent(key) + '/tunnel', {
          method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body),
        });
        const d = await res.json();
        if (!d.ok) toast(d.error || 'Failed', 'error');
        else toast(okMsg || 'Tunnel updated', 'success');
        setTimeout(loadServices, 600);
      } catch (err) { toast('Error: ' + err.message, 'error'); }
    }
    // Set/change the tunnel port (separate from on/off). 0 clears it (and turns off).
    async function svcTunnelSetPort(key, currentPort) {
      const cur = parseInt(currentPort, 10) || 0;
      const val = await askText({
        title: 'Tunnel port',
        label: 'Local port to expose via Cloudflare. Enter 0 (or blank) to clear.',
        value: cur > 0 ? String(cur) : '',
      });
      if (val === null) return;
      const port = parseInt(val, 10) || 0;
      svcTunnelPost(key, { port }, port > 0 ? 'Tunnel port set to :' + port : 'Tunnel port cleared');
    }
    // Toggle the tunnel on/off. Blocked when no port is set.
    async function svcTunnelToggle(key, enabled, port) {
      const p = parseInt(port, 10) || 0;
      if (!enabled && p <= 0) { toast('Set a tunnel port first', 'error'); return; }
      svcTunnelPost(key, { enabled: !enabled }, !enabled ? 'Tunnel on' : 'Tunnel off');
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
      form.innerHTML = '<div class="srf-title">Register a service</div>'
        + '<input type="text" id="svc-reg-name" class="svc-reg-input" placeholder="Service name" />'
        + '<input type="text" id="svc-reg-cmd" class="svc-reg-input" placeholder="Command (e.g. npm run dev)" />'
        + '<input type="text" id="svc-reg-cwd" class="svc-reg-input" placeholder="Working directory (optional)" />'
        + '<input type="text" id="svc-reg-stop" class="svc-reg-input" placeholder="Stop command (optional, e.g. docker compose down)" />'
        + '<input type="number" id="svc-reg-tunnel" class="svc-reg-input" placeholder="Tunnel port (optional) — expose via Cloudflare" min="0" />'
        + '<div class="srf-actions">'
        + '<button class="svc-btn primary" data-action="svc-reg-submit">' + ic('check') + 'Register</button>'
        + '<button class="svc-btn" data-action="svc-reg-cancel">Cancel</button>'
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
      if (!nameEl || !nameEl.value.trim()) { toast('Enter a terminal name', 'error'); return; }
      try {
        const project = projects.find(p => p.alias === currentProject);
        const r = await apiFetch('/api/terminals/spawn', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ alias: currentProject, name: nameEl.value.trim(), command: cmdEl?.value || undefined, cwd: project?.path }),
        });
        const data = await r.json();
        if (!data.ok) toast(data.error || 'Failed', 'error');
        else { toast('Terminal spawned'); nameEl.value = ''; if (cmdEl) cmdEl.value = ''; }
      } catch (e) { toast(e.message, 'error'); }
    }

    async function termViewOutput(name) {
      const logsEl = document.getElementById('term-logs-' + name);
      if (!logsEl) return;
      if (logsEl.style.display === 'block') { logsEl.style.display = 'none'; return; }
      try {
        const r = await apiFetch('/api/terminals/output?alias=' + encodeURIComponent(currentProject) + '&name=' + encodeURIComponent(name));
        const data = await r.json();
        if (!data.ok) { toast(data.error, 'error'); return; }
        const lines = data.lines || [];
        logsEl.textContent = lines.join('\\n') || '(no output)';
        logsEl.style.display = 'block';
        logsEl.scrollTop = logsEl.scrollHeight;
      } catch (e) { toast(e.message, 'error'); }
    }

    async function termCloseUser(name) {
      try {
        const r = await apiFetch('/api/terminals/close', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ alias: currentProject, name }),
        });
        const data = await r.json();
        if (!data.ok) toast(data.error, 'error');
        else toast('Terminal closed');
      } catch (e) { toast(e.message, 'error'); }
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
          + '<div class="icon">' + ic('globe') + '</div>'
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
          + '<button class="svc-btn" data-action="browser-screenshot" data-key="' + ek + '">' + ic('image') + 'Screenshot</button>'
          + '<button class="svc-btn danger" data-action="browser-close" data-key="' + ek + '">' + ic('x') + 'Close</button>'
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
          + '<div class="icon">' + ic('terminal') + '</div>'
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
      let logsHtml = '<div class="info-section">'
        + '<div style="display:flex;align-items:center;gap:8px;margin-bottom:10px;"><h4 style="margin:0;flex:1;">Server Logs</h4>'
        + '<button class="svc-btn" data-action="copy-logs" onmousedown="event.preventDefault()" title="Copy selection, or all logs">' + ic('copy') + 'Copy</button></div>'
        + '<div id="server-logs-area" style="user-select:text;-webkit-user-select:text;max-height:300px;overflow-y:auto;background:var(--bg-primary);'
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

    // Copy the current selection if it's within the logs box, otherwise all logs.
    // (Ctrl+C is dead in the frameless Electron window; this uses the clipboard API.)
    function copyServerLogs() {
      const area = document.getElementById('server-logs-area');
      if (!area) return;
      const sel = window.getSelection && window.getSelection();
      const text = (sel && !sel.isCollapsed && sel.toString().trim() && area.contains(sel.anchorNode))
        ? sel.toString() : area.innerText;
      navigator.clipboard.writeText(text).then(() => toast('Logs copied'), () => toast('Copy failed', 'error'));
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

        // Notification policy matrix, reflecting server state. Each event row gets
        // three lamps (Always / When Away / Never); the active one lights up.
        notifyPrefs = Object.assign({}, NOTIFY_DEFAULTS_CLIENT, data.notifyPrefs || {});
        const NOTIFY_MODES_UI = [['always', 'Always'], ['away', 'When Away'], ['never', 'Never']];
        const NOTIFY_GROUPS = [
          ['Agent', [['finished', 'When agent finishes'], ['needsInput', 'When agent needs input']]],
          ['Kanban', [['kanbanTask', 'Main task status change'], ['kanbanSubtask', 'Sub task status change']]],
          ['Schedule', [['scheduleRun', 'When a schedule executes']]],
          ['Services', [['serviceDown', 'When a service crashes / stops'], ['serviceUp', 'When a service starts']]],
          ['Mindmap', [['mindmapAdd', 'When a node is added'], ['mindmapDelete', 'When a node is deleted']]],
          ['MCP Browser', [['browserLaunch', 'When a browser launches'], ['browserStop', 'When a browser stops']]],
          ['Security', [['secretRequest', 'Secret access requested']]],
        ];
        const buildNotifyMatrix = () => {
          let m = '<div class="ntf-matrix"><div class="ntf-row head"><div class="ntf-h lbl">Event</div>'
            + NOTIFY_MODES_UI.map(([, l]) => '<div class="ntf-h">' + l + '</div>').join('') + '</div>';
          let di = 0;
          for (const [cat, evs] of NOTIFY_GROUPS) {
            m += '<div class="ntf-cat">' + cat + '</div>';
            for (const [key, label] of evs) {
              const cur = notifyPrefs[key] || 'never';
              m += '<div class="ntf-row event" data-notify-field="' + key + '" role="radiogroup" aria-label="' + escHtml(label) + '" style="animation-delay:' + (di++ * 22) + 'ms">'
                + '<div class="ntf-lbl">' + label + '</div>'
                + NOTIFY_MODES_UI.map(([v, l]) => '<div class="ntf-cell' + (cur === v ? ' on' : '') + '" role="radio" aria-checked="' + (cur === v) + '" title="' + l + '" data-action="notify-pref" data-field="' + key + '" data-val="' + v + '"><span class="ntf-lamp"></span></div>').join('')
                + '</div>';
            }
          }
          return m + '</div>';
        };

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
          + '</div></div>'
          // Notification policy — applied instantly, no restart. "When Away" only
          // pings when you are NOT focused on Crundi (browser tab / Electron app).
          + '<div class="info-section"><h4>Notification Settings</h4>'
          + '<p style="' + hintStyle + ';margin-bottom:14px;">Telegram pings per event. \\u201cAlways\\u201d sends every time; \\u201cWhen Away\\u201d only while you\\u2019re not focused on Crundi (browser tab / Electron app); \\u201cNever\\u201d is off.</p>'
          + buildNotifyMatrix()
          + '</div>'
          // Client-side terminal preferences — applied instantly, no restart.
          + '<div class="info-section"><h4>Terminal</h4>'
          + '<div style="' + fieldStyle + '"><label style="' + labelStyle + '">New Line Key</label>'
          + '<div class="seg-pref" id="set-newline-key">'
          + '<button type="button" data-action="term-newline-key" data-val="shift"' + (termNewlineKey === 'shift' ? ' class="active"' : '') + '><kbd>Shift</kbd> + <kbd>Enter</kbd></button>'
          + '<button type="button" data-action="term-newline-key" data-val="ctrl"' + (termNewlineKey === 'ctrl' ? ' class="active"' : '') + '><kbd>Ctrl</kbd> + <kbd>Enter</kbd></button>'
          + '</div>'
          + '<p style="' + hintStyle + '">Inserts a newline in the terminal instead of submitting \\u2014 the other key still submits. Applies instantly to all terminals.</p></div></div>';

        // Updates section — desktop app only (driven by Electron's auto-updater).
        if (window.api && window.api.getUpdateState) html += buildUpdatesSection();

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

    // Persist the notification policy on its own (applies live, no env rewrite).
    async function saveNotifyPrefs() {
      try {
        await apiFetch('/api/settings', {
          method: 'POST', headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ notifyPrefs }),
        });
      } catch (err) { toast('Failed to save notification setting: ' + err.message, 'error'); }
    }

    // ─── Event Delegation ───
    document.addEventListener('click', (e) => {
      // File-editor window arrange controls (tile / grid / cascade)
      const arrEl = e.target.closest('[data-fe-arrange]');
      if (arrEl) { feArrange(arrEl.dataset.feArrange); return; }
      // Tab clicks (tolerate clicks on the icon/label inside the button)
      const tabEl = e.target.closest('[data-tab]');
      if (tabEl) { switchTab(tabEl.dataset.tab); return; }
      const actionEl = e.target.closest('[data-action]');
      const action = e.target.dataset.action || actionEl?.dataset.action;
      if (!action) return;
      const d = actionEl ? actionEl.dataset : e.target.dataset;
      switch (action) {
        case 'toggle-sidebar': toggleSidebar(); break;
        case 'toggle-sidebar-collapse': toggleSidebarCollapse(); break;
        case 'close-sidebar': closeSidebar(); break;
        case 'add-project': closeSidebar(); showAddModal(); break;
        case 'modal-cancel': hideAddModal(); break;
        case 'modal-add': addProject(); break;
        case 'import-yes': doImport(); break;
        case 'import-skip': $('#import-dialog').classList.remove('visible'); break;
        case 'remove-project': {
          const project = d.project;
          if (project) { e.stopPropagation(); removeProject(project, d.name); }
          break;
        }
        case 'svc-start': svcAction('start', d.key); break;
        case 'svc-stop': svcAction('stop', d.key); break;
        case 'svc-restart': svcAction('restart', d.key); break;
        case 'svc-logs': svcLogs(d.key); break;
        case 'svc-tunnel-port': svcTunnelSetPort(d.key, d.port); break;
        case 'svc-tunnel-toggle': svcTunnelToggle(d.key, d.enabled === '1', d.port); break;
        case 'svc-delete': svcDelete(d.key); break;
        case 'svc-register': showRegisterServiceForm(); break;
        case 'svc-reg-submit': submitRegisterService(); break;
        case 'svc-reg-cancel': cancelRegisterService(); break;
        case 'settings-save': saveSettings(); break;
        case 'term-newline-key':
          setTermNewlineKey(d.val);
          $$('#set-newline-key button').forEach(b => b.classList.toggle('active', b.dataset.val === termNewlineKey));
          break;
        case 'update-toggle':
          if (window.api && window.api.setAutoUpdate) window.api.setAutoUpdate(!updateState.enabled).then(applyUpdateState).catch(() => {});
          break;
        case 'startup-toggle':
          if (window.api && window.api.setStartup) window.api.setStartup(!updateState.launch).then(applyUpdateState).catch(() => {});
          break;
        case 'update-check':
          if (window.api && window.api.checkForUpdates) { toast('Checking for updates\\u2026'); window.api.checkForUpdates().then(applyUpdateState).catch(() => {}); }
          break;
        case 'update-install':
          if (window.api && window.api.installUpdate) window.api.installUpdate().catch(() => {});
          break;
        case 'notify-pref': {
          const field = d.field, val = d.val;
          if (!field || !['always', 'away', 'never'].includes(val)) break;
          notifyPrefs[field] = val;
          $$('.ntf-row[data-notify-field="' + field + '"] .ntf-cell').forEach(c => {
            const on = c.dataset.val === val;
            c.classList.toggle('on', on);
            c.setAttribute('aria-checked', on ? 'true' : 'false');
          });
          saveNotifyPrefs();
          break;
        }
        case 'copy-logs': copyServerLogs(); break;
        case 'term-spawn': termSpawnUser(); break;
        case 'term-view-output': termViewOutput(d.name); break;
        case 'term-close-user': termCloseUser(d.name); break;
        case 'browser-open': showOpenBrowserForm(); break;
        case 'browser-close': closeBrowserInstance(d.key); break;
        case 'browser-navigate': browserNavigate(d.key); break;
        case 'browser-screenshot': browserScreenshot(d.key); break;
        // Git actions
        case 'diff-stage-chunk': stageChunk(e.target.closest('.fe-window'), parseInt(d.cidx)); break;
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
        case 'files-delete': filesDelete(d.file); break;
        case 'term-select': if (d.tid) termToggleSelect(d.tid); break;
        case 'term-scroll-bottom': { const v = d.tid && termViews.get(d.tid); if (v) v.term.scrollToBottom(); break; }
        case 'term-send': termSendInput(); break;
        case 'term-attach': termAttachFile(); break;
        case 'term-tool-toggle': termToggleTools(); break;
        case 'toggle-mobile-layout': toggleMobileMosaic(); break;
        case 'term-key': termSendKey(d.key); break;
        case 'term-add': addTerminalCell(); break;
        case 'wb-add-menu': e.stopPropagation(); toggleWbAddMenu(); break;
        case 'wb-add': { hideWbAddMenu(); if (d.kind === 'terminal') addTerminalCell(); else addWbPanel(d.kind); break; }
        case 'wb-layout': { hideWbAddMenu(); mosaicApplyPreset(d.mlayout); break; }
        case 'leaf-split': { e.stopPropagation(); const [id, dir] = (d.leaf || '').split('|'); setMosaic(mosaicSplitLeaf(currentMosaic(), id, dir)); renderTermGrid(); break; }
        case 'leaf-remove': { e.stopPropagation(); setMosaic(mosaicCollapseLeaf(currentMosaic(), d.leaf)); renderTermGrid(); break; }
        case 'wb-close': if (d.wbid) { e.stopPropagation(); closeWbCell(d.wbid); } break;
        case 'wb-refresh': if (d.wbid) { e.stopPropagation(); refreshWbCell(d.wbid); } break;
        case 'term-close': if (d.tid) { e.stopPropagation(); closeTerminal(d.tid); } break;
        case 'term-close-pending': if (d.lid) { e.stopPropagation(); closePendingCell(d.lid); } break;
        case 'term-rename': if (d.tid) renameTerminal(d.tid); break;
        case 'term-font': if (d.tid) setTermFont(d.tid, parseInt(d.dir, 10) || 1); break;
        case 'term-font-reset': if (d.tid) resetTermFont(d.tid); break;
        case 'launch-terminal': launchTerminal(d.mode, d.lid); break;
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
        await loadKanbanMedia();
        renderKanban();
      } catch (err) {
        panel.innerHTML = '<div class="kanban-empty">Failed to load board: ' + escHtml(err.message) + '</div>';
      }
    }

    // Attached media for the current board, grouped by task id and todo id, so
    // cards can show a thumbnail strip without a per-card request.
    let kanbanMediaByTask = {}, kanbanMediaByTodo = {};
    async function loadKanbanMedia() {
      kanbanMediaByTask = {}; kanbanMediaByTodo = {};
      if (!currentProject) return;
      const items = await fetchMedia({ scope: 'project', kind: 'kanban' });
      for (const it of items) {
        if (!it.link) continue;
        if (it.link.type === 'task') (kanbanMediaByTask[it.link.taskId] = kanbanMediaByTask[it.link.taskId] || []).push(it);
        else if (it.link.type === 'todo') (kanbanMediaByTodo[it.link.todoId] = kanbanMediaByTodo[it.link.todoId] || []).push(it);
      }
    }
    function mediaMiniThumb(it) {
      const inner = it.kind === 'image' ? '<img loading="lazy" src="' + mediaRawUrl(it.id) + '">' : ic(MEDIA_KIND_ICON[it.kind] || 'file');
      return '<span class="media-mini" data-media-open="' + it.id + '" title="' + escHtml(it.originalName) + '">' + inner + '</span>';
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
        + '<button class="kanban-btn" data-kact="media">' + ic('images') + '<span class="tb-lbl"> Media</span></button>'
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
      // Task-level media strip: thumbnails + attach.
      const tmedia = kanbanMediaByTask[t.id] || [];
      h += '<div class="card-media">'
        + tmedia.slice(0, 6).map(mediaMiniThumb).join('')
        + '<button class="card-media-add" data-kmedia-task="' + t.id + '" title="Attach / view media">' + ic('paperclip') + '</button>'
        + '</div>';
      if (todos.length) {
        h += '<div class="kanban-todos">';
        for (const td of todos) {
          const tdm = (kanbanMediaByTodo[td.id] || []).length;
          h += '<div class="kanban-todo" data-drag-ref="' + td.id + '" data-drag-kind="kanban-subtask" title="Drag onto a terminal to insert this subtask id">'
            + '<input type="checkbox" data-ktodo-toggle="' + t.id + '|' + td.id + '"' + (td.done ? ' checked' : '') + '>'
            + '<span class="todo-text' + (td.done ? ' done' : '') + '">' + escHtml(td.text) + '</span>'
            + '<button class="todo-media" data-kmedia-todo="' + t.id + '|' + td.id + '" title="Subtask media">' + ic('paperclip') + (tdm ? '<span class="cnt">' + tdm + '</span>' : '') + '</button>'
            + '<button class="todo-edit" data-ktodo-edit="' + t.id + '|' + td.id + '" title="Edit subtask">' + ic('pencil') + '</button>'
            + '<button class="todo-del" data-ktodo-del="' + t.id + '|' + td.id + '" title="Delete subtask">&times;</button>'
            + '</div>';
        }
        h += '</div><div class="kanban-progress">' + done + '/' + todos.length + ' done</div>';
      }
      h += '<div class="kanban-todo-add">'
        + '<input type="text" placeholder="+ add subtask" data-ktodo-input="' + t.id + '">'
        + '<button class="ktodo-add-btn" data-ktodo-add="' + t.id + '" title="Add subtask">' + ic('check') + '</button>'
        + '</div>';
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

    // ─── Auto-scroll the workbench while dragging near its edges ───
    // Lets a drag reach panels that are currently off-screen. Targets only the
    // workbench scrollers: #term-grid (desktop horizontal / mobile column) and
    // the mobile snap rows (.mosaic-split.row). Snap rows jump exactly one screen
    // then sit out a 1s cooldown so the scroll-snap can settle before the next jump.
    const WB_EDGE = 60;        // px hot-zone from each edge
    const WB_SPEED = 22;       // px/frame max continuous scroll
    const WB_SNAP_COOLDOWN = 1000; // ms between one-screen snap jumps
    let _wbScrollRAF = null, _wbSnapUntil = 0;
    const _wbPtr = { x: 0, y: 0, active: false };
    function wbDragScroller(x, y) {
      let el = document.elementFromPoint(x, y);
      while (el && el !== document.body) {
        if (el.id === 'term-grid' || (el.classList && el.classList.contains('mosaic-split'))) {
          const cs = getComputedStyle(el);
          const canX = el.scrollWidth > el.clientWidth + 2 && /(auto|scroll)/.test(cs.overflowX);
          const canY = el.scrollHeight > el.clientHeight + 2 && /(auto|scroll)/.test(cs.overflowY);
          if (canX || canY) return { el, canX, canY, snap: !!cs.scrollSnapType && cs.scrollSnapType !== 'none' };
        }
        el = el.parentElement;
      }
      return null;
    }
    function wbScrollTick() {
      _wbScrollRAF = _wbPtr.active ? requestAnimationFrame(wbScrollTick) : null;
      if (!_wbPtr.active) return;
      const s = wbDragScroller(_wbPtr.x, _wbPtr.y);
      if (!s) return;
      const r = s.el.getBoundingClientRect(), x = _wbPtr.x, y = _wbPtr.y;
      if (s.snap) {
        // Snap container (mobile horizontal): one screen per jump, then cooldown.
        if (performance.now() < _wbSnapUntil) return;
        let dx = 0, dy = 0;
        if (s.canX && x > r.right - WB_EDGE) dx = r.width;
        else if (s.canX && x < r.left + WB_EDGE) dx = -r.width;
        else if (s.canY && y > r.bottom - WB_EDGE) dy = r.height;
        else if (s.canY && y < r.top + WB_EDGE) dy = -r.height;
        if (dx || dy) { s.el.scrollBy({ left: dx, top: dy, behavior: 'smooth' }); _wbSnapUntil = performance.now() + WB_SNAP_COOLDOWN; }
      } else {
        // Normal container (desktop): continuous, ramps up nearer the edge.
        if (s.canX) {
          if (x > r.right - WB_EDGE) s.el.scrollLeft += WB_SPEED * Math.min(1, (x - (r.right - WB_EDGE)) / WB_EDGE);
          else if (x < r.left + WB_EDGE) s.el.scrollLeft -= WB_SPEED * Math.min(1, ((r.left + WB_EDGE) - x) / WB_EDGE);
        }
        if (s.canY) {
          if (y > r.bottom - WB_EDGE) s.el.scrollTop += WB_SPEED * Math.min(1, (y - (r.bottom - WB_EDGE)) / WB_EDGE);
          else if (y < r.top + WB_EDGE) s.el.scrollTop -= WB_SPEED * Math.min(1, ((r.top + WB_EDGE) - y) / WB_EDGE);
        }
      }
    }
    function wbAutoScroll(x, y) {
      _wbPtr.x = x; _wbPtr.y = y; _wbPtr.active = true;
      if (_wbScrollRAF == null) _wbScrollRAF = requestAnimationFrame(wbScrollTick);
    }
    function wbAutoScrollStop() {
      _wbPtr.active = false; _wbSnapUntil = 0;
      if (_wbScrollRAF != null) { cancelAnimationFrame(_wbScrollRAF); _wbScrollRAF = null; }
    }

    // ─── Unified drag controller (mouse drag + mobile long-press) ───
    // h: { onStart?(), onMove(x,y), onEnd(commit), handle? }. Ignores drags that
    // begin on interactive controls (buttons/inputs/etc.) so those keep working.
    // When h.handle is given, the pointer listeners attach to that element while
    // the whole cell element is what gets cloned/dragged.
    function makeDraggable(el, h) {
      const trigger = h.handle || el;
      const THRESH = 6, HOLD = 320;
      let armed = false, dragging = false, sx = 0, sy = 0, holdTimer = null, clone = null, cdx = 0, cdy = 0;
      // Buttons/fields never start a drag. A parent can also pass
      // h.interactiveSelector to exclude its own draggable children (e.g. a
      // kanban card excludes its subtask rows, which have their own drag).
      const sel = 'button, input, select, textarea, a, label' + (h.interactiveSelector ? ', ' + h.interactiveSelector : '');
      const interactive = (t) => t && t.closest && t.closest(sel);
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
      function move(x, y) { if (clone) { clone.style.left = (x + cdx) + 'px'; clone.style.top = (y + cdy) + 'px'; } wbAutoScroll(x, y); h.onMove && h.onMove(x, y); }
      function finish(commit) {
        wbAutoScrollStop();
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
      trigger.addEventListener('mousedown', onMD);
      trigger.addEventListener('touchstart', onTS, { passive: true });
      trigger.addEventListener('touchmove', onTM, { passive: false });
      trigger.addEventListener('touchend', onTE);
      trigger.addEventListener('touchcancel', onTE);
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
          // Dragging a card out onto a terminal / input inserts its task id.
          const ext = wbDropTargetAt(x, y);
          if (ext) { target = { ext }; return; }
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
          if (commit && target) {
            if (target.ext) insertRefToTarget(target.ext, formatDragRef('kanban-task', taskId));
            else { await kanbanPost({ action: 'moveTask', taskId, status: target.status, index: target.index }); loadKanban(); }
          }
          target = null;
        },
      };
    }
    function attachKanbanDrag() {
      // Cards reorder / drop-to-terminal; the interactiveSelector keeps a card
      // from arming when you grab one of its (separately draggable) subtasks.
      $('#kanban-panel').querySelectorAll('.kanban-card').forEach(card => makeDraggable(card, Object.assign({ interactiveSelector: '[data-drag-ref]' }, kanbanDragHandlers(card.dataset.task))));
      wireDragRefs($('#kanban-panel')); // subtasks → drop onto a terminal/input
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
          // Dragging a node out onto a terminal / input inserts its node id.
          const ext = wbDropTargetAt(x, y);
          if (ext) { target = { ext }; return; }
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
          if (commit && target) {
            if (target.ext) insertRefToTarget(target.ext, formatDragRef('mindmap', nodeId));
            else { await mindmapPost({ action: 'moveNode', id: nodeId, parentId: target.parentId, index: target.index }); loadMindmap(); }
          }
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
        // Save/add a subtask (mobile-friendly; the Enter key still works too)
        const addBtn = e.target.closest('[data-ktodo-add]');
        if (addBtn) {
          const inp = addBtn.closest('.kanban-todo-add').querySelector('input');
          const text = inp ? inp.value.trim() : '';
          if (text) { await kanbanPost({ action: 'addTodo', taskId: addBtn.dataset.ktodoAdd, text }); loadKanban(); }
          return;
        }
        // Rename a subtask inline
        const editTodoBtn = e.target.closest('[data-ktodo-edit]');
        if (editTodoBtn) {
          const [taskId, todoId] = editTodoBtn.dataset.ktodoEdit.split('|');
          const task = (kanbanBoard.tasks || []).find(t => t.id === taskId);
          const td = task && (task.todos || []).find(x => x.id === todoId);
          const text = await askText({ title: 'Edit subtask', label: 'Subtask', value: td ? td.text : '' });
          if (text && text.trim()) { await kanbanPost({ action: 'updateTodo', taskId, todoId, text: text.trim() }); loadKanban(); }
          return;
        }
        // Attach / view media for a task or subtask.
        const kmt = e.target.closest('[data-kmedia-task]');
        if (kmt) {
          const id = kmt.dataset.kmediaTask;
          openMediaModal('Task media', { scope: 'project', kind: 'all', linkFilter: { type: 'task', taskId: id }, uploadLink: { type: 'task', taskId: id }, uploadProject: currentProject });
          return;
        }
        const kmd = e.target.closest('[data-kmedia-todo]');
        if (kmd) {
          const [taskId2, todoId2] = kmd.dataset.kmediaTodo.split('|');
          openMediaModal('Subtask media', { scope: 'project', kind: 'all', linkFilter: { type: 'todo', taskId: taskId2, todoId: todoId2 }, uploadLink: { type: 'todo', taskId: taskId2, todoId: todoId2 }, uploadProject: currentProject });
          return;
        }
        const btn = e.target.closest('[data-kact]');
        if (!btn) return;
        const act = btn.dataset.kact;
        const taskId = btn.dataset.task;
        if (act === 'media') { openMediaModal('Media \\u00b7 Kanban', { scope: 'project', kind: 'kanban' }); return; }
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
        + '<button class="kanban-btn" data-mact="media">' + ic('images') + '<span class="tb-lbl"> Media</span></button>'
        + '<input type="text" class="mm-search" id="mm-search" placeholder="Filter ideas…" autocomplete="off" value="' + escHtml(mindmapSearch) + '">'
        + '<div class="spacer"></div>'
        + '<span class="mm-hint" style="font-size:0.74rem;color:var(--text-muted)">Tap a node for details · Ctrl+scroll / pinch to zoom · green = linked</span>'
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
          // Center the parent over its REAL children only. The trailing "add
          // idea" ghost still gets packed into its own row (so nothing overlaps),
          // but it must NOT drag the parent — and thus its edge endpoints — below
          // the children's true vertical middle.
          const realCs = cs.filter((_, i) => !kids[i].ghost);
          const span = realCs.length ? realCs : cs;
          pos[node.id].y = (span[0] + span[span.length - 1]) / 2;
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
          if (act === 'media') {
            openMediaModal('Media \\u00b7 Mindmap', { scope: 'project', kind: 'mindmap' });
          } else if (act === 'add-root') {
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
      setupMediaHandlers();
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
      loadMmDetailMedia(id);
      $('#mm-detail-modal').classList.add('visible');
    }
    // Thumbnail strip of media attached to a node, shown in its detail modal.
    async function loadMmDetailMedia(nodeId) {
      const host = $('#mmd-media'); if (!host) return;
      host.innerHTML = '';
      const items = await fetchMedia({ scope: 'project', linkFilter: { type: 'node', nodeId } });
      host.innerHTML = items.slice(0, 10).map(mediaMiniThumb).join('')
        + '<button class="card-media-add" data-kmedia-node="' + nodeId + '" title="Attach / view media">' + ic('paperclip') + '</button>';
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
      $('#mmd-media-btn').addEventListener('click', () => { const id = mmDetailId; if (id) openNodeMediaModal(id); });
      $('#mmd-del-btn').addEventListener('click', () => { const id = mmDetailId; closeMmDetail(); mmDelete(id); });
    }

    // ─── Media library ───
    // One reusable browser renders into a host element driven by a state object
    // ({ scope:'project'|'all', kind:'all'|'kanban'|'mindmap'|'unlinked' }). Used
    // by the Media tab/panel (#media-panel) and the Media modal (kanban/mindmap).
    const mediaTabState = { scope: 'project', kind: 'all' };
    const _mediaCache = new Map(); // id -> last-seen enriched item (for click handlers)
    function findLoadedMedia(id) { return _mediaCache.get(id) || null; }
    const MEDIA_KIND_ICON = { image: 'image', pdf: 'file-text', video: 'film', audio: 'music', other: 'file' };
    function mtrunc(s, n) { s = String(s || ''); return s.length > n ? s.slice(0, n - 1) + '\\u2026' : s; }

    function mediaRawUrl(id, download) {
      return '/api/media/raw/' + encodeURIComponent(id) + '?token=' + encodeURIComponent(token || '') + (download ? '&download=1' : '');
    }
    function mediaListUrl(state) {
      const p = new URLSearchParams();
      if (state.scope === 'project' && currentProject) { p.set('project', currentProject); p.set('includeGeneral', '1'); }
      if (state.linkFilter) {
        const f = state.linkFilter;
        p.set('linkType', f.type);
        if (f.taskId) p.set('taskId', f.taskId);
        if (f.todoId) p.set('todoId', f.todoId);
        if (f.nodeId) p.set('nodeId', f.nodeId);
      } else if (state.kind && state.kind !== 'all') {
        p.set('kind', state.kind);
      }
      return '/api/media/list?' + p.toString();
    }
    async function fetchMedia(state) {
      try {
        const res = await apiFetch(mediaListUrl(state));
        const d = await res.json();
        return d.ok ? d.items : [];
      } catch { return []; }
    }

    function mediaThumbHtml(it) {
      if (it.kind === 'image') return '<img loading="lazy" src="' + mediaRawUrl(it.id) + '" alt="">';
      const glyph = MEDIA_KIND_ICON[it.kind] || 'file';
      const ext = (it.ext || '').replace('.', '') || it.kind;
      return '<div class="media-glyph">' + ic(glyph) + '<span class="ext">' + escHtml(ext) + '</span></div>';
    }
    function mediaLinkSubHtml(it) {
      if (!it.link) return '<span>Unlinked</span>';
      const word = it.link.type === 'node' ? 'Idea' : it.link.type === 'todo' ? 'Subtask' : 'Task';
      if (it.linkStatus === 'deleted') return '<span class="media-link-tag deleted">' + ic('external-link') + word + ' (deleted)</span>';
      return '<span class="media-link-tag" data-media-jump="' + it.id + '" title="Jump to ' + word.toLowerCase() + '">' + ic('external-link') + escHtml(mtrunc(it.linkLabel || word, 22)) + '</span>';
    }
    function mediaCardHtml(it) {
      const previewable = it.kind !== 'other';
      // Whole card is draggable onto a terminal / input (inserts the file path),
      // just like the Files panel.
      const drag = it.path ? ' data-drag-ref="' + escHtml(it.path) + '" data-drag-kind="media" title="Drag onto a terminal to insert this path"' : '';
      let h = '<div class="media-card" data-media="' + it.id + '"' + drag + '>';
      h += '<div class="media-thumb" data-media-open="' + it.id + '">'
        + '<span class="media-kindbadge">' + ic(MEDIA_KIND_ICON[it.kind] || 'file') + '</span>'
        + mediaThumbHtml(it) + '</div>';
      h += '<div class="media-info"><div class="media-name" title="' + escHtml(it.originalName) + '">' + escHtml(it.originalName) + '</div>'
        + '<div class="media-sub">' + (it.projectName ? '<span>' + escHtml(mtrunc(it.projectName, 16)) + '</span>' : '') + mediaLinkSubHtml(it) + '</div></div>';
      h += '<div class="media-actions">';
      if (previewable) h += '<button data-media-open="' + it.id + '" title="Preview">' + ic('image') + 'View</button>';
      else h += '<button data-media-dl="' + it.id + '" title="Download">' + ic('download') + 'Get</button>';
      if (it.path) h += '<button data-media-copy="' + it.id + '" title="Copy file path">' + ic('copy') + '</button>';
      if (it.link) h += '<button data-media-jump="' + it.id + '" title="Jump to source"' + (it.linkStatus === 'deleted' ? ' disabled' : '') + '>' + ic('external-link') + '</button>';
      h += '<button class="danger" data-media-del="' + it.id + '" title="Delete">' + ic('trash') + '</button>';
      h += '</div></div>';
      return h;
    }
    // Filter loaded items by the search query (name, link label, project, type).
    function mediaFilter(items, q) {
      q = (q || '').trim().toLowerCase();
      if (!q) return items;
      return items.filter(it => (
        (it.originalName || '').toLowerCase().includes(q)
        || (it.linkLabel || '').toLowerCase().includes(q)
        || (it.projectName || '').toLowerCase().includes(q)
        || (it.ext || '').toLowerCase().includes(q)
        || (it.kind || '').toLowerCase().includes(q)
      ));
    }
    // Re-render just the grid for the current search query (keeps the search box
    // focused while typing — no refetch).
    function mediaApplyFilter(host) {
      const body = host.querySelector('.media-body'); if (!body) return;
      const items = host._mediaItems || [];
      const filtered = mediaFilter(items, host._mediaState.q);
      if (!items.length) body.innerHTML = '<div class="media-empty">No media here yet. Upload files, or attach them to a Kanban task/subtask or a Mindmap idea.</div>';
      else if (!filtered.length) body.innerHTML = '<div class="media-empty">No media matches “' + escHtml(host._mediaState.q) + '”.</div>';
      else body.innerHTML = '<div class="media-grid">' + filtered.map(mediaCardHtml).join('') + '</div>';
      wireDragRefs(host);
    }
    function renderMediaBrowser(host, state, items) {
      let h = '<div class="media-toolbar">';
      h += '<button class="media-upload-btn" data-media-upload="1">' + ic('upload') + 'Upload</button>';
      h += '<input type="text" class="media-search" placeholder="Search media…" autocomplete="off" value="' + escHtml(state.q || '') + '">';
      if (state.linkFilter) {
        // Target-specific view (one task / subtask / idea): no filter chips.
        h += '<span class="spacer"></span><span style="font-size:0.74rem;color:var(--text-muted)">Attached here</span>';
      } else {
        const chip = (k, label, icon) => '<button class="media-chip' + (state.kind === k ? ' on' : '') + '" data-media-kind="' + k + '">' + (icon ? ic(icon) : '') + label + '</button>';
        h += chip('all', 'All') + chip('kanban', 'Kanban', 'kanban') + chip('mindmap', 'Mindmap', 'mindmap') + chip('unlinked', 'Unlinked');
        h += '<div class="spacer"></div>';
        h += '<button class="media-chip' + (state.scope === 'project' ? ' on' : '') + '" data-media-scope="project">This project</button>';
        h += '<button class="media-chip' + (state.scope === 'all' ? ' on' : '') + '" data-media-scope="all">All projects</button>';
      }
      h += '</div><div class="media-body"></div>';
      host.innerHTML = h;
      host._mediaState = state;
      host._mediaItems = items;
      for (const it of items) _mediaCache.set(it.id, it);
      if (!state.linkFilter && state.scope === 'project' && !currentProject) {
        host.querySelector('.media-body').innerHTML = '<div class="media-empty">Select a project, or switch to <b>All projects</b>.</div>';
      } else {
        mediaApplyFilter(host);
      }
      const search = host.querySelector('.media-search');
      if (search) search.addEventListener('input', () => { host._mediaState.q = search.value; mediaApplyFilter(host); });
    }
    // Mount/refresh a media browser into a host element; remembers how to reload.
    async function mountMediaBrowser(host, state) {
      const reload = async () => {
        const items = (state.scope === 'project' && !currentProject) ? [] : await fetchMedia(state);
        renderMediaBrowser(host, state, items);
        host._mediaReload = reload;
      };
      host._mediaReload = reload;
      await reload();
      return reload;
    }
    // The Media tab / workbench panel (single #media-panel node, moved like the
    // other workbench panels).
    function loadMedia() {
      const host = $('#media-panel'); if (!host) return;
      mountMediaBrowser(host, mediaTabState);
    }
    // Refresh every mounted media browser (tab/panel + open modal) after a change.
    function refreshAllMediaViews() {
      document.querySelectorAll('.media-panel, .media-modal-body').forEach(h => { if (h._mediaReload) h._mediaReload(); });
      // Refresh the mindmap node-detail thumbnail strip if it's open.
      if (mmDetailId && $('#mm-detail-modal').classList.contains('visible')) loadMmDetailMedia(mmDetailId);
    }

    // ── Upload ──
    let mediaUploadCtx = null; // { project?, link?, reload? }
    function mediaTriggerUpload(ctx) {
      mediaUploadCtx = ctx || {};
      const inp = $('#media-file-input'); if (!inp) return;
      inp.value = ''; inp.click();
    }
    function fileToBase64(file) {
      return new Promise((resolve, reject) => {
        const r = new FileReader();
        r.onload = () => resolve(String(r.result).split(',')[1] || '');
        r.onerror = reject;
        r.readAsDataURL(file);
      });
    }
    async function mediaUploadFiles(files) {
      const ctx = mediaUploadCtx || {};
      let ok = 0;
      for (const f of files) {
        try {
          const data = await fileToBase64(f);
          const body = { name: f.name, data };
          if (ctx.link) body.link = ctx.link;
          // Scope: explicit ctx.project wins; else current project (so it shows
          // under the project) unless this is a link (server derives it then).
          if (ctx.project) body.project = ctx.project;
          else if (!ctx.link && currentProject) body.project = currentProject;
          const res = await apiFetch('/api/media/upload', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
          const d = await res.json();
          if (d.ok) ok++; else toast(d.error || 'Upload failed', 'error');
        } catch (err) { toast('Upload error: ' + err.message, 'error'); }
      }
      if (ok) toast('Uploaded ' + ok + ' file' + (ok === 1 ? '' : 's'), 'success');
      if (ctx.reload) ctx.reload();
      refreshAllMediaViews();
      // refresh attachment strips on cards/nodes if shown
      if (currentTab === 'kanban') loadKanban();
    }

    // ── Delete ──
    async function mediaDelete(id) {
      if (!confirm('Delete this media file? This permanently removes it and cannot be undone.')) return;
      try {
        const res = await apiFetch('/api/media/delete', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ id }) });
        const d = await res.json();
        if (!d.ok) { toast(d.error || 'Delete failed', 'error'); return; }
        toast('Deleted', 'success');
        refreshAllMediaViews();
        if (currentTab === 'kanban') loadKanban();
      } catch (err) { toast('Error: ' + err.message, 'error'); }
    }
    function mediaDownload(id) {
      const a = document.createElement('a');
      a.href = mediaRawUrl(id, true); a.download = '';
      document.body.appendChild(a); a.click(); a.remove();
    }

    // ── Lightbox ──
    function openMediaLightbox(it) {
      if (!it) return;
      if (it.kind === 'other') { mediaDownload(it.id); return; }
      const stage = $('#ml-stage'); const u = mediaRawUrl(it.id);
      $('#ml-title').textContent = it.originalName || '';
      if (it.kind === 'image') stage.innerHTML = '<img src="' + u + '" alt="">';
      else if (it.kind === 'pdf') stage.innerHTML = '<iframe src="' + u + '"></iframe>';
      else if (it.kind === 'video') stage.innerHTML = '<video src="' + u + '" controls autoplay></video>';
      else if (it.kind === 'audio') stage.innerHTML = '<audio src="' + u + '" controls autoplay></audio>';
      $('#ml-download').onclick = () => mediaDownload(it.id);
      $('#media-lightbox').classList.add('visible');
    }
    function closeMediaLightbox() {
      $('#media-lightbox').classList.remove('visible');
      $('#ml-stage').innerHTML = ''; // stop any playing media
    }

    // ── Media modal (kanban / mindmap "Media" buttons) ──
    function openMediaModal(title, state) {
      $('#media-modal-title').textContent = title || 'Media';
      $('#media-modal').classList.add('visible');
      mountMediaBrowser($('#media-modal-body'), state);
    }
    function closeMediaModal() {
      $('#media-modal').classList.remove('visible');
      const b = $('#media-modal-body'); if (b) { b.innerHTML = ''; b._mediaReload = null; }
    }
    function openNodeMediaModal(nodeId) {
      openMediaModal('Idea media', { scope: 'project', kind: 'all', linkFilter: { type: 'node', nodeId }, uploadLink: { type: 'node', nodeId } });
    }

    // ── Jump to source (switch project + tab, scroll & flash the target) ──
    async function mediaJump(it) {
      if (!it || !it.link) return;
      if (it.linkStatus === 'deleted') { toast('The original was deleted', 'error'); return; }
      closeMediaModal(); closeMediaLightbox();
      if (it.project && it.project !== currentProject) {
        if (projects.find(p => p.alias === it.project)) await selectProject(it.project);
        else { toast('Source project not available', 'error'); return; }
      }
      if (it.link.type === 'node') {
        switchTab('mindmap');
        await loadMindmap();
        setTimeout(() => flashMindmapNode(it.link.nodeId), 250);
      } else {
        switchTab('kanban');
        await loadKanban();
        setTimeout(() => flashKanbanTarget(it.link), 250);
      }
    }
    function flashEl(el) {
      if (!el) return;
      try { el.scrollIntoView({ behavior: 'smooth', block: 'center', inline: 'center' }); } catch { el.scrollIntoView(); }
      el.classList.remove('jump-flash'); void el.offsetWidth; el.classList.add('jump-flash');
      setTimeout(() => el.classList.remove('jump-flash'), 2400);
    }
    function flashKanbanTarget(link) {
      const panel = $('#kanban-panel'); if (!panel) return;
      if (link.type === 'todo') {
        const row = panel.querySelector('[data-drag-ref="' + link.todoId + '"]');
        if (row) { flashEl(row); return; }
      }
      const card = panel.querySelector('.kanban-card[data-task="' + link.taskId + '"]');
      flashEl(card);
    }
    function flashMindmapNode(nodeId) {
      const el = $('#mindmap-panel') && $('#mindmap-panel').querySelector('.mm-node[data-node="' + nodeId + '"]');
      if (el) { flashEl(el); openMmDetail(nodeId); }
      else { openMmDetail(nodeId); }
    }

    // ── One delegated handler for all media UI (tab, panel, modal, cards) ──
    let _mediaBound = false;
    function setupMediaHandlers() {
      if (_mediaBound) return; _mediaBound = true;
      document.addEventListener('click', (e) => {
        const t = e.target;
        const hostOf = (el) => el.closest('.media-panel, .media-modal-body');
        let m;
        if ((m = t.closest('[data-media-kind]'))) {
          const host = hostOf(m); if (host && host._mediaState) { host._mediaState.kind = m.dataset.mediaKind; host._mediaReload(); }
          return;
        }
        if ((m = t.closest('[data-media-scope]'))) {
          const host = hostOf(m); if (host && host._mediaState) { host._mediaState.scope = m.dataset.mediaScope; host._mediaReload(); }
          return;
        }
        if ((m = t.closest('[data-media-upload]'))) {
          const host = hostOf(m);
          const st = (host && host._mediaState) || {};
          mediaTriggerUpload({ link: st.uploadLink || null, project: st.uploadProject || null, reload: host && host._mediaReload });
          return;
        }
        if ((m = t.closest('[data-kmedia-node]'))) { openNodeMediaModal(m.dataset.kmediaNode); return; }
        if ((m = t.closest('[data-media-copy]'))) { const it = findLoadedMedia(m.dataset.mediaCopy); if (it && it.path) navigator.clipboard.writeText(it.path).then(() => toast('Path copied'), () => toast('Copy failed', 'error')); return; }
        if ((m = t.closest('[data-media-open]'))) { openMediaLightbox(findLoadedMedia(m.dataset.mediaOpen)); return; }
        if ((m = t.closest('[data-media-dl]'))) { mediaDownload(m.dataset.mediaDl); return; }
        if ((m = t.closest('[data-media-jump]'))) { mediaJump(findLoadedMedia(m.dataset.mediaJump)); return; }
        if ((m = t.closest('[data-media-del]'))) { mediaDelete(m.dataset.mediaDel); return; }
      });
      const inp = $('#media-file-input');
      if (inp) inp.addEventListener('change', () => { if (inp.files && inp.files.length) mediaUploadFiles([...inp.files]); });
      $('#ml-close').addEventListener('click', closeMediaLightbox);
      $('#media-lightbox').addEventListener('click', (e) => { if (e.target.id === 'media-lightbox') closeMediaLightbox(); });
      $('#media-modal-close').addEventListener('click', closeMediaModal);
      $('#media-modal').addEventListener('click', (e) => { if (e.target.id === 'media-modal') closeMediaModal(); });
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
    setupScheduleHandlers();
    applyStoredSidebarCollapsed();
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

    // ─── Schedule tab ───
    let schedules = [];
    let scheduleServices = [];
    let scheduleFilter = true;        // default: filter to current project
    let scheduleDraft = null;         // the schedule being added/edited
    let schedBoard = null;            // kanban board for the draft's project (for kanban conditions)
    const SCHED_MODELS = ['opus', 'sonnet', 'haiku', 'fable', 'mythos'];
    const SCHED_EFFORTS = ['low', 'medium', 'high', 'xhigh', 'max'];

    async function loadSchedules() {
      const panel = $('#schedule-panel'); if (!panel) return;
      const proj = scheduleFilter && currentProject ? currentProject : '';
      try {
        const res = await apiFetch('/api/schedules' + (proj ? '?project=' + encodeURIComponent(proj) : ''));
        const data = await res.json();
        if (!data.ok) { panel.innerHTML = '<div class="git-empty">' + escHtml(data.error || 'Failed') + '</div>'; return; }
        schedules = data.schedules || [];
        scheduleServices = data.services || [];
        renderSchedules();
      } catch (err) { panel.innerHTML = '<div class="git-empty">Error: ' + escHtml(err.message) + '</div>'; }
    }

    function schedActionSummary(a) {
      a = a || {};
      if (a.kind === 'agent') {
        const bits = ['Claude', a.mode === 'skip' ? 'skip-perms' : 'normal'];
        if (a.model) bits.push(a.model);
        if (a.effort) bits.push(a.effort);
        bits.push(a.session === 'continue' ? 'continue' : a.session === 'resume' ? ('resume ' + (a.sessionId || '').slice(0, 8)) : 'new session');
        return 'Agent · ' + bits.join(' · ');
      }
      if (a.kind === 'command') return 'Run: ' + (a.command || '');
      if (a.kind === 'service') {
        const svc = scheduleServices.find(s => s.key === a.serviceKey);
        return (a.op || 'start') + ' service · ' + (svc ? svc.name : (a.serviceKey || '?'));
      }
      return '';
    }
    function schedCondSummary(c) {
      if (c.type === 'time') return 'at ' + (c.at || '?');
      if (c.type === 'usage') return (c.metric === 'week' ? 'weekly' : '5h') + ' usage ' + (c.op || 'below') + ' ' + (c.value || 0) + '%';
      if (c.type === 'terminals') return 'open terminals ' + (c.op || 'below') + ' ' + (c.value || 0);
      if (c.type === 'kanban') return 'kanban ' + (c.todoId ? 'subtask' : 'task') + ' = ' + (c.status || '?');
      return '';
    }

    function renderSchedules() {
      const panel = $('#schedule-panel'); if (!panel) return;
      let h = '<div class="kanban-toolbar">'
        + '<button class="kanban-btn primary" data-sched="add">+ Add schedule</button>'
        + '<div class="spacer"></div>';
      if (currentProject) {
        h += '<button class="kanban-btn' + (scheduleFilter ? ' toggled' : '') + '" data-sched="filter">'
          + (scheduleFilter ? 'This project' : 'All projects') + '</button>';
      }
      h += '</div>';
      if (!schedules.length) {
        h += '<div class="services-empty"><div class="icon">' + ic('clock') + '</div><p>No scheduled tasks'
          + (scheduleFilter && currentProject ? ' for this project' : '') + '</p></div>';
      } else {
        h += '<div class="sched-list">';
        for (const s of schedules) {
          h += '<div class="sched-item' + (s.enabled ? '' : ' off') + '">'
            + '<button class="sched-power' + (s.enabled ? ' on' : '') + '" data-sched="enable" data-id="' + s.id + '" title="' + (s.enabled ? 'Enabled — click to disable' : 'Disabled — click to enable') + '"></button>'
            + '<div class="sched-body" data-sched="edit" data-id="' + s.id + '">'
            + '<div class="sched-name">' + escHtml(s.name) + (!scheduleFilter ? ' <span class="sched-proj">' + escHtml(s.project) + '</span>' : '') + '</div>'
            + '<div class="sched-sum">' + escHtml(schedActionSummary(s.action)) + '</div>'
            + '<div class="sched-conds"><span class="sched-cond when">' + escHtml(schedWhenSummary(effectiveWhen(s))) + '</span>'
            + (s.conditions || []).filter(c => c.type !== 'time').map(c => '<span class="sched-cond">' + escHtml(schedCondSummary(c)) + '</span>').join('') + '</div>'
            + (s.lastRun ? '<div class="sched-last">last run ' + escHtml(new Date(s.lastRun).toLocaleString()) + '</div>' : '')
            + '</div>'
            + '<div class="sched-rowacts">'
            + '<button class="term-head-btn" data-sched="edit" data-id="' + s.id + '" title="Edit">' + ic('pencil') + '</button>'
            + '<button class="term-head-btn" data-sched="delete" data-id="' + s.id + '" title="Delete">' + ic('trash') + '</button>'
            + '</div></div>';
        }
        h += '</div>';
      }
      panel.innerHTML = h;
    }

    const SCHED_DAYS = ['Su', 'Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa'];
    function defaultSchedule() {
      return {
        id: null, name: '', project: currentProject || '', enabled: true,
        action: { kind: 'agent', agent: 'claude', mode: 'normal', model: '', effort: '', session: 'new', sessionId: '', prompt: '', command: '', serviceKey: '', op: 'start' },
        when: { mode: 'recurring', time: '09:00', date: '', days: [0, 1, 2, 3, 4, 5, 6] },
        conditions: [],
      };
    }
    // Resolve a schedule's time gate, migrating any legacy time-type condition.
    function effectiveWhen(s) {
      if (s.when && s.when.time) return s.when;
      const t = (s.conditions || []).find(c => c.type === 'time');
      if (t && t.at) return { mode: 'recurring', days: [0, 1, 2, 3, 4, 5, 6], time: t.at };
      return null;
    }
    function schedWhenSummary(w) {
      if (!w || !w.time) return 'no time set';
      if (w.mode === 'once') return (w.date || '(no date)') + ' ' + w.time;
      const days = Array.isArray(w.days) ? w.days : [];
      if (!days.length) return 'no days · ' + w.time;
      if (days.length === 7) return 'Daily ' + w.time;
      return days.slice().sort((a, b) => a - b).map(d => SCHED_DAYS[d]).join(',') + ' ' + w.time;
    }
    async function openSchedModal(id) {
      if (id) {
        const s = schedules.find(x => x.id === id);
        if (!s) return;
        scheduleDraft = JSON.parse(JSON.stringify(s));
        // merge defaults so all action fields exist
        scheduleDraft.action = Object.assign(defaultSchedule().action, scheduleDraft.action || {});
        // Ensure a when-gate exists (migrate any legacy time-type condition).
        scheduleDraft.when = Object.assign(defaultSchedule().when, effectiveWhen(scheduleDraft) || {});
        scheduleDraft.conditions = (scheduleDraft.conditions || []).filter(c => c.type !== 'time');
      } else {
        scheduleDraft = defaultSchedule();
        scheduleDraft.project = currentProject || (projects[0] && projects[0].alias) || '';
      }
      // Default a service key if needed
      const svcs = scheduleServices.filter(s => String(s.alias || '').toLowerCase() === String(scheduleDraft.project).toLowerCase());
      if (!scheduleDraft.action.serviceKey && svcs[0]) scheduleDraft.action.serviceKey = svcs[0].key;
      schedBoard = null;
      try {
        const r = await apiFetch('/api/kanban?project=' + encodeURIComponent(scheduleDraft.project));
        const d = await r.json();
        if (d.ok) schedBoard = d.board;
      } catch { /* ignore */ }
      $('#sched-modal').classList.add('visible');
      renderSchedModal();
    }
    function closeSchedModal() { $('#sched-modal').classList.remove('visible'); scheduleDraft = null; }

    function schedSeg(group, val, label, cur) {
      return '<button class="sm-segb' + (cur === val ? ' on' : '') + '" data-seg="' + group + '" data-val="' + val + '">' + escHtml(label) + '</button>';
    }
    function schedOpts(values, cur, blank) {
      return values.map(v => '<option value="' + escHtml(v) + '"' + (v === cur ? ' selected' : '') + '>' + escHtml(v === '' ? blank : v) + '</option>').join('');
    }
    function schedCondRow(c, i) {
      let inner = '';
      if (c.type === 'time') {
        inner = '<span class="sm-condt">Time</span><input type="time" class="sm-input cc" data-f="at" value="' + escHtml(c.at || '09:00') + '">';
      } else if (c.type === 'usage') {
        inner = '<span class="sm-condt">' + (c.metric === 'week' ? 'Weekly usage' : '5h usage') + '</span>'
          + '<select class="sm-input cc" data-f="op">' + schedOpts(['below', 'above', 'equal'], c.op || 'below') + '</select>'
          + '<input type="number" class="sm-input cc" data-f="value" min="0" max="100" value="' + escHtml(c.value != null ? c.value : 50) + '" style="width:64px"> %';
      } else if (c.type === 'terminals') {
        inner = '<span class="sm-condt">Open terminals</span>'
          + '<select class="sm-input cc" data-f="op">' + schedOpts(['below', 'above', 'equal'], c.op || 'below') + '</select>'
          + '<input type="number" class="sm-input cc" data-f="value" min="0" value="' + escHtml(c.value != null ? c.value : 1) + '" style="width:64px">';
      } else if (c.type === 'kanban') {
        const tasks = (schedBoard && schedBoard.tasks) || [];
        const statuses = (schedBoard && schedBoard.statuses) || ['backlog', 'todo', 'in_progress', 'done'];
        const task = tasks.find(t => t.id === c.taskId);
        const todos = task ? (task.todos || []) : [];
        inner = '<span class="sm-condt">Kanban</span>'
          + '<select class="sm-input cc restruct" data-f="taskId"><option value="">— pick task —</option>'
          + tasks.map(t => '<option value="' + t.id + '"' + (t.id === c.taskId ? ' selected' : '') + '>' + escHtml(t.title) + '</option>').join('') + '</select>'
          + '<select class="sm-input cc restruct" data-f="todoId"><option value="">whole task</option>'
          + todos.map(td => '<option value="' + td.id + '"' + (td.id === c.todoId ? ' selected' : '') + '>' + escHtml(td.text) + '</option>').join('') + '</select>'
          + '<select class="sm-input cc" data-f="status">'
          + (c.todoId ? schedOpts(['done', 'pending'], c.status || 'done') : statuses.map(st => '<option value="' + st + '"' + (st === c.status ? ' selected' : '') + '>' + escHtml((KANBAN_STATUS_LABELS && KANBAN_STATUS_LABELS[st]) || st) + '</option>').join(''))
          + '</select>';
      }
      return '<div class="sm-cond" data-ci="' + i + '">' + inner + '<button class="sm-condx" data-sched="remove-cond" data-ci="' + i + '" title="Remove">' + '\\u00d7' + '</button></div>';
    }
    function renderSchedModal() {
      const card = $('#sched-modal-card'); if (!card || !scheduleDraft) return;
      const d = scheduleDraft, a = d.action, w = d.when;
      const svcs = scheduleServices.filter(s => String(s.alias || '').toLowerCase() === String(d.project).toLowerCase());
      let h = '<div class="sm-head"><h3>' + (d.id ? 'Edit schedule' : 'New schedule') + '</h3>'
        + '<button class="sm-x" data-sched="close" title="Close">\\u00d7</button></div><div class="sm-bodyscroll">';
      h += '<label class="sm-label">Name</label><input class="sm-input" id="sm-name" value="' + escHtml(d.name || '') + '" placeholder="My scheduled task">';

      // Project picker (defaults to the current project).
      h += '<label class="sm-label">Project</label><select class="sm-input" id="sm-project">'
        + (projects.length ? projects.map(p => '<option value="' + escHtml(p.alias) + '"' + (String(p.alias).toLowerCase() === String(d.project).toLowerCase() ? ' selected' : '') + '>' + escHtml(p.name || p.alias) + '</option>').join('') : '<option value="">No projects</option>')
        + '</select>';

      // When (mandatory, singular): one-time date+time OR recurring days+time.
      h += '<label class="sm-label">When</label><div class="sm-seg">'
        + schedSeg('whenmode', 'once', 'One-time', w.mode) + schedSeg('whenmode', 'recurring', 'Recurring', w.mode) + '</div>';
      h += '<div class="sm-when">';
      if (w.mode === 'once') {
        h += '<input type="date" class="sm-input" id="sm-when-date" value="' + escHtml(w.date || '') + '">';
      } else {
        h += '<div class="sm-days">' + SCHED_DAYS.map((lbl, i) => '<button class="sm-day' + ((w.days || []).includes(i) ? ' on' : '') + '" data-day="' + i + '" type="button">' + lbl + '</button>').join('') + '</div>';
      }
      h += '<input type="time" class="sm-input" id="sm-when-time" value="' + escHtml(w.time || '09:00') + '"></div>';

      // Action: pick what to run.
      h += '<label class="sm-label">Action</label><div class="sm-seg">'
        + schedSeg('kind', 'agent', 'Agent', a.kind) + schedSeg('kind', 'command', 'CLI command', a.kind) + schedSeg('kind', 'service', 'Service', a.kind) + '</div>';
      if (a.kind === 'agent') {
        // Choose the agent, then that agent's specific options. (Claude only for now.)
        h += '<label class="sm-label">Agent</label><div class="sm-seg">' + schedSeg('agent', 'claude', 'Claude', a.agent || 'claude') + '</div>';
        if ((a.agent || 'claude') === 'claude') {
          h += '<div class="sm-row2">'
            + '<div><label class="sm-label">Model</label><select class="sm-input" id="sm-model">' + schedOpts(['', ...SCHED_MODELS], a.model, 'Default') + '</select></div>'
            + '<div><label class="sm-label">Effort</label><select class="sm-input" id="sm-effort">' + schedOpts(['', ...SCHED_EFFORTS], a.effort, 'Default') + '</select></div></div>';
          h += '<label class="sm-label">Mode</label><div class="sm-seg">' + schedSeg('mode', 'normal', 'Normal', a.mode) + schedSeg('mode', 'skip', 'Skip permissions', a.mode) + '</div>';
          h += '<label class="sm-label">Session</label><div class="sm-seg">' + schedSeg('session', 'new', 'New', a.session) + schedSeg('session', 'continue', 'Continue last', a.session) + schedSeg('session', 'resume', 'Resume id', a.session) + '</div>';
          if (a.session === 'resume') h += '<input class="sm-input" id="sm-sessionId" value="' + escHtml(a.sessionId || '') + '" placeholder="session id e.g. 8441d374-fd1a-...">';
          h += '<label class="sm-label">Prompt</label><textarea class="sm-input" id="sm-prompt" rows="3" placeholder="What should the agent do?">' + escHtml(a.prompt || '') + '</textarea>';
          h += '<p class="sm-note">Tip: you can include slash commands like <code>/frontend-design</code> or <code>/loop</code> right in the prompt — they run just like typing them in the terminal.</p>';
        }
      } else if (a.kind === 'command') {
        h += '<label class="sm-label">Command</label><input class="sm-input" id="sm-command" value="' + escHtml(a.command || '') + '" placeholder="e.g. npm run build">';
      } else if (a.kind === 'service') {
        h += '<label class="sm-label">Service</label><select class="sm-input" id="sm-service">'
          + (svcs.length ? svcs.map(s => '<option value="' + escHtml(s.key) + '"' + (s.key === a.serviceKey ? ' selected' : '') + '>' + escHtml(s.name) + '</option>').join('') : '<option value="">No services for this project</option>') + '</select>';
        h += '<label class="sm-label">Operation</label><div class="sm-seg">' + schedSeg('op', 'start', 'Start if stopped', a.op) + schedSeg('op', 'stop', 'Stop if running', a.op) + schedSeg('op', 'toggle', 'Toggle', a.op) + '</div>';
      }

      // Extra conditions (all must also be true). Time is NOT here — see When.
      h += '<label class="sm-label">Extra conditions <span class="sm-hint">(optional — all must be true)</span></label>';
      h += '<div id="sm-conds">' + d.conditions.map((c, i) => schedCondRow(c, i)).join('') + '</div>';
      h += '<div class="sm-addcond">'
        + '<button class="kanban-btn" data-sched="add-cond" data-type="usage">+ 5h usage</button>'
        + '<button class="kanban-btn" data-sched="add-cond" data-type="usage-week">+ Weekly usage</button>'
        + '<button class="kanban-btn" data-sched="add-cond" data-type="terminals">+ Open terminals</button>'
        + '<button class="kanban-btn" data-sched="add-cond" data-type="kanban">+ Kanban status</button></div>';
      h += '</div><div class="sm-foot"><button class="kanban-btn" data-sched="close">Cancel</button><button class="kanban-btn primary" data-sched="save">Save</button></div>';
      card.innerHTML = h;
    }
    function captureSchedForm() {
      const card = $('#sched-modal-card'); if (!card || !scheduleDraft) return;
      const a = scheduleDraft.action;
      const val = (id) => { const el = card.querySelector('#' + id); return el ? el.value : undefined; };
      if (val('sm-name') !== undefined) scheduleDraft.name = val('sm-name');
      if (val('sm-project') !== undefined) scheduleDraft.project = val('sm-project');
      if (val('sm-when-time') !== undefined) scheduleDraft.when.time = val('sm-when-time');
      if (val('sm-when-date') !== undefined) scheduleDraft.when.date = val('sm-when-date');
      if (card.querySelector('.sm-day')) scheduleDraft.when.days = [...card.querySelectorAll('.sm-day.on')].map(el => +el.dataset.day);
      if (val('sm-model') !== undefined) a.model = val('sm-model');
      if (val('sm-effort') !== undefined) a.effort = val('sm-effort');
      if (val('sm-sessionId') !== undefined) a.sessionId = val('sm-sessionId');
      if (val('sm-prompt') !== undefined) a.prompt = val('sm-prompt');
      if (val('sm-command') !== undefined) a.command = val('sm-command');
      if (val('sm-service') !== undefined) a.serviceKey = val('sm-service');
      card.querySelectorAll('.sm-cond').forEach(row => {
        const c = scheduleDraft.conditions[+row.dataset.ci]; if (!c) return;
        row.querySelectorAll('[data-f]').forEach(el => { c[el.dataset.f] = el.value; });
      });
    }
    async function saveSchedule() {
      captureSchedForm();
      const d = scheduleDraft;
      if (!d.name.trim()) { toast('Give the schedule a name', 'error'); return; }
      if (!d.project) { toast('Pick a project', 'error'); return; }
      if (!d.when.time) { toast('Set a time', 'error'); return; }
      if (d.when.mode === 'once' && !d.when.date) { toast('Pick a date for the one-time run', 'error'); return; }
      if (d.when.mode === 'recurring' && !(d.when.days || []).length) { toast('Pick at least one day', 'error'); return; }
      if (d.action.kind === 'agent' && !d.action.prompt.trim()) { toast('Agent needs a prompt', 'error'); return; }
      if (d.action.kind === 'command' && !d.action.command.trim()) { toast('Enter a command', 'error'); return; }
      if (d.action.kind === 'service' && !d.action.serviceKey) { toast('Pick a service', 'error'); return; }
      const payload = { name: d.name, project: d.project, enabled: d.enabled, action: d.action, when: d.when, conditions: d.conditions };
      const body = d.id ? { action: 'update', id: d.id, schedule: payload } : { action: 'add', schedule: payload };
      try {
        const r = await apiFetch('/api/schedules', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
        const res = await r.json();
        if (!res.ok) { toast('Save failed: ' + (res.error || 'unknown'), 'error'); return; }
        closeSchedModal(); loadSchedules();
      } catch (err) { toast('Error: ' + err.message, 'error'); }
    }
    async function schedPost(body) {
      try {
        const r = await apiFetch('/api/schedules', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
        const res = await r.json();
        if (!res.ok) toast(res.error || 'Failed', 'error');
        loadSchedules();
      } catch (err) { toast('Error: ' + err.message, 'error'); }
    }

    function setupScheduleHandlers() {
      const panel = $('#schedule-panel');
      if (panel) panel.addEventListener('click', (e) => {
        const b = e.target.closest('[data-sched]'); if (!b) return;
        const act = b.dataset.sched, id = b.dataset.id;
        if (act === 'add') openSchedModal();
        else if (act === 'filter') { scheduleFilter = !scheduleFilter; loadSchedules(); }
        else if (act === 'edit') openSchedModal(id);
        else if (act === 'delete') { const s = schedules.find(x => x.id === id); if (s && confirm('Delete schedule "' + s.name + '"?')) schedPost({ action: 'delete', id }); }
        else if (act === 'enable') { const s = schedules.find(x => x.id === id); if (s) schedPost({ action: 'enable', id, enabled: !s.enabled }); }
      });
      const modal = $('#sched-modal');
      if (modal) {
        modal.addEventListener('click', (e) => {
          if (e.target === modal) { closeSchedModal(); return; }
          const day = e.target.closest('.sm-day');
          if (day) { day.classList.toggle('on'); return; } // toggled in place; read at capture
          const seg = e.target.closest('[data-seg]');
          if (seg) {
            captureSchedForm();
            const g = seg.dataset.seg, v = seg.dataset.val;
            if (g === 'kind') scheduleDraft.action.kind = v;
            else if (g === 'whenmode') scheduleDraft.when.mode = v;
            else scheduleDraft.action[g] = v; // mode | session | op | agent
            renderSchedModal();
            return;
          }
          const b = e.target.closest('[data-sched]'); if (!b) return;
          const act = b.dataset.sched;
          if (act === 'close') closeSchedModal();
          else if (act === 'save') saveSchedule();
          else if (act === 'add-cond') {
            captureSchedForm();
            const t = b.dataset.type;
            const def = t === 'usage' ? { type: 'usage', metric: '5h', op: 'below', value: 50 }
              : t === 'usage-week' ? { type: 'usage', metric: 'week', op: 'below', value: 50 }
              : t === 'terminals' ? { type: 'terminals', op: 'below', value: 1 }
              : { type: 'kanban', taskId: '', todoId: '', status: (schedBoard && schedBoard.statuses && schedBoard.statuses[0]) || 'todo' };
            scheduleDraft.conditions.push(def);
            renderSchedModal();
          } else if (act === 'remove-cond') {
            captureSchedForm();
            scheduleDraft.conditions.splice(+b.dataset.ci, 1);
            renderSchedModal();
          }
        });
        // Kanban task/subtask selects restructure the dependent fields; changing
        // the project reloads its kanban board (for kanban conditions) + services.
        modal.addEventListener('change', async (e) => {
          if (e.target.id === 'sm-project') {
            captureSchedForm();
            schedBoard = null;
            try { const r = await apiFetch('/api/kanban?project=' + encodeURIComponent(scheduleDraft.project)); const d = await r.json(); if (d.ok) schedBoard = d.board; } catch { /* ignore */ }
            renderSchedModal();
          } else if (e.target.classList.contains('restruct')) { captureSchedForm(); renderSchedModal(); }
        });
      }
    }

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
    initUpdateUi();

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
