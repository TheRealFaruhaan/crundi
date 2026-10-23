package mv.obitech.crundi.api.model

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

// ─── Panels (widgets) ───

/** A chip's face in the top bar: the value at [path] of data source [source]. */
@Serializable data class WidgetChip(val source: String = "", val path: String = "", val label: String = "", val icon: String = "")

/**
 * A panel (widget) Claude made (GET /api/widgets). [slot] is where it shows:
 * dock (under its chat), cell (a Workbench pane), tab (Panels tab only),
 * inline (in its chat), chip (top bar). [state] open or closed.
 */
@Serializable
data class Widget(
    val id: String,
    /** "_global" for a panel shown in every project. */
    val project: String = "",
    val title: String = "",
    val icon: String = "",
    val slot: String = "cell",
    val requestedSlot: String = "",
    /** The chat that made it (dock target, prompt actions). */
    val chat: String = "",
    val beside: String = "",
    val state: String = "open",
    /** pinned or task */
    val lifecycle: String = "",
    val rev: Int = 0,
    val version: Int = 0,
    val global: Boolean = false,
    val home: String = "",
    /** It has an element marked data-chip: the chip shows the live HTML, not a value. */
    val chipLive: Boolean = false,
    val size: JsonObject? = null,
    val chip: WidgetChip? = null,
    val collapsed: Boolean = false,
    /** Its chat is running now. */
    val chatLive: Boolean = false,
    /** It asks for more than a panel gets by default and the person has not approved it yet. */
    val needsGrant: Boolean = false,
    val faults: Int = 0,
    val createdAt: JsonElement? = null,
    val updatedAt: JsonElement? = null,
) {
    val open get() = state == "open"
    /** Counts toward the top bar's Panels button. */
    val isChip get() = open && slot == "chip"
    /** Shows as a Workbench pane (a dock whose chat is unknown shows as a pane too, as on the web). */
    fun isPane() = open && (slot == "cell" || (slot == "dock" && chat.isEmpty()))

    companion object {
        /** The slots a person can move a panel to, with the web's labels. */
        val SLOT_LABELS = linkedMapOf("dock" to "Docked to chat", "cell" to "Workbench pane", "tab" to "Panels tab only", "chip" to "Top bar chip")
    }
}

/** One thing a panel asks to be allowed (a command, a fetch, a file outside the project, an action). */
@Serializable data class GrantItem(val what: String = "", val name: String = "", val detail: String = "")
/** Approve with [hash] (it changes when the panel does). */
@Serializable data class WidgetGrant(val needed: Boolean = false, val granted: Boolean = true, val hash: String = "", val items: List<GrantItem> = emptyList())

/**
 * A panel's document to put in a frame (GET /api/widgets/doc): [html] is the
 * whole page, with the host script the server builds in. Load it with
 * loadDataWithBaseURL(server origin, html) and answer its messages as in
 * api.md "Panels: the frame protocol".
 */
@Serializable
data class WidgetDoc(
    val html: String = "",
    val rev: Int = 0,
    val widget: Widget? = null,
    val problems: List<JsonElement> = emptyList(),
    val grant: WidgetGrant = WidgetGrant(),
    val actions: List<String> = emptyList(),
)

/** One data source's current value (GET /api/widgets/data). kind: push, session, file, sqlite, command, http, crundi. */
@Serializable data class WidgetValue(val value: JsonElement? = null, val rev: Int = 0, val at: Long = 0, val kind: String = "", val error: String = "")

/** A panel's data by source name, and its saved store (what it keeps with store.set). */
@Serializable data class WidgetData(val data: Map<String, WidgetValue> = emptyMap(), val store: JsonObject? = null)

@Serializable data class WidgetVersion(val version: Int, val at: Long = 0, val bytes: Long = 0)

// ─── Browsers (the server's own, driven by Claude) ───

/** A browser the server runs (browser_open). [on]: server or desktop. */
@Serializable data class ServerBrowser(val key: String, val alias: String = "", val name: String = "", val url: String = "", val width: Int = 0, val height: Int = 0, val status: String = "", val createdAt: Long = 0, val lastActivityAt: Long = 0, val consoleLogCount: Int = 0, val on: String = "")

/** Where a new browser can run now. */
@Serializable data class BrowserPlaces(val server: Boolean = false, val desktop: Boolean = false)

@Serializable data class ServerBrowsers(val browsers: List<ServerBrowser> = emptyList(), val places: BrowserPlaces? = null)

// ─── Outside collaborators (Info tab) ───

/** One invitation: a person given one project, in their own worktree and branch, until [expiresAt]. */
@Serializable
data class CollabInvite(
    val id: String,
    val name: String = "",
    val project: String = "",
    val telegram: String = "",
    val worktreePath: String = "",
    val branch: String = "",
    val baseCommit: String = "",
    val createdAt: Long = 0,
    val expiresAt: Long = 0,
    val ttlHours: Double = 0.0,
    /** Tokens, for the person across their invitations; 0 = no limit. */
    val tokenLimit: Long = 0,
    val tokensUsed: Long = 0,
    val revoked: Boolean = false,
    val seenAt: Long = 0,
    val hasPasscode: Boolean = false,
    val expired: Boolean = false,
    val allowedDomains: List<String> = emptyList(),
)

/** Can collaborators be given a sandboxed shell on this machine? */
@Serializable data class SandboxStatus(val ok: Boolean = false, val supported: Boolean = false, val works: Boolean = false, val problems: List<String> = emptyList(), val canAutoFix: Boolean = false, val testError: String = "")

@Serializable data class Collaborators(val collaborators: List<CollabInvite> = emptyList(), val staleWorktrees: List<JsonElement> = emptyList(), val sandbox: SandboxStatus = SandboxStatus())

/** What creating access answers: the new invitations and, once, the passcode to hand over. */
@Serializable data class CollabCreated(val collaborators: List<CollabInvite> = emptyList(), val passcode: String = "", val failed: List<JsonElement> = emptyList(), val needsGit: List<String> = emptyList())

/** GET /api/tunnel: the server's own tunnel and each service's. */
@Serializable data class TunnelEntry(val port: Int = 0, val url: String? = null, val status: String = "", val pid: Int? = null)
@Serializable data class Tunnels(val tunnels: Map<String, TunnelEntry> = emptyMap(), val tunnelUrl: String? = null, val named: Boolean = false, val target: String = "")
