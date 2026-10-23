package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.longOrNull
import mv.obitech.crundi.api.model.ChatEntry
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.CrundiIconButton
import mv.obitech.crundi.ui.components.CrundiTextField
import mv.obitech.crundi.ui.components.MarkdownText
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** The ask card (.cc-ask): accent for questions and results, yellow for permission requests. */
@Composable
private fun AskBox(permission: Boolean, content: @Composable ColumnScope.() -> Unit) {
    val c = if (permission) CrundiColors.Yellow else CrundiColors.Accent
    val bg = if (permission) CrundiColors.YellowDim else CrundiColors.AccentDim
    Column(
        Modifier.fillMaxWidth().clip(Dimens.Shape).background(bg).border(Dimens.Hairline, c, Dimens.Shape).padding(horizontal = 12.dp, vertical = 11.dp),
        content = content,
    )
}

@Composable private fun AskTitle(t: String) = Text(t, style = ChatType.Body.copy(fontWeight = FontWeight.SemiBold), modifier = Modifier.padding(bottom = 3.dp))
@Composable private fun AskSub(t: String) = Text(t, style = ChatType.Small, modifier = Modifier.padding(bottom = 8.dp))
@Composable private fun AskReason(t: String) = Text(t, style = ChatType.Small.copy(color = CrundiColors.TextMuted, fontSize = 11.5.sp), modifier = Modifier.padding(bottom = 8.dp))

@Composable
private fun AskPre(text: String) {
    Box(
        Modifier.fillMaxWidth().padding(bottom = 9.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary).border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.ShapeSm)
            .heightIn(max = 200.dp).verticalScroll(rememberScrollState()).padding(horizontal = 9.dp, vertical = 7.dp),
    ) { Text(text, style = ChatType.ToolBody) }
}

/** The input a permission request shows: the command alone for Bash, else the input as JSON. */
internal fun permissionInput(e: ChatEntry): String {
    val cmd = (e.input["command"] as? JsonPrimitive)?.contentOrNull
    if (cmd != null && e.input.size <= 3) return cmd + ((e.input["description"] as? JsonPrimitive)?.contentOrNull?.let { "\n\n# $it" } ?: "")
    return truncate(prettyJson(e.input), 2500)
}

/** Allow, Allow always, Deny: the CLI's own choices. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun PermissionCard(e: ChatEntry, actions: EntryActions, busy: Boolean = false) {
    val name = e.displayName.ifEmpty { e.toolName }
    AskBox(permission = true) {
        if (e.status != "pending") {
            val closed = e.status == "unanswered" || e.status == "cancelled"
            val always = (e.raw["always"] as? JsonPrimitive)?.let { it.booleanOrNull == true || it.contentOrNull == "session" } == true
            val tag = when { e.status == "denied" -> "denied"; closed -> "never answered"; always -> "always allowed"; else -> "allowed" }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(name, style = ChatType.Small.copy(color = CrundiColors.TextPrimary, fontWeight = FontWeight.SemiBold))
                Tag(tag, ok = !(e.status == "denied" || closed))
            }
            if (closed && e.title.isNotEmpty()) Text(e.title, style = ChatType.Small, modifier = Modifier.padding(top = 4.dp))
            return@AskBox
        }
        AskTitle(e.title.ifEmpty { "Claude wants to use $name" })
        if (e.description.isNotEmpty()) AskSub(e.description)
        if (e.decisionReason.isNotEmpty()) AskReason(stripAnsi(e.decisionReason))
        AskPre(permissionInput(e))
        if (e.escalated) {
            val manager = (e.raw["routedTo"] as? JsonPrimitive)?.contentOrNull == "manager"
            AskReason(if (manager) "Sent to the Manager of this factory, who decides. You will see the answer here." else "Sent to the project owner. You will see the answer here.")
            return@AskBox
        }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalArrangement = Arrangement.spacedBy(7.dp)) {
            CrundiButton("Allow", { actions.allow(e, false) }, kind = ButtonKind.Primary, enabled = !busy)
            if (!e.suppressAlwaysAllow && e.hasSuggestions) CrundiButton("Allow always", { actions.allow(e, true) }, enabled = !busy)
            CrundiButton("Deny", { actions.deny(e) }, kind = ButtonKind.Danger, enabled = !busy)
        }
    }
}

/** What the person has picked on a question card, per question. */
class QuestionPicks {
    val chosen = mutableStateMapOf<Int, List<String>>()
    val other = mutableStateMapOf<Int, String>()
    // A note for Claude, one per question (the tool's annotations[question].notes),
    // and the choice it is shown under, as on the web.
    val note = mutableStateMapOf<Int, String>()
    val noteOpt = mutableStateMapOf<Int, String>()
    fun answer(q: Int): String = other[q]?.takeIf { it.isNotBlank() }?.trim() ?: chosen[q].orEmpty().joinToString(", ")
    fun noteFor(q: Int): String = note[q].orEmpty().trim()

