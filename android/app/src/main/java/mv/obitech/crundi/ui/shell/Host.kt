package mv.obitech.crundi.ui.shell

import androidx.compose.runtime.staticCompositionLocalOf

/**
 * What a screen may ask of the window it is in: the things the old web page
 * asked of the app through its CrundiNative bridge. MainActivity and
 * ChatActivity (the bubble) each answer them in their own way; tests use
 * [Host.None].
 */
interface Host {
    /** True in a bubble (ChatActivity): one chat, compact, no shell around it. */
    val isBubble: Boolean get() = false

    /**
     * A chat is on screen (call with true when its screen is resumed and
     * visible, false when it leaves). Clears its notification and keeps new
     * ones for it from showing while it is in front of the person.
     */
    fun chatOnScreen(chatId: String, onScreen: Boolean) {}

    /**
     * A chat woken from sleep came back under another id ([from] to [to]):
     * whatever this window keys on the chat's id follows it (the bubble's
     * open count, what is on screen, the id it reopens with).
     */
    fun chatMoved(from: String, to: String) {}

    /** "Open as bubble" from a chat's menu. Returns a reason if it could not. */
    fun openBubble(chatId: String) {}

    /** A server path ("/chat/abc") to show in the main window. From a bubble this opens the app. */
    fun openInMain(path: String) {}

    /** Close this window (the bubble's back arrow); in the main window, go back. */
    fun close() {}

    /** The server refused this phone's sign-in: pair it again. */
    fun pairAgain(reason: String = "") {}

    /** Pair with another server (the drawer's "Connect to another server"). */
    fun pairAnother() {}

    /** The app's own settings on this phone (notifications, bubbles, servers). */
    fun openAppSettings() {}

    /** An address outside the server: open it in the phone's browser. */
    fun openExternal(url: String) {}

    object None : Host
}

val LocalHost = staticCompositionLocalOf<Host> { Host.None }
