package mv.obitech.crundi.ui.planning

import mv.obitech.crundi.api.model.Secret
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** The picker and the start prompts, as the web's (Inspector.jsx SecretsPicker, crundi-factories.js askSecretPins). */
class FactorySecretsTest {
    private val known = listOf(Secret("s1", "Deploy token"), Secret("s2", "Other token"))

    @Test fun shownOnlyWhenTheServerSaysSo() {
        assertTrue(FactorySecrets.supported(setOf("handshake", "factory-secrets")))
        assertFalse(FactorySecrets.supported(setOf("handshake", "factory-pause-options")))
    }

    @Test fun toggleKeepsIdsOnceInOrder() {
        var list = emptyList<String>()
        list = FactorySecrets.toggle(list, "s2", true)
        list = FactorySecrets.toggle(list, "s1", true)
        list = FactorySecrets.toggle(list, "s2", true)
        assertEquals(listOf("s2", "s1"), list)
        assertEquals(listOf("s1"), FactorySecrets.toggle(list, "s2", false))
    }

    @Test fun promptsFollowTheFactorysOrderAndSkipTheGone() {
        val ask = FactorySecrets.toUnlock(listOf("s2", "gone", "s1"), known)
        assertEquals(listOf("Other token", "Deploy token"), ask.map { it.name })
        assertEquals(listOf("gone"), FactorySecrets.missing(listOf("s2", "gone", "s1"), known))
    }
}
