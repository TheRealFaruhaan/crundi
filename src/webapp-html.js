/**
 * webapp-html.js — Crundi web terminal UI.
 *
 * Returns a self-contained HTML string with embedded CSS and JS.
 * xterm.js and addon-fit are loaded from /vendor/ routes served by webapp.js.
 * Telegram Login Widget is used for authentication.
 */

export function getWebappHtml(botUsername) {
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no">
  <title>Crundi</title>
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
    }

    body {
      font-family: -apple-system, "Segoe UI", system-ui, sans-serif;
      background: var(--bg-primary);
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
    #login-screen h1 {
      font-size: 2.5rem;
      font-weight: 700;
      letter-spacing: -0.02em;
    }
    #login-screen .subtitle {
      color: var(--text-secondary);
      font-size: 0.95rem;
      margin-top: -20px;
    }
    #login-screen .login-box {
      background: var(--bg-secondary);
      border: 1px solid var(--border);
      border-radius: var(--radius);
      padding: 40px 48px;
      text-align: center;
      display: flex;
      flex-direction: column;
      align-items: center;
      gap: 24px;
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
    }
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
      font-weight: 700;
      font-size: 1.1rem;
      letter-spacing: -0.01em;
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
      display: flex;
      align-items: center;
      gap: 10px;
      padding: 8px 12px;
      border-radius: var(--radius-sm);
      cursor: pointer;
      font-size: 0.88rem;
      color: var(--text-secondary);
      transition: background 0.1s;
      user-select: none;
    }
    .sidebar-item:hover { background: var(--bg-hover); color: var(--text-primary); }
    .sidebar-item.active { background: var(--accent-dim); color: var(--accent-hover); }
    .sidebar-item .dot {
      width: 8px; height: 8px;
      border-radius: 50%;
      background: var(--text-muted);
      flex-shrink: 0;
    }
    .sidebar-item.active .dot { background: var(--green); }
    .sidebar-item.has-terminal .dot { background: var(--green); }
    .sidebar-item .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
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
      padding: 10px 14px;
      white-space: nowrap;
      border: none;
      background: none;
      color: var(--text-muted);
      font-size: 0.82rem;
      font-weight: 500;
      cursor: pointer;
      border-bottom: 2px solid transparent;
      transition: color 0.15s;
    }
    .tab-btn:hover { color: var(--text-primary); }
    .tab-btn.active { color: var(--accent-hover); border-bottom-color: var(--accent); }

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
      .tab-btn { padding: 8px 12px; font-size: 0.76rem; }
      .svc-card { padding: 12px; }
      .info-panel { padding: 12px; }
      .info-row { flex-direction: column; gap: 2px; }
      .info-row .value { text-align: left; max-width: 100%; }
      /* Always show close button on mobile (no hover) */
      .sidebar-item .close-btn { display: block; }
      /* Terminal touch scrolling */
      .terminal-container .xterm .xterm-viewport { overflow-y: scroll !important; -webkit-overflow-scrolling: touch; touch-action: pan-y; }
      .terminal-container .xterm .xterm-screen { touch-action: pan-y; pointer-events: auto; }
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

    /* ─── Scrollbar ─── */
    ::-webkit-scrollbar { width: 6px; }
    ::-webkit-scrollbar-track { background: transparent; }
    ::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }
    ::-webkit-scrollbar-thumb:hover { background: var(--text-muted); }
  </style>
