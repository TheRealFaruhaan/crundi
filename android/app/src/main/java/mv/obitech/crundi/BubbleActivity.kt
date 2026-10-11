package mv.obitech.crundi

import android.app.Activity
import android.os.Bundle
import android.widget.Toast

/**
 * The live card's Bubble button.
 *
 * It is a window that shows nothing and is gone at once, and it is a window
 * on purpose: Android opens a bubble by itself only for an app that is in
 * front at that moment, which a button pressed in the notification shade
 * otherwise is not. Being a window also closes the shade, so the bubble is
 * not opened behind it.
 */
class BubbleActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val serverId = intent.getStringExtra(Notifier.EXTRA_SERVER).orEmpty()
        val chatId = intent.getStringExtra(Notifier.EXTRA_CHAT).orEmpty()
        // Only a chat the card itself knows about: what to call it and how
        // to draw its head are kept there, and nothing else is trusted.
        val chat = LiveCards.active(this, serverId).firstOrNull { it.id == chatId }
        val problem = if (chat == null) "That chat has finished." else Notifier.bubble(this, chat, expand = true)
        if (problem != null) Toast.makeText(this, problem, Toast.LENGTH_LONG).show()
        finish()
    }
}
