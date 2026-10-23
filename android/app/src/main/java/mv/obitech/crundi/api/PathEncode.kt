package mv.obitech.crundi.api

import java.net.URLEncoder

/**
 * One value as a segment of a URL path, encoded as the web's
 * encodeURIComponent does: "dhivo-socials:Social Desk" becomes
 * "dhivo-socials%3ASocial%20Desk". URLEncoder alone is form encoding, which
 * writes a space as "+"; in a path the server reads "+" as a plus, so a
 * service with a space in its name was never found. Use this for every
 * /api/.../<value> path; query strings go through [Rpc]'s query map or
 * URLEncoder, which the server reads with searchParams (+ is a space there).
 * Plain JDK, so JVM unit tests can run it.
 */
fun seg(s: String): String =
    URLEncoder.encode(s, "UTF-8")
        .replace("+", "%20") // a literal plus is already %2B, so only spaces are "+" here
        .replace("%21", "!").replace("%27", "'").replace("%28", "(").replace("%29", ")").replace("%7E", "~")
