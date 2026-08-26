# Crundi server on Linux

The server half of Crundi in a container: Claude sessions, terminals, projects,
the web UI. No display, no Electron.

```bash
cp docker/.env.example docker/.env      # fill in PROJECTS_DIR at minimum
docker compose -f docker/docker-compose.yml up -d
docker compose -f docker/docker-compose.yml exec crundi claude   # log in to Claude, once
```

Then open `http://<host>:8888`. On first load it will ask you to set up a way to
sign in — until you do, that is the **only** thing the server will let anyone do.

---

## Persist these volumes

Everything worth keeping lives in three places. Lose them and you lose your
Claude login, your project list, and your history — the container itself holds
nothing you want.

| Volume | Holds | If you lose it |
|---|---|---|
| `claude-auth` → `/root/.claude` | Claude Code's credentials and its own session transcripts | You log in to Claude again, and past conversations are gone |
| `crundi-data` → `/data` | Projects list, kanban, mindmap, media index, schedules, chat transcripts, **sign-in credentials**, push subscriptions, TLS certificates | You set the server up from scratch |
| your code → `/projects` | The repositories Claude works on | Your actual work |

The compose file declares the first two as **named volumes**, which survive
`docker compose down` and image rebuilds. They do *not* survive
`docker compose down -v` — that flag deletes them, and it is the usual way
people lose a Claude login without meaning to.

`/projects` is a bind mount to a path you choose, so it is only as safe as that
path. Point it at real storage, not a temporary directory.

### Backing up

```bash
docker run --rm -v crundi_crundi-data:/data -v "$PWD:/backup" \
  busybox tar czf /backup/crundi-data.tgz -C /data .
docker run --rm -v crundi_claude-auth:/auth -v "$PWD:/backup" \
  busybox tar czf /backup/claude-auth.tgz -C /auth .
```

Both archives contain **live credentials** — Claude's tokens, your password
hash, the TOTP secret. Treat them as secrets.

---

## Signing in

Two methods; either is enough and both can be on.

**Password + authenticator code** — set from the first-run screen or from
Settings. A password alone is not offered: this server runs commands on the
machine it sits on.

**Telegram** — needs `TELEGRAM_BOT_TOKEN` and `ALLOWED_USERNAME`. Optional.

You can also set a password before the container ever starts:

```bash
docker compose -f docker/docker-compose.yml run --rm crundi node scripts/make-login.mjs 'a long password'
```

and put the two printed variables in `docker/.env`.

**Neither method configured** puts the server in setup mode: unauthenticated,
but able to do nothing except configure one. It is still first-come — do not
expose the port before you have set that up.

---

## HTTPS

Not needed behind the Cloudflare tunnel or any reverse proxy that terminates
TLS — leave `TLS_MODE=off` and let them handle it.

For a box with its own public address, or to sit behind Cloudflare in
**Full (strict)**:

```env
TLS_MODE=letsencrypt
TLS_DOMAIN=crundi.example.com
TLS_EMAIL=you@example.com
TLS_STAGING=1          # remove once you have confirmed DNS and ports work
```

and publish both ports:

```yaml
ports:
  - "443:443"
  - "80:80"      # the CA fetches the challenge here and will not follow a redirect
```

The certificate renews automatically at 30 days remaining and is swapped into
the running server without a restart. It is stored in `/data/tls`, so it belongs
to the `crundi-data` volume like everything else.

> Leave `TLS_STAGING=1` until it works. Let's Encrypt rate-limits real issuance
> sharply, and a misconfigured domain can burn a week's allowance in an hour.

Already have a certificate:

```env
TLS_MODE=provided
TLS_CERT_PATH=/certs/fullchain.pem
TLS_KEY_PATH=/certs/privkey.pem
```

mounting `/certs` read-only. Renewal is then yours to manage.

A self-signed certificate is deliberately not an option. It would satisfy "the
port speaks TLS" while failing Cloudflare Full (strict) — the case this exists
for — so it would mostly produce confusing failures.

---

## The browser panel

The container has no display, so the browser panel and screen capture are not
available in it. Install the **Crundi Client** on a Windows machine and point it
here (Tray → Server…): it lends this server its GUI, and the browser panel opens
in front of you while everything else keeps running in the container.

Without a client attached, those tools report plainly that they need one. The
rest of Crundi is unaffected.
