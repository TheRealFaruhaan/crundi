package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth

import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.model.ChatAgent
import mv.obitech.crundi.api.model.ChatEntry
import mv.obitech.crundi.api.model.ChatMedia
import mv.obitech.crundi.api.rememberServerImage
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.MarkdownText
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/** The chat's text sizes: the desktop's chat runs at 13px with 12px tool rows. */
internal object ChatType {
    val Body = CrundiType.Body
    val Small = CrundiType.Small.copy(fontSize = 12.sp, lineHeight = 17.sp)
    val ToolName = CrundiType.Code.copy(fontSize = 12.sp, fontWeight = FontWeight.SemiBold, color = CrundiColors.AccentHover)
    val ToolBody = CrundiType.Code.copy(fontSize = 11.5.sp, lineHeight = 16.sp, color = CrundiColors.TextSecondary)
}

private val ansi = Regex("\u001b\\[[0-9;?]*[A-Za-z]")
internal fun stripAnsi(s: String) = ansi.replace(s, "")
internal fun truncate(s: String?, n: Int) = s.orEmpty().let { if (it.length > n) it.take(n) + "…" else it }

private fun JsonObject.s(k: String): String = (this[k] as? JsonPrimitive)?.contentOrNull ?: ""

private fun shortPath(p: String): String {
    if (p.isEmpty()) return ""
    val parts = p.split('/').filter { it.isNotEmpty() }
    return if (parts.size <= 3) p else "…/" + parts.takeLast(3).joinToString("/")
}

/** The tool row's one line, as the desktop's toolSummary. */
fun toolSummary(name: String, input: JsonObject): String = when (name) {
    "Read", "Write", "Edit", "MultiEdit" -> shortPath(input.s("file_path"))
    "NotebookEdit" -> shortPath(input.s("notebook_path"))
    "Bash" -> truncate(input.s("command"), 140)
    "Glob" -> input.s("pattern") + (input.s("path").takeIf { it.isNotEmpty() }?.let { " in " + shortPath(it) } ?: "")
    "Grep" -> input.s("pattern") + (input.s("glob").takeIf { it.isNotEmpty() }?.let { " ($it)" } ?: "")
    "WebFetch" -> input.s("url")
    "WebSearch" -> input.s("query")
    "Task", "Agent" -> input.s("description").ifEmpty { input.s("subagent_type") }
    "TodoWrite" -> "${(input["todos"] as? JsonArray)?.size ?: 0} items"
    "Skill" -> input.s("skill")
    else -> if (input.isEmpty()) "" else truncate(input.entries.joinToString(" ") { (k, v) -> "$k=" + v.toString() }, 140)
}

/** "mcp__crundi__show_image" -> "show_image", as a phone has no room for the server's name. */
fun toolLabel(name: String): String = if (name.startsWith("mcp__")) name.substringAfterLast("__") else name

