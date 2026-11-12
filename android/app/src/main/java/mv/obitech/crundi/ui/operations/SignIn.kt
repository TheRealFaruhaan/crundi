package mv.obitech.crundi.ui.operations

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import mv.obitech.crundi.api.ApiException
import mv.obitech.crundi.api.AuthApi
import mv.obitech.crundi.api.AuthConfig
import mv.obitech.crundi.api.OAuthView
import mv.obitech.crundi.api.PasswordSet
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiDialog
import mv.obitech.crundi.ui.components.CrundiSwitchRow
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.ErrorState
import mv.obitech.crundi.ui.components.LoadingState
import mv.obitech.crundi.ui.components.errorText
import mv.obitech.crundi.ui.nav.LocalNavigator
import mv.obitech.crundi.ui.shell.LocalAppServices
import mv.obitech.crundi.ui.shell.LocalHost
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/*
 * Settings > Sign-in, as the web's Settings > Access > Sign-in: Telegram, a
 * password with an authenticator code, Google and GitHub. Any one is enough,
 * any mix can be on, and the server refuses to turn off the last one (its
 * words are shown). Google and GitHub must pass a Test before they can be
 * turned on: from the phone the provider opens in the browser, and the page
 * waits for the server to say how it went.
 *
 * An older server without GET /api/auth/config: "Update the server to use
 * this". One with it but without Google and GitHub: those are left out.
 */

/** A provider as this page shows it. */
data class OAuthProvider(val key: String, val label: String, val account: String, val placeholder: String, val where: String, val link: String)

val OAUTH_PROVIDERS = listOf(
    OAuthProvider(
        "google", "Google", "Allowed Google email", "you@gmail.com",
        "Google Cloud console, APIs and Services, Credentials: create an OAuth client ID of type Web application, and add this as an authorised redirect URI.",
        "https://console.cloud.google.com/apis/credentials",
    ),
    OAuthProvider(
        "github", "GitHub", "Allowed GitHub username", "your-username",
        "GitHub, Settings, Developer settings, OAuth Apps, New OAuth App: use this as the authorisation callback URL.",
        "https://github.com/settings/applications/new",
    ),
)

/** The rules the page shows, kept apart so they can be tested. */
object SignInRules {
    /** The header's word for a method: On, Off or Not set up. */
    fun state(c: AuthConfig, method: String): String = when {
        method == "telegram" && c.telegramBot?.restartRequired == true -> "Restart needed"
        c.isOn(method) -> "On"
        method == "telegram" && c.telegramBot?.tokenSet == false -> "Not set up"
        method == "telegram" && !c.telegramAvailable -> "Not set up"
        method == "password" && !c.passwordSet -> "Not set up"
        method in AuthApi.OAUTH && c.oauth?.of(method)?.configured != true -> "Not set up"
        else -> "Off"
    }

    /** The switch can be moved: on (turning it off is the server's to refuse), or set up (and, for Google and GitHub, tested). */
    fun canSwitch(c: AuthConfig, method: String): Boolean = c.isOn(method) || when (method) {
        "telegram" -> c.telegramAvailable
        "password" -> c.passwordSet
        else -> c.oauth?.of(method)?.let { it.configured && it.verified } == true
    }

    /** What a provider's section says about itself. */
    fun oauthStatus(p: OAuthProvider, o: OAuthView): String = when {
        !o.configured -> "Not set up. Create an OAuth app with ${p.label}, then fill this in, Save and Test."
        !o.verified -> "Saved, not tested since the last change. Run Test before turning it on."
        else -> "Tested: signed in as ${o.verifiedAs.ifEmpty { o.allowed }}." + if (o.idPinned) " Pinned to that account's id, so a renamed username cannot take it over." else ""
    }

    /** What the Telegram section says about the bot (a newer server), as the web's. */
    fun telegramStatus(t: mv.obitech.crundi.api.TelegramBot): String = when {
        !t.tokenSet -> "Not set up. Create a bot with @BotFather, then enter its token and your Telegram username, and Save."
        t.restartRequired -> "Saved. Restart Crundi to start the bot with these settings; it can be turned on after that."
        t.status == "connected" -> "Bot " + (if (t.botUsername.isNotEmpty()) "@${t.botUsername} " else "") + "is connected. Sign in with the Telegram account you set here."
        else -> "The bot is offline. Check the token, and that no other server is using the same bot."
    }

