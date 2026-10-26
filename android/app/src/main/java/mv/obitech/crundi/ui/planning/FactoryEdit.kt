package mv.obitech.crundi.ui.planning

import kotlinx.serialization.Serializable
import mv.obitech.crundi.api.CrundiJson
import mv.obitech.crundi.api.model.CrField
import mv.obitech.crundi.api.model.Factory
import mv.obitech.crundi.api.model.FactoryAsk
import mv.obitech.crundi.api.model.FactoryDecision
import mv.obitech.crundi.api.model.FactoryEdge
import mv.obitech.crundi.api.model.FactoryJoin
import mv.obitech.crundi.api.model.FactoryMember
import mv.obitech.crundi.api.model.FactoryNode
import mv.obitech.crundi.api.model.FactoryTools
import mv.obitech.crundi.api.model.FactoryWait
import mv.obitech.crundi.api.model.Option
import mv.obitech.crundi.api.model.Position
import kotlin.math.max
import kotlin.math.roundToInt

// ═══ What the editor offers, as the web's (src/factory-personas.js, factory-model.js, factory-cr.js) ═══

@Serializable data class PersonaTemplate(val key: String, val name: String = "", val memory: String = "keep", val deny: List<String> = emptyList(), val skills: List<String> = emptyList(), val presets: List<String> = emptyList(), val persona: String = "")
@Serializable data class PresetInfo(val key: String, val label: String = "")
@Serializable data class CrLimits(val title: Int = 120, val section: Int = 6000, val sections: Int = 12, val label: Int = 60, val hint: Int = 300)

/**
 * The persona templates, permission presets, the Manager's default persona
 * and the default change request form: the server's own lists, copied into
 * resources/…/factory-editor-data.json by android/tools/gen-factory-data.mjs
 * (npm run check fails when they drift apart).
 */
@Serializable
data class FactoryEditorData(
    val personas: List<PersonaTemplate> = emptyList(),
    val readOnly: List<String> = listOf("Edit", "Write", "MultiEdit", "NotebookEdit"),
    val presets: List<PresetInfo> = emptyList(),
    val defaultPresets: List<String> = listOf("git-read", "tests"),
    val managerPersona: String = "",
    val crTemplate: List<CrField> = emptyList(),
    val crLimits: CrLimits = CrLimits(),
) {
    companion object {
        val bundled: FactoryEditorData by lazy {
            runCatching {
                val text = FactoryEditorData::class.java.getResourceAsStream("/mv/obitech/crundi/factory-editor-data.json")!!.bufferedReader().use { it.readText() }
                CrundiJson.decodeFromString(serializer(), text)
            }.getOrDefault(FactoryEditorData())
        }
    }
}

// ═══ Changing a factory: the web editor's moves (tools/factory-flow/src), as pure functions ═══

/**
 * Every change the web editor makes to a factory, kept the same: adding a
 * step lands where the web's palette puts it, connecting replaces what the
 * output led to and gives a loop a round limit of 3, deleting a person takes
 * their steps with them. Pure, so the unit tests hold them to the web.
 */
object FactoryEdit {
    const val MANAGER_ID = "manager"
    val AGENTS = setOf("manager", "employee")
    /** Start, the Manager and End are always there; they cannot be deleted. */
    val FIXED = setOf("start", "manager", "end")
    val EDIT_TOOLS = listOf("Edit", "Write", "MultiEdit", "NotebookEdit")
    val MODELS = listOf("" to "Default", "opus" to "Opus", "sonnet" to "Sonnet", "haiku" to "Haiku")
    val EFFORTS = listOf("" to "Default") + listOf("low", "medium", "high", "xhigh", "max").map { it to it }
    val KIND_LABEL = mapOf("start" to "Start", "manager" to "Manager", "employee" to "Employee", "decision" to "Decision", "join" to "Join", "ask" to "Ask me", "wait" to "Wait", "end" to "End")

    private val rnd = java.security.SecureRandom()

    /** As factory-model.js genId: the prefix and 12 hex digits. */
    fun genId(prefix: String = ""): String {
        val b = ByteArray(6).also { rnd.nextBytes(it) }
        return prefix + b.joinToString("") { "%02x".format(it.toInt() and 0xff) }
    }

    // ─── The graph ───