/** The open tool card's body: the interesting part of its input. */
@Composable
private fun ToolDetail(name: String, input: JsonObject) {
    when (name) {
        "Bash" -> {
            Text(input.s("command"), style = ChatType.ToolBody.copy(color = CrundiColors.TextPrimary))
            if (input.s("description").isNotEmpty()) Text(input.s("description"), style = ChatType.ToolBody.copy(color = CrundiColors.TextMuted), modifier = Modifier.padding(top = 5.dp))
        }
        "Edit", "MultiEdit" -> {
            Text(input.s("file_path"), style = ChatType.ToolBody.copy(color = CrundiColors.TextMuted))
            val edits = (input["edits"] as? JsonArray)?.mapNotNull { it as? JsonObject } ?: listOf(input)
            for (e in edits) Text(diffText(e.s("old_string"), e.s("new_string")), style = ChatType.ToolBody, modifier = Modifier.padding(top = 4.dp))
        }
        "Write" -> {
            Text(input.s("file_path"), style = ChatType.ToolBody.copy(color = CrundiColors.TextMuted))
            Text(truncate(input.s("content"), 3000), style = ChatType.ToolBody, modifier = Modifier.padding(top = 4.dp))
        }
        "TodoWrite" -> Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            for (t in (input["todos"] as? JsonArray)?.mapNotNull { it as? JsonObject }.orEmpty()) {
                val st = t.s("status")
                Row(horizontalArrangement = Arrangement.spacedBy(7.dp)) {
                    CrundiIcon(if (st == "completed") "check" else if (st == "in_progress") "play" else "circle", size = 12.dp,
                        tint = if (st == "in_progress") CrundiColors.AccentHover else CrundiColors.TextMuted, modifier = Modifier.padding(top = 2.dp))
                    Text(
                        t.s("content"),
                        style = ChatType.ToolBody.copy(
                            color = when (st) { "completed" -> CrundiColors.TextMuted; "in_progress" -> CrundiColors.AccentHover; else -> CrundiColors.TextSecondary },
                            textDecoration = if (st == "completed") androidx.compose.ui.text.style.TextDecoration.LineThrough else null,
                        ),
                    )
                }
            }
        }
        else -> Text(truncate(prettyJson(input), 4000), style = ChatType.ToolBody)
    }
}

internal fun prettyJson(o: JsonElement): String = runCatching {
    kotlinx.serialization.json.Json { prettyPrint = true }.encodeToString(JsonElement.serializer(), o)
}.getOrDefault(o.toString())

/** Old lines in red, new in green, as .cc-diff-del / .cc-diff-add. */
private fun diffText(old: String, new: String): AnnotatedString = buildAnnotatedString {
    for (l in old.lines().take(60)) withStyle(SpanStyle(color = CrundiColors.Red, background = CrundiColors.RedDim)) { append("- $l\n") }
    val n = new.lines().take(60)
    n.forEachIndexed { i, l -> withStyle(SpanStyle(color = CrundiColors.Green, background = CrundiColors.GreenDim)) { append("+ $l"); if (i < n.size - 1) append("\n") } }
}

// ─── Entries ───

/** What an entry's buttons do. Defaults do nothing, for screenshots. */
interface EntryActions {
    fun toggle(id: String) {}
    fun allow(e: ChatEntry, always: Boolean) {}
    fun deny(e: ChatEntry) {}
    fun answer(e: ChatEntry, answers: Map<String, String>) {}
    fun dismissQuestion(e: ChatEntry) {}
    fun factory(e: ChatEntry, choice: String, note: String? = null) {}
    fun factoryDiff(e: ChatEntry) {}
    fun openAgent(toolUseId: String) {}
    fun media(m: ChatMedia) {}
}

/**
 * One entry of the conversation, drawn as the desktop's chat draws it.
 * [open]: the ids of folded things (tools, thinking) the person opened.
 */
