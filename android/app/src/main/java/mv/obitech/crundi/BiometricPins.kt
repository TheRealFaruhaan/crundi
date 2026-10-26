package mv.obitech.crundi

import android.content.Context
import android.content.SharedPreferences
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import android.widget.Toast
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricManager.Authenticators.BIOMETRIC_STRONG
import androidx.biometric.BiometricPrompt
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Secret PINs kept on the phone behind the fingerprint (or face, any
 * BIOMETRIC_STRONG sensor). Off unless the person turns it on.
 *
 * Each PIN is encrypted with one AES key in the Android Keystore that
 * Android will only use after a biometric check, every time (no grace
 * period), and that Android itself destroys when the enrolled biometrics
 * change: a new fingerprint added cannot open a PIN saved before it. When
 * that happens the stored PINs are useless and are forgotten, and the
 * person is told. A PIN is never stored any other way: no key, no PIN.
 *
 * Kept per server (its host) and per secret, so the same secret id on two
 * servers is two entries. The server never knows: a PIN unlocked here is
 * sent exactly as a typed one is.
 */
object BiometricPins {
    /** Whether this phone can do it: a strong biometric sensor, with something enrolled. */
    fun available(ctx: Context): Boolean =
        runCatching { BiometricManager.from(ctx).canAuthenticate(BIOMETRIC_STRONG) == BiometricManager.BIOMETRIC_SUCCESS }.getOrDefault(false)

    fun store(ctx: Context): SavedPins = SavedPins(ctx.getSharedPreferences(SavedPins.PREFS, Context.MODE_PRIVATE))

    const val INVALIDATED = "Your fingerprints or face changed, so the PINs saved behind them were forgotten. Type a PIN once to save it again."
}

/** What the PIN window shows in the place of its submit button. Pure, for tests. */
object PinButtons {
    /**
     * The fingerprint button instead of Submit: while a PIN for this secret is
     * saved and nothing is typed. Anything typed brings Submit back; emptying
     * the field again brings the fingerprint back.
     */
    fun fingerprint(saved: Boolean, typed: String): Boolean = saved && typed.isEmpty()

    /** After a typed PIN was accepted: offer to save it, when the option is on and it is not saved yet. */
    fun offerSave(optionOn: Boolean, saved: Boolean, typed: Boolean): Boolean = optionOn && !saved && typed
}

/** A PIN as stored: the Keystore's IV and the ciphertext. Never the PIN itself. */
class Sealed(val iv: ByteArray, val data: ByteArray) {
    fun encode(): String = Base64.encodeToString(iv, Base64.NO_WRAP) + ":" + Base64.encodeToString(data, Base64.NO_WRAP)

    companion object {
        fun decode(s: String?): Sealed? {
            val parts = s?.split(':') ?: return null
            if (parts.size != 2) return null
            return runCatching { Sealed(Base64.decode(parts[0], Base64.NO_WRAP), Base64.decode(parts[1], Base64.NO_WRAP)) }.getOrNull()
                ?.takeIf { it.iv.isNotEmpty() && it.data.isNotEmpty() }
        }
    }
}

/** One saved PIN, as Settings lists it: never the PIN. */
data class SavedPin(val server: String, val secretId: String, val name: String)

/**
 * Where the sealed PINs live (their own preferences file, kept out of
 * backups by the app's backup rules being off). Only sealed data is ever
 * written here.
 */
class SavedPins(private val prefs: SharedPreferences) {
    var enabled: Boolean
        get() = prefs.getBoolean(KEY_ON, false)
        set(on) { prefs.edit().putBoolean(KEY_ON, on).apply(); if (!on) forgetAll() }

    private fun key(server: String, secretId: String) = "pin|$server|$secretId"
    fun has(server: String, secretId: String): Boolean = enabled && prefs.contains(key(server, secretId))
    fun get(server: String, secretId: String): Sealed? = if (enabled) Sealed.decode(prefs.getString(key(server, secretId), null)) else null
    fun put(server: String, secretId: String, name: String, sealed: Sealed) {
        if (!enabled) return
        prefs.edit().putString(key(server, secretId), sealed.encode()).putString("name|$server|$secretId", name).apply()
    }
    fun forget(server: String, secretId: String) { prefs.edit().remove(key(server, secretId)).remove("name|$server|$secretId").apply() }
    fun forgetServer(server: String) { for (p in list()) if (p.server == server) forget(p.server, p.secretId) }
    fun forgetAll() {
        val e = prefs.edit()
        for (k in prefs.all.keys) if (k.startsWith("pin|") || k.startsWith("name|")) e.remove(k)
        e.apply()
    }
    fun list(): List<SavedPin> = prefs.all.keys.filter { it.startsWith("pin|") }.mapNotNull { k ->
        val parts = k.split('|')
        if (parts.size != 3) null else SavedPin(parts[1], parts[2], prefs.getString("name|${parts[1]}|${parts[2]}", null).orEmpty())
    }.sortedWith(compareBy({ it.server }, { it.name.lowercase() }))

