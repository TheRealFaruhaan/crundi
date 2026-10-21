package mv.obitech.crundi.ui.browser

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.model.BrowserPlaces
import mv.obitech.crundi.api.model.ServerBrowser
import mv.obitech.crundi.ui.components.BadgeKind
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiCard
import mv.obitech.crundi.ui.components.CrundiChip
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiDivider
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.EmptyState
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.ListRow
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.SectionHeader
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens
import mv.obitech.crundi.ui.workspace.rememberToast

/**
 * The Browsers tab, in two parts:
 * - **On this phone**: the interactive browser ([LiveBrowser]), the same as a Browser pane.
 * - **On the server**: the desktop's Browsers panel. These are the browsers Claude drives with
 *   browser_open, on this server or on a desktop app. Open one at a device size, send it to a URL,
 *   take a screenshot, or close it.
 */

/** The desktop's DEVICE_PROFILES (the server browsers' sizes), grouped as its picker groups them. */
data class DeviceProfile(val name: String, val w: Int, val h: Int, val group: String)

val DEVICE_PROFILES = listOf(
    DeviceProfile("Desktop 1280×720", 1280, 720, "Desktop"),
    DeviceProfile("Full HD 1920×1080", 1920, 1080, "Desktop"),
    DeviceProfile("iPhone SE", 375, 667, "Apple"),
    DeviceProfile("iPhone X", 375, 812, "Apple"),
    DeviceProfile("iPhone 12 Pro", 390, 844, "Apple"),
    DeviceProfile("iPhone 14 Pro", 393, 852, "Apple"),
    DeviceProfile("iPhone 15 Pro", 393, 852, "Apple"),
    DeviceProfile("iPhone 16 Pro", 393, 852, "Apple"),
    DeviceProfile("iPhone 16 Pro Max", 430, 932, "Apple"),
    DeviceProfile("iPad Air", 820, 1180, "Apple"),
    DeviceProfile("iPad Pro", 1024, 1366, "Apple"),
    DeviceProfile("Pixel 7", 412, 915, "Google"),
    DeviceProfile("Pixel 9 Pro", 430, 932, "Google"),
    DeviceProfile("Galaxy S24 Ultra", 412, 915, "Samsung"),
    DeviceProfile("Galaxy Z Fold 5", 344, 882, "Samsung"),
    DeviceProfile("Surface Pro 9", 1200, 1800, "Microsoft"),
)
val DEFAULT_PROFILE = DEVICE_PROFILES.indexOfFirst { it.name == "iPhone 16 Pro" }

/** A server browser's screenshot: being taken, taken, or failed. */
data class Shot(val image: ImageBitmap? = null, val loading: Boolean = false, val error: String? = null)

enum class BrowsersSection { Phone, Server }

data class ServerBrowsersState(
    val project: String?,
    val loading: Boolean = true,
    val error: String? = null,
    val browsers: List<ServerBrowser> = emptyList(),
    val places: BrowserPlaces? = null,
    val device: Int = DEFAULT_PROFILE,
    /** "", "server" or "desktop" (a choice only when both can run one). */
    val on: String = "",
    val newUrl: String = "http://localhost:3000",
    /** URLs being typed into each card, by key. */
    val urls: Map<String, String> = emptyMap(),
    val shots: Map<String, Shot> = emptyMap(),
    val opening: Boolean = false,
)

/** "localhost:3000" gets http://, a bare domain https://, as the web's browserNavigate. */
fun serverTarget(typed: String): String? {
    val v = typed.trim()
    if (v.isEmpty()) return null
    if (Regex("^https?://", RegexOption.IGNORE_CASE).containsMatchIn(v) || v.startsWith("about:")) return v
    return (if (Regex("^localhost(:|$|/)", RegexOption.IGNORE_CASE).containsMatchIn(v)) "http://" else "https://") + v
}

/** Where a server browser runs, in the web's words. */
fun whereWords(b: ServerBrowser) = when (b.on) { "server" -> "on the server"; "desktop" -> "on the desktop app"; else -> "open" }

/** The section switch at the top of the Browsers tab. */
@Composable
fun BrowsersSections(section: BrowsersSection, onSection: (BrowsersSection) -> Unit) {
    Column(Modifier.fillMaxWidth().background(CrundiColors.BgPrimary)) {
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiChip("On this phone", section == BrowsersSection.Phone, { onSection(BrowsersSection.Phone) }, icon = "smartphone")
            CrundiChip("On the server", section == BrowsersSection.Server, { onSection(BrowsersSection.Server) }, icon = "server")
        }
        CrundiDivider()
    }
}