    /** The callback URL is the computer's own: it only works when signing in there. */
    fun isLocal(url: String) = Regex("^http://localhost[:/]").containsMatchIn(url)
}

/** A Test under way or done, for one provider. */
data class OAuthTest(val waiting: Boolean = false, val text: String = "", val bad: Boolean = false)

data class SignInState(
    /** Ready(null): the server has no sign-in settings to give (older). */
    val config: Load<AuthConfig?> = Load.Loading,
    val busy: String? = null,
    val note: Actions.Note? = null,
    /** A password was just set: the new authenticator secret, shown once. */
    val enrol: PasswordSet? = null,
    val tests: Map<String, OAuthTest> = emptyMap(),
)

class SignInEvents(
    val onToggle: (String, Boolean) -> Unit = { _, _ -> },
    val onSetPassword: (String) -> Unit = {},
    val onSaveOAuth: (provider: String, clientId: String, secret: String, allowed: String) -> Unit = { _, _, _, _ -> },
    val onTest: (String) -> Unit = {},
    val onCancelTest: (String) -> Unit = {},
    val onRemove: (String) -> Unit = {},
    val onCopy: (String) -> Unit = {},
    val onOpen: (String) -> Unit = {},
    /** Hand the new secret to an authenticator app on this phone (otpauth://). */
    val onAuthenticator: (String) -> Unit = {},
    val onConfiguration: () -> Unit = {},
    /** The bot token ("" keeps the saved one) and the allowed Telegram username. */
    val onSaveTelegram: (String, String) -> Unit = { _, _ -> },
    val onClearTelegram: () -> Unit = {},
    /** Settings > Server and Claude Code, where the restart is (with its confirmation). */
    val onRestartPage: () -> Unit = {},
    val onCloseNote: () -> Unit = {},
    val onBack: () -> Unit = {},
)

