package mv.obitech.crundi.ui.chats

import mv.obitech.crundi.api.model.ChatEntry
import mv.obitech.crundi.api.model.ChatRecap
import mv.obitech.crundi.api.model.ChatState
import mv.obitech.crundi.api.model.SlashCommand
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** The chat's side of Claude Code's summaries (ChatSummaries.kt): folding batches, /btw, the recap card's rules. */
class ChatSummariesTest {
    private fun tool(id: String, group: String = "", label: String = "", status: String = "done", error: Boolean = false) =
        ChatEntry(id, "tool", name = "Read", status = status, isError = error, group = group, groupSummary = label)
    private fun text(id: String) = ChatEntry(id, "assistant-text", text = "…")

    // ─── Folding ───

    @Test fun consecutiveCallsOfABatchFoldIntoOneRow() {
        val rows = foldToolBatches(listOf(text("a"), tool("t1", "t1", "Read a.txt twice, listed files"), tool("t2", "t1", "Read a.txt twice, listed files"), tool("t3", "t1", "Read a.txt twice, listed files"), text("b")))
        assertEquals(3, rows.size)
        val b = rows[1] as ChatRow.Batch
        assertEquals("t1", b.group)
        assertEquals("Read a.txt twice, listed files", b.summary)
        assertEquals(listOf("t1", "t2", "t3"), b.entries.map { it.id })
        assertEquals("g:t1", b.key)
        assertEquals(listOf("a", "b"), rows.filterIsInstance<ChatRow.One>().map { it.entry.id })
    }

    @Test fun callsWithoutABatchStayAsTheyAre() {
        val list = listOf(tool("t1"), tool("t2"), text("a"))
        val rows = foldToolBatches(list)
        assertEquals(list, rows.map { (it as ChatRow.One).entry })
        assertEquals(listOf("t1", "t2", "a"), rows.map { it.key })
        // A group with no label yet is not folded.
        assertTrue(foldToolBatches(listOf(tool("t1", "t1", ""))).single() is ChatRow.One)
    }

    @Test fun twoBatchesSideBySideStayApart() {
        val rows = foldToolBatches(listOf(tool("t1", "t1", "Read"), tool("t2", "t1", "Read"), tool("t3", "t3", "Ran tests"), tool("t4")))
        assertEquals(listOf("g:t1", "g:t3", "t4"), rows.map { it.key })
        assertEquals(2, (rows[0] as ChatRow.Batch).entries.size)
        assertEquals("Ran tests", (rows[1] as ChatRow.Batch).summary)
    }

    @Test fun somethingBetweenTheCallsSplitsTheBatch() {
        val permission = ChatEntry("p1", "permission", status = "allowed")
        val rows = foldToolBatches(listOf(tool("t1", "t1", "Edited files"), permission, tool("t2", "t1", "Edited files")))
        assertEquals(listOf("g:t1", "p1", "g:t2"), rows.map { it.key })
    }

    // As the web: a subagent's row is the way into its transcript, so it is never folded away.
    private val agents = mapOf(
        "toolu_ag" to mv.obitech.crundi.api.model.ChatAgent("toolu_ag", kind = "agent"),
        "toolu_bg" to mv.obitech.crundi.api.model.ChatAgent("toolu_bg", kind = "task"),
    )
    private fun agentCall(id: String, toolUseId: String = "toolu_ag", group: String = "t1") =
        ChatEntry(id, "tool", toolUseId = toolUseId, name = "Agent", status = "running", group = group, groupSummary = "Read files, started an agent")

    @Test fun aSubagentStaysOutsideTheFold() {
        val rows = foldToolBatches(listOf(tool("t1", "t1", "Read files, started an agent"), agentCall("t2"), tool("t3", "t1", "Read files, started an agent")), agents)
        assertEquals(listOf("g:t1", "t2"), rows.map { it.key })
        // The rest of the batch folds around it, under one label.
        assertEquals(listOf("t1", "t3"), (rows[0] as ChatRow.Batch).entries.map { it.id })
        assertEquals("t2", (rows[1] as ChatRow.One).entry.id)
    }

    @Test fun aBatchOfOnlySubagentsHasNoLabelRow() {
        val rows = foldToolBatches(listOf(agentCall("t1"), agentCall("t2", "toolu_ag")), agents)
        assertTrue(rows.all { it is ChatRow.One })
        assertEquals(listOf("t1", "t2"), rows.map { it.key })
    }

    @Test fun aSubagentFirstThenTheRestFolds() {
        val rows = foldToolBatches(listOf(agentCall("t1"), tool("t2", "t1", "x"), tool("t3", "t1", "x")), agents)
        assertEquals(listOf("t1", "g:t2"), rows.map { it.key })
        assertEquals(2, (rows[1] as ChatRow.Batch).entries.size)
    }

