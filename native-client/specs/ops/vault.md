# Ops: vault (the Secrets tab)

Reference: src/webapp-html.js "Secrets" (loadSecrets, updateSecretBadge, renderSecrets, setupSecretsHandlers, openPinModal), app/vendor/crundi-secret-lock.js (edit, upgrade).
Native: ui/ops/vault.slint, ui/ops/vault-parts.slint, src/ops/vault.rs. Test: test/ops/vault.mjs.

## Parts (top to bottom)
- Pending access requests (yellow box, only when there are any): name, "project: X · reason" (or "Claude requested access"), Approve (primary), Deny.
- Add a secret: Name, Description, Secret value, Passphrase, Confirm; the rule line (yellow when it warns: "At least 8 characters, and not only digits." / "The two do not match."); the "never stored" hint; Save secret (off until a name, a value and a good passphrase typed twice).
- SECRETS (N): rows with name, Old PIN / Old lock tag, description, the value (green mono) only while revealed; Reveal or Copy + Hide; Edit; Delete (red on hover).
- States: loading, "Failed to load: …" with retry, empty ("No secrets yet."), data.

## Dialogs
- PIN modal (Reveal secret / Approve access): title, words, the lock field (PIN: 6 digits, mono, centred; passphrase: plain), the error line, Cancel / action. Enter submits; a refusal keeps the modal, selects the text.
- Edit secret (capability secret-edit, else the "not supported" toast): name and description, new value, "Change the passphrase" / "Replace the 6-digit PIN with a passphrase" with the two fields, the current lock, Cancel / Save. Name or description only: `updateMeta`, no lock.
- Update this secret's lock (capability secret-upgrade): after Reveal or Approve answers needsUpgrade; Not now / Update; one dialog at a time (queued).
- Delete: the shared confirm ('Delete secret "X"? This cannot be undone.').

## Rules
- A value is on screen only after its lock was typed in the PIN modal; leaving the tab or signing out forgets every value.
- The badge (Shell.badge-secrets): the requests waiting, live from `secret-requests`; loaded at sign-in and on reconnect.

## Endpoints
GET /api/secrets; POST /api/secrets {add | reveal | approve | deny | delete | edit | updateMeta}; GET /api/handshake (capabilities).

## Checklist
- [x] every part and state above; [x] wording as the web's; [x] Esc closes each dialog; [x] snaps: vault-empty, -add-filled, -list, -pin, -pin-wrong, -revealed, -edit, -edit-wrong, -upgrade, -delete, -request.
- [ ] the Secrets tab's badge drawn by main.slint (request).
- [ ] the requests dock (app.rs) offering the lock update after Approve (request to workspace).