@Composable
fun ChatEntryView(
    e: ChatEntry,
    chatId: String,
    open: Set<String>,
    actions: EntryActions,
    agents: Map<String, ChatAgent> = emptyMap(),
    busy: Set<String> = emptySet(),
    factoryDiffs: Map<String, String> = emptyMap(),
) {
    when (e.kind) {
        "user" -> UserBubble(e)
        "assistant-text" -> Assistant(e)
        "thinking" -> Thinking(e, e.id in open || (e.streaming && "!" + e.id !in open), actions)
        "tool" -> {
            val agent = agents[e.toolUseId]
            if (agent != null && agent.kind != "task") AgentRow(agent, actions)
            else Tool(e, e.id in open, actions)
            val media = if (e.status == "done") e.media(chatId) else emptyList()
            if (media.isNotEmpty()) Column(Modifier.padding(top = 6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) { for (m in media) MediaView(m, actions) }
        }
        "permission" -> PermissionCard(e, actions, busy = e.id in busy)
        "question" -> QuestionCard(e, actions, busy = e.id in busy)
        "factory-result" -> FactoryResultCard(e, actions, busy = e.id in busy, diff = factoryDiffs[e.id])
        "result" -> ResultLine(e)
        "goal-verdict" -> GoalVerdict(e)
        "notice" -> Text(e.text, style = ChatType.Small.copy(color = CrundiColors.TextMuted, fontStyle = FontStyle.Italic), textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth())
        "error" -> ErrorBox(e.text)
        "gone" -> {}
        else -> if (e.text.isNotEmpty()) Text(e.text, style = ChatType.Small)
    }
}

@Composable
private fun UserBubble(e: ChatEntry) {
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.End) {
        val by = when (e.by) { "owner" -> "Owner"; "system" -> "Crundi"; else -> e.by.takeIf { it.isNotEmpty() && it != "self" } }
        if (by != null) Text(by, style = CrundiType.Meta.copy(color = CrundiColors.Sky, fontWeight = FontWeight.SemiBold, fontFamily = CrundiType.Sans), modifier = Modifier.padding(end = 4.dp, bottom = 2.dp))
        val shape = RoundedCornerShape(12.dp, 12.dp, 3.dp, 12.dp)
        SelectionContainer {
            Text(
                e.text, style = ChatType.Body,
                modifier = Modifier.widthIn(max = 330.dp).clip(shape).background(CrundiColors.AccentDim)
                    .border(Dimens.Hairline, CrundiColors.Accent.copy(alpha = 0.35f), shape).padding(horizontal = 11.dp, vertical = 7.dp),
            )
        }
    }
}

@Composable
private fun Assistant(e: ChatEntry) {
    Column {
        SelectionContainer { MarkdownText(e.text, compact = true) }
        if (e.streaming) Box(Modifier.padding(top = 2.dp).size(6.dp, 13.dp).background(CrundiColors.Accent))
    }
}

@Composable
private fun Thinking(e: ChatEntry, open: Boolean, actions: EntryActions) {
    if (e.text.isEmpty()) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Dot(CrundiColors.TextMuted.copy(alpha = 0.55f), 5.dp)
            Text("Thought" + (if (e.tokens > 0) " for ~${tokens(e.tokens)} tokens" else ""), style = ChatType.Small.copy(color = CrundiColors.TextMuted, fontStyle = FontStyle.Italic))
        }
        return
    }
    Row(Modifier.height(IntrinsicSize.Min)) {
        Box(Modifier.width(2.dp).fillMaxHeight().background(CrundiColors.Border))
        Column(Modifier.padding(start = 9.dp)) {
            Row(Modifier.clickable { actions.toggle(if (e.streaming) "!" + e.id else e.id) }.padding(vertical = 2.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                CrundiIcon(if (open) "chevron-down" else "chevron-right", size = 12.dp, tint = CrundiColors.TextSecondary)
                Text(if (e.streaming) "Thinking" else "Thinking", style = ChatType.Small.copy(color = CrundiColors.TextSecondary))
                if (e.streaming) Spinner(9.dp)
            }
            if (open) Text(
                e.text, style = ChatType.Small.copy(color = CrundiColors.TextMuted, fontStyle = FontStyle.Italic),
                modifier = Modifier.padding(top = 4.dp).heightIn(max = 260.dp).verticalScroll(rememberScrollState()),
            )
        }
    }
}

@Composable
private fun StatusMark(running: Boolean, error: Boolean) {
    Box(Modifier.size(10.dp), contentAlignment = Alignment.Center) {
        when {
            running -> Spinner(9.dp)
            error -> Dot(CrundiColors.Red)
            else -> Dot(CrundiColors.Green)
        }
    }
}

