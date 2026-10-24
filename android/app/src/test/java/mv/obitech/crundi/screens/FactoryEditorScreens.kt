package mv.obitech.crundi.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.model.Factory
import mv.obitech.crundi.api.model.FactoryDetail
import mv.obitech.crundi.api.model.FactoryIssue
import mv.obitech.crundi.api.model.FactoryValidation
import mv.obitech.crundi.api.model.SkillBrief
import mv.obitech.crundi.ui.planning.EditSel
import mv.obitech.crundi.ui.planning.EditorTab
import mv.obitech.crundi.ui.planning.FactoriesData
import mv.obitech.crundi.ui.planning.FactoriesScreen
import mv.obitech.crundi.ui.planning.FactoriesTab
import mv.obitech.crundi.ui.planning.FactoryDetailScreen
import mv.obitech.crundi.ui.planning.FactoryEdit
import mv.obitech.crundi.ui.planning.FactoryEditorScreen
import mv.obitech.crundi.ui.planning.FactoryEditorState
import mv.obitech.crundi.ui.planning.Load
import mv.obitech.crundi.ui.planning.NewFactoryFields
import mv.obitech.crundi.ui.planning.NewFactoryForm
import mv.obitech.crundi.ui.planning.SaveState
import mv.obitech.crundi.ui.planning.SheetCard
import mv.obitech.crundi.ui.theme.CrundiColors
import org.junit.Test

/**
 * The factory editor at phone size, as the web's phone layout of
 * tools/factory-flow: the flow, and each panel of the bottom sheet (Add,
 * a step, a connection, the factory's settings and limits, the team, a
 * person, issues), connecting, a save conflict; New factory and the
 * factory page's tappable issues.
 */
class FactoryEditorScreens : ScreenshotTest() {
    private fun detail(name: String, validation: FactoryValidation? = null): FactoryDetail {
        val o = FakeApi.json("factory-template-$name")
        val raw = o["factory"]!!.jsonObject
        return FactoryDetail(CrundiJson.decodeFromJsonElement<Factory>(raw), validation ?: CrundiJson.decodeFromJsonElement(o["validation"]!!), raw)
    }
    private val skills = listOf("crundi", "frontend-design", "run", "logo-design").map { SkillBrief(it, scope = "global") }
    private fun editor(name: String = "squad", validation: FactoryValidation? = null, setup: FactoryEditorState.() -> Unit = {}) =
        FactoryEditorState(detail(name, validation), skills).apply(setup)

    private fun shot(name: String, st: FactoryEditorState) = shoot(name) { FactoryEditorScreen(Load.Ready(st)) }

    @Test fun flow() = shot("factories-editor", editor())
    @Test fun add() = shot("factories-editor-add", editor { tab = EditorTab.Add })
    @Test fun step() = shot("factories-editor-step", editor { select(EditSel.Node("ok")) })
    @Test fun fork() = shot("factories-editor-fork", editor { select(EditSel.Node("parts")) })
    @Test fun connection() = shot("factories-editor-connection", editor { select(EditSel.Edge("e23")) })
    @Test fun settings() = shot("factories-editor-settings", editor { tab = EditorTab.Edit })
    @Test fun team() = shot("factories-editor-team", editor { tab = EditorTab.Team })
    @Test fun person() = shot("factories-editor-person", editor { tab = EditorTab.Team; rosterSel = "rev" })
    @Test fun connect() = shot("factories-editor-connect", editor { startConnect("ok", "no") })

    /** A draft: the review's No leads nowhere and the Builder has no persona. */
    private val draft = FactoryValidation(
        ok = false,
        errors = listOf(
            FactoryIssue("output-unwired", "\"Approved?\" has nowhere to go for the answer \"no\".", "Connect it to the next step.", nodeId = "ok", handle = "no"),
            FactoryIssue("decision-no-question", "A decision has no question.", "Write the question this decision asks.", nodeId = "ok"),
        ),
        warnings = listOf(FactoryIssue("roster-no-persona", "\"Developer\" has no persona.", "Describe who this employee is and how they work.", employeeId = "dev")),
    )
    @Test fun issues() = shot("factories-editor-issues", editor("pair", draft) {
        mutate { f -> f.copy(edges = f.edges.filterNot { it.source == "ok" && it.sourceHandle == "no" }) }
        savedRev = rev; save = SaveState.Saved
        tab = EditorTab.Issues
    })
    @Test fun conflict() = shot("factories-editor-conflict", editor("pair") {
        mutate { f -> FactoryEdit.updateMember(f, "dev") { it.copy(model = "sonnet") } }
        save = SaveState.Conflict
    })

    // ─── New factory, and the factory page's issues ───

    @Test fun newFactory() = shoot("factories-new") {
        val fdata = FactoriesData(Load.Ready(PlanningFakes.summaries), PlanningFakes.templates)
        FactoriesScreen(fdata, FactoriesTab.Library, {}, "crundi")
        Box(Modifier.fillMaxSize().background(CrundiColors.Scrim)) {
            SheetCard("New factory", {}, Modifier.align(Alignment.BottomCenter)) {
                NewFactoryFields(NewFactoryForm("Pair and review", "template", PlanningFakes.templates.first().id), PlanningFakes.templates, PlanningFakes.summaries) {}
            }
        }
    }

    @Test fun factoryIssues() = shoot("factories-factory-issues") { FactoryDetailScreen(Load.Ready(detail("pair", draft))) }
}