@Composable
fun SignInRoute() {
    val api = LocalAppServices.current.api
    val nav = LocalNavigator.current
    val host = LocalHost.current
    val ctx = LocalContext.current
    val clipboard = LocalClipboardManager.current
    val actions = rememberActions()
    val scope = rememberCoroutineScope()
    val config = rememberLoad { api.auth.config() }
    var enrol by remember { mutableStateOf<PasswordSet?>(null) }
    val tests = remember { mutableStateMapOf<String, OAuthTest>() }
    val jobs = remember { HashMap<String, Job>() }
    val label = { m: String -> mapOf("telegram" to "Telegram", "password" to "Password", "google" to "Google", "github" to "GitHub")[m] ?: m }

    SignInScreen(
        SignInState(config.value, actions.busy, actions.note, enrol, tests.toMap()),
        SignInEvents(
            onToggle = { m, on -> actions.run("toggle-$m", "${label(m)} sign-in ${if (on) "on" else "off"}", config::reload) { api.auth.setEnabled(m, on) } },
            onSetPassword = { pw ->
                if (pw.isEmpty()) actions.note = Actions.Note("Type a password first", true)
                else actions.run("password", "Password set. Add the new code to your authenticator.", config::reload) { enrol = api.auth.setPassword(pw) }
            },
            onSaveOAuth = { p, id, secret, allowed -> actions.run("save-$p", "Saved. Run Test to check it works.", config::reload) { api.auth.saveOAuth(p, id, secret, allowed) } },
            onTest = { p ->
                jobs[p]?.cancel()
                jobs[p] = scope.launch {
                    tests[p] = OAuthTest(waiting = true, text = "Starting the test…")
                    val start = try { api.auth.startTest(p) } catch (e: ApiException) { tests[p] = OAuthTest(text = errorText(e), bad = true); return@launch }
                    host.openExternal(start.url)
                    tests[p] = OAuthTest(waiting = true, text = "Waiting for the sign-in in the browser. Sign in with ${label(p)} there, then come back.")
                    val until = System.currentTimeMillis() + 10 * 60_000
                    while (System.currentTimeMillis() < until) {
                        delay(1500)
                        val r = try { api.auth.poll(start.pollId) } catch (_: ApiException) { continue }
                        when (r.status) {
                            "done" -> {
                                tests[p] = OAuthTest(text = "${label(p)} sign-in works" + (if (r.account.isNotEmpty()) " (${r.account})." else ".") + " It can be turned on now.")
                                config.reload(); return@launch
                            }
                            "failed" -> { tests[p] = OAuthTest(text = r.error.ifEmpty { "The test did not pass" } + (if (r.account.isNotEmpty()) " (signed in as ${r.account})" else ""), bad = true); return@launch }
                            "expired" -> { tests[p] = OAuthTest(text = "The test expired. Try again.", bad = true); return@launch }
                        }
                    }
                    tests[p] = OAuthTest(text = "The test timed out.", bad = true)
                }
            },
            onCancelTest = { p -> jobs.remove(p)?.cancel(); tests.remove(p) },
            onRemove = { p -> actions.run("remove-$p", "Removed", config::reload) { api.auth.clearOAuth(p) } },
            onCopy = { t -> clipboard.setText(AnnotatedString(t)); actions.note = Actions.Note("Copied", false) },
            onOpen = { host.openExternal(it) },
            onAuthenticator = { uri ->
                val ok = runCatching { ctx.startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse(uri)).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)) }.isSuccess
                if (!ok) actions.note = Actions.Note("No authenticator app on this phone opens it. Type the code in by hand.", true)
            },
            onConfiguration = { nav.to(SettingsPage.Config.route) },
            onSaveTelegram = { token, user ->
                actions.run("tg-save", after = config::reload) {
                    val r = api.auth.saveTelegram(token, user)
                    actions.note = Actions.Note(if (r.restartRequired) "Saved. Restart Crundi to start the bot with these settings." else "Saved", false)
                }
            },
            onClearTelegram = { actions.run("tg-clear", "Telegram bot removed. Restart Crundi to stop it.", config::reload) { api.auth.clearTelegram() } },
            onRestartPage = { nav.to(SettingsPage.Server.route) },
            onCloseNote = actions::clear,
            onBack = { nav.back() },
        ),
    )
}

/** [open]: sections unfolded at first (screenshots); otherwise all start folded. */
@Composable
fun SignInScreen(state: SignInState, events: SignInEvents = SignInEvents(), open: Set<String> = emptySet()) {
    var remove by remember { mutableStateOf<OAuthProvider?>(null) }
    var removeTelegram by remember { mutableStateOf(false) }
    SettingsPageFrame(SettingsPage.SignIn, state.note, events.onCloseNote, events.onBack) {
        when (val l = state.config) {
            Load.Loading -> LoadingState()
            is Load.Failed -> ErrorState(l.message)
            is Load.Ready -> {
                val c = l.value
                if (c == null) mv.obitech.crundi.ui.components.EmptyState(icon = "key", title = "Not supported by this server", body = "This server's sign-in settings cannot be changed from the app. Update the server to use this.")
                else Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Dimens.ScreenPad), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Text("Any one of these is enough, any mix can be on, and one must always stay on.", style = CrundiType.Small, modifier = Modifier.padding(horizontal = 2.dp))
                    MethodFold(c, "telegram", "Telegram", open) {
                        val tb = c.telegramBot
                        if (tb != null) TelegramPart(tb, c, state, events) { removeTelegram = true }
                        else {
                            // An older server: the bot is still set in Configuration.
                            Switch(c, "telegram", "Sign in with Telegram", if (c.telegramAvailable) "Sign in with the Telegram account set in Configuration." else "Needs a bot token and username in Configuration before it can be turned on.", state, events)
                            CrundiButton("Open Configuration", events.onConfiguration, icon = "settings", modifier = Modifier.padding(top = 4.dp))
                        }
                    }
                    MethodFold(c, "password", "Password + authenticator code", open) { PasswordPart(c, state, events) }
                    c.oauth?.let { o ->
                        for (p in OAUTH_PROVIDERS) MethodFold(c, p.key, p.label, open) { OAuthPart(p, o.of(p.key), c, state, events) { remove = p } }
                    }
                }
            }
        }
    }
    if (removeTelegram) CrundiDialog("Remove the Telegram bot?", { removeTelegram = false },
        message = "Telegram sign-in and the Telegram notification channel both stop after the next restart. To use them again, enter the bot token again.",
        confirm = "Remove", danger = true, onConfirm = { removeTelegram = false; events.onClearTelegram() })
    remove?.let { p ->
        CrundiDialog("Remove ${p.label} sign-in?", { remove = null },
            message = "This forgets its client ID and secret. To use it again, fill them in and Test it again.",
            confirm = "Remove", danger = true, onConfirm = { remove = null; events.onRemove(p.key) })
    }
}