    companion object {
        const val PREFS = "crundi_pins"
        private const val KEY_ON = "on"
        /** The host a server's PINs are kept under. */
        fun serverKey(baseUrl: String): String = runCatching { java.net.URI(baseUrl).host }.getOrNull()?.lowercase().orEmpty().ifEmpty { baseUrl }
    }
}

/** The Keystore key, bound to the biometrics enrolled now. */
object PinKey {
    private const val ALIAS = "crundi.secret-pins"
    private const val STORE = "AndroidKeyStore"

    class Invalidated : Exception("The key was invalidated")

    private fun keyStore(): KeyStore = KeyStore.getInstance(STORE).apply { load(null) }

    private fun key(): SecretKey {
        (keyStore().getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val spec = KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
            .setUserAuthenticationRequired(true)
            .setInvalidatedByBiometricEnrollment(true)
            .apply {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) setUserAuthenticationParameters(0, KeyProperties.AUTH_BIOMETRIC_STRONG)
                else @Suppress("DEPRECATION") setUserAuthenticationValidityDurationSeconds(-1)
            }
            .build()
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, STORE).apply { init(spec) }.generateKey()
    }

    /** A cipher to seal a new PIN with, usable once the biometric check passes. */
    fun encrypter(): Cipher = init { Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) } }

    /** A cipher to open [sealed] with, usable once the biometric check passes. */
    fun decrypter(sealed: Sealed): Cipher = init { Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, sealed.iv)) } }

    private fun init(make: () -> Cipher): Cipher = try { make() } catch (e: KeyPermanentlyInvalidatedException) { delete(); throw Invalidated() }

    /** Still usable? False (and deleted) when the enrolled biometrics changed. */
    fun stillValid(): Boolean = try {
        if (keyStore().containsAlias(ALIAS)) encrypter()
        true
    } catch (e: Invalidated) { false } catch (e: Exception) { true }

    fun delete() { runCatching { keyStore().deleteEntry(ALIAS) } }
}

/**
 * What the PIN windows ask of the phone. [LocalPinKeeper] gives screens the
 * activity's own; tests and screens outside an activity get [None].
 */
interface PinKeeper {
    /** The option is on (and the phone can do it). */
    val on: Boolean
    fun saved(server: String, secretId: String): Boolean
    /** Ask for the fingerprint and hand back the PIN; [onFail] with words to show (null: the person cancelled). */
    fun unlock(server: String, secretId: String, name: String, onPin: (String) -> Unit, onFail: (String?) -> Unit)
    /** Ask for the fingerprint and save [pin] behind it. */
    fun save(server: String, secretId: String, name: String, pin: String, done: (Boolean, String?) -> Unit)
    /** A saved PIN the server no longer takes (the secret was made again): drop it. */
    fun forget(server: String, secretId: String)

    object None : PinKeeper {
        override val on = false
        override fun saved(server: String, secretId: String) = false
        override fun unlock(server: String, secretId: String, name: String, onPin: (String) -> Unit, onFail: (String?) -> Unit) = onFail(null)
        override fun save(server: String, secretId: String, name: String, pin: String, done: (Boolean, String?) -> Unit) = done(false, null)
        override fun forget(server: String, secretId: String) {}
    }
}

val LocalPinKeeper = staticCompositionLocalOf<PinKeeper> { PinKeeper.None }

/** The real one, for an activity: BiometricPrompt over it. */
class ActivityPinKeeper(private val activity: FragmentActivity) : PinKeeper {
    private val store = BiometricPins.store(activity)
    // Read by saved() so a screen redraws when a PIN is saved or forgotten.
    private val version = mutableIntStateOf(0)

    override val on: Boolean get() { version.intValue; return store.enabled && BiometricPins.available(activity) }

    override fun saved(server: String, secretId: String): Boolean { version.intValue; return on && store.has(server, secretId) }

    private fun invalidated() {
        store.forgetAll()
        version.intValue++
        Toast.makeText(activity, BiometricPins.INVALIDATED, Toast.LENGTH_LONG).show()
    }