    /** A note follows the selection: it moves to the newly picked choice, or goes out of sight. */
    fun followSelection(q: Int) {
        val at = noteOpt[q] ?: return
        val cur = chosen[q].orEmpty()
        if (at !in cur) { val to = cur.lastOrNull(); if (to == null) noteOpt.remove(q) else noteOpt[q] = to }
    }
}

/** AskUserQuestion: up to four questions, each with options, multi-select, and "Other". */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun QuestionCard(e: ChatEntry, actions: EntryActions, busy: Boolean = false, picks: QuestionPicks = remember(e.id) { QuestionPicks() }) {
    val questions = e.questions
    AskBox(permission = false) {
        val orphaned = e.status == "unanswered" || e.status == "cancelled"
        if (e.status != "pending" && !orphaned) {
            val answered = (e.answeredInput?.get("answers") as? JsonObject).orEmpty()
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                if (questions.isNotEmpty()) Text(questions.first().header.ifEmpty { "Question" }, style = ChatType.Small)
                when {
                    e.status == "denied" -> Tag("dismissed", ok = false)
                    answered.isEmpty() -> Tag("answered")
                    else -> for ((_, v) in answered) Tag((v as? JsonPrimitive)?.contentOrNull ?: v.toString(), ok = true)
                }
            }
            val ann = (e.answeredInput?.get("annotations") as? JsonObject).orEmpty()
            for ((_, v) in ann) ((v as? JsonObject)?.get("notes") as? JsonPrimitive)?.contentOrNull?.let { Text("Note: $it", style = ChatType.Small.copy(color = CrundiColors.TextMuted), modifier = Modifier.padding(top = 4.dp)) }
            return@AskBox
        }
        if (orphaned) AskSub("This question outlived the session that asked it. Answering sends your choice as a message.")
        questions.forEachIndexed { qi, q ->
            Column(Modifier.padding(bottom = 11.dp)) {
                if (q.header.isNotEmpty()) Text(
                    q.header.uppercase(), style = CrundiType.Meta.copy(color = Color.White, fontWeight = FontWeight.Bold, fontFamily = CrundiType.Sans, fontSize = 10.5.sp),
                    modifier = Modifier.padding(bottom = 5.dp).clip(RoundedCornerShape(99.dp)).background(CrundiColors.Accent).padding(horizontal = 7.dp, vertical = 1.dp),
                )
                Text(q.question, style = ChatType.Body.copy(fontWeight = FontWeight.SemiBold), modifier = Modifier.padding(bottom = 7.dp))
                for (o in q.options) {
                    val sel = picks.chosen[qi].orEmpty().contains(o.label)
                    val noteHere = picks.noteOpt[qi] == o.label
                    Row(
                        Modifier.fillMaxWidth().padding(bottom = 5.dp).clip(Dimens.ShapeSm)
                            .background(if (sel) CrundiColors.AccentDim else CrundiColors.BgPrimary)
                            .border(Dimens.Hairline, if (sel) CrundiColors.Accent else CrundiColors.Border, Dimens.ShapeSm)
                            .clickable(enabled = !busy) {
                                val cur = picks.chosen[qi].orEmpty()
                                picks.chosen[qi] = if (q.multiSelect) (if (sel) cur - o.label else cur + o.label) else listOf(o.label)
                                picks.other.remove(qi)
                                picks.followSelection(qi)
                            }
                            .padding(start = 10.dp, top = 9.dp, bottom = 9.dp, end = 2.dp),
                        horizontalArrangement = Arrangement.spacedBy(9.dp),
                    ) {
                        Choice(sel, q.multiSelect)
                        Column(Modifier.weight(1f)) {
                            Text(o.label, style = ChatType.Body.copy(fontWeight = FontWeight.SemiBold))
                            if (o.description.isNotEmpty()) Text(o.description, style = ChatType.Small)
                            if (noteHere) OptionNote(
                                picks.note[qi].orEmpty(), { picks.note[qi] = it },
                                onRemove = { picks.note.remove(qi); picks.noteOpt.remove(qi) }, enabled = !busy,
                            )
                        }
                        // The web's note button, in the choice's corner: picks
                        // the choice if it is not picked, and opens the note under it.
                        CrundiIconButton(
                            "note", {
                                if (!sel) {
                                    picks.chosen[qi] = if (q.multiSelect) picks.chosen[qi].orEmpty() + o.label else listOf(o.label)
                                    picks.other.remove(qi)
                                }
                                picks.noteOpt[qi] = o.label
                            },
                            modifier = Modifier.padding(top = 0.dp).offset(y = (-6).dp),
                            contentDescription = "Add a note to this choice", size = 15.dp, touch = 32.dp, enabled = !busy,
                            tint = if (noteHere) CrundiColors.AccentHover else CrundiColors.TextMuted,
                        )
                    }
                }
                CrundiTextField(
                    picks.other[qi].orEmpty(),
                    { picks.other[qi] = it; if (it.isNotEmpty()) picks.chosen.remove(qi) },
                    placeholder = "Other (type your own answer)", enabled = !busy, modifier = Modifier.fillMaxWidth().padding(top = 2.dp),
                )
            }
        }
        val ready = questions.indices.all { picks.answer(it).isNotEmpty() }
        FlowRow(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalArrangement = Arrangement.spacedBy(7.dp)) {
            CrundiButton("Submit", {
                actions.answer(
                    e, questions.mapIndexed { i, q -> q.question to picks.answer(i) }.toMap(),
                    questions.mapIndexedNotNull { i, q -> picks.noteFor(i).takeIf { it.isNotEmpty() }?.let { q.question to it } }.toMap(),
                )
            }, kind = ButtonKind.Primary, enabled = ready && !busy)
            if (!orphaned) CrundiButton("Dismiss", { actions.dismissQuestion(e) }, kind = ButtonKind.Danger, enabled = !busy)
        }
    }
}