@Composable
private fun MethodFold(c: AuthConfig, method: String, title: String, open: Set<String>, content: @Composable () -> Unit) {
    val s = SignInRules.state(c, method)
    FoldSection(method, title, s, open, summaryColor = when (s) { "On" -> CrundiColors.Green; "Not set up" -> CrundiColors.TextMuted; "Restart needed" -> CrundiColors.Yellow; else -> null }) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { content() }
    }
}

@Composable
private fun Switch(c: AuthConfig, method: String, label: String, detail: String, state: SignInState, events: SignInEvents) {
    val can = SignInRules.canSwitch(c, method) && state.busy != "toggle-$method"
    CrundiSwitchRow(label, c.isOn(method), { events.onToggle(method, it) }, Modifier.padding(horizontal = 0.dp), detail = detail, enabled = can)
}

/** Telegram on a newer server: the bot (shared with the notification channel), its account, and the switch. */
@Composable
private fun TelegramPart(tb: mv.obitech.crundi.api.TelegramBot, c: AuthConfig, state: SignInState, events: SignInEvents, onRemove: () -> Unit) {
    Switch(c, "telegram", "Sign in with Telegram", SignInRules.telegramStatus(tb), state, events)
    Text("In Telegram, message @BotFather, send /newbot and copy the token it gives you. The same bot sends the Telegram notification channel.", style = CrundiType.Small)
    CrundiButton("Open @BotFather", { events.onOpen("https://t.me/BotFather") }, icon = "external-link")
    var token by remember(tb.tokenSet) { mutableStateOf("") }
    var user by remember(tb.allowedUsername) { mutableStateOf(tb.allowedUsername) }
    Text("Bot token", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary), modifier = Modifier.padding(top = 4.dp))
    CrundiTextField(token, { token = it }, Modifier.fillMaxWidth(), placeholder = if (tb.tokenSet) "Saved (leave blank to keep)" else "123456:ABC-DEF...", mono = true, password = true)
    Text("Allowed Telegram username", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary))
    CrundiTextField(user, { user = it }, Modifier.fillMaxWidth(), placeholder = "your-username")
    @OptIn(ExperimentalLayoutApi::class)
    FlowRow(Modifier.padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        CrundiButton("Save", { events.onSaveTelegram(token, user); token = "" }, kind = ButtonKind.Primary, icon = "check", enabled = state.busy != "tg-save")
        if (tb.restartRequired) CrundiButton("Restart now…", events.onRestartPage, icon = "rotate-ccw")
        if (tb.tokenSet) CrundiButton("Remove", onRemove, kind = ButtonKind.Danger, icon = "trash", enabled = state.busy != "tg-clear")
    }
}

