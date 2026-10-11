package mv.obitech.crundi

import java.math.BigInteger
import java.security.AlgorithmParameters
import java.security.KeyFactory
import java.security.KeyPairGenerator
import java.security.SecureRandom
import java.security.interfaces.ECPrivateKey
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import java.security.spec.ECParameterSpec
import java.security.spec.ECPoint
import java.security.spec.ECPublicKeySpec
import java.security.spec.PKCS8EncodedKeySpec
import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.KeyAgreement
import javax.crypto.Mac
import javax.crypto.spec.GCMParameterSpec
import javax.crypto.spec.SecretKeySpec

/**
 * Opening what the server seals for this phone.
 *
 * A notification crosses the relay and Google as ciphertext that only this
 * phone can open. The scheme is the one browsers use for Web Push (RFC 8291,
 * "aes128gcm"), because that is what the server already had: an elliptic
 * curve key pair and a 16 byte secret made HERE, the public half and the
 * secret given to the server once, the private half never leaving the phone.
 *
 * Plain java.security throughout, no library. PushBoxTest opens a message the
 * real server sealed, so the two cannot drift apart unnoticed.
 */
object PushBox {
    class Keys(val privatePkcs8: ByteArray, val publicPoint: ByteArray, val auth: ByteArray) {
        val p256dh: String get() = b64(publicPoint)
        val authText: String get() = b64(auth)
    }

    private val enc = Base64.getUrlEncoder().withoutPadding()
    private val dec = Base64.getUrlDecoder()
    fun b64(bytes: ByteArray): String = enc.encodeToString(bytes)
    fun unb64(text: String): ByteArray = dec.decode(text)

    private fun curve(): ECParameterSpec =
        AlgorithmParameters.getInstance("EC").apply { init(ECGenParameterSpec("secp256r1")) }.getParameterSpec(ECParameterSpec::class.java)

    // A coordinate as exactly 32 bytes: BigInteger adds a sign byte or drops leading zeros.
    private fun fixed(n: BigInteger): ByteArray {
        val raw = n.toByteArray()
        val out = ByteArray(32)
        val take = minOf(32, raw.size)
        System.arraycopy(raw, raw.size - take, out, 32 - take, take)
        return out
    }

    fun generate(): Keys {
        val pair = KeyPairGenerator.getInstance("EC").apply { initialize(ECGenParameterSpec("secp256r1")) }.generateKeyPair()
        val w = (pair.public as ECPublicKey).w
        val point = byteArrayOf(4) + fixed(w.affineX) + fixed(w.affineY)
        return Keys(pair.private.encoded, point, ByteArray(16).also { SecureRandom().nextBytes(it) })
    }

    private fun hmac(key: ByteArray, data: ByteArray): ByteArray =
        Mac.getInstance("HmacSHA256").apply { init(SecretKeySpec(key, "HmacSHA256")) }.doFinal(data)

    // HKDF with one block of output, which is all 32 bytes or fewer ever needs.
    private fun hkdf(salt: ByteArray, ikm: ByteArray, info: ByteArray, length: Int): ByteArray =
        hmac(hmac(salt, ikm), info + byteArrayOf(1)).copyOf(length)

    /**
     * @param sealed base64url, as it arrives in the push
     * @return the text inside, or null if it was not sealed for these keys.
     *   Null is ordinary: with several servers paired, each one's keys are
     *   tried, and all but one are the wrong ones.
     */
    fun open(sealed: String, keys: Keys): String? = try {
        val buf = unb64(sealed)
        val salt = buf.copyOfRange(0, 16)
        val idLen = buf[20].toInt() and 0xff
        val serverPoint = buf.copyOfRange(21, 21 + idLen)
        val body = buf.copyOfRange(21 + idLen, buf.size)
        require(idLen == 65 && serverPoint[0] == 4.toByte())

        val factory = KeyFactory.getInstance("EC")
        val mine = factory.generatePrivate(PKCS8EncodedKeySpec(keys.privatePkcs8)) as ECPrivateKey
        val theirs = factory.generatePublic(
            ECPublicKeySpec(ECPoint(BigInteger(1, serverPoint.copyOfRange(1, 33)), BigInteger(1, serverPoint.copyOfRange(33, 65))), curve())
        )
        val shared = KeyAgreement.getInstance("ECDH").apply { init(mine); doPhase(theirs, true) }.generateSecret()

        // The info binds both public keys in, so a secret agreed for one
        // registration cannot be replayed against another.
        val ikm = hkdf(keys.auth, shared, "WebPush: info\u0000".toByteArray(Charsets.US_ASCII) + keys.publicPoint + serverPoint, 32)
        val cek = hkdf(salt, ikm, "Content-Encoding: aes128gcm\u0000".toByteArray(Charsets.US_ASCII), 16)
        val nonce = hkdf(salt, ikm, "Content-Encoding: nonce\u0000".toByteArray(Charsets.US_ASCII), 12)
        val plain = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, SecretKeySpec(cek, "AES"), GCMParameterSpec(128, nonce)) }.doFinal(body)
        // One record: the text, a 0x02 marking its end, then any padding.
        var end = plain.size - 1
        while (end >= 0 && plain[end] == 0.toByte()) end--
        require(end >= 0 && plain[end] == 2.toByte())
        String(plain, 0, end, Charsets.UTF_8)
    } catch (e: Exception) {
        null
    }
}
