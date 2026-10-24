package mv.obitech.crundi.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import mv.obitech.crundi.api.model.SkillDetail
import mv.obitech.crundi.api.model.SkillFile
import mv.obitech.crundi.api.model.SkillFileEntry
import mv.obitech.crundi.api.model.SkillLinkFound
import mv.obitech.crundi.api.model.SkillList
import mv.obitech.crundi.api.model.SkillSource
import mv.obitech.crundi.ui.components.DialogCard
import mv.obitech.crundi.ui.operations.Actions
import mv.obitech.crundi.ui.operations.Load
import mv.obitech.crundi.ui.operations.ScopeOptions
import mv.obitech.crundi.ui.operations.SkillEdit
import mv.obitech.crundi.ui.operations.SkillForm
import mv.obitech.crundi.ui.operations.SkillPending
import mv.obitech.crundi.ui.operations.SkillRef
import mv.obitech.crundi.ui.operations.SkillsScreen
import mv.obitech.crundi.ui.operations.SkillsUi
import mv.obitech.crundi.ui.theme.CrundiColors
import org.junit.Test

/**
 * Settings > Skills: the list, a skill of the user's own, one that ships
 * with Crundi (read-only), the editor, and a server too old to have skills.
 */
class SkillsScreens : ScreenshotTest() {
    private val list: SkillList = FakeApi.model("skills")
    private val detail: SkillDetail = FakeApi.model("skills-detail")
    private val file: SkillFile = FakeApi.model("skills-file")
    private val ready = SkillsUi(data = Load.Ready(list))

    private val own = ready.copy(open = SkillRef("global", "frontend-design"), detail = Load.Ready(detail))
    private val shipped = ready.copy(
        open = SkillRef("global", "crundi"),
        detail = Load.Ready(
            SkillDetail(
                skill = list.skills[0], path = "/home/crundi/.claude/skills/crundi",
                files = listOf(SkillFileEntry("SKILL.md", 21480), SkillFileEntry("references/linux.md", 14210), SkillFileEntry("references/windows.md", 11870), SkillFileEntry("references/ports.md", 653)),
                readOnlyReason = "This skill ships with Crundi and is replaced on every update. Copy it to change it.",
            ),
        ),
    )

    @Composable
    private fun InDialog(content: @Composable () -> Unit) {
        Box(Modifier.fillMaxSize().background(CrundiColors.Scrim).padding(16.dp), contentAlignment = Alignment.Center) { content() }
    }

    @Test fun skillsList() = shoot("settings-skills") { SkillsScreen(ready.copy(showSynced = true)) }

    @Test fun skillsAdd() = shoot("settings-skills-add") {
        SkillsScreen(ready.copy(target = "project:crundi", form = SkillForm.New, pending = SkillPending(upload = true, scope = "global", existing = listOf("frontend-design"))))
    }

    @Test fun skillsNeedsName() = shoot("settings-skills-name") {
        SkillsScreen(ready.copy(pending = SkillPending(upload = true, scope = "global", needsName = true), error = ""))
    }

    @Test fun skillsDetail() = shoot("settings-skills-detail") { SkillsScreen(own.copy(note = Actions.Note("Installed frontend-design. New chats will have it.", false))) }

    @Test fun skillsDetailForms() = shoot("settings-skills-newfile") { SkillsScreen(own.copy(form = SkillForm.NewFile)) }

    @Test fun skillsShipped() = shoot("settings-skills-shipped") { SkillsScreen(shipped) }

    @Test fun skillsShippedCopy() = shoot("settings-skills-copy") { SkillsScreen(shipped.copy(form = SkillForm.Transfer)) }

    @Test fun skillsEditor() = shoot("settings-skills-editor") {
        SkillsScreen(own.copy(file = SkillEdit(file.path, file.text, file.text + "\n## Motion\n\n- One idea per screen.\n", size = file.size)))
    }

    @Test fun skillsEditorShipped() = shoot("settings-skills-editor-shipped") {
        SkillsScreen(shipped.copy(file = SkillEdit("SKILL.md", file.text.replace("frontend-design", "crundi"), readOnly = true, size = 21480)))
    }

    @Test fun skillsEditorBinary() = shoot("settings-skills-editor-binary") {
        SkillsScreen(own.copy(file = SkillEdit("assets/logo.png", binary = true, size = 48120)))
    }

    @Test fun skillsUnsupported() = shoot("settings-skills-unsupported") {
        SkillsScreen(SkillsUi(data = Load.Failed("This server does not support this yet. Update the server to use it."), unsupported = true))
    }

    @Test fun skillsDelete() = shoot("settings-skills-delete") {
        InDialog {
            DialogCard(
                "Delete this skill?", message = "Delete the skill \"frontend-design\" and all its files? This cannot be undone.",
                confirm = "Delete skill", danger = true,
            )
        }
    }

    @Test fun skillsReplaceFile() = shoot("settings-skills-replace") {
        InDialog { DialogCard("Replace this file?", message = "Replace SKILL.md in this skill?", confirm = "Replace file", danger = true) }
    }

    /** The list could not catch up (the reload failed): the skill still shows from its own detail. */
    @Test fun skillsDetailWithoutList() = shoot("settings-skills-detail-nolist") {
        SkillsScreen(SkillsUi(data = Load.Failed("The server cannot be reached."), open = SkillRef("global", "frontend-design"), detail = Load.Ready(detail)))
    }

    /** A link to a repository with several skills: which one, or all. */
    @Test fun skillsLinkChoose() = shoot("settings-skills-link-choose") {
        SkillsScreen(ready.copy(link = "npx skills add https://github.com/latent-spaces/brag", pending = SkillPending(
            upload = false, scope = "global", link = "https://github.com/latent-spaces/brag",
            message = "latent-spaces/brag holds 2 skills. Choose one, or install all of them.",
            choices = listOf(SkillLinkFound("brag", description = "Make a launch video from the project's code."), SkillLinkFound("brag-slim", description = "A shorter launch video.")),
        )))
    }

    /** An older server: the link field says so, and upload still works. */
    @Test fun skillsLinkUnsupported() = shoot("settings-skills-link-unsupported") { SkillsScreen(ready.copy(linkSupported = false)) }

    /** A skill installed from GitHub: where from, and Update. */
    @Test fun skillsFromGithub() = shoot("settings-skills-github") {
        val src = SkillSource("latent-spaces/brag", "main", "skills/brag-slim", "3f2a9c1d0b8e7f6a5c4d3e2f1a0b9c8d7e6f5a4b", "2026-10-24T10:00:00.000Z")
        SkillsScreen(own.copy(data = Load.Ready(list.copy(skills = list.skills.map { if (it.name == "frontend-design") it.copy(source = src) else it }))))
    }

    @Test fun skillsScopePicker() = shoot("settings-skills-where") {
        InDialog { DialogCard("Where", dismiss = "Cancel") { ScopeOptions(list.scopes, "project:crundi") {} } }
    }
}
