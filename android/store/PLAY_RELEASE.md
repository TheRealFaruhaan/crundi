# Releasing Crundi for Android on Google Play

Step by step, from where the app is now (internal testing) to a public
listing. Written October 2026. Google changes these screens and rules often:
where this file and the Play Console disagree, the Console is right.

App: `mv.obitech.crundi`. Bundle to upload: the signed `.aab` for version
1.0.0 (version code 11). Listing text and pictures: `listing.md` and the
images in this folder.

## 0. Before anything in the Console

1. **Install the 1.0.0 build from the internal track** and pair it with a
   1.20.0 server once, before sending anything to review.
2. **Put the privacy policy online.** It is `docs/privacy.html` in the
   repository and is served at https://crundi.faruhaan.com/privacy.html once
   that file is on `main`. Open the address in a browser and check it loads
   before giving it to Google. Add a contact email to it if you want one
   there; Play shows a developer email on the listing either way.
3. **Check that Google's reviewers can get in.** This is the one thing most
   likely to get the app rejected, so it has its own section (step 5).

## 1. Know which kind of account you have

Play Console, top right, your account, then **Account details**. It says
either *Personal* or *Organization*.

- **Organization**, or a personal account created before 13 November 2023:
  you can apply for production directly. Skip step 6.
- **Personal, created on or after 13 November 2023**: Google requires a
  closed test with at least 12 testers opted in for 14 days running before
  it will let you apply for production. That is step 6, and it sets the
  earliest possible release date: about two and a half weeks from the day
  the twelfth tester joins.

## 2. Finish developer verification

Play Console, **Settings**, **Developer account**, **Account details** and
the **Android developer verification** page.

1. Identity: legal name, address, and an identity document (an organization
   also needs a D-U-N-S number and website). Phone and email must be
   verified. If any of this shows as incomplete, finish it first: Google
   will not review an app from an unverified account.
2. Package registration: since 30 September 2026 every app on Play must be
   registered to its verified developer. For an app that uses Play App
   Signing (this one does) the Console does this for you; check the
   verification page lists `mv.obitech.crundi` as registered, and follow
   its prompt if it does not.

## 3. Set up the store listing

**Grow users**, **Store presence**, **Main store listing**. Copy from
`listing.md`:

| Field | Value |
|---|---|
| App name | Crundi |
| Short description | from `listing.md` |
| Full description | from `listing.md` |
| App icon | `play-icon-512.png` |
| Feature graphic | `feature-graphic-1024x500.png` |
| Phone screenshots | at least 2, up to 8 |

Screenshots: `screenshot-2` and `-3` in this folder are of the server's
settings in a phone-sized browser, not of the app. Replace them with real
captures from your phone before a public release: a chat, a bubble open over
another app, the live card, a notification with its reply box, and the
pairing screen. Reviewers do reject listings whose pictures are not of the
app.

Then **Store settings**: category *Tools* or *Productivity*, contact email
(required, shown publicly), website https://crundi.faruhaan.com.

## 4. Fill in App content

**Policy and programs**, **App content**. Every item must show a green
tick before a release can be reviewed.

| Item | Answer |
|---|---|
| Privacy policy | https://crundi.faruhaan.com/privacy.html |
| Ads | No, the app contains no ads |
| App access | *All or some functionality is restricted*: see step 5 |
| Content rating | Fill in the questionnaire: category *Utility, productivity, communication or other*. No violence, sexual content, gambling or user-to-user sharing run by us. Result is normally *Everyone* |
| Target audience | 18 and over. Not designed for children |
| Data safety | see below |
| Government apps | No |
| Financial features | None |
| Health | None |
| News app | No |
| Advertising ID | The app does not use an advertising ID |

### Data safety answers

- Does the app collect or share any of the required user data types? **Yes**
  (because of the notification token).
- Is all data encrypted in transit? **Yes.**
- Can users request that data is deleted? **Yes** (uninstalling, or removing
  the phone on their server, ends it; the policy says how).
- Data type: **Device or other IDs** (the Firebase messaging token).
  - Collected: yes. Shared: no.
  - Processed ephemerally: no (the user's own server keeps it to send
    notifications).
  - Required, not optional.
  - Purpose: **App functionality**.
- Nothing else. Messages, files, photos and audio go to the user's own
  server, which is not us, so they are not "collected" in Google's sense.
  If you would rather be cautious, add *Photos* and *Files and docs* as
  collected for app functionality, not shared: it costs nothing and cannot
  be called an under-declaration.

Permissions: camera, microphone and notifications need no separate
declaration form. The app asks for none of the ones that do (SMS, call log,
all files, background location, accessibility).

## 5. App access: how a reviewer gets in

The app does nothing until it is paired with a Crundi server, and a pairing
code lasts five minutes and works once. A reviewer cannot be handed one in
advance. If they open the app and cannot get past the first screen, the app
is rejected for "app access" or "broken functionality".

