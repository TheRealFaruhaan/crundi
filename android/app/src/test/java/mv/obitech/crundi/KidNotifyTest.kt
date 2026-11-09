package mv.obitech.crundi

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Which question a kid-ask push is about, and its answers as the notification's buttons. */
class KidNotifyTest {
    private val approvals = JSONObject("""{"items":[
        {"id":"c1","source":"chat","title":"Claude asks","createdAt":1},
        {"id":"kid-1","source":"kid","kind":"video","title":"Sara wants to watch: Blippi","choices":[{"id":"once","label":"Allow once"},{"id":"always","label":"Always allow"},{"id":"no","label":"No"}],"createdAt":5},
        {"id":"kid-2","source":"kid","kind":"other","title":"Sara asks: a story","createdAt":9}
    ],"count":3}""")

    @Test fun theQuestionWithThePushsWords() {
        val a = KidNotify.pick(approvals, "Sara wants to watch: Blippi")!!
        assertEquals("kid-1", a.id)
        assertEquals(listOf("once" to "Allow once", "always" to "Always allow", "no" to "No"), a.choices)
    }

    @Test fun elseTheNewestWithOkAndNoWhenItSaysNone() {
        val a = KidNotify.pick(approvals, "something else")!!
        assertEquals("kid-2", a.id)
        assertEquals(listOf("yes" to "OK", "no" to "No"), a.choices)
    }

    @Test fun noneWaitingOrAnOlderServer() {
        assertNull(KidNotify.pick(JSONObject("""{"items":[{"id":"c1","source":"chat"}]}"""), "x"))
        assertNull(KidNotify.pick(JSONObject("{}"), "x"))
        assertNull(KidNotify.pick(null, "x"))
    }
}
