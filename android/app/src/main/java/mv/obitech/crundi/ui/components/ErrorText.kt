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
}

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
