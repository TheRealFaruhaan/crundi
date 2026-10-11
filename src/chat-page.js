// The compact chat: one conversation and nothing else, at /chat/<id>.
//
// This is what sits inside a chat bubble on a phone. It is the same chat as in the workbench, drawn by the same
// component (app/vendor/claude-chat.js), with a small header in place of the
// whole application round it. It is its own page and not a mode of the main
// one because a bubble has to open at once, and the main page is seventeen
// thousand lines that a single conversation needs almost none of.
//
// There is no script in this file. The page's behaviour is
// app/vendor/crundi-chat-page.js, an ordinary script, so it is checked like
// any other and cannot be broken by a stray escape in a template string.

const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

/**
 * @param {{ version: string, rootCss: string, tag: (file: string) => string }} o
 *   rootCss is the main page's :root block, so both pages share one palette
 *   and a change to it reaches both.
 */
export function getChatPageHtml({ version, rootCss, tag }) {
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no, viewport-fit=cover, interactive-widget=resizes-content">
<meta name="theme-color" content="#0a0a0f">
<title>Crundi</title>
<link rel="icon" href="/assets/icon_32x32.png">
<style>
${rootCss}
*{box-sizing:border-box}
html,body{margin:0;height:100%;background:var(--bg-primary);color:var(--text-primary);font-family:-apple-system,"Segoe UI",system-ui,sans-serif;-webkit-text-size-adjust:100%;overscroll-behavior:none}
button{font:inherit;color:inherit}
#cp{display:flex;flex-direction:column;height:100dvh;min-height:0}
#cp-head{display:flex;align-items:center;gap:10px;padding:calc(8px + env(safe-area-inset-top)) 10px 8px;background:var(--bg-secondary);border-bottom:1px solid var(--border-subtle);flex:none}
.cp-btn{width:38px;height:38px;flex:none;display:inline-flex;align-items:center;justify-content:center;border:0;border-radius:50%;background:transparent;cursor:pointer;color:var(--text-secondary)}
.cp-btn:active{background:var(--bg-hover)}
.cp-btn svg{width:20px;height:20px;fill:none;stroke:currentColor;stroke-width:2;stroke-linecap:round;stroke-linejoin:round}
#cp-mark{width:38px;height:38px;flex:none;border-radius:12px;display:flex;align-items:center;justify-content:center;font-weight:700;font-size:15px;letter-spacing:.3px;color:#fff;background:var(--bg-tertiary)}
#cp-mark img{width:100%;height:100%;object-fit:cover;border-radius:12px;display:block}
#cp-names{flex:1;min-width:0;line-height:1.25}
#cp-title{font-size:15px;font-weight:650;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
#cp-sub{font-size:12px;color:var(--text-secondary);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;display:flex;align-items:center;gap:6px}
#cp-dot{width:7px;height:7px;border-radius:50%;background:var(--text-muted);flex:none}
#cp-dot.working{background:var(--amber);animation:cp-pulse 1.2s ease-in-out infinite}
#cp-dot.needs-input{background:var(--accent-hover)}
#cp-dot.idle{background:var(--green)}
@keyframes cp-pulse{50%{opacity:.35}}
/* A column, so a panel docked to this chat can sit above the conversation
   (crundi-widgets.js puts it there), exactly as it does in the workbench. */
#cp-body{flex:1;min-height:0;position:relative;display:flex;flex-direction:column}
#cp-chat{flex:1 1 0;min-height:0;min-width:0;position:relative}
/* Every icon in the app is an inline SVG sized by this one rule. The main
   page has it; without it here a panel's icon filled the whole bubble. */
.ic{width:1em;height:1em;display:inline-block;vertical-align:-0.125em;flex-shrink:0;pointer-events:none}
#cp-note{position:absolute;inset:0;display:none;flex-direction:column;align-items:center;justify-content:center;gap:14px;padding:28px;text-align:center;color:var(--text-secondary);font-size:14px;line-height:1.5;background:var(--bg-primary)}
#cp-note.on{display:flex}
#cp-note b{color:var(--text-primary);font-size:16px;font-weight:650}
#cp-note button{padding:9px 18px;border-radius:8px;border:1px solid var(--accent);background:var(--accent);color:#fff;cursor:pointer}
#cp-menu{position:absolute;right:8px;top:calc(52px + env(safe-area-inset-top));z-index:30;display:none;min-width:190px;padding:6px;background:var(--bg-tertiary);border:1px solid var(--border);border-radius:var(--radius);box-shadow:var(--shadow-md)}
#cp-menu.on{display:block}
#cp-menu button{display:block;width:100%;padding:10px 12px;border:0;border-radius:var(--radius-sm);background:transparent;text-align:left;font-size:14px;cursor:pointer}
#cp-menu button:active{background:var(--bg-hover)}
#cp-menu button[disabled]{color:var(--text-muted);cursor:default}
#cp-toast{position:fixed;left:50%;bottom:calc(86px + env(safe-area-inset-bottom));transform:translateX(-50%);z-index:40;max-width:86vw;padding:9px 14px;border-radius:8px;background:var(--bg-tertiary);border:1px solid var(--border);font-size:13px;opacity:0;pointer-events:none;transition:opacity .2s}
#cp-toast.on{opacity:1}
#cp-toast.error{border-color:var(--red)}
/* A secret Claude is asking for. It sits over the top of the chat, where it
   cannot be missed: an unanswered one times out and fails the work. */
#cp-ask{flex:none;display:none;flex-direction:column;gap:8px;padding:10px;background:var(--bg-secondary);border-bottom:1px solid var(--border);max-height:60vh;overflow:auto}
#cp-ask.on{display:flex}
.cp-ask-card{border:1px solid var(--yellow);background:var(--yellow-dim);border-radius:var(--radius);padding:12px}
.cp-ask-head{font-weight:650;font-size:14px}
.cp-ask-why{font-size:12.5px;color:var(--text-secondary);margin-top:4px;line-height:1.4}
.cp-ask-cmd{font-family:var(--mono);font-size:12px;background:var(--bg-primary);border:1px solid var(--border);border-radius:var(--radius-sm);padding:8px;margin-top:8px;white-space:pre-wrap;word-break:break-word;max-height:28vh;overflow:auto}
.cp-ask-row{display:flex;gap:8px;margin-top:10px;flex-wrap:wrap}
.cp-ask-row button{padding:9px 16px;border-radius:8px;border:1px solid var(--border);background:var(--bg-tertiary);cursor:pointer;font-size:14px}
.cp-ask-row button.go{background:var(--accent);border-color:var(--accent);color:#fff}
.cp-ask-row input{flex:1;min-width:110px;padding:9px 10px;border-radius:8px;border:1px solid var(--border);background:var(--bg-primary);color:var(--text-primary);font:inherit;font-size:16px;letter-spacing:3px}
.cp-ask-err{color:var(--red);font-size:12.5px;margin-top:6px}
#cp-conn{display:none;padding:4px 10px;font-size:12px;text-align:center;color:var(--text-secondary);background:var(--bg-tertiary);flex:none}
#cp-conn.on{display:block}
@media (prefers-reduced-motion:reduce){#cp-dot.working{animation:none}}
</style>
</head>
<body data-version="${esc(version)}">
<div id="cp">
  <div id="cp-head">
    <button class="cp-btn" id="cp-back" type="button" aria-label="Back"><svg viewBox="0 0 24 24"><path d="M19 12H5M11 6l-6 6 6 6"/></svg></button>
    <div id="cp-mark" aria-hidden="true"></div>
    <div id="cp-names"><div id="cp-title">Crundi</div><div id="cp-sub"><span id="cp-dot"></span><span id="cp-proj"></span></div></div>
    <button class="cp-btn" id="cp-more" type="button" aria-label="More" aria-haspopup="true"><svg viewBox="0 0 24 24"><circle cx="12" cy="5" r="1.4"/><circle cx="12" cy="12" r="1.4"/><circle cx="12" cy="19" r="1.4"/></svg></button>
  </div>
  <div id="cp-conn" role="status">Reconnecting…</div>
  <div id="cp-ask" role="alert"></div>
  <div id="cp-body">
    <div id="cp-chat" class="chat-mount"></div>
    <div id="cp-note" role="status"></div>
  </div>
</div>
<div id="cp-menu" role="menu">
  <button type="button" data-do="plan" role="menuitem" style="display:none">Show the plan</button>
  <button type="button" data-do="rename" role="menuitem">Rename chat</button>
  <button type="button" data-do="stop" role="menuitem">Stop Claude</button>
  <button type="button" data-do="open" role="menuitem">Open in Crundi</button>
</div>
<div id="cp-toast" role="status"></div>
<script src="/vendor/crundi-mini.js?v=${esc(tag('crundi-mini.js'))}"></script>
<script src="/vendor/claude-chat.js?v=${esc(tag('claude-chat.js'))}"></script>
<script src="/vendor/crundi-widgets.js?v=${esc(tag('crundi-widgets.js'))}"></script>
<script src="/vendor/crundi-chat-page.js?v=${esc(tag('crundi-chat-page.js'))}"></script>
</body>
</html>`;
}