/** The note under a choice: free text for Claude, and a cross that removes it. */
@Composable
private fun OptionNote(value: String, onChange: (String) -> Unit, onRemove: () -> Unit, enabled: Boolean) {
    val focus = remember { FocusRequester() }
    // Opened by the note button: the cursor goes straight into it.
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    Row(Modifier.fillMaxWidth().padding(top = 7.dp), verticalAlignment = Alignment.CenterVertically) {
        CrundiTextField(
            value, onChange, placeholder = "Note for Claude about this choice", enabled = enabled,
            modifier = Modifier.weight(1f).focusRequester(focus),
        )
        CrundiIconButton("x", onRemove, contentDescription = "Remove the note", size = 13.dp, touch = 32.dp, enabled = enabled, tint = CrundiColors.TextMuted)
    }
}

/** A radio (round) or checkbox (square), drawn in the desktop's accent. */
@Composable
private fun Choice(on: Boolean, multi: Boolean) {
    val shape = if (multi) RoundedCornerShape(3.dp) else CircleShape
    Box(
        Modifier.padding(top = 2.dp).size(16.dp).clip(shape).background(if (on) CrundiColors.Accent else CrundiColors.BgPrimary)
            .border(Dimens.Hairline, if (on) CrundiColors.Accent else CrundiColors.TextMuted, shape),
        contentAlignment = Alignment.Center,
    ) {
        if (on) { if (multi) CrundiIcon("check", size = 11.dp, tint = Color.White) else Box(Modifier.size(6.dp).clip(CircleShape).background(Color.White)) }
    }
}

// ─── A factory's result ───

private fun JsonObject.str(k: String) = (this[k] as? JsonPrimitive)?.contentOrNull ?: ""
private fun JsonObject.num(k: String) = (this[k] as? JsonPrimitive)?.let { it.longOrNull ?: it.doubleOrNull?.toLong() } ?: 0L
private fun JsonObject.dbl(k: String) = (this[k] as? JsonPrimitive)?.doubleOrNull ?: 0.0

