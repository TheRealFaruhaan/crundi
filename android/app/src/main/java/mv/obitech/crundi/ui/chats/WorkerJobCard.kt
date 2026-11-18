package mv.obitech.crundi.ui.chats

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.api.model.ChatEntry
import mv.obitech.crundi.api.model.WorkerJob
import mv.obitech.crundi.ui.components.ButtonKind
import mv.obitech.crundi.ui.components.CrundiButton
import mv.obitech.crundi.ui.components.StatusBadge
import mv.obitech.crundi.ui.icons.CrundiIcon
import mv.obitech.crundi.ui.operations.Fmt
import mv.obitech.crundi.ui.operations.jobChip
import mv.obitech.crundi.ui.operations.jobDuration
import mv.obitech.crundi.ui.theme.CrundiColors
import mv.obitech.crundi.ui.theme.CrundiType
import mv.obitech.crundi.ui.theme.Dimens

/**
 * The line under a worker-job card's command, as the web's workerJobNode:
 * branch and short commit (or the source), "N of M files sent" for a copy,
 * the duration, and "N files back (X MB)".
 */
internal fun workerJobMeta(j: WorkerJob): String = listOfNotNull(
    if (!j.branch.isNullOrEmpty()) j.branch + (j.commit?.takeIf { it.isNotEmpty() }?.let { " " + it.take(7) } ?: "") else j.source?.ifEmpty { null },
    j.copy?.takeIf { it.files > 0 || it.sent > 0 }?.let { c -> "${c.sent} of ${c.files} file${if (c.files == 1) "" else "s"} sent" },
    j.durationMs?.takeIf { it > 0 }?.let { jobDuration(it) },
    j.artifacts?.takeIf { it.count > 0 }?.let { a -> "${a.count} file${if (a.count == 1) "" else "s"} back (${Fmt.bytes(a.bytes)})" },
).joinToString(" · ")

/** A command a chat ran on one of the owner's PCs (worker_run), patched live by the server. */
@Composable
fun WorkerJobCard(e: ChatEntry, actions: EntryActions, busy: Boolean = false) {
    val j = e.workerJob
    Column(
        Modifier.fillMaxWidth().clip(Dimens.Shape).background(CrundiColors.BgCard).border(Dimens.Hairline, CrundiColors.Border, Dimens.Shape)
            .padding(horizontal = Dimens.S3, vertical = 10.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(Dimens.S2)) {
            CrundiIcon("server", size = 14.dp, tint = CrundiColors.TextSecondary)
            Text(
                "On " + (j.worker?.ifEmpty { null } ?: j.workerId?.ifEmpty { null } ?: "a PC"),
                style = ChatType.Body.copy(fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (j.state.isNotEmpty()) { val (word, kind) = jobChip(j); StatusBadge(word, kind) }
        }
        if (j.command.isNotEmpty()) SelectionContainer {
            Text(j.command, style = ChatType.ToolBody.copy(color = CrundiColors.TextPrimary), maxLines = 6, overflow = TextOverflow.Ellipsis)
        }
        val meta = workerJobMeta(j)
        if (meta.isNotEmpty()) Text(meta, style = CrundiType.Meta)
        if (j.error.isNotEmpty()) Text(j.error, style = ChatType.Small.copy(color = CrundiColors.Red))
        if (j.notes.isNotEmpty()) Text(j.notes.joinToString("\n"), style = ChatType.Small)
        val tail = j.tail?.let(::stripAnsi)?.trimEnd()
        if (!tail.isNullOrEmpty()) {
            val scroll = rememberScrollState()
            // Kept at the end as the output grows, as the web's box.
            LaunchedEffect(tail, scroll.maxValue) { scroll.scrollTo(scroll.maxValue) }
            SelectionContainer {
                Box(
                    Modifier.fillMaxWidth().heightIn(max = 220.dp).clip(Dimens.ShapeSm).background(CrundiColors.BgPrimary)
                        .border(Dimens.Hairline, CrundiColors.BorderSubtle, Dimens.ShapeSm).verticalScroll(scroll).padding(Dimens.S2),
                ) {
                    Text(tail, style = ChatType.ToolBody, softWrap = false, modifier = Modifier.horizontalScroll(rememberScrollState()))
                }
            }
        }
        val saved = j.artifacts?.takeIf { it.count > 0 && it.path.isNotEmpty() }
        if (saved != null) SelectionContainer {
            Text("Saved on the server: " + saved.path, style = ChatType.Small)
        }
        if (j.running && j.job.isNotEmpty()) Row(Modifier.padding(top = 2.dp)) {
            CrundiButton("Cancel", { actions.cancelWorkerJob(e) }, kind = ButtonKind.Danger, icon = "stop", enabled = !busy)
        }
    }
}