</head>
<body>
  <!-- ─── Login Screen ─── -->
  <div id="login-screen">
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
    <div class="topbar">
      <button class="hamburger" data-action="toggle-sidebar">&#9776;</button>
      <span class="logo">Crundi</span>
      <span class="separator">/</span>
      <span class="project-name" id="current-project">No project</span>
      <span class="spacer"></span>
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
          <button class="tab-btn" data-tab="services">Services</button>
          <button class="tab-btn" data-tab="terminals">Terminals</button>
          <button class="tab-btn" data-tab="browsers">Browsers</button>
          <button class="tab-btn" data-tab="info">Info</button>
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
            <button class="term-send-btn" data-action="term-send" title="Send to terminal">Send</button>
          </div>
        </div>
        <div class="git-panel tab-panel" id="git-panel" data-panel="git"></div>
        <div class="files-panel tab-panel" id="files-panel" data-panel="files"></div>
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
    let resizeTimer = null;
    let projectMode = 'multi'; // 'single' or 'multi'
    let projectsDir = '';

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
      script.setAttribute('data-onauth', 'onTelegramAuth(user)');
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
    function showApp() {
      $('#login-screen').style.display = 'none';
      $('#app').classList.add('visible');
      connectWS();
      loadProjectConfig();
      loadProjects();
      initTerminal();
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
        item.innerHTML = '<span class="dot"></span>'
          + '<span class="name">' + escHtml(p.name || p.alias) + '</span>'
          + (hasTerminal ? '<button class="close-btn" data-action="close-terminal" data-project="' + escHtml(p.alias) + '">&times;</button>' : '');
        item.addEventListener('click', (e) => {
          if (e.target.dataset.action === 'close-terminal') return;
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

      // Handle Ctrl+V paste directly into xterm — text and images
      term.attachCustomKeyEventHandler((e) => {
        if (e.type === 'keydown' && e.key === 'v' && (e.ctrlKey || e.metaKey)) {
          (async () => {
            try {
              const items = await navigator.clipboard.read();
              for (const item of items) {
                if (item.types.some(t => t.startsWith('image/'))) {
                  const imageType = item.types.find(t => t.startsWith('image/'));
                  const blob = await item.getType(imageType);
                  const buf = await blob.arrayBuffer();
                  const b64 = btoa(String.fromCharCode(...new Uint8Array(buf)));
                  const r = await apiFetch('/api/clipboard/paste-image', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ project: currentProject, data: b64 }),
                  });
                  const data = await r.json();
                  if (data.ok && data.path) {
                    if (ws && ws.readyState === 1 && currentProject) {
                      ws.send(JSON.stringify({ type: 'input', project: currentProject, data: data.path }));
                    }
                    toast('Screenshot saved: ' + data.name);
                  }
                  return;
                }
              }
              // No image — paste text
              const text = await navigator.clipboard.readText();
              if (text && ws && ws.readyState === 1 && currentProject) {
                ws.send(JSON.stringify({ type: 'input', project: currentProject, data: text }));
              }
            } catch { /* clipboard access denied or empty */ }
          })();
          return false;
        }
        return true;
      });

      // Clipboard paste — only intercept images, never block text paste
      const termInputEl = document.getElementById('term-input');
      termInputEl.addEventListener('paste', async (e) => {
        if (!currentProject) return;
        const items = e.clipboardData?.items;
        if (!items) return;
        for (const item of items) {
          if (item.type.startsWith('image/')) {
            e.preventDefault();
            const blob = item.getAsFile();
            if (!blob) return;
            const buf = await blob.arrayBuffer();
            const b64 = btoa(String.fromCharCode(...new Uint8Array(buf)));
            try {
              const r = await apiFetch('/api/clipboard/paste-image', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ project: currentProject, data: b64 }),
              });
              const data = await r.json();
              if (data.ok && data.path) {
                termInputEl.value += data.path;
                termInputEl.focus();
                toast('Screenshot saved: ' + data.name);
              }
            } catch (err) { toast('Paste failed: ' + err.message, 'error'); }
            return;
          }
        }
        // In Electron: check for copied file paths when no text is being pasted
        if (!e.clipboardData?.getData('text/plain') && window.api) {
          e.preventDefault();
          try {
            const filePaths = await window.api.getClipboardFilePaths();
            if (filePaths?.length) {
              termInputEl.value += filePaths.join(' ');
              termInputEl.focus();
              return;
            }
            const image = await window.api.getClipboardImage();
            if (image) {
              termInputEl.value += image.tmpPath;
              termInputEl.focus();
              toast('Screenshot from clipboard: ' + image.name);
            }
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
          // Check for image files — upload and insert path
          const paths = [];
          for (const file of files) {
            if (file.type.startsWith('image/')) {
              const buf = await file.arrayBuffer();
              const b64 = btoa(String.fromCharCode(...new Uint8Array(buf)));
              try {
                const r = await apiFetch('/api/clipboard/paste-image', {
                  method: 'POST',
                  headers: { 'Content-Type': 'application/json' },
                  body: JSON.stringify({ project: currentProject, data: b64 }),
                });
                const data = await r.json();
                if (data.ok) paths.push(data.path);
              } catch { /* ignore */ }
            } else {
              const resolved = window.api?.getPathForFile?.(file);
              paths.push(resolved || file.path || file.name);
            }
          }
          if (paths.length) {
            const ta = document.getElementById('term-input');
            if (ta) { ta.value += paths.join(' '); ta.focus(); }
            toast(paths.length === 1 ? 'File added' : paths.length + ' files added');
          }
          return;
        }
        // Text drop
        const text = e.dataTransfer?.getData('text/plain');
        if (text) {
          const ta = document.getElementById('term-input');
          if (ta) { ta.value += text; ta.focus(); }
        }
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
      if (ws) { try { ws.close(); } catch {} }
      clearTimeout(reconnectTimer);

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
      const es = new EventSource('/api/events?token=' + encodeURIComponent(token));
      es.addEventListener('state', (e) => {
        try {
          const state = JSON.parse(e.data);
          terminals = state.terminals || [];
          userTerminals = state.userTerminals || [];
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
      es.onerror = () => {
        es.close();
        setTimeout(connectSSE, 5000);
      };
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
      currentTab = tab;
      $$('.tab-btn').forEach(b => b.classList.toggle('active', b.dataset.tab === tab));
      $$('.tab-panel').forEach(p => p.classList.toggle('visible', p.dataset.panel === tab));
      if (tab === 'terminal' && term) { fitTerminal(); term.focus(); }
      if (tab === 'services') loadServices();
      if (tab === 'terminals') renderTerminals();
      if (tab === 'browsers') loadBrowsers();
      if (tab === 'git') loadGitInfo();
      if (tab === 'files') loadFiles();
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
        const tunnelHtml = s.tunnelUrl
          ? '<br><a class="tunnel-link" href="' + escHtml(s.tunnelUrl) + '" target="_blank">' + escHtml(s.tunnelUrl) + '</a>'
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
      if (!name.trim()) { toast('Service name is required', 'error'); return; }
      if (!command.trim()) { toast('Command is required', 'error'); return; }
      try {
        const res = await apiFetch('/api/services', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ alias: currentProject, name: name.trim(), command: command.trim(), cwd: cwd.trim(), stopCommand: stopCommand.trim() }),
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
        case 'svc-start': svcAction('start', d.key); break;
        case 'svc-stop': svcAction('stop', d.key); break;
        case 'svc-restart': svcAction('restart', d.key); break;
        case 'svc-logs': svcLogs(d.key); break;
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

    // ─── Init ───
    async function init() {
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
  })();
  <\/script>
</body>
</html>`;
}
