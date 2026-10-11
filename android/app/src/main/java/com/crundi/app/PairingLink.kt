package com.crundi.app

/**
 * The link a pairing picture carries: https://server/pair#<24 hex>.
 *
 * The same shape the server writes (src/pairing.js). The code is in the
 * fragment so that a browser handed this link never sends it anywhere.
 */
class PairingLink(val baseUrl: String, val code: String) {
    val insecure get() = baseUrl.startsWith("http://")

    companion object {
        private val RE = Regex("""^(https?)://([A-Za-z0-9.-]{1,253})(?::(\d{1,5}))?/pair/?#([a-f0-9]{24})$""")

        fun parse(text: String?): PairingLink? {
            val m = RE.find((text ?: "").trim()) ?: return null
            val port = m.groupValues[3]
            if (port.isNotEmpty() && port.toInt() > 65535) return null
            val base = m.groupValues[1] + "://" + m.groupValues[2].lowercase() + (if (port.isNotEmpty()) ":$port" else "")
            return PairingLink(base, m.groupValues[4])
        }
    }
}
