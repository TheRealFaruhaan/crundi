package mv.obitech.crundi.voice

import java.util.Calendar
import java.util.Locale

/**
 * The rules of "Hey Cru" (Phase 6, D3–D7), with no Android in them:
 * which durations there are, when background listening ends, what the
 * notification says about the time left, who may hold the microphone, and
 * what a voice command or a restart turns into. [HeyCrundi] and
 * [HeyCrundiService] do what these say.
 */
object HeyCrundiPlan {
    /** "Until I turn it off" in the settings' choice (D7). Never offered to the voice tool (D5). */
    const val UNTIL_OFF = 0
    /** D6: the time limits, in minutes. */
    val DURATIONS = listOf(15, 30, 45, 60, 120)
    /** D6: the default, also what the voice tool gets when it names none. */
    const val DEFAULT_MINUTES = 30
    /** D6: the most the voice tool may ask for. */
    const val MAX_MINUTES = 120
    /** The settings' choices, in the order shown: until off first (D7). */
    val CHOICES = listOf(UNTIL_OFF) + DURATIONS

    /** A settings choice as shown: "Until I turn it off", "15 min", "1 hour", "2 hours". */
    fun label(minutes: Int): String = when {
        minutes <= UNTIL_OFF -> "Until I turn it off"
        minutes % 60 == 0 -> (minutes / 60).let { "$it hour" + if (it == 1) "" else "s" }
        else -> "$minutes min"
    }

    /** A stored settings choice that isn't one of [CHOICES] (an older build, a bad value) falls back to the default. */
    fun choice(stored: Int): Int = if (stored in CHOICES) stored else DEFAULT_MINUTES

    /**
     * The minutes the voice tool asked for, made one of D6's: 0 or less is
     * "off" (0); otherwise the nearest allowed duration, at most two hours,
     * never "until turned off" (D5).
     */
    fun voiceMinutes(asked: Int): Int {
        if (asked <= 0) return 0
        if (asked >= MAX_MINUTES) return MAX_MINUTES
        return DURATIONS.minBy { kotlin.math.abs(it - asked) * 1000 - it } // ties go to the longer one
    }

    /** When listening started at [now] for [minutes] ends; null means until turned off. */
    fun deadline(now: Long, minutes: Int): Long? = if (minutes <= UNTIL_OFF) null else now + minutes * 60_000L

    /** Whether background listening with this [deadline] is over at [now]. */
    fun expired(now: Long, deadline: Long?): Boolean = deadline != null && now >= deadline

    /** "29 min left", "1 h 5 min left", "Less than a minute left"; "Until you turn it off" without a deadline. */
    fun timeLeft(now: Long, deadline: Long?): String {
        if (deadline == null) return "Until you turn it off"
        val ms = deadline - now
        if (ms <= 0) return "Time is up"
        val min = ((ms + 59_999) / 60_000).toInt() // rounded up: "1 min left" until the last second
        if (ms < 60_000) return "Less than a minute left"
        val h = min / 60
        val m = min % 60
        return when {
            h == 0 -> "$m min left"
            m == 0 -> "$h h left"
            else -> "$h h $m min left"
        }
    }

    /** "until 14:30" for the notification, in the phone's clock. */
    fun until(deadline: Long, cal: Calendar = Calendar.getInstance()): String {
        cal.timeInMillis = deadline
        return "until %02d:%02d".format(Locale.US, cal.get(Calendar.HOUR_OF_DAY), cal.get(Calendar.MINUTE))
    }

    /** How often the notification's text needs refreshing so the time left is right to the minute. */
    fun nextRefresh(now: Long, deadline: Long?): Long? {
        if (deadline == null) return null
        val ms = deadline - now
        if (ms <= 0) return 0
        val toMinute = ms % 60_000
        return if (toMinute == 0L) 60_000 else toMinute
    }

    // ─── who listens ───

    enum class Listener { None, InApp, Background, Screen }

