# Ops: extras (Add project, approvals, What's new)

Native: ui/ops/extras.slint, ui/ops/extras-appr.slint (+ one line in ui/shell.slint), src/ops/extras.rs. Test: test/ops/extras.mjs.

## Add project (#add-project-modal, showAddModal / addProject)
- From the sidebar's "+ Add project" (Shell.add-project). Single mode (GET /api/projects/config): PROJECT NAME + "Folder: <dir>/..."; multi: ALIAS, PATH, DISPLAY NAME (OPTIONAL). Cancel / Add; Enter adds, Esc closes.
- Errors as toasts: "Project name is required", "Invalid project name", "Alias and path are required", the server's error, "Error: …".
- Multi: POST /api/projects/check-path; a missing path asks "Path does not exist. Create empty folder?". Then POST /api/projects; "Project created" / "Project added"; the new project opens when the state lists it.
- Not here: offerGitInit (that is the collaborators' share flow in Settings, not Add project).

## Approvals (#approvals-btn, renderApprovals)
- Top bar button beside the connection badge, hidden at zero, yellow with the count. Click toggles the panel under the bar (right).
- Panel: "N waiting" + Clear all (confirm when any would be declined), items: source pill, "name — title", project, detail (scrolls past 140), buttons by source (collab host/command/tool/other, kid/cru choices, chat: Open the chat, secret: Open secrets), Dismiss.
- Live from the `approvals` event; GET /api/approvals at sign-in and reconnect. POST /api/approvals {id, approve, scope} / {action:'dismiss', items}; /api/voice/ask, /api/voice/kid/ask.

## What's new (crundi-whatsnew.js)
- 1.6 s after sign-in, not over a dialog (retries 20 x 1.5 s): GET /api/whatsnew; full window: brand + version + Skip; eyebrow, title, body, keys; the art frame; footer Back, dots, "n / N", "Arrow keys, or swipe", Next / Done; the recap ("You are up to date", every step clickable).
- Esc skips; Right/Enter/PageDown next (Done on the recap); Left/PageUp back. Closing marks it seen (POST /api/whatsnew/seen) unless opened from Settings (`open_whatsnew(true)`, "?all=1").
- Not shown automatically under the test driver (CRUNDI_TEST_PORT) unless CRUNDI_WHATSNEW is set, so it never covers other areas' tests.
- Gap: the step's animation is HTML/CSS; the stage shows the mark, tag and title instead (request to desktop).
