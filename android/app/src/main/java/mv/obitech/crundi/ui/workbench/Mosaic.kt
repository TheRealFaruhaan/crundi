package mv.obitech.crundi.ui.workbench

import org.json.JSONArray
import org.json.JSONObject

/**
 * The workbench layout: a tree of splits and leaves, the same shape and the
 * same operations as the web's mosaic (src/webapp-html.js, "Mosaic workbench
 * layout": mosLeaf, mosSplit, mosaicSync, mosaicAddKey, mosaicCollapseLeaf,
 * mosaicSplitLeaf, mosaicAssignToLeaf). A leaf holds one pane by its key, or
 * nothing (an empty slot that shows the launcher).
 *
 * On a phone, as on the mobile web, a top-level ROW split is the row of
 * full-width columns that snap when swiped; a COL split stacks panes in a
 * column with a draggable divider.
 *
 * Immutable: every operation returns a new tree. Pure Kotlin, unit-tested.
 */
sealed interface Mosaic {
    val id: String

    data class Leaf(override val id: String, val key: String?) : Mosaic

    data class Split(override val id: String, val dir: Dir, val kids: List<Mosaic>, val sizes: List<Float>) : Mosaic

    /** ROW: side by side (on a phone, the swipe columns). COL: one above the other. */
    enum class Dir { Row, Col }

    companion object {
        private var seq = 0L
        fun newId(): String = "m" + java.lang.Long.toString(System.nanoTime(), 36) + (seq++).toString(36)
        fun leaf(key: String? = null) = Leaf(newId(), key)
        fun split(dir: Dir, kids: List<Mosaic>) = Split(newId(), dir, kids, even(kids.size))
        fun even(n: Int) = List(n) { 100f / n }

        /** The web's mosaicDefault: nothing, one pane, or a row of columns. */
        fun default(keys: List<String>): Mosaic = when (keys.size) {
            0 -> leaf(null)
            1 -> leaf(keys[0])
            else -> split(Dir.Row, keys.map { leaf(it) })
        }

        fun fromJson(s: String?): Mosaic? = runCatching { s?.let { read(JSONObject(it)) } }.getOrNull()

        private fun read(o: JSONObject): Mosaic = if (o.optString("t") == "split") {
            val kids = o.getJSONArray("kids").let { a -> List(a.length()) { read(a.getJSONObject(it)) } }
            val sizes = o.optJSONArray("sizes")?.let { a -> List(a.length()) { a.getDouble(it).toFloat() } }?.takeIf { it.size == kids.size } ?: even(kids.size)
            Split(o.optString("_id").ifEmpty { newId() }, if (o.optString("dir") == "col") Dir.Col else Dir.Row, kids, sizes)
        } else Leaf(o.optString("_id").ifEmpty { newId() }, o.optString("key").ifEmpty { null })
    }
}

/** The web's JSON shape, so a layout reads the same everywhere. */
fun Mosaic.toJson(): String = write(this).toString()

private fun write(m: Mosaic): JSONObject = when (m) {
    is Mosaic.Leaf -> JSONObject().put("t", "leaf").put("_id", m.id).put("key", m.key ?: JSONObject.NULL)
    is Mosaic.Split -> JSONObject().put("t", "split").put("_id", m.id).put("dir", if (m.dir == Mosaic.Dir.Col) "col" else "row")
        .put("sizes", JSONArray(m.sizes)).put("kids", JSONArray(m.kids.map { write(it) }))
}

fun Mosaic.leaves(): List<Mosaic.Leaf> = when (this) {
    is Mosaic.Leaf -> listOf(this)
    is Mosaic.Split -> kids.flatMap { it.leaves() }
}

fun Mosaic.keys(): List<String> = leaves().mapNotNull { it.key }
fun Mosaic.hasKey(key: String) = leaves().any { it.key == key }
fun Mosaic.leafOf(key: String) = leaves().firstOrNull { it.key == key }

/** Replace the node with this id by what [f] makes of it (null removes nothing: use collapse). */
private fun Mosaic.map(id: String, f: (Mosaic) -> Mosaic): Mosaic = when {
    this.id == id -> f(this)
    this is Mosaic.Split -> copy(kids = kids.map { it.map(id, f) })
    else -> this
}

private fun Mosaic.parentOf(id: String): Mosaic.Split? = when (this) {
    is Mosaic.Leaf -> null
    is Mosaic.Split -> if (kids.any { it.id == id }) this else kids.firstNotNullOfOrNull { it.parentOf(id) }
}

