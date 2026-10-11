# Crundi push relay

A Cloudflare Worker that forwards sealed notifications from Crundi servers to
Google (and later Apple). What it must do is written in
[`../android/RELAY.md`](../android/RELAY.md); why it exists is at the top of
[`src/index.js`](src/index.js).

It runs at `https://cr-rl-pub1.obitech.mv`.

## Deploying

Automatic. `.github/workflows/relay.yml` tests and deploys it when a push to
`main` changes anything under `relay/`. Nothing else triggers a deploy. To put
a change out from another branch, run that workflow by hand on the branch.

By hand, from this folder, with a Cloudflare API token in the environment:

```bash
node test.mjs
npx wrangler@4 deploy
```

## Secrets

Held by Cloudflare, set once, kept across deploys. Never in this repo.

```bash
npx wrangler@4 secret put ADMIN_TOKEN            # lets its holder issue and revoke keys
npx wrangler@4 secret put FCM_SERVICE_ACCOUNT    # the Firebase service account JSON, whole
```

Without `FCM_SERVICE_ACCOUNT` the relay runs and checks keys, and answers
every Android message `failed` with `android_not_configured`.

## Keys

One per server, issued by hand. `$ADMIN` is the admin token.

```bash
R=https://cr-rl-pub1.obitech.mv

# Issue. The key is shown once, here, and cannot be shown again.
curl -sX POST $R/admin/keys -H "Authorization: Bearer $ADMIN" \
  -H 'Content-Type: application/json' -d '{"label":"Who it is for"}'

# List: labels and ids, never keys.
curl -s $R/admin/keys -H "Authorization: Bearer $ADMIN"

# Revoke, by id.
curl -sX POST $R/admin/keys/revoke -H "Authorization: Bearer $ADMIN" \
  -H 'Content-Type: application/json' -d '{"id":"<id from the list>"}'
```

The relay stores a hash of each key, with its label. Revoking affects nobody
else's key.

The store behind it (Cloudflare KV) is eventually consistent, which shows in
two places:

- **A revoked key can keep working for up to about a minute**, until
  Cloudflare's cached copy of its record expires. Prompt, not instant.
- **The list can lag a new key by up to a minute.** Issuing and revoking do
  not depend on the list, so both work straight away.

## Limits

Sixty requests a minute per key (`[[ratelimits]]` in `wrangler.toml`), each
carrying up to twenty messages. It is counted per Cloudflare location and is
approximate on purpose. There is no daily cap yet.
