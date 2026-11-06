package mv.obitech.crundi.voice

/**
 * Is it the owner talking over Cru? Decided on the phone, from the
 * microphone alone, while Cru speaks or a cue plays: nothing is sent to the
 * server then (crundi-voice.js createGate has the same rule). Levels are
 * 0..1 on a square-root scale (MicStream.level, Speaker.level alike).
 *
 * With Cru's playback level known (Deepgram's voice, a cue clip), how much
 * of it comes back through the microphone is learnt as it plays (only while
 * it plays steadily; it falls fast and rises slowly, so the owner's voice
 * hardly moves it) and only a voice [overEcho] times louder than that echo
 * counts. With it unknown (the phone's own voice) the bar is the loudest of
 * the last couple of seconds, and high until there are some. Either way it
 * must hold for [holdMs] and be voiced (a pitch, as speech has: a cough, a
 * clap or a door is not him).
 *
 * Words are never looked at here: the owner may well say "on it" himself.
 */
class OwnerGate(
    private val holdMs: Long = 300,
    private val floorMin: Float = 0.03f,
    private val overNoise: Float = 3.2f,
    private val overEcho: Float = 2f,
    private val unknownOver: Float = 1.6f,
    private val unknownStart: Float = 0.45f,
    private val minVoiced: Double = 0.3,
) {
    private var est = 0.6f
    private var noise = 0.01f
    private var loud = 0L
    private val recent = FloatArray(4)
    private var hist = ArrayList<Float>()
    private val run = ArrayList<Float>()

    /** How long it has been loud: his words began about then. */
    val loudMs: Long get() = loud

        /** Microphone level per unit of Cru's playback level, as learnt. */
    val echo: Float get() = est

    /** Listening, nothing playing: how loud the room is. */
    fun learn(level: Float) { noise = (noise * 0.95f + level * 0.05f).coerceIn(0.002f, 0.2f) }

    /** Cru starts to play: count afresh (the echo learnt is kept). */
    fun start() { loud = 0L; recent.fill(0f); hist = ArrayList(); run.clear() }

    /**
     * One stretch of microphone, [ms] long. [play] is Cru's level now, or
     * null when unknown; [voiced] the share of voiced frames lately (asked
     * only once it has been loud for long enough). True: it is him.
     */
    fun hear(level: Float, play: Float?, ms: Long, voiced: () -> Double): Boolean {
        val gate = if (play != null) {
            System.arraycopy(recent, 1, recent, 0, recent.size - 1); recent[recent.size - 1] = play
            val peak = recent.max()
            if (peak > 0.05f && recent.min() >= peak * 0.5f) {
                val r = level / peak
                est = (if (r < est) est + (r - est) * 0.3f else est + (r - est) * 0.01f).coerceIn(0.02f, 3f)
            }
            maxOf(floorMin, noise * overNoise, est * maxOf(peak, 0.05f) * overEcho + 0.03f)
        } else {
            var g = maxOf(floorMin, noise * overNoise, pct90(hist) * unknownOver)
            if (hist.size < 6) g = maxOf(g, unknownStart)
            g
        }
        if (level >= gate) { loud += ms; run += level }
        else {
            // A short burst (a loud syllable of Cru's) joins what its echo sounds like.
            if (play == null) { hist.addAll(run); hist.add(level); while (hist.size > 24) hist.removeAt(0) }
            loud = 0L; run.clear()
        }
        if (loud < holdMs) return false
        return voiced() >= minVoiced
    }

    private fun pct90(a: List<Float>): Float {
        if (a.isEmpty()) return 0f
        val b = a.sorted()
        return b[minOf(b.size - 1, (b.size * 0.9).toInt())]
    }
}

/** The last [maxMs] of microphone chunks, oldest first: what the owner said just before the gate opened. */
class PreRoll(private val maxMs: Long = 1_300) {
    private val q = ArrayDeque<Pair<ByteArray, Long>>()
    private var total = 0L

    @Synchronized fun push(chunk: ByteArray, ms: Long) {
        q.addLast(chunk to ms); total += ms
        while (q.size > 1 && total - q.first().second >= maxMs) total -= q.removeFirst().second
    }

    /** Oldest first; [lastMs]: only about that much of the latest (what came before was not him). */
    @Synchronized fun drain(lastMs: Long? = null): List<ByteArray> {
        var keep: List<Pair<ByteArray, Long>> = q.toList()
        if (lastMs != null) { var t = 0L; var i = q.size; while (i > 0 && t < lastMs) { i--; t += q[i].second }; keep = keep.subList(i, keep.size) }
        val out = keep.map { it.first }; q.clear(); total = 0L; return out
    }

    @Synchronized fun clear() { q.clear(); total = 0L }

    val ms: Long @Synchronized get() = total
}

/** The latest [size] samples of microphone (-1..1), for the voicing check. */
class VoiceRing(size: Int = 6_400) {
    private val r = FloatArray(size)
    private var at = 0

    /** 16-bit little-endian PCM. */
    @Synchronized fun add(pcm: ByteArray) {
        var i = 0
        while (i + 1 < pcm.size) {
            r[at] = ((pcm[i + 1].toInt() shl 8) or (pcm[i].toInt() and 0xff)).toShort() / 32768f
            at = (at + 1) % r.size; i += 2
        }
    }

    @Synchronized fun snapshot(): FloatArray = FloatArray(r.size) { r[(at + it) % r.size] }
}

/**
 * How Cru's voice is played, so the phone's echo canceller can take it out
 * of the call-mode microphone: as call audio (USAGE_VOICE_COMMUNICATION) on
 * the built-in speaker, the way a speakerphone call is. With anything else
 * to play on (headphones, a headset, Bluetooth: no echo to cancel) or an
 * Android older than 12 (no way to pick the speaker for call audio) it stays
 * ordinary assistant audio, and the gate alone keeps Cru's voice out.
 */
object CruAudio {
    enum class Route { Call, Media }

    /** AudioDeviceInfo types that mean the owner hears Cru in his ears. */
    val HEADSETS = setOf(
        3,  // TYPE_WIRED_HEADSET
        4,  // TYPE_WIRED_HEADPHONES
        7,  // TYPE_BLUETOOTH_SCO
        8,  // TYPE_BLUETOOTH_A2DP
        11, // TYPE_USB_DEVICE
        22, // TYPE_USB_HEADSET
        23, // TYPE_HEARING_AID
        26, // TYPE_BLE_HEADSET
        27, // TYPE_BLE_SPEAKER
        30, // TYPE_BLE_BROADCAST
    )
    const val BUILTIN_SPEAKER = 2
    /** AudioAttributes usages. */
    const val USAGE_VOICE_COMMUNICATION = 2
    const val USAGE_ASSISTANT = 16

    fun route(sdk: Int, outputs: Collection<Int>): Route =
        if (sdk >= 31 && BUILTIN_SPEAKER in outputs && outputs.none { it in HEADSETS }) Route.Call else Route.Media

    fun usage(route: Route): Int = if (route == Route.Call) USAGE_VOICE_COMMUNICATION else USAGE_ASSISTANT

    /** Call audio has its own volume: at least as loud, as a share of its range, as the media volume was. */
    fun callVolume(current: Int, max: Int, mediaShare: Float): Int =
        maxOf(current, Math.round(mediaShare.coerceIn(0f, 1f) * max)).coerceIn(1, maxOf(1, max))
}