    /** The outputs a step offers (factory-model.js outputsOf). */
    fun outputsOf(n: FactoryNode): List<String> = when (n.type) {
        "start", "employee", "join" -> listOf("out")
        "manager" -> listOf("out", "finish")
        "decision" -> {
            val d = n.decision ?: FactoryDecision()
            if (d.kind == "yesno") listOf("yes", "no") else d.options.map { it.id } + if (d.kind == "many") listOf("none") else emptyList()
        }
        "ask" -> n.ask?.replies?.map { it.id }.orEmpty()
        "wait" -> if (n.wait?.kind == "command" || n.wait?.kind == "signal") listOf("done", "timeout") else listOf("out")
        else -> emptyList()
    }

    /** The words for an output, as the canvas shows them (convert.js handleLabel). */
    fun handleLabel(n: FactoryNode, h: String): String = when {
        h == "out" -> ""
        h == "finish" -> "Finish"
        h == "note" -> "Note"
        n.type == "decision" -> when (h) { "yes" -> "Yes"; "no" -> "No"; "none" -> "None"; else -> n.decision?.options?.firstOrNull { it.id == h }?.label ?: "?" }
        n.type == "ask" -> n.ask?.replies?.firstOrNull { it.id == h }?.label ?: "?"
        n.type == "wait" -> when (h) { "done" -> "Done"; "timeout" -> "Timed out"; else -> "" }
        else -> h
    }

    /** What a step is called in the panels (Inspector.jsx name()). */
    fun nodeName(f: Factory, n: FactoryNode?): String = when {
        n == null -> "?"
        n.label.isNotEmpty() -> n.label
        n.type == "employee" -> f.roster.firstOrNull { it.id == n.employeeId }?.name?.ifEmpty { null } ?: KIND_LABEL[n.type].orEmpty()
        else -> KIND_LABEL[n.type] ?: n.type
    }

    /** Steps reachable from [roots] over flow connections (factory-model.js reachable). */
    fun reachable(f: Factory, roots: Collection<String>): Set<String> {
        val ids = f.nodes.map { it.id }.toSet()
        val out = f.edges.filter { it.kind == "flow" && it.source in ids && it.target in ids }.groupBy { it.source }
        val seen = LinkedHashSet<String>()
        val stack = ArrayDeque(roots.filter { it in ids })
        while (stack.isNotEmpty()) {
            val id = stack.removeLast()
            if (!seen.add(id)) continue
            for (e in out[id].orEmpty()) stack.addLast(e.target)
        }
        return seen
    }

    // ─── Steps ───

    /** A new step, as Editor.jsx newNode: kind is employee:<id>, decision:yesno|one|many, join, ask or wait. */
    fun newNode(kind: String, position: Position): FactoryNode {
        val type = kind.substringBefore(':')
        val arg = kind.substringAfter(':', "")
        val base = FactoryNode(genId("n_"), type, "", position)
        return when (type) {
            "employee" -> base.copy(employeeId = arg)
            "decision" -> base.copy(decision = FactoryDecision(arg.ifEmpty { "yesno" }, "", if (arg == "yesno" || arg.isEmpty()) emptyList() else listOf(Option(genId("o_"), "Option 1"), Option(genId("o_"), "Option 2")), null))
            "join" -> base.copy(join = FactoryJoin(""))
            "ask" -> base.copy(ask = FactoryAsk("", listOf(Option(genId("o_"), "Go ahead"), Option(genId("o_"), "Stop"))))
            "wait" -> base.copy(wait = FactoryWait("command", 10.0, "09:00", "", 60.0, 60.0, ""))
            else -> base
        }
    }

    /** Snapped to the canvas grid, as the web's (16). */
    fun snap(p: Position) = Position((p.x / 16).roundToInt() * 16.0, (p.y / 16).roundToInt() * 16.0)

    /** Add a step at [at]; a pick-any decision comes with its Join, 240 below. Answers the factory and the new step's id. */
    fun addNode(f: Factory, kind: String, at: Position): Pair<Factory, String> {
        val p = snap(at)
        val n = newNode(kind, p)
        val extra = if (kind == "decision:many") listOf(newNode("join", Position(p.x, p.y + 240)).copy(label = "Join", join = FactoryJoin(n.id))) else emptyList()
        return f.copy(nodes = f.nodes + n + extra) to n.id
    }

    fun updateNode(f: Factory, id: String, fn: (FactoryNode) -> FactoryNode) = f.copy(nodes = f.nodes.map { if (it.id == id) fn(it) else it })
    fun updateEdge(f: Factory, id: String, fn: (FactoryEdge) -> FactoryEdge) = f.copy(edges = f.edges.map { if (it.id == id) fn(it) else it })
    fun updateMember(f: Factory, id: String, fn: (FactoryMember) -> FactoryMember) = f.copy(roster = f.roster.map { if (it.id == id) fn(it) else it })