    private fun prompt(title: String, subtitle: String, cipher: Cipher, ok: (Cipher) -> Unit, fail: (String?) -> Unit) {
        val p = BiometricPrompt(activity, ContextCompat.getMainExecutor(activity), object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                val c = result.cryptoObject?.cipher
                if (c == null) fail("The fingerprint check did not unlock the key. Type the PIN.") else ok(c)
            }
            override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
                val cancelled = errorCode == BiometricPrompt.ERROR_NEGATIVE_BUTTON || errorCode == BiometricPrompt.ERROR_USER_CANCELED || errorCode == BiometricPrompt.ERROR_CANCELED
                fail(if (cancelled) null else errString.toString())
            }
        })
        p.authenticate(
            BiometricPrompt.PromptInfo.Builder().setTitle(title).setSubtitle(subtitle)
                .setAllowedAuthenticators(BIOMETRIC_STRONG).setNegativeButtonText("Type the PIN").build(),
            BiometricPrompt.CryptoObject(cipher),
        )
    }

    override fun forget(server: String, secretId: String) { store.forget(server, secretId); version.intValue++ }

    override fun unlock(server: String, secretId: String, name: String, onPin: (String) -> Unit, onFail: (String?) -> Unit) {
        val sealed = store.get(server, secretId) ?: return onFail("No PIN is saved for $name.")
        val cipher = try { PinKey.decrypter(sealed) } catch (e: PinKey.Invalidated) { invalidated(); return onFail(BiometricPins.INVALIDATED) } catch (e: Exception) { return onFail("The saved PIN could not be opened. Type it.") }
        prompt("Unlock $name", "Use your fingerprint to fill in its PIN", cipher, { c ->
            val pin = runCatching { String(c.doFinal(sealed.data), Charsets.UTF_8) }.getOrNull()
            if (pin == null || !Regex("^\\d{6}$").matches(pin)) { store.forget(server, secretId); version.intValue++; onFail("The saved PIN could not be opened, so it was forgotten. Type it.") }
            else onPin(pin)
        }, onFail)
    }

    override fun save(server: String, secretId: String, name: String, pin: String, done: (Boolean, String?) -> Unit) {
        if (!on || !Regex("^\\d{6}$").matches(pin)) return done(false, null)
        val cipher = try { PinKey.encrypter() } catch (e: PinKey.Invalidated) { invalidated(); return done(false, BiometricPins.INVALIDATED) } catch (e: Exception) { return done(false, "This phone could not make a key for it.") }
        prompt("Save the PIN for $name", "Use your fingerprint to keep it on this phone", cipher, { c ->
            val ok = runCatching { store.put(server, secretId, name, Sealed(c.iv, c.doFinal(pin.toByteArray(Charsets.UTF_8)))) }.isSuccess
            version.intValue++
            done(ok, if (ok) null else "It could not be saved.")
        }) { why -> done(false, why) }
    }
}

/**
 * Once, after install and once for people who update: offered right after
 * the notification permission and the urgent alert setup, and only on a
 * phone with biometrics enrolled. Turned on or off later in App settings.
 */
object BiometricSetup {
    private const val PREF_ASKED = "asked.biometricPins"

    fun offer(activity: android.app.Activity) {
        val prefs = activity.getSharedPreferences("crundi", Context.MODE_PRIVATE)
        if (prefs.getBoolean(PREF_ASKED, false)) return
        // Nothing enrolled: not asked yet, so it is offered once there is.
        if (!BiometricPins.available(activity)) return
        val store = BiometricPins.store(activity)
        prefs.edit().putBoolean(PREF_ASKED, true).apply()
        if (store.enabled) return
        android.app.AlertDialog.Builder(activity, android.R.style.Theme_DeviceDefault_Dialog_Alert)
            .setTitle("Secret PINs behind your fingerprint")
            .setMessage("When Claude asks for a secret, approve it with your fingerprint instead of typing the PIN. A PIN is saved on this phone only when you choose, encrypted with a key that only your fingerprint unlocks. You can change this in App settings.")
            .setPositiveButton("Turn on") { _, _ -> store.enabled = true }
            .setNegativeButton("Not now", null)
            .show()
    }
}

/** App settings' "Secret PINs" section (SettingsActivity). Pure, for tests and screenshots. */
object PinSettings {
    const val TITLE = "Secret PINs behind your fingerprint"

    fun items(
        available: Boolean, on: Boolean, saved: List<SavedPin>, forgottenNow: Boolean,
        toggle: () -> Unit = {}, forget: (SavedPin) -> Unit = {}, forgetAll: () -> Unit = {},
    ): List<mv.obitech.crundi.ui.pair.SettingItem> = buildList {
        val Line = mv.obitech.crundi.ui.pair.SettingItem::Line
        if (!available && !on) {
            add(Line(TITLE, "This phone has no strong fingerprint or face unlock set up. Set one up in Android's settings to use this.", null, null) {})
            return@buildList
        }
        add(
            if (on) Line(TITLE, "On. When Claude asks for a secret, a PIN saved here is filled in with your fingerprint. A PIN is saved only when you choose. Turning this off forgets them all.", true, "Turn off", toggle)
            else Line(TITLE, "Off. Turn on to approve secrets with your fingerprint instead of typing the PIN. PINs are encrypted with a key only your fingerprint unlocks.", null, "Turn on", toggle)
        )
        if (forgottenNow) add(Line("Saved PINs forgotten", BiometricPins.INVALIDATED, false, null) {})
        if (!on) return@buildList
        if (saved.isEmpty()) add(Line("Saved PINs", "None yet. Type a secret's PIN when Claude asks for it, and you will be offered to save it.", null, null) {})
        for (p in saved) add(Line(p.name.ifEmpty { "A secret" }, "Saved for ${p.server}", null, "Forget") { forget(p) })
        if (saved.size > 1) add(mv.obitech.crundi.ui.pair.SettingItem.Button("Forget all saved PINs") { forgetAll() })
    }
}
