package mv.obitech.crundi.ui.components

import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.LiveStatus

/**
 * The one way the app words a failure, in every area: what happened and what
 * to do about it, in the interface's voice. Use it for ErrorState, banners
 * and toasts ("Upload failed: " + errorText(e)). Never show e.message raw.
 */
object ErrorWords {
    const val OFFLINE = "The server cannot be reached. Check the connection and try again."
    const val SIGNED_OUT = "This phone is no longer signed in to the server. Pair it again from App settings."
    const val UNKNOWN = "Something went wrong. Try again."
    /** An older server than this app: the route is not there yet (src/client-version.js, THE RULE). */
    const val NOT_SUPPORTED = "This server does not support this yet. Update the server to use it."
}

/**
 * The server has no such route: it is older than this app. That is a 404 (or
 * 405) carrying the router's own "Not found", not a route answering that the
 * thing it was asked about is gone ("Skill not found", "File not found").
 */
fun routeMissing(t: Throwable): Boolean =
    t is ApiException && (t.status == 404 || t.status == 405) && t.message.trim().trimEnd('.').equals("Not found", ignoreCase = true)

/** [errorText], but a missing route says the server needs an update. */
fun errorOrUnsupported(t: Throwable): String = if (routeMissing(t)) ErrorWords.NOT_SUPPORTED else errorText(t)

fun errorText(t: Throwable): String = when {
    t is ApiException && t.offline -> ErrorWords.OFFLINE
    t is ApiException && t.signedOut -> ErrorWords.SIGNED_OUT
    t is ApiException && t.status >= 500 -> "The server ran into a problem (${t.message.trim().trimEnd('.')}). Try again in a moment."
    t is ApiException -> t.message.trim().ifEmpty { ErrorWords.UNKNOWN }
    t is java.io.IOException -> ErrorWords.OFFLINE
    else -> t.message?.trim()?.takeIf { it.isNotEmpty() } ?: ErrorWords.UNKNOWN
}

/**
 * Why live data is not arriving, for a screen that has none yet; null while it
 * is still connecting. `failure` is the stream's last error (LiveEvents.failure):
 * a server that answers with an error is worded as such (errorText), and only
 * a stream that could not reach the server says it cannot be reached.
 */
fun liveErrorText(status: LiveStatus, failure: Throwable? = null): String? = when (status) {
    LiveStatus.Reconnecting, LiveStatus.Offline -> failure?.let { errorText(it) } ?: ErrorWords.OFFLINE
    LiveStatus.SignedOut -> ErrorWords.SIGNED_OUT
    else -> null
}
