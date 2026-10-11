package mv.obitech.crundi

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Seals text with a key that lives in the phone's keystore and cannot be
 * taken out of it. What is written to disk is ciphertext; a copy of the
 * app's files is no use on another device.
 */
object Vault {
    private const val ALIAS = "crundi.vault.v1"

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build()
        )
        return gen.generateKey()
    }

    fun seal(plain: String): String {
        val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val out = c.iv + c.doFinal(plain.toByteArray(Charsets.UTF_8))
        return Base64.encodeToString(out, Base64.NO_WRAP)
    }

    /** Null if it cannot be opened: a restored backup, a wiped keystore. The caller starts again from nothing. */
    fun open(sealed: String): String? = try {
        val raw = Base64.decode(sealed, Base64.NO_WRAP)
        val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, raw, 0, 12)) }
        String(c.doFinal(raw, 12, raw.size - 12), Charsets.UTF_8)
    } catch (e: Exception) {
        null
    }
}

/**
 * One server this phone is paired with.
 *
 * Pairing hands back two logins (see pairDevice in the server's webapp.js).
 * `bg*` is this app's own, for the things it does with no page open. The
 * page's login belongs to the web view: it is held here only until the web
 * view has taken it (`page*`, cleared by MainActivity once it has), after
 * which the page keeps and renews it by itself.
 */
class Server(
    val id: String,
    val baseUrl: String,
    val username: String,
    val deviceName: String,
    var bgToken: String,
    var bgRefresh: String,
    var pageToken: String = "",
    var pageRefresh: String = "",
    // Notifications from this server are sealed for these keys (see PushBox).
    // One pair per server: whichever pair opens a message says which server
    // sent it. `pushToken` is the Google token last given to this server.
    var pushPrivate: String = "",
    var pushPublic: String = "",
    var pushAuth: String = "",
    var pushToken: String = "",
    // Which build of the app last registered. A new build registers again,
    // which also puts right a registration an older build got wrong.
    var pushBuild: Int = 0,
) {
    val host: String get() = baseUrl.substringAfter("://")
    val insecure: Boolean get() = baseUrl.startsWith("http://")

    fun toJson() = JSONObject().apply {
        put("id", id); put("baseUrl", baseUrl); put("username", username); put("deviceName", deviceName)
        put("bgToken", bgToken); put("bgRefresh", bgRefresh); put("pageToken", pageToken); put("pageRefresh", pageRefresh)
        put("pushPrivate", pushPrivate); put("pushPublic", pushPublic); put("pushAuth", pushAuth); put("pushToken", pushToken); put("pushBuild", pushBuild)
    }

    companion object {
        fun fromJson(o: JSONObject) = Server(
            o.getString("id"), o.getString("baseUrl"), o.optString("username"), o.optString("deviceName"),
            o.optString("bgToken"), o.optString("bgRefresh"), o.optString("pageToken"), o.optString("pageRefresh"),
            o.optString("pushPrivate"), o.optString("pushPublic"), o.optString("pushAuth"), o.optString("pushToken"), o.optInt("pushBuild"),
        )
    }
}

/** The servers this phone is paired with, and which one is showing. */
object Servers {
    private const val PREFS = "crundi"
    private const val KEY_LIST = "servers.sealed"
    private const val KEY_CURRENT = "servers.current"

    fun all(ctx: Context): MutableList<Server> {
        val sealed = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY_LIST, null) ?: return mutableListOf()
        val plain = Vault.open(sealed) ?: return mutableListOf()
        return try {
            val arr = JSONArray(plain)
            MutableList(arr.length()) { Server.fromJson(arr.getJSONObject(it)) }
        } catch (e: Exception) {
            mutableListOf()
        }
    }

    private fun write(ctx: Context, list: List<Server>) {
        val arr = JSONArray().apply { list.forEach { put(it.toJson()) } }
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString(KEY_LIST, Vault.seal(arr.toString())).apply()
    }

    /** Add, or replace the one with the same id. */
    fun save(ctx: Context, server: Server) {
        val list = all(ctx).filter { it.id != server.id }.toMutableList().apply { add(server) }
        write(ctx, list)
    }

    fun remove(ctx: Context, id: String) {
        write(ctx, all(ctx).filter { it.id != id })
        if (currentId(ctx) == id) setCurrent(ctx, all(ctx).firstOrNull()?.id ?: "")
    }

    fun currentId(ctx: Context): String = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).getString(KEY_CURRENT, "") ?: ""
    fun setCurrent(ctx: Context, id: String) = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString(KEY_CURRENT, id).apply()

    fun byId(ctx: Context, id: String): Server? = all(ctx).firstOrNull { it.id == id }

    fun current(ctx: Context): Server? {
        val list = all(ctx)
        return list.firstOrNull { it.id == currentId(ctx) } ?: list.firstOrNull()
    }
}