    /**
     * What decides who holds the microphone for the wake word. One owner at
     * a time: an open voice session has it; otherwise background listening
     * (which also covers the app being in front); otherwise in-app
     * listening while the app is in front (D3). The voice screen left open
     * and asleep listens too, whatever the settings say: The owner is looking
     * at it.
     */
    data class Situation(
        val inAppOn: Boolean = false,
        val appInFront: Boolean = false,
        /** The background service is running (and in its time). */
        val backgroundOn: Boolean = false,
        /** A voice session (side key overlay or the voice screen) has the microphone. */
        val sessionOpen: Boolean = false,
        /** The voice screen is open, its session asleep: "Hey Cru" wakes it where it is. */
        val screenWaiting: Boolean = false,
        /** Just heard: waiting for the voice screen to open, so the microphone is free for it. */
        val handingOver: Boolean = false,
        val micAllowed: Boolean = true,
        /** The owner has trained "Hey Cru" (a voice profile exists). Untrained, nothing listens. */
        val trained: Boolean = false,
        /** The training screen is recording. */
        val training: Boolean = false,
        /** A chat's voice mode (the composer's mic) has the microphone. */
        val chatVoice: Boolean = false,
        /** The keyword spotter could load on this phone. */
        val engineReady: Boolean = true,
        /**
         * The owner told Cru to stop listening (its background-listening off order, 6 Nov: the
         * phone stopped the service but the asleep voice screen and in-app listening went on):
         * nothing listens until listening is turned on again, by Cru or in Settings.
         */
        val heldOff: Boolean = false,
    )

    fun listener(s: Situation): Listener = when {
        !s.micAllowed || !s.engineReady || !s.trained || s.training || s.chatVoice || s.sessionOpen || s.handingOver || s.heldOff -> Listener.None
        s.screenWaiting -> Listener.Screen
        s.backgroundOn -> Listener.Background
        s.inAppOn && s.appInFront -> Listener.InApp
        else -> Listener.None
    }

    /** What to do on hearing the wake word: open the voice screen directly when Crundi is in front, otherwise ask Android. */
    enum class Wake { OpenScreen, FromBackground, WakeScreen }

    fun onHeard(s: Situation): Wake = when {
        s.screenWaiting -> Wake.WakeScreen
        s.appInFront -> Wake.OpenScreen
        else -> Wake.FromBackground
    }

    /** How long a hand-over waits for the voice session before the wake word listens again. */
    const val HANDOVER_MS = 12_000L
    /** After a hit, ignore the wake word this long (the end of "Hey Cru" heard twice). */
    const val COOLDOWN_MS = 2_000L

    // ─── the voice command (capability "voice-phone-actions") ───

    sealed interface Command {
        data class Start(val minutes: Int) : Command
        data object Stop : Command
    }

    /** A reply's phoneAction as a command; null for one this app doesn't know. */
    fun command(type: String, minutes: Int): Command? = when (type) {
        "background-listening" -> voiceMinutes(minutes).let { if (it == 0) Command.Stop else Command.Start(it) }
        else -> null
    }

    // ─── after a restart ───

    enum class Restart {
        /** Nothing was on, or it had run out anyway: forget it. */
        Clear,
        /** It was on and still in its time: Android stopped it, say so ("tap to resume"). */
        NotifyStopped,
    }

    /**
     * The service was started again without Crundi asking (Android restarted
     * it after killing it, or the process came back). It can't take the
     * microphone from the background, so it tells the owner instead.
     */
    fun afterRestart(wasOn: Boolean, now: Long, deadline: Long?): Restart =
        if (wasOn && !expired(now, deadline)) Restart.NotifyStopped else Restart.Clear

    /** Resuming from "tap to resume": what is left of the old time, or the old choice again if it ran out meanwhile. */
    fun resumeMinutes(now: Long, deadline: Long?, chosen: Int): Int {
        if (deadline == null) return UNTIL_OFF
        val left = ((deadline - now) / 60_000).toInt()
        return if (left >= 1) left else choice(chosen).let { if (it == UNTIL_OFF) DEFAULT_MINUTES else it }
    }
}