@Composable
private fun Tool(e: ChatEntry, open: Boolean, actions: EntryActions) {
    Column(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgSecondary).border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.ShapeSm),
    ) {
        Row(
            Modifier.fillMaxWidth().clickable { actions.toggle(e.id) }.padding(horizontal = 9.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            StatusMark(e.status == "running", e.isError)
            Text(toolLabel(e.name), style = ChatType.ToolName.copy(color = if (e.isError) CrundiColors.Red else CrundiColors.AccentHover), maxLines = 1)
            Text(toolSummary(e.name, e.input), style = ChatType.Small, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            CrundiIcon(if (open) "chevron-down" else "chevron-right", size = 12.dp, tint = CrundiColors.TextMuted)
        }
        if (open) {
            Box(Modifier.fillMaxWidth().height(Dimens.Hairline).background(CrundiColors.BorderSubtle))
            SelectionContainer {
                Column(Modifier.fillMaxWidth().heightIn(max = 340.dp).verticalScroll(rememberScrollState()).padding(horizontal = 9.dp, vertical = 8.dp)) {
                    ToolDetail(e.name, e.input)
                    if (!e.result.isNullOrEmpty()) {
                        Box(Modifier.padding(top = 7.dp).fillMaxWidth().height(Dimens.Hairline).background(CrundiColors.BorderSubtle))
                        Text(truncate(stripAnsi(e.result), 6000), style = ChatType.ToolBody.copy(color = if (e.isError) CrundiColors.Red else CrundiColors.TextMuted), modifier = Modifier.padding(top = 7.dp))
                    }
                }
            }
        }
    }
}

/** A subagent's row in the conversation: opens its transcript. */
@Composable
fun AgentRow(a: ChatAgent, actions: EntryActions) {
    Row(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgSecondary).border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.ShapeSm)
            .clickable { actions.openAgent(a.toolUseId) }.padding(horizontal = 9.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp),
    ) {
        StatusMark(a.running || a.status.isEmpty(), a.status == "failed")
        Text(a.subagentType.ifEmpty { "Agent" }, style = ChatType.ToolName, maxLines = 1)
        Column(Modifier.weight(1f)) {
            Text(a.description, style = ChatType.Small, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (a.running && a.step.isNotEmpty()) Text(a.step, style = CrundiType.Meta, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Text("transcript", style = CrundiType.Meta)
        CrundiIcon("chevron-right", size = 12.dp, tint = CrundiColors.TextMuted)
    }
}

/** A picture, clip, sound, file or frame Claude showed. */
@Composable
fun MediaView(m: ChatMedia, actions: EntryActions) {
    when (m.kind) {
        "image" -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            val img = rememberServerImage(m.path, 1280)
            val ratio = if (m.width > 0 && m.height > 0) (m.width.toFloat() / m.height).coerceIn(0.4f, 3f) else 16f / 10f
            Box(
                Modifier.widthIn(max = 360.dp).heightIn(max = 420.dp).aspectRatio(ratio).clip(Dimens.ShapeSm).background(CrundiColors.BgSecondary)
                    .border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).clickable { actions.media(m) },
                contentAlignment = Alignment.Center,
            ) {
                if (img != null) Image(img, contentDescription = m.caption.ifEmpty { "Image" }, contentScale = ContentScale.Fit, modifier = Modifier.fillMaxWidth())
                else CrundiIcon("image", size = 28.dp, tint = CrundiColors.TextMuted)
            }
            if (m.caption.isNotEmpty()) Text(m.caption, style = ChatType.Small)
        }
        else -> {
            val (icon, label, verb) = when (m.kind) {
                "video" -> Triple("film", m.caption.ifEmpty { "Video" }, "Download")
                "audio" -> Triple("music", m.caption.ifEmpty { "Audio" }, "Download")
                "embed" -> Triple("globe", m.title.ifEmpty { m.provider.ifEmpty { "Live frame" } }, if (m.openUrl != null) "Open" else "Download")
                else -> Triple("file", m.filename.ifEmpty { "File" }, "Download")
            }
            Row(
                Modifier.widthIn(max = 360.dp).fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.BgSecondary)
                    .border(Dimens.Hairline, CrundiColors.Border, Dimens.ShapeSm).padding(start = 12.dp, end = 6.dp, top = 6.dp, bottom = 6.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp),
            ) {
                CrundiIcon(icon, size = 20.dp, tint = CrundiColors.AccentHover)
                Column(Modifier.weight(1f)) {
                    Text(label, style = ChatType.Body, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    val meta = listOfNotNull(
                        m.filename.takeIf { it.isNotEmpty() && it != label },
                        if (m.size > 0) bytes(m.size) else null,
                        if (m.durationSec > 0) "%d:%02d".format(m.durationSec.toInt() / 60, m.durationSec.toInt() % 60) else null,
                        m.provider.takeIf { m.kind == "embed" && it.isNotEmpty() && it != label },
                    ).joinToString(" · ")
                    if (meta.isNotEmpty()) Text(meta, style = CrundiType.Meta, maxLines = 1)
                    if (m.kind != "file" && m.caption.isNotEmpty() && m.caption != label) Text(m.caption, style = ChatType.Small, maxLines = 2)
                }
                CrundiButton(verb, { actions.media(m) }, kind = ButtonKind.Ghost, icon = if (verb == "Open") "external-link" else "download")
            }
        }
    }
}

