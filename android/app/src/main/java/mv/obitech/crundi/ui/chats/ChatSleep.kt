package mv.obitech.crundi.ui.chats

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.model.Pane
import mv.obitech.crundi.api.model.SendResult
import mv.obitech.crundi.ui.components.errorText

/**
 * A chat put to sleep (parked: for idleness, by hand, or by a restart). Its
 * Claude is gone; what is left is a row in the live state's terminals of
 * kind "parked" under the chat's own id, which is what the workbench draws
 * as ParkedBody. The bubble and the chat screen go by the same row.
 *
 * Waking is the server's /api/panes/<id>/resume, as a scheduled message does
 * it (src/chat-schedule.js: panes.resume, then deliver once it is up). The
 * chat usually comes back under the same id; when that id was taken it comes
 * back under another, and the screen has to follow it there.
 */
object ChatSleep {
    const val WAKE_FAILED = "This chat is asleep and could not be woken: "

    /** The row says this chat is asleep (a parked chat, not a parked terminal). */
    fun asleep(row: Pane?): Boolean = row != null && row.isParked && row.paneKind != "terminal"

    /** The row says the chat [id] is running again (a live chat, not parked, not ended). */
    fun up(rows: List<Pane>?, id: String): Boolean = rows.orEmpty().any { it.id == id && it.isChat && it.status != "exited" }

    /**
     * The server has no live Claude for this chat: claude-ui's `No session "<id>"`,
     * or "Session is not running" for one still starting. Never shown as it is.
     */
    fun noSession(e: Throwable): Boolean = e is ApiException && !e.offline &&
        (e.message.trim().startsWith("No session") || e.message.trim().startsWith("Session is not running"))

    /** An older server, from before parked chats: it has no /api/panes routes at all. */
    fun unsupported(e: Throwable): Boolean = e is ApiException && (e.notFound || e.status == 405)

    /** The server's answer when the chat is not asleep there (woken elsewhere, or closed for good). */
    fun notParked(e: Throwable): Boolean = e is ApiException && e.message.contains("not parked", ignoreCase = true)

    /** Why waking failed, in the interface's words. */
    fun wakeError(e: Throwable): String = WAKE_FAILED + when {
        unsupported(e) -> "this server cannot wake chats from the app. Resume it from Crundi on the computer, or update the server."
        notParked(e) -> "it is no longer on the server. Start a new chat to carry on."
        else -> errorText(e).trim().trimEnd('.').replaceFirstChar { it.lowercase() } + "."
    }

    const val NOT_UP = WAKE_FAILED + "it did not start in time. Try again in a moment."

    /** Anything but a send (a mode, a rename, an answer) meeting no live chat: asleep, or gone. */
    fun idleError(row: Pane?): String = if (asleep(row)) "This chat is asleep. Resume it first." else GONE

    /** What a send meeting no live chat says, when the chat was not known to be asleep. */
    const val GONE = "This chat is no longer running on the server. Start a new chat to carry on."

    /** What waking needs from the server, apart, so the order can be tested without one. */
    interface Server {
        /** POST /api/panes/<id>/resume. Answers the id it came back under. */
        suspend fun resume(id: String, mode: String): String
        /** Wait until the chat [id] is running (the live state lists it as a chat) and catch its stream up; false if not within [timeoutMs]. */
        suspend fun awaitUp(id: String, timeoutMs: Long): Boolean
        suspend fun send(id: String, text: String): SendResult
    }

    sealed interface Outcome {
        /** Where the chat is now: its id after waking (the old one if waking failed). */
        val id: String
        data class Sent(override val id: String, val result: SendResult) : Outcome
        data class Woken(override val id: String) : Outcome
        /** [message] is for the screen as it is; the text, if any, did not go and stays in the box. */
        data class Failed(override val id: String, val message: String) : Outcome
    }

    /**
     * Wake the chat [id], wait until it is up, then send [text] (none: only
     * wake it). Nothing is sent before the chat is running, and a failure at
     * any step leaves the text unsent ([Outcome.Failed]), for the box to keep.
     *
     * [known]: the live state said it was asleep. A send that met "No session"
     * without that wakes it too, in case it was put to sleep a moment ago;
     * if the server then says it is not asleep, the chat is gone, not sleeping.
     */
    suspend fun wake(server: Server, id: String, text: String?, mode: String = "resume", known: Boolean = true, upTimeoutMs: Long = 30_000, sendTries: Int = 4, retryMs: Long = 1_500): Outcome {
        val now = try {
            server.resume(id, mode)
        } catch (e: CancellationException) { throw e } catch (e: Exception) {
            when {
                // Woken by someone else meanwhile (the web, a scheduled message): carry on with it.
                notParked(e) && server.awaitUp(id, 3_000) -> id
                notParked(e) && !known -> return Outcome.Failed(id, GONE)
                unsupported(e) && !known -> return Outcome.Failed(id, GONE)
                else -> return Outcome.Failed(id, wakeError(e))
            }
        }.ifEmpty { id }
        if (!server.awaitUp(now, upTimeoutMs)) return Outcome.Failed(now, NOT_UP)
        if (text == null) return Outcome.Woken(now)
        // Listed as running, but its Claude may still be starting: a few tries.
        repeat(sendTries) { n ->
            try {
                return Outcome.Sent(now, server.send(now, text))
            } catch (e: CancellationException) { throw e } catch (e: Exception) {
                if (!noSession(e)) return Outcome.Failed(now, "The message did not go: " + errorText(e))
                if (n < sendTries - 1) delay(retryMs)
            }
        }
        return Outcome.Failed(now, NOT_UP)
    }
}