@Composable
private fun PasswordPart(c: AuthConfig, state: SignInState, events: SignInEvents) {
    Switch(
        c, "password", "Sign in with a password",
        if (c.passwordSet) "A password and a 6-digit code. Setting a new password issues a new code source." else "Not set up yet. Choose a password below to turn it on.",
        state, events,
    )
    var pw by remember { mutableStateOf("") }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        CrundiTextField(pw, { pw = it }, Modifier.weight(1f), placeholder = if (c.passwordSet) "New password (12+ characters)" else "Choose a password (12+ characters)", icon = "key", password = true)
        CrundiButton(if (c.passwordSet) "Change" else "Set", { events.onSetPassword(pw); pw = "" }, kind = ButtonKind.Primary, enabled = state.busy != "password")
    }
    state.enrol?.takeIf { it.totpSecret.isNotEmpty() }?.let { e ->
        Column(
            Modifier.fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.AccentDim).border(Dimens.Hairline, CrundiColors.Accent, Dimens.Shape).padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text("Add this to your authenticator app", style = CrundiType.Heading)
            Text("Shown once. Without it you will not be able to sign in with the new password.", style = CrundiType.Small)
            Text(e.totpSecret, style = CrundiType.Code, modifier = Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary).padding(10.dp))
            @OptIn(ExperimentalLayoutApi::class)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                CrundiButton("Copy", { events.onCopy(e.totpSecret) }, icon = "copy")
                if (e.uri.startsWith("otpauth://")) CrundiButton("Open in an authenticator app", { events.onAuthenticator(e.uri) }, icon = "external-link")
            }
        }
    }
}

@Composable
private fun OAuthPart(p: OAuthProvider, o: OAuthView, c: AuthConfig, state: SignInState, events: SignInEvents, onRemove: () -> Unit) {
    Switch(c, p.key, "Sign in with ${p.label}", SignInRules.oauthStatus(p, o), state, events)
    val url = c.callbackUrls[p.key].orEmpty()
    Text("Callback URL to give ${p.label}", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary))
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            url.ifEmpty { "Unknown until this server has a public address" }, style = CrundiType.Code,
            modifier = Modifier.weight(1f).clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary).border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).padding(10.dp),
        )
        if (url.isNotEmpty()) CrundiButton("Copy", { events.onCopy(url) }, icon = "copy")
    }
    Text(p.where + if (SignInRules.isLocal(url)) " This is a local address: it only works when signing in on that computer." else "", style = CrundiType.Small)
    CrundiButton("Open ${p.label}'s page", { events.onOpen(p.link) }, icon = "external-link")
    var id by remember(o.clientId) { mutableStateOf(o.clientId) }
    var secret by remember(o.secretSet) { mutableStateOf("") }
    var allowed by remember(o.allowed) { mutableStateOf(o.allowed) }
    Text("Client ID", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary), modifier = Modifier.padding(top = 4.dp))
    CrundiTextField(id, { id = it }, Modifier.fillMaxWidth(), mono = true)
    Text("Client secret", style = CrundiType.Small.copy(color = CrundiColors.TextSecondary))
    CrundiTextField(secret, { secret = it }, Modifier.fillMaxWidth(), placeholder = if (o.secretSet) "Saved (leave blank to keep)" else "Not set", mono = true, password = true)
    Text(p.account, style = CrundiType.Small.copy(color = CrundiColors.TextSecondary))
    CrundiTextField(allowed, { allowed = it }, Modifier.fillMaxWidth(), placeholder = p.placeholder)
    val test = state.tests[p.key]
    @OptIn(ExperimentalLayoutApi::class)
    FlowRow(Modifier.padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        CrundiButton("Save", { events.onSaveOAuth(p.key, id, secret, allowed); secret = "" }, kind = ButtonKind.Primary, icon = "check", enabled = state.busy != "save-${p.key}")
        if (test?.waiting == true) CrundiButton("Stop waiting", { events.onCancelTest(p.key) }, icon = "x")
        else CrundiButton("Test", { events.onTest(p.key) }, icon = "play", enabled = o.configured)
        if (o.configured) CrundiButton("Remove", onRemove, kind = ButtonKind.Danger, icon = "trash", enabled = state.busy != "remove-${p.key}")
    }
    if (test != null && test.text.isNotEmpty()) Text(test.text, style = CrundiType.Small.copy(color = if (test.bad) CrundiColors.Red else if (test.waiting) CrundiColors.TextSecondary else CrundiColors.Green))
    if (!o.configured) Text("Save first, then Test: it opens ${p.label} in the browser and signs in once with what is saved.", style = CrundiType.Small)
}