@Composable
private fun ResultLine(e: ChatEntry) {
    if (e.subtype.isNotEmpty() && e.subtype != "success") { ErrorBox(e.text.ifEmpty { e.subtype }); return }
    val bits = listOfNotNull(
        if (e.durationMs > 0) "%.1fs".format(e.durationMs / 1000.0) else null,
        if (e.costUsd > 0) "$" + "%.4f".format(e.costUsd) else null,
    )
    Column(Modifier.fillMaxWidth()) {
        DashedRule()
        Text(bits.joinToString(" · "), style = CrundiType.Meta.copy(fontFamily = CrundiType.Sans), textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth().padding(top = 3.dp))
    }
}

@Composable
private fun DashedRule() {
    androidx.compose.foundation.Canvas(Modifier.fillMaxWidth().height(1.dp)) {
        drawLine(
            CrundiColors.BorderSubtle, androidx.compose.ui.geometry.Offset(0f, 0f), androidx.compose.ui.geometry.Offset(size.width, 0f),
            strokeWidth = 1.dp.toPx(), pathEffect = androidx.compose.ui.graphics.PathEffect.dashPathEffect(floatArrayOf(4f, 4f)),
        )
    }
}

@Composable
private fun GoalVerdict(e: ChatEntry) {
    Column(
        Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.YellowDim).border(Dimens.Hairline, CrundiColors.Yellow.copy(alpha = 0.5f), Dimens.ShapeSm).padding(horizontal = 10.dp, vertical = 8.dp),
    ) {
        val n = (e.raw["n"] as? JsonPrimitive)?.contentOrNull ?: "1"
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Dot(CrundiColors.Yellow)
            Text("Goal not yet met · check $n", style = ChatType.Small.copy(color = CrundiColors.Yellow, fontWeight = FontWeight.SemiBold))
        }
        Text(e.text, style = ChatType.Small.copy(color = CrundiColors.TextPrimary), modifier = Modifier.padding(top = 4.dp))
    }
}

@Composable
internal fun ErrorBox(text: String) {
    Text(
        text, style = ChatType.Small.copy(color = CrundiColors.Red),
        modifier = Modifier.fillMaxWidth().clip(Dimens.ShapeSm).background(CrundiColors.RedDim).border(Dimens.Hairline, CrundiColors.Red, Dimens.ShapeSm).padding(horizontal = 10.dp, vertical = 7.dp),
    )
}

/** The in-log activity row: a turn that streams nothing still shows it is alive. */
@Composable
fun ActivityRow(label: String) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(vertical = 2.dp)) {
        Spinner(11.dp)
        Text(label, style = ChatType.Small.copy(color = CrundiColors.TextMuted, fontStyle = FontStyle.Italic))
    }
}

