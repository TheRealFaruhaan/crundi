package mv.obitech.crundi.voice

/**
 * A file or a shared bit of text waiting to go to Cru with the next turn
 * (capability "voice-attach"), as the tray above the bottom row shows it:
 * uploading ([up]), failed ([bad]), ready ([id] from the server, or [ref] for
 * shared text, which needs no upload). [thumb]: a small picture of an image.
 */
data class CruAtt(
    val key: String,
    val name: String,
    val up: Boolean = false,
    val bad: String = "",
    val id: String = "",
    val kind: String = "",
    val ref: String = "",
    val thumb: android.graphics.Bitmap? = null,
) {
    val ready: Boolean get() = !up && bad.isEmpty() && (id.isNotEmpty() || ref.isNotEmpty())
}

/** A piece of the owner's message in the chat view: plain text, or a chip for a file or shared item. */
sealed interface SaidPart {
    data class Words(val text: String) : SaidPart
    /** [id]: the attachment's id when the line has one (its picture can be fetched). */
    data class Chip(val name: String, val kind: String = "", val id: String = "") : SaidPart
}

/** The tray and the chat view's chips, with no Android in it (CruAttachTest). */
object CruAttach {
    /** Shared text up to this long goes in as a reference; longer goes as a text file. */
    const val REF_MAX = 400

    private val attached = Regex("""^\[The owner attached: "([^"]*)" \(([a-z]+)[^)]*\)(?:.*?\bid (att_[A-Za-z0-9_-]+))?""")
    private val dropped = Regex("""^\[The owner dropped in: (.*)]$""")
    /** What the server puts in when only files were sent; not shown. */
    private const val NOTHING_SAID = "(nothing said: look at what was attached and respond to it)"

    /** What goes with the next turn: the files' ids, the shared text, and the line shown for them meanwhile. */
    data class Taken(val attachments: List<String>, val refs: List<String>, val label: String)

    fun take(list: List<CruAtt>): Taken {
        val ready = list.filter { it.ready }
        return Taken(
            ready.filter { it.id.isNotEmpty() }.map { it.id },
            ready.filter { it.id.isEmpty() && it.ref.isNotEmpty() }.map { it.ref },
            ready.joinToString("\n") { a ->
                if (a.id.isEmpty()) "[The owner dropped in: ${a.ref}]"
                else "[The owner attached: \"${a.name}\" (${a.kind.ifEmpty { "file" }}), id ${a.id}.]"
            },
        )
    }

    /** What stays in the tray once a turn has taken the ready ones: those still uploading. */
    fun left(list: List<CruAtt>): List<CruAtt> = list.filter { it.up }

    /** What the owner said, as the chat view shows it: [label] lines then the words. */
    fun said(label: String, text: String): String = if (label.isEmpty()) text else if (text.isEmpty()) label else label + "\n" + text

    /** Shared text, made one line and short enough to be a reference. */
    fun refOf(text: String): String = text.replace(Regex("\\s+"), " ").trim().take(REF_MAX)

    /** The owner's message split into chips (attached files, dropped items) and words. */
    fun parts(said: String): List<SaidPart> {
        val out = ArrayList<SaidPart>()
        val words = StringBuilder()
        fun flush() { if (words.isNotBlank()) out += SaidPart.Words(words.toString().trim()); words.clear() }
        for (line in said.split('\n')) {
            val a = attached.find(line)
            val d = if (a == null) dropped.find(line) else null
            when {
                a != null -> { flush(); out += SaidPart.Chip(a.groupValues[1], a.groupValues[2], a.groupValues[3]) }
                d != null -> { flush(); out += SaidPart.Chip(d.groupValues[1]) }
                line.trim() == NOTHING_SAID -> {}
                else -> { if (words.isNotEmpty()) words.append('\n'); words.append(line) }
            }
        }
        flush()
        return out
    }

    /** The words alone, without the chips' lines: for matching a message here to the server's copy of it. */
    fun plain(said: String): String = parts(said).filterIsInstance<SaidPart.Words>().joinToString("\n") { it.text }.trim()

    /** "Too large (50 MB at most)" over [max] bytes, else "". */
    fun tooLarge(size: Long, max: Long): String = if (size > max) "Too large (${max / (1024 * 1024)} MB at most)" else ""

    /** Pictures Cru can be given a smaller copy of to look at. */
    fun previewable(mime: String): Boolean = mime.lowercase() in setOf("image/png", "image/jpeg", "image/jpg", "image/webp", "image/gif", "image/heic", "image/heif")
}
