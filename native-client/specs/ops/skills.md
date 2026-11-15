# Settings → Skills, Backup & restore, Usage; restore at sign-in (ops `skills`)

Web: `src/webapp-html.js` renderSkillsSection/drawSkills (~14660-15134),
renderBackupSection/drawBackupForm/onBackupStatus/drawBackupNotice
(~15135-15386), renderSettings' "Usage" section (~15796) and its
`toggle-limit-warmup` handler (~16267), showLoginRestore/lrPoll/lrWaitBack
(~14549-14660). Native: `ui/ops/skills.slint` (+ `skills-parts.slint`),
`src/ops/skills.rs` (+ `skills/list.rs`, `skills/backup.rs`, `skills/common.rs`).
Settings (`ui/ops/settings.slint`) places `OpsSkillsSection`, `OpsUsageSection`
(group "Claude") and `OpsBackupSection` (group "This server") in its folds.

## Skills
- Parts, in order: the hint; "Add to [scope]" Upload (primary) New; the upload
  hint (limit in MB); the link form (field, "Install from link"/"Fetching…",
  "not supported" hint on an older server); the error line; the pending
  question (which skill / a name / replace); the New form; the groups:
  "All projects <path>" (always, "None yet." when empty), each project scope
  with skills, "Synced from your Claude account (N)" with Show/Hide.
- A skill row: name, tags (Included with Crundi, Synced, Link, GitHub, Needs
  attention), files and size, description (two lines until open). Click opens
  the detail below it (one at a time; again closes).
- Detail: read-only note, problem note, source note (repo link, ref, path,
  commit, installed date); Update · Rename · Copy or move…/Copy to… ·
  Download · Delete; the rename and transfer forms; the file list (click
  opens, trash deletes, not SKILL.md or read-only); "only the first N" and
  skipped-links hints; New file · Upload file; the new-file form; the editor
  (path, size, text or the binary note; Save · Download · Close; Ctrl+Enter
  saves).
- Confirms as the web: discard unsaved changes, delete skill, delete file,
  replace a file on upload, update from source.
- Endpoints: GET /api/skills, /api/skills/detail, /api/skills/file,
  /api/skills/download, /api/skills/file-download, /api/handshake
  (capability `skills-install-link`); POST /api/skills/create, rename,
  delete, transfer (409 conflict), file, file-delete, upload and file-upload
  (raw body, 409 conflict / needsName), install-from-link (choose / conflict;
  404 → not supported), update-from-source.

## Backup & restore
- Status box: running (phase, bar: sliding when unknown) or waiting (Back up
  now, Skip this one); staged restore (Restart now or the hint, Cancel the
  restore); failed restore; Last backup; Next backup; Restored (7 days);
  Back up now (off until set up).
- Storage (Endpoint, Bucket, Region, Folder, Addresses Path/Virtual-hosted,
  Access key ID, Secret access key; Test the connection + its answer),
  Passphrase (two fields, hint), Schedule (Back up automatically; Daily /
  Weekly / Every few hours with weekday or "every N hours from"; time; time
  zone; Keep the last N), Save + its answer, Backups in storage (Show /
  Refresh; rows: date, tag, size, host, Restore, delete; the restore
  confirmation with the passphrase and "Restore and restart").
- Live: the `backup` event (toasts: uploaded, failed, restarting, restore
  failed); the notice bottom left (Back up now, Skip this one, ×) while a
  scheduled backup waits.
- Endpoints: GET /api/backup, /api/backup/list; POST /api/backup/settings,
  test, run, skip, restore, cancel-restore, restart, delete.

## Usage
- "Keep the 5-hour window warm", its hint, "Last: …", the window line; the
  switch. Off asks first when messages wait on the reset. GET/POST /api/settings.

## Restore at sign-in
- `OpsLoginRestore`: Endpoint, Bucket + Region, Folder, keys, Virtual-hosted;
  Find backups/Looking…; the list (radio rows) or "No backups in this folder";
  passphrase; Restore; progress (phase, bar); the error; "Set up a new
  password instead". POST /api/backup/setup/list, /setup/restore; GET
  /setup/status, /api/auth/methods (until it is back, then the sign-in page
  checks the server again). **Placement over the sign-in page is a request
  to the Lead** (shared/requests.md); the native sign-in has no first-run
  setup of its own.

## Checklist
- [x] every part and control above, the web's wording
- [x] loading / error / data for each section; empty groups and lists
- [x] pending questions (name, replace, choose) and the confirms
- [x] live backup status and the notice
- [ ] sign-in restore placed by the window (request)
- Gaps: no drag-and-drop upload (the web has none either); text in the
  notes is plain (no bold run inside a line); the editor's read-only state
  is a disabled field (no selection).

Screens: `screens/ops/skills-*.png`.
