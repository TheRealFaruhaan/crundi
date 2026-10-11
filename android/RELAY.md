# Push relay contract (v1)

The relay sits between a Crundi server and Google's and Apple's push services.
It is run by whoever publishes the app, because only the publisher's keys are
accepted for that app. The server side is `src/mobile-push.js`. The two are
released separately, so this document is the contract between them.

## What the relay must never need

- **The content of a notification.** It receives ciphertext sealed for one
  phone (RFC 8291, `aes128gcm`, the scheme browsers use for Web Push) and
  forwards it untouched.
- **Accounts or a user database.** It stores issued keys (as hashes, with a
  label and a status) and per-key counters. Nothing about devices or messages.

## Authentication

Every request carries `Authorization: Bearer <key>`.

- A key is issued by hand, one per server, with a label saying who it is for.
- The relay stores only a hash of it.
- No key, an unknown key or a revoked key gets no service at all. A
  revocation takes effect within about a minute, not instantly.
- Keys match `[A-Za-z0-9._~-]{20,200}`. Suggested form: `crp_` followed by 40
  random base62 characters.

## `POST /v1/push`

```json
{
  "messages": [
    {
      "platform": "android",
      "token": "<the device's FCM or APNs token>",
      "payload": "<base64url ciphertext>",
      "ttl": 14400,
      "priority": "high",
      "collapse": "finished"
    }
  ]
}
```

- At most 20 messages per request.
- `platform` is `android` or `ios`.
- `payload` is at most 3000 base64url characters.
- `collapse` is a short tag: a later message with the same tag replaces an
  undelivered earlier one on the same device.

Answer, HTTP 200:

```json
{ "ok": true, "results": [ { "status": "sent" } ] }
```

`results` has one entry per message, in order:

- `sent`: Google or Apple accepted it.
- `gone`: the token is no longer valid (app uninstalled, token replaced). The
  server deletes that registration and stops sending to it.
- `failed`, with an `error` string: anything else. The server keeps the
  registration and does not retry this message.

How the relay hands it on:

- **Android:** an FCM data message, `{ "data": { "c": "<payload>" } }`, with
  the given priority, TTL and collapse key. The app decrypts and shows it.
- **iOS:** an APNs alert with `mutable-content: 1`, a neutral placeholder
  title and body, and `"c": "<payload>"` beside `aps`. A notification service
  extension decrypts it and replaces the placeholder.

## `GET /v1/key`

Checks a key without sending anything. Used when a key is entered in Settings.

```json
{ "ok": true, "label": "Faruhaan dev server" }
```

## `GET /v1/info`

No key. This is what a server reads before it has one, to tell its owner who
to ask.

```json
{ "ok": true, "name": "Acme Crundi", "contact": { "telegram": "acme_ops", "url": "https://acme.example/push", "text": "Email ops@acme.example for a key." } }
```

- Every field of `contact` is optional. Give at least one.
- The server shows only a Telegram name, an `https` link and a short line of
  plain text, and discards anything else.
- The published relay is not asked: its contact is built into the server.

## Errors

All are JSON, `{ "ok": false, "error": "<code>" }`.

| Status | `error` | Meaning |
|---|---|---|
| 400 | `bad_request` | Malformed body, too many messages, payload too long |
| 401 | `invalid_key` | Missing or unknown key |
| 403 | `key_revoked` | The key was revoked |
| 426 | `server_too_old` | The relay no longer speaks this version |
| 429 | `rate_limited` | Too many for now; `Retry-After` in seconds |

## What the phone decrypts

```json
{ "v": 1, "title": "Crundi", "body": "…", "kind": "finished", "url": "/", "at": 1760000000000 }
```

- `v` is the version of this shape. Fields are added, never renamed or removed.
- `chat` is present when the notification is about one chat:
  `{ "id", "title", "project", "mark": { "initials", "color", "colorDeep" } }`.
  The app files the notification under that chat's bubble and opens
  `/chat/<id>` from it. `url` is that page's path.
- With a chat, the collapse tag is `<kind>.<chat id>`, so one chat's
  notification does not replace another's.
- `kind` is the event: `finished`, `needsInput`, `serviceDown`, `test` and so
  on. It doubles as the collapse tag.
- `body` is cut to 1800 bytes so the whole message stays inside the 4096 bytes
  Google and Apple allow.

## How a phone registers

After pairing, the app's background login calls the server (not the relay):

`POST /api/push/mobile/register`

```json
{ "platform": "android", "token": "<FCM token>", "keys": { "p256dh": "<87 chars>", "auth": "<22 chars>" } }
```

- `p256dh` is the phone's P-256 public key, uncompressed, base64url without
  padding. `auth` is 16 random bytes, the same encoding. The phone makes both
  and keeps the private key in its keystore.
- Which device this is comes from the login making the call, so one device
  cannot register as another.
- Call it again whenever the token changes. `POST /api/push/mobile/unregister`
  withdraws it. Revoking the device in the sessions list also removes it.
- Collaborators' devices cannot register in v1: what is sent is the owner's
  notifications.

## Running your own relay

You can, but **not for the published app.** A relay works only with the app
whose push keys it holds:

- Google issues an Android device token for one Firebase project, and only
  that project can send to it.
- Apple issues an iOS device token for one app and one developer account, and
  only that account's key can send to it.

The published app's tokens are therefore usable by the published relay and by
nothing else. A second relay for the same app would be refused by Google and
Apple, however it was configured. So your own relay means your own build of
the app.

What that involves:

1. **Build the app under your own identity.** Your own Android application ID
   and iOS bundle ID, signed with your own keys, distributed by you.
2. **Create the push credentials for it.** A Firebase project with that
   Android app in it, and an Apple push key for that bundle ID.
3. **Deploy a relay that meets this contract** and holds those credentials.
4. **Issue keys.** One per server, by whatever means you choose. Have
   `GET /v1/info` say how to ask you for one.
5. **Point each server at it.** In the server's push settings, set the relay
   address to yours (`POST /api/push/mobile/relay` with `{ "relayUrl" }`),
   then enter the key you issued.

What the server does for a custom relay:

- It reads `GET /v1/info` and shows your contact in Settings, in place of the
  publisher's.
- **Changing the relay address removes the stored key.** A key belongs to the
  relay that issued it; sending it to a different relay would hand that
  relay's operator a working key to someone else's.
- The address must be `https`. Plain `http` is accepted only for `localhost`
  and `127.0.0.1`, for testing.

People using your build ask you for a key, not the publisher of the original
app. People using the published app cannot use your relay at all.

## Open

- The default relay address is `https://cr-rl-pub1.faruhaan.com`
  (`DEFAULT_RELAY_URL` in `src/mobile-push.js`). Nothing is deployed there yet.
- Rate limits per key are the relay's to choose. Suggested start: 60 a minute,
  5000 a day.
