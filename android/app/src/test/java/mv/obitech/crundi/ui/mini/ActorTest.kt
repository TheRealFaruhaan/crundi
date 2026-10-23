package mv.obitech.crundi.ui.mini

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Mini's body without a screen: the springs, walking, falling and the dealt packs (plain JVM). */
class ActorTest {
    private fun Actor.run(seconds: Float) { repeat((seconds * 60).toInt()) { step(1f / 60) } }

    @Test fun aPartGoesWhereItIsToldWithoutJumping() {
        val a = Actor(50f)
        a.pose(P.ArmR to 152)
        a.step(1f / 60)
        assertTrue("one frame moves it only a little", a.v[P.ArmR.ordinal] < 20f)
        a.run(3f)
        assertEquals(152f, a.v[P.ArmR.ordinal], 0.5f)
    }

    @Test fun aChangeOfMindCarriesOnFromWhereItIs() {
        val a = Actor(50f)
        a.pose(P.ArmR to 152); a.run(0.15f)
        val mid = a.v[P.ArmR.ordinal]
        a.pose(P.ArmR to 0); a.step(1f / 60)
        assertEquals(mid, a.v[P.ArmR.ordinal], 15f)
    }

    @Test fun itWalksToItsMarkAndStopsThere() {
        val a = Actor(50f)
        a.surf = { Surf(100f, 0f, 1000f) }; a.baseY = 100f
        val arrived = a.go(200f, 60)
        a.run(8f)
        assertTrue(arrived.isCompleted)
        assertEquals(200f, a.x, 2f)   // it settles on its mark, as on the web
        assertEquals(0f, a.t[P.Turn.ordinal], 0f)
    }

    @Test fun theSurfaceKeepsItOnTheCard() {
        val a = Actor(50f)
        a.surf = { Surf(100f, 10f, 80f) }; a.baseY = 100f
        a.go(500f, 60)
        a.run(5f)
        assertEquals(80f, a.x, 2f)
    }

    @Test fun aLongFallIsTakenByParachuteAndItLands() {
        val a = Actor(50f)
        a.surf = { Surf(600f, -100f, 1000f) }; a.baseY = 600f; a.yOff = -500f; a.air = true
        var opened = false
        repeat(60 * 20) { a.step(1f / 60); if (a.chuting) opened = true }
        assertTrue(opened)
        assertFalse(a.air)
        assertEquals(0f, a.t[P.Chute.ordinal], 0f)
    }

    @Test fun aShortDropIsJustAFall() {
        val a = Actor(50f)
        a.surf = { Surf(100f, -100f, 1000f) }; a.baseY = 100f; a.yOff = -30f; a.air = true
        var opened = false
        repeat(120) { a.step(1f / 60); if (a.chuting) opened = true }
        assertFalse(opened); assertFalse(a.air)
    }

    @Test fun packsDealEachOnceAndNeverTheSameTwiceRunning() {
        val seen = (0 until 5).map { Packs.deal("test-pack", 5) }
        assertEquals((0 until 5).toSet(), seen.toSet())
        var last = seen.last()
        repeat(200) { val n = Packs.deal("test-pack", 5); assertNotEquals(last, n); last = n }
    }
}