What Google needs, in **App content**, **App access**, **Add instructions**:
a way in that works at any time, for several reviewers, without anyone on
your side doing anything.

That is the demonstration server at https://demo.dev.faruhaan.com. It has
one sample project and a sign-in of its own (a password and an authenticator
secret, in `/home/crundi/crundi-demo/credentials.txt` on the server). It
runs in a container with no way out: no internet, and no route to the
machine it is on. Claude is not signed in there, so chats cannot be started;
terminals, files, the board and the mind map all work.

In **App access** choose *All or some functionality is restricted* and add
one set of instructions. Put the password in the password field, and this
in the instructions box (fill in the two values):

> Crundi is a client for a self-hosted server, so there is no account to
> create in the app. Use our demonstration server:
>
> 1. On the test device, open https://demo.dev.faruhaan.com in Chrome.
> 2. Sign in. Password: <password>. The six-digit code comes from an
>    authenticator app (for example Google Authenticator): add an account
>    with "Enter a setup key", any name, key <authenticator secret>,
>    time based.
> 3. Open the menu, Settings, Phone app, Connect a phone, then tap
>    "Copy the pairing link instead".
> 4. Open the Crundi app, paste the link into "Or paste a pairing link"
>    and continue. The app is now connected and shows the server.
>
> AI chats are switched off on the demonstration server, so a new chat
> cannot be started there. Files, terminals, the board and the mind map
> are all available.
>
> A pairing link works once and for five minutes; repeat step 3 for a new
> one. With a second screen you can instead scan the picture from step 3
> with the app.

"Copy the pairing link" is new in server 1.20.0. The demonstration server
must be running a build that has it before you submit: it runs whichever
image `docker-compose.yml` beside the credentials names, and is moved to a
new one with `docker compose pull && docker compose up -d` in that folder.

Try the four steps yourself on a phone before submitting. If they do not
work for you they will not work for a reviewer.

## 6. Closed test (only if step 1 says you must)

1. **Test and release**, **Testing**, **Closed testing**, create a track
   (the default *Alpha* is fine).
2. **Testers**: make an email list and add at least 12 Google accounts. Add
   a few more than 12: if the count of opted-in testers drops below 12 the
   14 days start again.
3. Create a release on the track, upload the same `.aab`, add the release
   notes from `listing.md`, and send it for review. A closed track is
   reviewed too, and needs steps 3 to 5 done.
4. Send each tester the opt-in link from the track's **Testers** tab. Being
   on the email list is not enough: each must open the link, accept, and
   install the app from Play. They need a server to pair with, so point
   them at the demonstration server from step 5.
5. Wait 14 days with 12 or more opted in. Google also looks at whether the
   testers actually used the app, so ask them to open it a few times.
6. **Dashboard**, **Apply for production**. You are asked how the test
   went, what feedback you had and what you changed. Answer in a few plain
   sentences. Approval usually takes up to a week.

## 7. Production

1. **Test and release**, **Production**, **Countries / regions**: add the
   countries to publish in.
2. **Create new release**. Either *Promote* the tested release from the
   closed track, or upload the `.aab` again. Release name `1.0.0`, release
   notes from `listing.md`.
3. Use a **staged rollout** if offered (20% is a sensible start). It can be
   raised to 100% at any time and halted if something is wrong.
4. **Review release**, fix anything listed under *Errors*, then **Start
   rollout to production**.
5. Review of a first production release takes anywhere from a few hours to
   about a week. The status is on the **Publishing overview** page. If
   *Managed publishing* is on, an approved release waits there until you
   press Publish.

## 8. After it is live

- The store page is https://play.google.com/store/apps/details?id=mv.obitech.crundi
- Every update needs a higher **version code** than the last upload, on any
  track. 1.0.0 is 11; the next upload must be 12 or more.
- Keep the upload key (`~/android-keys/crundi-upload.jks`) and its password
  safe. Losing it is recoverable (Play Console, **App integrity**, request
  an upload key reset) but takes days.
- Crash reports and ANRs: **Monitor and improve**, **Android vitals**. The
  bundle carries its R8 mapping file, so stack traces are readable.
- A policy email from Google has a deadline in it. Answer inside it.

## Tested before release

The owner ran server 1.20.0-dev.14 with app 1.0.0-dev.10 on a Galaxy S24
Ultra (One UI 8.5, Android 16) and reported no noticeable failures. 1.0.0 is
that build with its version number changed. Not separately confirmed one by
one: whether the "Live notification on Samsung" switch removes the glow, and
whether copied pictures now list in the Samsung keyboard's clipboard panel.

## Not part of this release

- The GitHub build of the app (an APK installed outside Play, which updates
  itself from GitHub releases). CI builds it for testing only: it is not
  signed with a release key and is not attached to releases. Publishing it
  needs its own signing key, and, since September 2026, its own
  registration under Android developer verification.
- iOS, the Linux desktop client, the Play in-app update prompt.