/** The server's browsers: the toolbar to open one, then a card each. */
@Composable
fun ServerBrowsersScreen(
    state: ServerBrowsersState,
    onDevice: (Int) -> Unit = {},
    onOn: (String) -> Unit = {},
    onNewUrl: (String) -> Unit = {},
    onOpen: () -> Unit = {},
    onUrl: (String, String) -> Unit = { _, _ -> },
    onGo: (ServerBrowser) -> Unit = {},
    onShot: (ServerBrowser) -> Unit = {},
    onClose: (ServerBrowser) -> Unit = {},
    onRetry: () -> Unit = {},
    pickingDevice: Boolean = false,
) {
    var picking by remember { mutableStateOf(pickingDevice) }
    var closing by remember { mutableStateOf<ServerBrowser?>(null) }
    LazyColumn(Modifier.fillMaxSize().background(CrundiColors.BgPrimary), contentPadding = androidx.compose.foundation.layout.PaddingValues(12.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        item("toolbar") {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                    val d = DEVICE_PROFILES.getOrElse(state.device) { DEVICE_PROFILES[0] }
                    PickBox(d.name + "  (${d.w}×${d.h})", if (d.group == "Desktop") "server" else "smartphone", Modifier.weight(1f)) { picking = true }
                    val pl = state.places
                    if (pl != null && pl.server && pl.desktop) {
                        PickBox(when (state.on) { "server" -> "Run on this server"; "desktop" -> "Run on the desktop app"; else -> "Run on: default" }, null, Modifier.weight(1f)) {
                            onOn(when (state.on) { "" -> "server"; "server" -> "desktop"; else -> "" })
                        }
                    }
                }
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                    CrundiTextField(
                        state.newUrl, onNewUrl, Modifier.weight(1f), placeholder = "URL to open", mono = true,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go, autoCorrectEnabled = false),
                        keyboardActions = KeyboardActions(onGo = { onOpen() }),
                    )
                    CrundiButton("Open", onOpen, kind = ButtonKind.Primary, icon = "plus", enabled = !state.opening && state.project != null)
                }
            }
        }
        when {
            state.loading && state.browsers.isEmpty() -> item("loading") { LoadingState(Modifier.height(240.dp), text = "Loading browsers") }
            state.error != null && state.browsers.isEmpty() -> item("error") { Box(Modifier.height(300.dp)) { ErrorState(state.error, onRetry = onRetry) } }
            state.browsers.isEmpty() -> item("empty") {
                Box(Modifier.height(280.dp)) { EmptyState("globe", "No browser instances open", body = "Open one above at a device size, or ask Claude to use browser_open.") }
            }
            else -> items(state.browsers, key = { it.key }) { b ->
                ServerBrowserCard(
                    b, state.urls[b.key] ?: b.url, state.shots[b.key],
                    onUrl = { onUrl(b.key, it) }, onGo = { onGo(b) }, onShot = { onShot(b) }, onClose = { closing = b },
                )
            }
        }
    }
    if (picking) {
        CrundiDialog("Device", onDismiss = { picking = false }) {
            LazyColumn(Modifier.height(420.dp)) {
                var group = ""
                DEVICE_PROFILES.forEachIndexed { i, d ->
                    if (d.group != group) { group = d.group; val g = group; item("g-$g") { SectionHeader(g) } }
                    item("d-$i") { ListRow(d.name, subtitle = "${d.w}×${d.h}", active = i == state.device, onClick = { picking = false; onDevice(i) }) }
                }
            }
        }
    }
    closing?.let { b ->
        CrundiDialog(
            "Close this browser?", onDismiss = { closing = null },
            message = "${b.name.ifEmpty { b.key }} (${whereWords(b)}) closes. Claude can open it again with browser_open.",
            confirm = "Close browser", danger = true, onConfirm = { closing = null; onClose(b) },
        )
    }
}

@Composable
private fun PickBox(text: String, icon: String?, modifier: Modifier, onClick: () -> Unit) {
    Row(
        modifier.height(40.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgTertiary).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm)
            .clickable(onClick = onClick).padding(horizontal = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (icon != null) CrundiIcon(icon, size = 14.dp, tint = CrundiColors.TextSecondary)
        Text(text, style = CrundiType.Small.copy(color = CrundiColors.TextPrimary), maxLines = 1, modifier = Modifier.weight(1f))
        CrundiIcon("chevron-down", size = 12.dp, tint = CrundiColors.TextMuted)
    }
}