    fun moveNode(f: Factory, id: String, to: Position) = updateNode(f, id) { it.copy(position = Position(to.x.roundToInt().toDouble(), to.y.roundToInt().toDouble())) }

    /** Delete steps (never Start, the Manager or End), their connections, and any Join's link to them. */
    fun deleteNodes(f: Factory, ids: Collection<String>): Factory {
        val gone = ids.filter { id -> f.nodes.firstOrNull { it.id == id }?.let { it.type !in FIXED } == true }.toSet()
        if (gone.isEmpty()) return f
        return f.copy(
            nodes = f.nodes.filter { it.id !in gone }.map { n -> if (n.type == "join" && n.join?.forkId in gone) n.copy(join = FactoryJoin("")) else n },
            edges = f.edges.filter { it.source !in gone && it.target !in gone },
        )
    }

    /** Delete a connection; the Manager's Finish stays. */
    fun deleteEdge(f: Factory, id: String) = f.copy(edges = f.edges.filter { it.id != id || it.sourceHandle == "finish" })

    /** Drop the flow connections leaving [nodeId] from outputs not in [keep]. */
    private fun dropHandles(f: Factory, nodeId: String, keep: Collection<String>) = f.copy(edges = f.edges.filterNot { it.source == nodeId && it.kind == "flow" && it.sourceHandle !in keep })

    // ─── Connecting (Editor.jsx isValidConnection, onConnect) ───

    /** May [source]'s output [handle] lead to [target]? "note" is the note handle of a person. */
    fun canConnect(f: Factory, source: String, handle: String, target: String): Boolean {
        val s = f.nodes.firstOrNull { it.id == source } ?: return false
        val t = f.nodes.firstOrNull { it.id == target } ?: return false
        if (source == target) return false
        if (handle == "note") return s.type in AGENTS && t.type in AGENTS
        if (handle !in outputsOf(s) || t.type == "start") return false
        if (handle == "finish") return t.type == "end"
        if (t.type == "end") return false
        return true
    }

    /**
     * Connect, as the web: a flow output leads to one place, so its old
     * connection goes; one that closes a loop gets a round limit of 3 (a loop
     * without one cannot run). Null when it may not connect. The flag: it got
     * that limit.
     */
    fun connect(f: Factory, source: String, handle: String, target: String): Pair<Factory, Boolean>? {
        if (!canConnect(f, source, handle, target)) return null
        if (handle == "note") {
            if (f.edges.any { it.kind == "note" && it.source == source && it.target == target }) return f to false
            return f.copy(edges = f.edges + FactoryEdge(genId("e_"), "note", source, "note", target, false, "", 0)) to false
        }
        val kept = f.copy(edges = f.edges.filterNot { it.kind == "flow" && it.source == source && it.sourceHandle == handle })
        val loop = source in reachable(kept, listOf(target))
        return kept.copy(edges = kept.edges + FactoryEdge(genId("e_"), "flow", source, handle, target, false, "", if (loop) 3 else 0)) to loop
    }

    /** Where each output of a step leads now (null: nowhere yet), and its notes. */
    fun connectionsOf(f: Factory, n: FactoryNode): List<Pair<String, FactoryEdge?>> =
        outputsOf(n).map { h -> h to f.edges.firstOrNull { it.kind == "flow" && it.source == n.id && it.sourceHandle == h } }

    // ─── Decisions, asks, waits (Inspector.jsx) ───

    /** Change a decision's kind; outputs that no longer exist lose their connections. */
    fun setDecisionKind(f: Factory, nodeId: String, kind: String): Factory {
        val n = f.nodes.firstOrNull { it.id == nodeId } ?: return f
        val d = n.decision ?: FactoryDecision()
        return if (kind == "yesno") dropHandles(updateNode(f, nodeId) { it.copy(decision = d.copy(kind = kind, options = emptyList())) }, nodeId, listOf("yes", "no"))
        else {
            val opts = if (d.options.size < 2) listOf(Option(genId("o_"), "Option 1"), Option(genId("o_"), "Option 2")) else d.options
            dropHandles(updateNode(f, nodeId) { it.copy(decision = d.copy(kind = kind, options = opts)) }, nodeId, opts.map { it.id } + if (kind == "many") listOf("none") else emptyList())
        }
    }