/** Put a pane in the first empty slot, else as a new column at the end (mosaicAddKey). */
fun Mosaic.addKey(key: String): Mosaic {
    val empty = leaves().firstOrNull { it.key == null }
    if (empty != null) return map(empty.id) { (it as Mosaic.Leaf).copy(key = key) }
    return when (this) {
        is Mosaic.Leaf -> Mosaic.split(Mosaic.Dir.Row, listOf(this, Mosaic.leaf(key)))
        is Mosaic.Split -> (kids + Mosaic.leaf(key)).let { copy(kids = it, sizes = Mosaic.even(it.size)) }
    }
}

/** Remove a leaf; a split left with one child is replaced by that child (mosaicCollapseLeaf). */
fun Mosaic.collapse(leafId: String): Mosaic {
    if (this.id == leafId) return Mosaic.leaf(null)
    val parent = parentOf(leafId) ?: return this
    val kids = parent.kids.filter { it.id != leafId }
    val replaced: Mosaic = if (kids.size == 1) kids[0] else parent.copy(kids = kids, sizes = Mosaic.even(kids.size))
    return map(parent.id) { replaced }
}

fun Mosaic.removeKey(key: String): Mosaic = leafOf(key)?.let { collapse(it.id) } ?: this

/** Keep the tree in step with the panes there are: drop gone ones, add new ones (mosaicSync). */
fun Mosaic?.sync(keys: List<String>): Mosaic {
    var t = this ?: return Mosaic.default(keys)
    for (k in t.keys()) if (k !in keys) t = t.removeKey(k)
    for (k in keys) if (!t.hasKey(k)) t = t.addKey(k)
    return t
}

/** Split a leaf: it keeps its pane, an empty slot goes beside it (mosaicSplitLeaf). */
fun Mosaic.splitLeaf(leafId: String, dir: Mosaic.Dir): Mosaic {
    val parent = parentOf(leafId)
    val fresh = Mosaic.leaf(null)
    if (parent != null && parent.dir == dir) {
        val i = parent.kids.indexOfFirst { it.id == leafId }
        val kids = parent.kids.toMutableList().apply { add(i + 1, fresh) }
        return map(parent.id) { parent.copy(kids = kids, sizes = Mosaic.even(kids.size)) }
    }
    return map(leafId) { Mosaic.split(dir, listOf(it, fresh)) }
}

/** Put a pane into a slot: swap if the slot is taken, else move it (mosaicAssignToLeaf). */
fun Mosaic.assign(leafId: String, key: String): Mosaic {
    val target = leaves().firstOrNull { it.id == leafId } ?: return this
    val source = leafOf(key)
    if (source?.id == target.id) return this
    if (source == null) return map(leafId) { (it as Mosaic.Leaf).copy(key = key) }
    if (target.key != null) {
        val tk = target.key
        return map(target.id) { (it as Mosaic.Leaf).copy(key = key) }.map(source.id) { (it as Mosaic.Leaf).copy(key = tk) }
    }
    return map(target.id) { (it as Mosaic.Leaf).copy(key = key) }.map(source.id) { (it as Mosaic.Leaf).copy(key = null) }.collapse(source.id)
}

/** Swap a pane with its neighbour in the top-level row (move a column left or right). */
fun Mosaic.moveColumn(key: String, by: Int): Mosaic {
    if (this !is Mosaic.Split || dir != Mosaic.Dir.Row) return this
    val i = kids.indexOfFirst { it.hasKey(key) }
    val j = i + by
    if (i < 0 || j !in kids.indices) return this
    val k = kids.toMutableList(); val tmp = k[i]; k[i] = k[j]; k[j] = tmp
    return copy(kids = k)
}

/** New sizes for a split (the divider was dragged). */
fun Mosaic.resize(splitId: String, sizes: List<Float>): Mosaic = map(splitId) { (it as? Mosaic.Split)?.copy(sizes = sizes) ?: it }

/** Which top-level column holds this key (for jumping to a pane), or -1. */
fun Mosaic.columnOf(key: String): Int = when {
    this is Mosaic.Split && dir == Mosaic.Dir.Row -> kids.indexOfFirst { it.hasKey(key) }
    hasKey(key) -> 0
    else -> -1
}

/** The top-level columns: a row split's children, or the whole tree as one. */
fun Mosaic.columns(): List<Mosaic> = if (this is Mosaic.Split && dir == Mosaic.Dir.Row) kids else listOf(this)
