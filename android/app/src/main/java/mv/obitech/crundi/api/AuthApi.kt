package mv.obitech.crundi.api

import kotlinx.serialization.Serializable

/**
 * How the owner signs in to the server (Settings > Sign-in, as on the web):
 * Telegram, a password with an authenticator code, Google and GitHub.
 *
 *   config()     what is set up and on (GET /api/auth/config); null from a
 *                server that predates it (404)
 *   setEnabled() turn a method on or off; the server refuses to turn off the
 *                last one, and to turn Google or GitHub on before a passed Test
 *   startTest()  a Google or GitHub sign-in with what is saved, in a browser;
 *                [poll] says how it went
 */
class AuthApi internal constructor(private val rpc: Rpc) {

    suspend fun config(): AuthConfig? = try {
        rpc.read(rpc.get("/api/auth/config"), null)
    } catch (e: ApiException) {
        if (e.status == 404) null else throw e
    }

    /** "telegram", "password", "google" or "github". Fails with the server's words (the last method, not tested yet). */
    suspend fun setEnabled(method: String, on: Boolean) {
        require(method in METHODS) { "Not a sign-in method: $method" }
        rpc.post("/api/auth/config", mapOf("action" to if (on) "enable" else "disable", "method" to method))
    }

    /** A new password (12 characters or more). Always mints a new authenticator secret: shown once. */
    suspend fun setPassword(password: String): PasswordSet =
        rpc.read(rpc.post("/api/auth/config", mapOf("action" to "set-password", "password" to password)), null)

    /** The client ID, secret and allowed account. A blank secret keeps the saved one; any change needs a new Test. */
    suspend fun saveOAuth(provider: String, clientId: String, clientSecret: String, allowed: String) {
        require(provider in OAUTH) { "Not an OAuth provider: $provider" }
        rpc.post("/api/auth/config", mapOf("action" to "oauth-save", "provider" to provider, "clientId" to clientId.trim(), "clientSecret" to clientSecret.trim(), "allowed" to allowed.trim()))
    }

    /** Forget a provider altogether (refused while it is the only way in). */
    suspend fun clearOAuth(provider: String) {
        require(provider in OAUTH) { "Not an OAuth provider: $provider" }
        rpc.post("/api/auth/config", mapOf("action" to "oauth-clear", "provider" to provider))
    }

    /**
     * A test sign-in with what is saved. Open [OAuthStart.url] in a browser;
     * the server waits for the provider to come back, and [poll] reports it.
     */
    suspend fun startTest(provider: String): OAuthStart {
        require(provider in OAUTH) { "Not an OAuth provider: $provider" }
        return rpc.read(rpc.post("/api/auth/oauth/start", mapOf("provider" to provider, "purpose" to "test", "desktop" to true)), null)
    }

    /**
     * The Telegram bot, shared by the sign-in method and the notification
     * channel: a token ("" keeps the saved one) and the Telegram username
     * allowed to sign in. Written to the server's .env: it applies after a
     * restart ([TelegramSaved.restartRequired]). Only on a server that sends
     * [AuthConfig.telegramBot].
     */
    suspend fun saveTelegram(botToken: String, allowedUsername: String): TelegramSaved =
        rpc.read(rpc.post("/api/auth/config", mapOf("action" to "telegram-save", "botToken" to botToken.trim(), "allowedUsername" to allowedUsername.trim().removePrefix("@"))), null)

    /** Take the bot away (refused while Telegram is the only way to sign in). */
    suspend fun clearTelegram(): TelegramSaved = rpc.read(rpc.post("/api/auth/config", mapOf("action" to "telegram-clear")), null)

    suspend fun poll(pollId: String): OAuthPoll = rpc.read(rpc.post("/api/auth/oauth/poll", mapOf("pollId" to pollId)), null)

    companion object {
        val OAUTH = listOf("google", "github")
        val METHODS = setOf("telegram", "password") + OAUTH
    }
}

/** GET /api/auth/config (src/auth-config.js status()). */
@Serializable
data class AuthConfig(
    val telegram: Boolean = false,
    val password: Boolean = false,
    val google: Boolean = false,
    val github: Boolean = false,
    /** A bot token and username are set, so Telegram can be turned on. */
    val telegramAvailable: Boolean = false,
    val passwordSet: Boolean = false,
    val passwordFromEnv: Boolean = false,
    /** Google and GitHub; null from a server without them, which then offers neither. */
    val oauth: OAuthConfigs? = null,
    /** The exact address each provider must send people back to ("" until the server knows its own). */
    val callbackUrls: Map<String, String> = emptyMap(),
    /**
     * The Telegram bot, edited here rather than in Configuration. Null from an
     * older server, where the token and username are still Configuration fields.
     */
    val telegramBot: TelegramBot? = null,
) {
    fun isOn(method: String): Boolean = when (method) { "telegram" -> telegram; "password" -> password; "google" -> google; "github" -> github; else -> false }
    /** How many methods are on: the last one cannot be turned off. */
    val onCount: Int get() = listOf(telegram, password, google, github).count { it }
}

@Serializable
data class OAuthConfigs(val google: OAuthView = OAuthView(), val github: OAuthView = OAuthView()) {
    fun of(provider: String): OAuthView = if (provider == "github") github else google
}

/** A provider as Settings may see it: never the secret, only whether one is saved. */
@Serializable
data class OAuthView(
    val configured: Boolean = false,
    val clientId: String = "",
    val secretSet: Boolean = false,
    /** The Google email or GitHub username allowed to sign in. */
    val allowed: String = "",
    /** GitHub: pinned to that account's id, so a renamed username cannot take it over. */
    val idPinned: Boolean = false,
    /** A Test went through since the last change: only then can it be turned on. */
    val verified: Boolean = false,
    val verifiedAs: String = "",
)

/** The bot as Settings sees it: never the token. status: not-set-up, connected or offline. */
@Serializable
data class TelegramBot(
    val tokenSet: Boolean = false,
    val allowedUsername: String = "",
    val botUsername: String = "",
    val status: String = "not-set-up",
    /** Saved, but the running bot still has the old settings until Crundi restarts. */
    val restartRequired: Boolean = false,
    val chatId: String = "",
)

@Serializable
data class TelegramSaved(val ok: Boolean = false, val restartRequired: Boolean = false)

@Serializable
data class PasswordSet(val ok: Boolean = false, val totpSecret: String = "", val uri: String = "")

@Serializable
data class OAuthStart(val ok: Boolean = false, val url: String = "", val mode: String = "", val pollId: String = "", val confirmCode: String = "")

/** waiting, confirm, done, failed or expired. */
@Serializable
data class OAuthPoll(val ok: Boolean = false, val status: String = "", val error: String = "", val account: String = "")