    fun addOption(f: Factory, nodeId: String) = updateNode(f, nodeId) { n -> n.decision?.let { d -> n.copy(decision = d.copy(options = d.options + Option(genId("o_"), "Option " + (d.options.size + 1)))) } ?: n }
    fun removeOption(f: Factory, nodeId: String, optionId: String) = updateNode(f, nodeId) { n -> n.decision?.let { d -> n.copy(decision = d.copy(options = d.options.filter { it.id != optionId })) } ?: n }
        .let { it.copy(edges = it.edges.filterNot { e -> e.source == nodeId && e.sourceHandle == optionId }) }
    fun addReply(f: Factory, nodeId: String) = updateNode(f, nodeId) { n -> n.ask?.let { a -> n.copy(ask = a.copy(replies = a.replies + Option(genId("o_"), "Reply " + (a.replies.size + 1)))) } ?: n }
    fun removeReply(f: Factory, nodeId: String, replyId: String) = updateNode(f, nodeId) { n -> n.ask?.let { a -> n.copy(ask = a.copy(replies = a.replies.filter { it.id != replyId })) } ?: n }
        .let { it.copy(edges = it.edges.filterNot { e -> e.source == nodeId && e.sourceHandle == replyId }) }

    /** Switching between one route and Done / Timed out: the old routes no longer fit. */
    fun setWaitKind(f: Factory, nodeId: String, kind: String): Factory {
        val n = f.nodes.firstOrNull { it.id == nodeId } ?: return f
        val w = n.wait ?: FactoryWait()
        fun two(k: String) = k == "command" || k == "signal"
        val g = if (two(kind) != two(w.kind)) f.copy(edges = f.edges.filter { it.source != nodeId || it.kind == "note" }) else f
        return updateNode(g, nodeId) { it.copy(wait = w.copy(kind = kind)) }
    }

    // ─── The team (Roster.jsx) ───

    /** A new person from a persona template (or blank), with a name nobody else has. Skills not on this machine are left off. */
    fun newEmployee(f: Factory, template: PersonaTemplate?, skills: Collection<String>? = null): FactoryMember {
        val stem = template?.name ?: "New employee"
        var name = stem
        var i = 2
        while (f.roster.any { it.name.equals(name, ignoreCase = true) }) name = stem + " " + i++
        return FactoryMember(
            genId("emp_"), name, template?.persona.orEmpty(), "", "", template?.memory ?: "keep",
            FactoryTools(emptyList(), template?.deny.orEmpty()),
            template?.skills.orEmpty().filter { skills == null || it in skills },
            template?.presets.orEmpty(),
        )
    }

    /** Add a person; with [assignTo], that step becomes theirs. */
    fun addEmployee(f: Factory, m: FactoryMember, assignTo: String? = null): Factory {
        val g = f.copy(roster = f.roster + m)
        return if (assignTo == null) g else updateNode(g, assignTo) { it.copy(employeeId = m.id) }
    }

    /** Remove a person, their steps and those steps' connections; decisions they answered go back to whoever arrives. */
    fun removeEmployee(f: Factory, id: String): Factory {
        val gone = f.nodes.filter { it.type == "employee" && it.employeeId == id }.map { it.id }.toSet()
        return f.copy(
            nodes = f.nodes.filter { it.id !in gone }.map { n -> if (n.type == "decision" && n.decision?.answeredBy == id) n.copy(decision = n.decision.copy(answeredBy = null)) else n },
            edges = f.edges.filter { it.source !in gone && it.target !in gone },
            roster = f.roster.filter { it.id != id },
        )
    }

    fun stepsOf(f: Factory, id: String) = f.nodes.count { it.type == "employee" && it.employeeId == id }

    fun canEditFiles(m: FactoryMember) = EDIT_TOOLS.none { it in m.tools.deny }
    fun setCanEditFiles(m: FactoryMember, on: Boolean) = m.copy(tools = m.tools.copy(deny = if (on) m.tools.deny.filter { it !in EDIT_TOOLS } else (m.tools.deny + EDIT_TOOLS).distinct()))

    /** "a, /b ,c" as the web's pinned skills field reads it. */
    fun splitList(v: String, stripSlash: Boolean = false) = v.split(',').map { it.trim().let { s -> if (stripSlash) s.removePrefix("/") else s } }.filter { it.isNotEmpty() }

    // ─── Skills named in text (factory-skills.js findMentions) ───