/**
 * Posted into the Manager's chat when a run finishes and patched as the
 * person decides: merge, request changes, keep, discard, undo.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun FactoryResultCard(e: ChatEntry, actions: EntryActions, busy: Boolean = false, diff: String? = null, startOpen: Boolean = false) {
    val r = e.raw
    val d = r["diff"] as? JsonObject ?: JsonObject(emptyMap())
    val files = (d["files"] as? JsonArray)?.mapNotNull { it as? JsonObject }.orEmpty()
    val count = d.num("count").toInt()
    val parent = r.str("parentBranch").ifEmpty { "the parent branch" }
    val branch = r.str("branch")
    val state = r.str("state")
    var showFiles by remember(e.id) { mutableStateOf(startOpen) }
    var asking by remember(e.id) { mutableStateOf(false) }
    var note by remember(e.id) { mutableStateOf("") }
    var armed by remember(e.id) { mutableStateOf(false) }
    AskBox(permission = false) {
        AskTitle("Factory result" + r.str("factory").takeIf { it.isNotEmpty() }?.let { " · $it" }.orEmpty())
        if (r.str("summary").isNotEmpty()) Box(Modifier.padding(top = 4.dp, bottom = 8.dp)) { MarkdownText(r.str("summary"), compact = true) }
        Row(Modifier.clickable { showFiles = !showFiles }.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            CrundiIcon(if (showFiles) "chevron-down" else "chevron-right", size = 12.dp, tint = CrundiColors.TextSecondary)
            Text(buildAnnotatedString {
                withStyle(SpanStyle(color = CrundiColors.Green)) { append("+${d.num("added")}") }
                append(" ")
                withStyle(SpanStyle(color = CrundiColors.Red)) { append("−${d.num("removed")}") }
                withStyle(SpanStyle(color = CrundiColors.TextSecondary)) { append(" in $count file" + if (count == 1) "" else "s") }
                if (branch.isNotEmpty()) withStyle(SpanStyle(color = CrundiColors.TextMuted)) { append(" on $branch") }
            }, style = ChatType.Small)
        }
        if (showFiles) Column(Modifier.padding(top = 4.dp, bottom = 8.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            for (f in files) Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(f.str("file"), style = ChatType.ToolBody.copy(color = CrundiColors.TextPrimary), maxLines = 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                Text(buildAnnotatedString {
                    withStyle(SpanStyle(color = CrundiColors.Green)) { append("+" + (f.str("added").ifEmpty { "?" })) }
                    append(" ")
                    withStyle(SpanStyle(color = CrundiColors.Red)) { append("−" + (f.str("removed").ifEmpty { "?" })) }
                }, style = ChatType.ToolBody)
            }
            if (count > files.size) Text("and ${count - files.size} more", style = ChatType.Small)
            if (state in setOf("ask", "conflict", "changes")) {
                if (diff == null) CrundiButton("Show the full diff", { actions.factoryDiff(e) }, modifier = Modifier.padding(top = 6.dp))
                else Box(
                    Modifier.padding(top = 6.dp).fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary).border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.ShapeSm)
                        .heightIn(max = 380.dp).verticalScroll(rememberScrollState()).horizontalScroll(rememberScrollState()).padding(8.dp),
                ) { Text(diffColored(diff.ifEmpty { "(no changes)" }), style = ChatType.ToolBody, softWrap = false) }
            }
        }
        val commit = r.str("commit").take(7)
        val auto = (r["auto"] as? JsonPrimitive)?.booleanOrNull == true
        val msg = when (state) {
            "merged" -> (if (auto) "Merged automatically" else "Merged") + " into $parent as one commit" + (if (commit.isNotEmpty()) " $commit" else "") + "."
            "undone" -> "Undone. $parent is back where it was; the work is kept on $branch."
            "kept" -> "Kept on the branch $branch. Nothing was merged."
            "discarded" -> "Discarded. The worktree and branch are gone."
            "conflict" -> "Merging into $parent conflicts. The Manager is bringing $parent in and resolving it, then it will finish again."
            "changes" -> "Changes requested. The team is working on them; this card updates when the Manager finishes again."
            "ask" -> if (count == 0) "The team made no changes, so there is nothing to merge. Ask for changes, or close the run." else "Look it over, then choose what happens to it."
            else -> ""
        }
        if (msg.isNotEmpty()) Text(msg, style = ChatType.Small, modifier = Modifier.padding(top = 4.dp, bottom = 8.dp))
        (r["usage"] as? JsonObject)?.let { u ->
            if (u.num("turns") > 0 || u.dbl("costUsd") > 0) {
                val mins = (u.num("activeMs") / 60000).toInt()
                val bits = listOfNotNull(
                    if (u.dbl("costUsd") > 0) "$" + "%.2f".format(u.dbl("costUsd")) else null,
                    "${u.num("turns")} turns", "${u.num("handoffs")} handoffs", "${u.num("chats")} chats",
                    if (mins > 0) (if (mins >= 60) "${mins / 60}h ${mins % 60}m" else "${mins}m") + " active" else null,
                )
                Text(bits.joinToString(" · "), style = ChatType.Small.copy(fontSize = 11.sp), modifier = Modifier.padding(bottom = 6.dp))
            }
        }
        val followUps = (r["followUps"] as? JsonArray)?.mapNotNull { it as? JsonObject }.orEmpty()
        if (followUps.isNotEmpty()) {
            var open by remember(e.id) { mutableStateOf(false) }
            Row(Modifier.clickable { open = !open }.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                CrundiIcon(if (open) "chevron-down" else "chevron-right", size = 12.dp, tint = CrundiColors.TextSecondary)
                Text("${followUps.size} follow-up" + (if (followUps.size == 1) "" else "s") + " from review", style = ChatType.Small)
            }
            if (open) for (f in followUps) Text(buildAnnotatedString {
                withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(f.str("id") + " ") }
                if ((f["blocking"] as? JsonPrimitive)?.booleanOrNull == true) withStyle(SpanStyle(color = CrundiColors.Red)) { append("blocking ") }
                append(f.str("text"))
                if (f.str("where").isNotEmpty()) withStyle(SpanStyle(color = CrundiColors.TextMuted)) { append(" (" + f.str("where") + ")") }
            }, style = ChatType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.padding(bottom = 4.dp))
        }
        if (r.str("error").isNotEmpty()) AskReason(r.str("error"))
        when (state) {
            "ask" -> {
                val empty = count == 0
                FlowRow(horizontalArrangement = Arrangement.spacedBy(7.dp), verticalArrangement = Arrangement.spacedBy(7.dp), modifier = Modifier.padding(top = 2.dp)) {
                    if (!empty) CrundiButton("Merge into $parent", { actions.factory(e, "merge") }, kind = ButtonKind.Primary, enabled = !busy)
                    CrundiButton("Request changes", { asking = !asking }, kind = if (empty) ButtonKind.Primary else ButtonKind.Secondary, enabled = !busy)
                    if (!empty) CrundiButton("Keep branch", { actions.factory(e, "keep") }, enabled = !busy)
                    CrundiButton(
                        if (empty) "Close the run" else if (armed) "Sure? Discard" else "Discard",
                        { if (empty || armed) { armed = false; actions.factory(e, "discard") } else armed = true },
                        kind = ButtonKind.Danger, enabled = !busy,
                    )
                }
                if (asking) Column(Modifier.padding(top = 8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    CrundiTextField(note, { note = it }, placeholder = "What should change?", singleLine = false, minLines = 3, modifier = Modifier.fillMaxWidth())
                    CrundiButton("Send to the Manager", { if (note.isNotBlank()) actions.factory(e, "changes", note.trim()) }, kind = ButtonKind.Primary, enabled = !busy && note.isNotBlank())
                }
            }
            "merged" -> CrundiButton("Undo", { actions.factory(e, "undo") }, enabled = !busy)
        }
    }
}

private fun diffColored(diff: String) = buildAnnotatedString {
    diff.lines().take(4000).forEachIndexed { i, l ->
        if (i > 0) append("\n")
        val c = when {
            l.startsWith("+++") || l.startsWith("---") -> CrundiColors.TextPrimary
            l.startsWith("+") -> CrundiColors.Green
            l.startsWith("-") -> CrundiColors.Red
            l.startsWith("@@") -> CrundiColors.Sky
            l.startsWith("diff ") -> CrundiColors.AccentHover
            else -> CrundiColors.TextSecondary
        }
        withStyle(SpanStyle(color = c)) { append(l) }
    }
}
