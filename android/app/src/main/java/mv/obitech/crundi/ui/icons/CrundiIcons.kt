package mv.obitech.crundi.ui.icons

import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/**
 * The app's only icons: the desktop's flat stroke set (ICON_PATHS in
 * src/webapp-html.js), converted by android/tools/gen-icons.mjs into
 * IconPaths.kt. Drawn exactly as the desktop's ic(): a 24 grid, no fill,
 * 2-wide strokes with round ends and joins, in the current content colour.
 *
 * Never emoji, never a Material icon, never a drawable made by hand for a
 * screen. Need one that is missing? Look at ICON_PATHS first; if nothing fits,
 * add it to android/tools/icons-extra.json in the same style and run the
 * generator.
 */
object CrundiIcons {
    private val cache = HashMap<String, ImageVector>()

    /** Every name there is. */
    val names: Set<String> get() = IconPaths.all.keys

    /** The icon by its ICON_PATHS name ("folder", "git-branch", ...), or null if there is none. */
    fun get(name: String): ImageVector? = synchronized(cache) {
        cache[name] ?: IconPaths.all[name]?.let { build(name, it) }?.also { cache[name] = it }
    }

    private fun build(name: String, paths: List<String>): ImageVector {
        val b = ImageVector.Builder(name = name, defaultWidth = 24.dp, defaultHeight = 24.dp, viewportWidth = 24f, viewportHeight = 24f)
        for (d in paths) {
            b.addPath(
                pathData = PathParser().parsePathString(d).toNodes(),
                fill = null,
                stroke = SolidColor(Color.White),
                strokeLineWidth = 2f,
                strokeLineCap = StrokeCap.Round,
                strokeLineJoin = StrokeJoin.Round,
            )
        }
        return b.build()
    }
}

/**
 * One icon. `size` defaults to the desktop's 18px; tab icons are 17, inline
 * icons in small text 15. The colour is the surrounding content colour unless
 * `tint` says otherwise. An unknown name draws nothing (and fails IconPathsTest
 * if it is used in a test).
 */
@Composable
fun CrundiIcon(
    name: String,
    modifier: Modifier = Modifier,
    size: Dp = 18.dp,
    tint: Color = LocalContentColor.current,
    contentDescription: String? = null,
) {
    val v = CrundiIcons.get(name) ?: return
    Icon(imageVector = v, contentDescription = contentDescription, modifier = modifier.size(size), tint = tint)
}