    private val MENTION = Regex("""(^|[\s(\[{,;"'`])/([a-z0-9][a-z0-9-]*(?::[a-z0-9][a-z0-9-]*)?)(?![\w/:-]|\.\w)""")
    fun findMentions(text: String?): List<String> = MENTION.findAll(text.orEmpty()).map { it.groupValues[2] }.distinct().toList()

    // ─── Tidy the layout (factory-model.js layoutEdges, layoutFactory) ───

    /** Flow connections that read top to bottom: every one except those going back up a loop. */
    fun layoutEdges(f: Factory): List<Pair<String, String>> {
        val out = LinkedHashMap<String, MutableList<FactoryEdge>>().apply { for (n in f.nodes) put(n.id, mutableListOf()) }
        for (e in f.edges) if (e.kind != "note" && e.source in out && e.target in out) out[e.source]!!.add(e)
        for ((k, list) in out) out[k] = list.sortedBy { if (it.maxRounds > 0) 1 else 0 }.toMutableList()
        val state = HashMap<String, Int>()
        val keep = mutableListOf<Pair<String, String>>()
        fun visit(id: String) {
            state[id] = 1
            for (e in out[id].orEmpty()) {
                if (state[e.target] == 1) continue
                keep.add(id to e.target)
                if (e.target !in state) visit(e.target)
            }
            state[id] = 2
        }
        f.nodes.firstOrNull { it.type == "start" }?.let { visit(it.id) }
        for (n in f.nodes) if (n.id !in state) visit(n.id)
        return keep
    }

    /** Top to bottom: layers by longest path from Start, each layer centred, End last. */
    fun layout(f: Factory): Factory {
        val w = 240; val h = 130; val gap = 40
        val dag = LinkedHashMap<String, MutableList<String>>().apply { for (n in f.nodes) put(n.id, mutableListOf()) }
        for ((a, b) in layoutEdges(f)) dag[a]!!.add(b)
        val order = mutableListOf<String>()
        val seen = HashSet<String>()
        fun topo(id: String) { if (!seen.add(id)) return; for (t in dag[id].orEmpty()) topo(t); order.add(id) }
        for (n in f.nodes) topo(n.id)
        order.reverse()
        val layer = HashMap<String, Int>()
        for (id in order) if (id !in layer) layer[id] = 0
        for (id in order) for (t in dag[id].orEmpty()) layer[t] = max(layer[t] ?: 0, layer[id]!! + 1)
        f.nodes.firstOrNull { it.type == "end" }?.let { end ->
            val top = layer.values.maxOrNull() ?: 0
            layer[end.id] = top + if (layer[end.id] == top) 0 else 1
        }
        val rows = LinkedHashMap<Int, MutableList<String>>()
        for (n in f.nodes) rows.getOrPut(layer[n.id] ?: 0) { mutableListOf() }.add(n.id)
        val pos = HashMap<String, Position>()
        for ((l, ids) in rows) {
            val width = ids.size * w + (ids.size - 1) * gap
            ids.forEachIndexed { i, id -> pos[id] = Position(Math.round(-width / 2.0 + i * (w + gap)).toDouble(), (l * (h + gap)).toDouble()) }
        }
        return f.copy(nodes = f.nodes.map { it.copy(position = pos[it.id] ?: Position()) })
    }
}

/**
 * Undo and redo, as the web editor's: up to 100 steps; typing into one field
 * ([key]) within 1.5 seconds is one step, not one per letter.
 */
class EditHistory(private val max: Int = 100) {
    private val past = ArrayDeque<Factory>()
    private val future = ArrayDeque<Factory>()
    private var lastKey = ""
    private var lastAt = 0L

    val canUndo get() = past.isNotEmpty()
    val canRedo get() = future.isNotEmpty()

    /** [prev] is the factory before a change. */
    fun record(prev: Factory, key: String = "", now: Long = System.currentTimeMillis()) {
        val coalesce = key.isNotEmpty() && key == lastKey && now - lastAt < 1500
        lastKey = key; lastAt = now
        if (coalesce) return
        past.addLast(prev)
        if (past.size > max) past.removeFirst()
        future.clear()
    }

    fun undo(cur: Factory): Factory? = past.removeLastOrNull()?.also { future.addLast(cur); lastKey = "" }
    fun redo(cur: Factory): Factory? = future.removeLastOrNull()?.also { past.addLast(cur); lastKey = "" }
    fun clear() { past.clear(); future.clear(); lastKey = "" }
}
