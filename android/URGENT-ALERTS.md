# Urgent alerts (alert_user)

The `alert_user` MCP tool sends a push of kind `alert` to the paired phones.
An urgent one rings on the app's Urgent channel until the person sees it. This
note records what the stores allow for that, what was chosen, and how the
phone decides what to do with an alert.

## Policy check, 25 October 2026

Checked against the current Google Play and Apple pages before building.

**Full-screen intent (USE_FULL_SCREEN_INTENT).** For apps targeting Android 14
and up, Play grants it by default only to apps whose core function is calling
or alarms, declared in Play Console; Play revokes it on install for the rest,
which must ask the person and degrade gracefully if refused. Crundi is neither
kind of app. **Used, opt-in only** (since Android 1.1.1-dev.3): see "Full
screen" below. The Play Console's full-screen intent declaration must say the
app's core function is neither calling nor alarms, so Play leaves it off on
install and the person turns it on.
- https://support.google.com/googleplay/android-developer/answer/13392821
- https://source.android.com/docs/core/permissions/fsi-limits

**Do Not Disturb bypass (ACCESS_NOTIFICATION_POLICY, NotificationChannel.setBypassDnd).**
Allowed; the person grants Do Not Disturb access themselves in Android's
settings. The app may set bypass on its own channel only while it holds that
access, and only until the person has changed the channel. **Used, opt-in
only**, behind "Ring through Do Not Disturb" in App settings.
- https://developer.android.com/reference/android/app/NotificationChannel#setBypassDnd(boolean)
- https://developer.android.com/reference/android/app/NotificationManager#isNotificationPolicyAccessGranted()

**Fake calls (CallStyle, ConnectionService).** CallStyle is the template for
incoming, ongoing and screened calls, with required Answer/Decline/Hang up
actions, and is meant to be used with the Telecom APIs. An alert is not a
call. **Not used.**
- https://developer.android.com/develop/ui/views/notifications/call-style
- https://developer.android.com/develop/connectivity/telecom/selfManaged

**Exact alarms (USE_EXACT_ALARM, SCHEDULE_EXACT_ALARM).** Play allows
USE_EXACT_ALARM only for apps whose core function needs precisely timed
actions (alarm, timer, calendar apps); others are disallowed from publishing.
An alert arrives by push and needs no alarm at all. **Not used.**
- https://support.google.com/googleplay/android-developer/answer/13161072
- https://developer.android.com/develop/background-work/services/alarms/schedule

**iOS (there is no iOS app; a note only).** Time-sensitive notifications
(interruption level `timeSensitive`) can break through Focus and the
notification summary where the person allows it, and need only the Time
Sensitive Notifications capability. Critical Alerts (`critical`, sound
through the ring/silent switch and Do Not Disturb) need an entitlement Apple
grants on request, for health, safety and security apps. **Time-sensitive
only, never Critical Alerts.**
- https://developer.apple.com/documentation/usernotifications/unnotificationinterruptionlevel
- https://developer.apple.com/videos/play/wwdc2021/10091/

Decision: no fake call, no exact alarms; full-screen intent only when the
person turns it on (changed 25 October 2026, at the owner's request). A high-importance channel with an alarm-like sound,
FLAG_INSISTENT, heads-up and a strong vibration. Do Not Disturb bypass only
when the person turns it on. Look again before each store release that
changes this.

## What the phone does

Two channels, because Android does not let an app change a channel's sound,
audio usage or Do Not Disturb bypass once the channel exists:

| Channel | When | Sound | Through Do Not Disturb |
|---|---|---|---|
| `urgent` ("Urgent") | always made | the phone's alarm tone, as a notification sound (`USAGE_NOTIFICATION_EVENT`) | no |
| `urgent.dnd` ("Urgent, through Do Not Disturb") | made only once the person has turned the option on AND granted Do Not Disturb access; deleted when they turn it off | the alarm tone at alarm volume (`USAGE_ALARM`) | yes (`setBypassDnd`) |

The plain channel deliberately does not use `USAGE_ALARM` or
`CATEGORY_ALARM`: Android treats either as an alarm, and alarms are let
through Do Not Disturb by default and ring at alarm volume in silent mode.
That would bypass Do Not Disturb without the person asking, which is what the
opt-in is for. The opt-in channel uses alarm usage because there the person
has asked for exactly that.

`urgent.dnd` is created only while access is held, so the bypass Android
records for it at creation is the one asked for. Made earlier, without
access, Android would have stored it without bypass and would not let the app
change it later.

Every alert is posted with `FLAG_INSISTENT` when it rings (the sound repeats
until the notification is opened, dismissed or the shade is pulled down),
priority max, and a public version on the lock screen that names only the
server. Tapping opens the chat it is about, or the server's page.

How an alert is shown (`UrgentAlerts.decide`, unit-tested):

| Payload | "Urgent alerts" for the server | Result | Log line |
|---|---|---|---|
| `urgent: true` | on | rings (`urgent`, or `urgent.dnd` when opted in and access is granted) | rang |
| `urgent: true` | off | ordinary notification, no ringing | shown quietly: urgent alerts are off for this server |
| `downgraded: true` | either | ordinary notification | shown quietly: downgraded by the server |
| neither (`level: normal`) | either | ordinary notification | shown (normal) |

The notification log (App settings, Notification check) gets one line per
alert with the server, the level and the outcome, and never the text.

An app older than this shows an alert as it shows any notification of a kind
it does not know: under its chat when one is attached, otherwise on the
Server channel.

## Full screen (opt-in)

"Full-screen urgent alerts" in App settings, also offered once by the urgent
alert setup that follows the notification permission (first install, or the
update that brought it; `UrgentSetup`). On Android 14 and up it also needs
Android's own switch for the app (`canUseFullScreenIntent`), which the row
and the setup open (`ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT`); before 14 the
permission comes with the install.

When both hold, a ringing alert carries a full-screen intent to
`AlertActivity`: over the lock screen (`showWhenLocked`, `turnScreenOn`), the
title, the body and where it came from, and a slider: right to attend (asks
for the phone to be unlocked, then opens the chat), left to dismiss. Both take
the notification away, which stops the ringing; TalkBack gets both as actions.
While the phone is in use Android shows the heads-up instead, as before. An
alert that does not ring is never full screen (`UrgentAlerts.decide`).

## Ring time, snooze and answers (1.1.1-dev.7)

An urgent alert rings for the person's ring time ("Urgent alerts ring for"
in App settings, 1 to 15 minutes, 5 by default): the notification has
`setTimeoutAfter`, and the full-screen screen closes at the same moment.
Then it counts as unanswered and leaves a quiet "Missed urgent alert" notice
(posted from the delete intent, which tells a time-out from a swipe by when
it comes).

Snooze (the notification's Snooze button opens the alert screen; the screen
has 5 min, 15 min, 30 min, 1 hour) takes the alert away and brings it back,
ringing again, with an inexact `setAndAllowWhileIdle` alarm: exact alarms
need a permission Play keeps for alarm clocks (see the policy check above).

What happened is told to the server that sent it (POST /api/alerts/answer,
capability `alert-answers`): `ringing` with `ringMs` when it is posted, then
`attended` (slid, or opened from the notification), `dismissed` (slid, the
Dismiss button, or swiped) or `snoozed` with `minutes`. alert_user waits for
it, at most 15 minutes, and gives the caller the answer, or "no answer" once
the ring time is up, or "not confirmed" when no "ringing" came within a
minute (an older app, or the phone offline).