    @Test fun aBackgroundTaskOrAnUnknownAgentFoldsAsAnyCall() {
        val rows = foldToolBatches(listOf(tool("t1", "t1", "x"), agentCall("t2", "toolu_bg"), agentCall("t3", "toolu_none")), agents)
        assertEquals(listOf("t1", "t2", "t3"), (rows.single() as ChatRow.Batch).entries.map { it.id })
        // Without the agents, as before: everything with the label folds.
        assertTrue(foldToolBatches(listOf(tool("t1", "t1", "x"), agentCall("t2"))).single() is ChatRow.Batch)
    }

    @Test fun aBatchIsRunningOrFailedIfAnyCallIs() {
        val b = foldToolBatches(listOf(tool("t1", "t1", "x"), tool("t2", "t1", "x", status = "running"), tool("t3", "t1", "x", error = true))).single() as ChatRow.Batch
        assertTrue(b.running)
        assertTrue(b.failed)
        val done = foldToolBatches(listOf(tool("t1", "t1", "x"))).single() as ChatRow.Batch
        assertTrue(!done.running && !done.failed)
    }

    // ─── /btw ───

    @Test fun btwIsAskedOnTheSide() {
        assertEquals("what did it change?", ChatAside.question("/btw what did it change?", supported = true))
        assertEquals("what did it change?", ChatAside.question("  /BTW   what did it change?  ", supported = true))
        assertEquals("two\nlines", ChatAside.question("/btw two\nlines", supported = true))
        // Alone: the sheet opens to write one.
        assertEquals("", ChatAside.question("/btw", supported = true))
    }

    @Test fun otherMessagesAreSent() {
        assertNull(ChatAside.question("btw what did it change?", supported = true))
        assertNull(ChatAside.question("/btwx what", supported = true))
        assertNull(ChatAside.question("/compact", supported = true))
        assertNull(ChatAside.question("see /btw here", supported = true))
    }

    @Test fun anOlderServerGetsBtwAsTyped() {
        assertNull(ChatAside.question("/btw what did it change?", supported = false))
        assertNull(ChatAside.question("/btw", supported = false))
    }

    @Test fun btwIsInTheSlashListOnlyWhenOffered() {
        val list = listOf(SlashCommand("compact"))
        assertEquals(list, ChatAside.commands(list, supported = false))
        assertEquals(listOf("compact", "btw"), ChatAside.commands(list, supported = true).map { it.name })
        // Not twice, should Claude Code list it itself one day.
        assertEquals(1, ChatAside.commands(list + SlashCommand("btw"), supported = true).count { it.name == "btw" })
    }

    @Test fun capabilities() {
        assertTrue(ChatAside.supported(setOf("chat-aside")))
        assertTrue(!ChatAside.supported(setOf("chat-recap")))
        assertTrue(ChatRecaps.supported(setOf("chat-recap")))
        assertTrue(!ChatRecaps.supported(emptySet()))
    }

    // ─── The recap card ───

    @Test fun theRecapShowsUntilClosedOrSentOver() {
        val r = ChatRecap("Tidying the parser.", 1792400000000, "idle")
        assertEquals(r, ChatRecaps.shown(r, supported = true, retired = null))
        assertNull("not on a server without recaps", ChatRecaps.shown(r, supported = false, retired = null))
        assertNull("closed here", ChatRecaps.shown(r, supported = true, retired = r.key))
        // A newer recap after one was closed shows again.
        val next = r.copy(text = "Tests pass; open the pull request.", at = r.at + 300_000)
        assertEquals(next, ChatRecaps.shown(next, supported = true, retired = r.key))
        assertNull(ChatRecaps.shown(null, supported = true, retired = null))
    }

    @Test fun theScreensRecapFollowsTheRules() {
        val r = ChatRecap("Tidying the parser.", 1, "resumed")
        val ui = ChatUi(ChatState("c1", recap = r), recapOk = true)
        assertEquals(r, ui.recap)
        assertNull(ui.copy(retiredRecap = r.key).recap)
        assertNull(ui.copy(recapOk = false).recap)
    }

    @Test fun theMenuOffersAskOnTheSideOnlyWhenGiven() {
        val without = chatMenu(null, false, {}, {}, {}, {})
        assertTrue(without.none { it.label == "Ask on the side" })
        val with = chatMenu(null, false, {}, {}, {}, {}, aside = {})
        assertTrue(with.any { it.label == "Ask on the side" })
    }
}
