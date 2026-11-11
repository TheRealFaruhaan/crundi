package mv.obitech.crundi.homepool

import android.content.Context
import android.content.SharedPreferences
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONObject
import java.security.KeyStore
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The node key sealed with an AES-GCM key that lives in the phone's keystore
 * (never leaves it). The homepool package's own copy of the idea behind the
 * Crundi app's Vault, because the standalone node app has no Store.kt.
 */
internal object NodeKeyBox {
    private const val ALIAS = "homepool.node.v1"

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

    /** "k1:<base64 iv+ciphertext>", or null if the keystore is unusable on this phone. */
    fun seal(plain: String): String? = try {
        val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        "k1:" + Base64.encodeToString(c.iv + c.doFinal(plain.toByteArray(Charsets.UTF_8)), Base64.NO_WRAP)
    } catch (e: Exception) {
        null
    }

    /** Null if it cannot be opened (a restored backup, a wiped keystore): the node is then simply not configured. */
    fun open(sealed: String): String? = try {
        val raw = Base64.decode(sealed.removePrefix("k1:"), Base64.NO_WRAP)
        val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, raw, 0, 12)) }
        String(c.doFinal(raw, 12, raw.size - 12), Charsets.UTF_8)
    } catch (e: Exception) {
        null
    }
}

/** The node's settings and today's data, in SharedPreferences "homepool_node". */
internal class NodePrefs(ctx: Context) {
    private val p: SharedPreferences = ctx.applicationContext.getSharedPreferences("homepool_node", Context.MODE_PRIVATE)

    var config: NodeConfig?
        get() {
            val raw = p.getString("config", null) ?: return null
            val json = when {
                raw.startsWith("k1:") -> NodeKeyBox.open(raw) ?: return null
                raw.startsWith("plain:") -> raw.removePrefix("plain:")
                else -> return null
            }
            return try {
                val j = JSONObject(json)
                NodeConfig(j.getString("server"), j.getString("key"))
            } catch (e: Exception) { null }
        }
        set(v) {
            if (v == null) { p.edit().remove("config").apply(); return }
            val json = JSONObject().put("server", v.server).put("key", v.key).toString()
            // Sealed if the keystore works here; a phone whose keystore is broken still gets a node.
            p.edit().putString("config", NodeKeyBox.seal(json) ?: "plain:$json").apply()
        }

    /** The node should be running now (Start / Stop). */
    var enabled: Boolean
        get() = p.getBoolean("enabled", false)
        set(v) = p.edit().putBoolean("enabled", v).apply()

    /** Registered: starts at boot / after an update, and START_STICKY. */
    var autoStart: Boolean
        get() = p.getBoolean("autoStart", false)
        set(v) = p.edit().putBoolean("autoStart", v).apply()

    /** Only relay on Wi-Fi / Ethernet: pause on mobile data. */
    var wifiOnly: Boolean
        get() = p.getBoolean("wifiOnly", false)
        set(v) = p.edit().putBoolean("wifiOnly", v).apply()

    /** Mobile-data cap per day in MB; 0 = no cap. */
    var mobileCapMb: Int
        get() = p.getInt("mobileCapMb", DEFAULT_MOBILE_CAP_MB)
        set(v) = p.edit().putInt("mobileCapMb", v.coerceAtLeast(0)).apply()

    /** Why the server refused the key, kept until a new code or Start; null when it was not. */
    var refused: String?
        get() = p.getString("refused", null)
        set(v) = p.edit().apply { if (v == null) remove("refused") else putString("refused", v) }.apply()

    // ── today's data, counted on the node's own streams ──
    private val usageDay: String get() = p.getString("usageDay", "") ?: ""
    val bytesToday: Long get() { roll(); return p.getLong("usageTotal", 0) }
    val mobileBytesToday: Long get() { roll(); return p.getLong("usageMobile", 0) }

    @Synchronized
    fun addUsage(total: Long, mobile: Long) {
        roll()
        if (total == 0L && mobile == 0L) return
        p.edit()
            .putLong("usageTotal", p.getLong("usageTotal", 0) + total)
            .putLong("usageMobile", p.getLong("usageMobile", 0) + mobile)
            .apply()
    }

    @Synchronized
    private fun roll() {
        val d = SimpleDateFormat("yyyy-MM-dd", Locale.US).format(Date())
        if (usageDay != d) p.edit().putString("usageDay", d).putLong("usageTotal", 0).putLong("usageMobile", 0).apply()
    }
}