/** One .svc-card: name and where it runs, its URL with Go, the screenshot (tap to take one), Screenshot and Close. */
@Composable
fun ServerBrowserCard(b: ServerBrowser, url: String, shot: Shot?, onUrl: (String) -> Unit, onGo: () -> Unit, onShot: () -> Unit, onClose: () -> Unit) {
    CrundiCard(Modifier.fillMaxWidth()) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(b.name.ifEmpty { b.key.ifEmpty { "browser" } }, style = CrundiType.Heading, maxLines = 1, modifier = Modifier.weight(1f))
            StatusBadge(whereWords(b), BadgeKind.Good)
        }
        Text(
            listOfNotNull(b.alias.ifEmpty { null }, if (b.width > 0) "${b.width}×${b.height}" else null, if (b.consoleLogCount > 0) "${b.consoleLogCount} console lines" else null).joinToString(" · "),
            style = CrundiType.Meta, modifier = Modifier.padding(top = 2.dp, bottom = 8.dp),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            CrundiTextField(
                url, onUrl, Modifier.weight(1f), placeholder = "URL", mono = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go, autoCorrectEnabled = false),
                keyboardActions = KeyboardActions(onGo = { onGo() }),
            )
            CrundiButton("Go", onGo)
        }
        Box(
            Modifier.padding(vertical = 10.dp).fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary)
                .border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).clickable(onClick = onShot),
            contentAlignment = Alignment.Center,
        ) {
            when {
                shot?.image != null -> Image(shot.image, "What ${b.name} shows", Modifier.fillMaxWidth(), contentScale = ContentScale.FillWidth)
                shot?.loading == true -> Text("Loading...", style = CrundiType.Small.copy(color = CrundiColors.TextMuted), modifier = Modifier.padding(20.dp))
                shot?.error != null -> Text(shot.error, style = CrundiType.Small.copy(color = CrundiColors.Red), modifier = Modifier.padding(20.dp))
                else -> Text("Tap to take a screenshot", style = CrundiType.Small.copy(color = CrundiColors.TextMuted), modifier = Modifier.padding(20.dp))
            }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CrundiButton("Screenshot", onShot, icon = "image", enabled = shot?.loading != true)
            CrundiButton("Close", onClose, kind = ButtonKind.Danger, icon = "x")
        }
    }
}

/** The Browsers tab, live. */
@Composable
fun BrowsersRoute() {
    var section by rememberSaveable { mutableStateOf(BrowsersSection.Phone) }
    Column(Modifier.fillMaxSize()) {
        BrowsersSections(section) { section = it }
        Box(Modifier.weight(1f).fillMaxWidth()) {
            when (section) {
                BrowsersSection.Phone -> LiveBrowser("tab")
                BrowsersSection.Server -> ServerBrowsersRoute()
            }
        }
    }
}

@Composable
private fun ServerBrowsersRoute() {
    val services = LocalAppServices.current
    val api = services.api
    val toast = rememberToast()
    val scope = rememberCoroutineScope()
    val shell by services.shell.state.collectAsState()
    val project = shell.currentProject
    var state by remember(project) { mutableStateOf(ServerBrowsersState(project)) }
    var reload by remember { mutableIntStateOf(0) }
    LaunchedEffect(project, reload) {
        runCatching { api.browsers.list(project.orEmpty()) }
            .onSuccess { state = state.copy(loading = false, error = null, browsers = it.browsers, places = it.places) }
            .onFailure { state = state.copy(loading = false, error = errorText(it)) }
    }
    fun later() = scope.launch { delay(500); reload++ }
    ServerBrowsersScreen(
        state,
        onDevice = { state = state.copy(device = it) },
        onOn = { state = state.copy(on = it) },
        onNewUrl = { state = state.copy(newUrl = it) },
        onOpen = {
            val p = project
            val url = state.newUrl.trim()
            if (p == null) return@ServerBrowsersScreen
            if (url.isEmpty()) { toast("Enter a URL first"); return@ServerBrowsersScreen }
            val prof = DEVICE_PROFILES.getOrElse(state.device) { DEVICE_PROFILES[0] }
            state = state.copy(opening = true)
            scope.launch {
                runCatching { api.browsers.open(p, url, name = "browser-" + System.currentTimeMillis(), width = prof.w, height = prof.h, on = state.on.ifEmpty { null }) }
                    .onSuccess { toast("Browser opened (${prof.name})") }
                    .onFailure { toast("Could not open a browser: " + errorText(it)) }
                state = state.copy(opening = false)
                later()
            }
        },
        onUrl = { k, v -> state = state.copy(urls = state.urls + (k to v)) },
        onGo = { b ->
            val url = serverTarget(state.urls[b.key] ?: b.url) ?: return@ServerBrowsersScreen
            scope.launch { runCatching { api.browsers.navigate(b.key, url) }.onFailure { toast("Navigate failed: " + errorText(it)) }; later() }
        },
        onShot = { b ->
            state = state.copy(shots = state.shots + (b.key to Shot(loading = true)))
            scope.launch {
                val shot = runCatching { api.browsers.screenshot(b.key) }.fold(
                    { bytes -> BitmapFactory.decodeByteArray(bytes, 0, bytes.size)?.let { Shot(it.asImageBitmap()) } ?: Shot(error = "The picture could not be read.") },
                    { Shot(error = "Screenshot failed: " + errorText(it)) },
                )
                state = state.copy(shots = state.shots + (b.key to shot))
            }
        },
        onClose = { b ->
            scope.launch { runCatching { api.browsers.close(b.key) }.onSuccess { toast("Browser closed") }.onFailure { toast("Could not close it: " + errorText(it)) }; later() }
        },
        onRetry = { state = state.copy(loading = true, error = null); reload++ },
    )
}
