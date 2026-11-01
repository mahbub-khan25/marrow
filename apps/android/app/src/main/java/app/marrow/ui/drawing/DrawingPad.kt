package app.marrow.ui.drawing

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowRightAlt
import androidx.compose.material.icons.automirrored.outlined.Undo
import androidx.compose.material.icons.outlined.BorderColor
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Draw
import androidx.compose.material.icons.outlined.OpenInFull
import androidx.compose.material.icons.outlined.PanTool
import androidx.compose.material.icons.outlined.TextFields
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Fill
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFontFamilyResolver
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.marrow.R
import app.marrow.core.Board
import app.marrow.core.CanvasNode
import app.marrow.core.ShapeGeometry
import app.marrow.core.shapeKinds
import app.marrow.core.shapeNode
import app.marrow.core.strokeNode
import app.marrow.core.textNode
import app.marrow.ui.common.LocalEngine
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin

/*
 * Drawings inside notes: a small canvas (a `.canvas` file) shown where its
 * `![[Drawing ….canvas|360]]` line is. Tap it to draw right there, with a
 * finger or a stylus: pen, highlighter, lines, arrows, shapes (filled, light
 * or see-through) and words, in colour. The cards it
 * makes are the same as on the desktop and in canvases, so the drawing looks
 * the same everywhere.
 */

/** JSON Canvas colours "1"–"6". */
private val PALETTE = listOf("1" to Color(0xFFE03E3E), "2" to Color(0xFFE8780C), "3" to Color(0xFFD4A106), "4" to Color(0xFF2F9E44), "5" to Color(0xFF0B8CB5), "6" to Color(0xFF8E44AD))

private fun colorOf(c: String): Color? = PALETTE.firstOrNull { it.first == c }?.second
    ?: if (Regex("#[0-9a-fA-F]{6}").matches(c)) Color(android.graphics.Color.parseColor(c)) else null

enum class DrawTool(val stroke: String = "") { SELECT, PEN("pen"), SHAPE, LINE("line"), ARROW("arrow"), DOUBLE("double"), HIGHLIGHT("highlighter"), TEXT, ERASE }

/** The highlighter's colour when none is picked. */
private val HIGHLIGHT = Color(0xFFF2C94C)

/** The engine's shapes (the same list as the desktop's): outlines in a 100 × 100 box. */
private val SHAPES: List<ShapeGeometry> by lazy { runCatching { shapeKinds() }.getOrDefault(emptyList()) }
internal fun shapeOf(kind: String): ShapeGeometry? = SHAPES.firstOrNull { it.kind == kind } ?: SHAPES.firstOrNull()

/** The distance from `p` to the segment a–b. */
private fun segDist(p: Offset, a: Offset, b: Offset): Float {
    val ab = b - a
    val len2 = ab.x * ab.x + ab.y * ab.y
    val t = if (len2 == 0f) 0f else (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).coerceIn(0f, 1f)
    return (p - (a + ab * t)).getDistance()
}

/**
 * The topmost card at `p` (world units). Strokes count only near their line,
 * and see-through shapes only near their outline, so what's under them can
 * still be picked.
 */
internal fun hitNode(b: Board, p: Offset, tol: Float): CanvasNode? = b.nodes.lastOrNull { n ->
    val x0 = n.x.toFloat(); val y0 = n.y.toFloat(); val w = n.width.toFloat(); val h = n.height.toFloat()
    if (p.x < x0 - tol || p.x > x0 + w + tol || p.y < y0 - tol || p.y > y0 + h + tol) return@lastOrNull false
    val ink = n.ink
    when {
        ink != null -> {
            val sx = w / ink.w.toFloat(); val sy = h / ink.h.toFloat()
            val pts = ink.points.chunked(2).filter { it.size == 2 }.map { Offset(x0 + it[0].toFloat() * sx, y0 + it[1].toFloat() * sy) }
            val reach = tol + ink.size.toFloat() / 2
            if (pts.size == 1) (pts[0] - p).getDistance() < reach else pts.zipWithNext().any { (a, z) -> segDist(p, a, z) < reach }
        }
        n.shape.isNotEmpty() && n.fill == "none" -> {
            val g = shapeOf(n.shape)
            val m = androidx.compose.ui.graphics.PathMeasure()
            m.setPath(g?.let { placePath(it.outline, Offset(x0, y0), Size(w, h)) } ?: Path().apply { addRect(androidx.compose.ui.geometry.Rect(Offset(x0, y0), Size(w, h))) }, false)
            val len = m.length
            (0..96).any { i -> (m.getPosition(len * i / 96f) - p).getDistance() < tol } || (n.text.isNotBlank() && abs(p.x - (x0 + w / 2)) < w / 4 && abs(p.y - (y0 + h / 2)) < h / 4)
        }
        else -> true
    }
}

/** SVG path data (only M, L, C and Z) from the 100 × 100 box, placed in the box at `tl`. */
internal fun placePath(d: String, tl: Offset, sz: Size): Path {
    val p = Path()
    val tokens = Regex("[MLCZ]|-?\\d*\\.?\\d+").findAll(d).map { it.value }.toList()
    var i = 0
    var cmd = 'M'
    fun x() = tl.x + tokens[i++].toFloat() / 100f * sz.width
    fun y() = tl.y + tokens[i++].toFloat() / 100f * sz.height
    runCatching {
        while (i < tokens.size) {
            val t = tokens[i]
            if (t[0].isLetter()) {
                cmd = t[0]
                i++
                if (cmd == 'Z') p.close()
                continue
            }
            when (cmd) {
                'M' -> { p.moveTo(x(), y()); cmd = 'L' }
                'L' -> p.lineTo(x(), y())
                'C' -> p.cubicTo(x(), y(), x(), y(), x(), y())
                else -> i++
            }
        }
    }
    return p
}

private val PEN_SIZES = listOf(2.0, 4.0, 8.0)

/** How the drawing maps to the box on screen: screen = world * k + (tx, ty), in pixels. */
private data class View(val k: Float, val tx: Float, val ty: Float) {
    fun world(p: Offset) = Offset((p.x - tx) / k, (p.y - ty) / k)
}

/** Fits everything drawn into a box of `size` (pixels), never larger than life. */
private fun fit(b: Board?, size: IntSize, density: Float): View {
    val nodes = b?.nodes.orEmpty()
    val pad = 16f * density
    if (nodes.isEmpty() || size.width == 0) return View(density, pad, pad)
    val x0 = nodes.minOf { it.x }.toFloat()
    val y0 = nodes.minOf { it.y }.toFloat()
    val x1 = nodes.maxOf { it.x + it.width }.toFloat()
    val y1 = nodes.maxOf { it.y + it.height }.toFloat()
    val k = min(min((size.width - 2 * pad) / max(1f, x1 - x0), (size.height - 2 * pad) / max(1f, y1 - y0)), density).coerceAtLeast(0.05f)
    return View(k, (size.width - (x1 - x0) * k) / 2f - x0 * k, (size.height - (y1 - y0) * k) / 2f - y0 * k)
}

/**
 * A drawing in a note. Shows it; a tap starts drawing on it in place (the
 * note doesn't scroll under the pen while it's on). "Done" finishes.
 */
@Composable
fun InlineDrawing(path: String, height: Int, onOpenFull: () -> Unit, modifier: Modifier = Modifier, onDelete: (() -> Unit)? = null) {
    var active by remember(path) { mutableStateOf(false) }
    DrawingPad(path, active, { active = it }, onOpenFull, modifier.fillMaxWidth(), onDelete, boardHeight = height)
}

/** A drawing that is being drawn on, filling `modifier` (used full size after inserting one). */
@Composable
fun DrawingEditor(path: String, onDone: () -> Unit, modifier: Modifier = Modifier) {
    DrawingPad(path, true, { if (!it) onDone() }, null, modifier)
}

@Composable
private fun DrawingPad(path: String, active: Boolean, setActive: (Boolean) -> Unit, onOpenFull: (() -> Unit)?, modifier: Modifier, onDelete: (() -> Unit)? = null, boardHeight: Int? = null) {
    val engine = LocalEngine.current
    val scope = rememberCoroutineScope()
    val density = LocalDensity.current.density
    var board by remember(path) { mutableStateOf<Board?>(null) }
    var missing by remember(path) { mutableStateOf(false) }
    val past = remember(path) { mutableStateListOf<Board>() }
    var tool by remember { mutableStateOf(DrawTool.PEN) }
    var color by remember { mutableStateOf("") }
    var penSize by remember { mutableStateOf(4.0) }
    var shapeKind by remember { mutableStateOf("rect") }
    var fill by remember { mutableStateOf("") }
    var box by remember { mutableStateOf(IntSize.Zero) }
    var frozen by remember { mutableStateOf<View?>(null) }
    var draft by remember { mutableStateOf<List<Offset>>(emptyList()) }
    var asking by remember { mutableStateOf<((String) -> Unit)?>(null) }
    var askText by remember { mutableStateOf("") }
    var askWords by remember { mutableStateOf(false) }
    // The card picked with the select tool (moved by dragging it, resized by its corner).
    var selected by remember(path) { mutableStateOf<String?>(null) }
    var saveJob by remember { mutableStateOf<Job?>(null) }
    var askDelete by remember { mutableStateOf(false) }

    LaunchedEffect(path) {
        val b = runCatching { engine.run { readCanvas(path) } }.getOrNull()
        if (b == null) missing = true else board = b
    }
    // While drawing, the view stays put (it would jump as the drawing grows).
    val view = frozen ?: fit(board, box, density)
    LaunchedEffect(active, box, board != null) {
        if (!active) frozen = null
        else if (frozen == null && box.width > 0 && board != null) frozen = fit(board, box, density)
    }

    fun change(next: Board) {
        board?.let { past.add(it); if (past.size > 100) past.removeAt(0) }
        board = next
        saveJob?.cancel()
        saveJob = scope.launch {
            delay(400)
            runCatching { engine.run { writeCanvas(path, next, null) } }.onFailure { engine.say(it.message ?: "") }
        }
    }

    fun add(n: CanvasNode?) {
        val b = board ?: return
        if (n != null) change(b.copy(nodes = b.nodes + n))
    }

    fun update(id: String, f: (CanvasNode) -> CanvasNode) {
        val b = board ?: return
        change(b.copy(nodes = b.nodes.map { if (it.id == id) f(it) else it }))
    }

    /** After a shape, line, arrow or words: pick it, ready to move (pen and highlighter keep drawing). */
    fun pick(n: CanvasNode?) {
        if (n == null) return
        selected = n.id
        tool = DrawTool.SELECT
    }

    fun finish(pts: List<Offset>) {
        if (pts.isEmpty()) return
        val w = pts.map { view.world(it) }
        val a = w.first()
        val z = w.last()
        when (tool) {
            DrawTool.SELECT -> {}
            DrawTool.PEN, DrawTool.LINE, DrawTool.ARROW, DrawTool.DOUBLE, DrawTool.HIGHLIGHT -> {
                val n = strokeNode(w.flatMap { listOf(it.x.toDouble(), it.y.toDouble()) }, penSize, tool.stroke, color, (view.k / density).toDouble())
                add(n)
                if (tool != DrawTool.PEN && tool != DrawTool.HIGHLIGHT) pick(n)
            }
            DrawTool.SHAPE -> {
                val n = shapeNode(shapeKind, a.x.toDouble(), a.y.toDouble(), z.x.toDouble(), z.y.toDouble(), color, fill)
                add(n)
                pick(n)
                askText = ""
                askWords = false
                asking = { label -> if (label.isNotBlank()) update(n.id) { it.copy(text = label.trim()) } }
            }
            DrawTool.TEXT -> {
                askText = ""
                askWords = true
                asking = { text -> if (text.isNotBlank()) textNode(text.trim(), a.x.toDouble(), a.y.toDouble(), color).let { add(it); pick(it) } }
            }
            DrawTool.ERASE -> {
                val b = board ?: return
                val tol = 10.0 * density / view.k
                val hit = b.nodes.lastOrNull { n -> w.any { p -> p.x >= n.x - tol && p.x <= n.x + n.width + tol && p.y >= n.y - tol && p.y <= n.y + n.height + tol } } ?: return
                change(b.copy(nodes = b.nodes - hit, edges = b.edges.filter { it.fromNode != hit.id && it.toNode != hit.id }))
            }
        }
    }

    val outline = MaterialTheme.colorScheme.outlineVariant
    val accent = MaterialTheme.colorScheme.primary
    val ink = MaterialTheme.colorScheme.onBackground
    val paper = MaterialTheme.colorScheme.background
    val fonts = LocalFontFamilyResolver.current
    val dir = LocalLayoutDirection.current
    val measurer = remember(fonts, dir) { TextMeasurer(fonts, Density(1f), dir) }

    Column(modifier) {
        if (active) {
            val sel = if (tool == DrawTool.SELECT) board?.nodes?.firstOrNull { it.id == selected } else null
            Toolbar(
                tool, { tool = it; if (it != DrawTool.SELECT) selected = null },
                if (sel != null) sel.color else color, { c -> color = c; sel?.let { n -> update(n.id) { it.copy(color = c) } } },
                penSize, { penSize = it }, shapeKind, { shapeKind = it },
                if (sel != null) sel.fill else fill, { f -> fill = f; sel?.let { n -> update(n.id) { it.copy(fill = f) } } },
                selection = sel,
                onDelete = { sel?.let { n -> board?.let { b -> change(b.copy(nodes = b.nodes - n, edges = b.edges.filter { it.fromNode != n.id && it.toNode != n.id })) }; selected = null } },
                onEditWords = {
                    sel?.let { n ->
                        askText = n.text
                        askWords = n.shape.isEmpty()
                        asking = { t -> update(n.id) { it.copy(text = t.trim()) } }
                    }
                },
                onDeleteDrawing = onDelete?.let { { askDelete = true } },
                canUndo = past.isNotEmpty(), onUndo = {
                val prev = past.removeAt(past.lastIndex)
                board = prev
                saveJob?.cancel()
                saveJob = scope.launch { runCatching { engine.run { writeCanvas(path, prev, null) } } }
            }, onDone = { setActive(false) })
        }
        Box(
            // In a note the drawing keeps its height (the tools sit above it); full screen it fills the rest.
            Modifier.fillMaxWidth().then(if (boardHeight != null) Modifier.height(boardHeight.dp) else Modifier.weight(1f))
                .clip(RoundedCornerShape(12.dp))
                .border(if (active) BorderStroke(2.dp, accent.copy(alpha = 0.7f)) else BorderStroke(1.dp, outline), RoundedCornerShape(12.dp))
                .background(paper)
                .onSizeChanged { box = it }
                .semantics { contentDescription = "Drawing" }
                .then(
                    if (active) {
                        Modifier.pointerInput(tool, color, penSize, view, shapeKind, fill) {
                            awaitEachGesture {
                                val down = awaitFirstDown(requireUnconsumed = false)
                                down.consume()
                                if (tool == DrawTool.SELECT) {
                                    // Pick what's under the finger (or the picked card's corner), then drag it.
                                    val b0 = board
                                    val start = view.world(down.position)
                                    val tol = 12f * density / view.k
                                    val cur = b0?.nodes?.firstOrNull { it.id == selected }
                                    val corner = cur?.let { Offset((it.x + it.width).toFloat(), (it.y + it.height).toFloat()) }
                                    val resizing = corner != null && (start - corner).getDistance() < tol * 1.8f
                                    val target = if (resizing) cur else b0?.let { hitNode(it, start, tol) }
                                    selected = target?.id
                                    var moved = false
                                    while (true) {
                                        val ev = awaitPointerEvent()
                                        ev.changes.forEach { it.consume() }
                                        val c = ev.changes.firstOrNull { it.id == down.id } ?: break
                                        if (!c.pressed) break
                                        if (b0 == null || target == null) continue
                                        val d = view.world(c.position) - start
                                        if (!moved && d.getDistance() * view.k < 4f * density) continue
                                        moved = true
                                        board = b0.copy(nodes = b0.nodes.map { n ->
                                            if (n.id != target.id) n
                                            else if (resizing) n.copy(width = max(16.0, Math.round(target.width + d.x).toDouble()), height = max(16.0, Math.round(target.height + d.y).toDouble()))
                                            else n.copy(x = Math.round(target.x + d.x).toDouble(), y = Math.round(target.y + d.y).toDouble())
                                        })
                                    }
                                    if (moved && b0 != null) board?.let { now -> board = b0; change(now) }
                                    return@awaitEachGesture
                                }
                                val pts = mutableListOf(down.position)
                                draft = pts.toList()
                                while (true) {
                                    val ev = awaitPointerEvent()
                                    ev.changes.forEach { it.consume() }
                                    val c = ev.changes.firstOrNull { it.id == down.id } ?: break
                                    if (!c.pressed) break
                                    if ((c.position - pts.last()).getDistance() > 1.5f) {
                                        pts.add(c.position)
                                        draft = pts.toList()
                                    }
                                }
                                draft = emptyList()
                                finish(pts)
                            }
                        }
                    } else Modifier.clickable { setActive(true) },
                ),
        ) {
            Canvas(Modifier.fillMaxSize()) {
                val b = board ?: return@Canvas
                withTransform({
                    translate(view.tx, view.ty)
                    scale(view.k, view.k, Offset.Zero)
                }) {
                    drawBoard(b, measurer, ink, paper)
                    if (tool == DrawTool.SELECT) b.nodes.firstOrNull { it.id == selected }?.let { n ->
                        val pad = 4f / view.k
                        val tl = Offset(n.x.toFloat() - pad, n.y.toFloat() - pad)
                        val sz = Size(n.width.toFloat() + 2 * pad, n.height.toFloat() + 2 * pad)
                        drawRect(accent, tl, sz, style = Stroke(1.5f / view.k, pathEffect = androidx.compose.ui.graphics.PathEffect.dashPathEffect(floatArrayOf(8f / view.k, 6f / view.k))))
                        val c = Offset((n.x + n.width).toFloat(), (n.y + n.height).toFloat())
                        drawCircle(paper, 8f / view.k, c)
                        drawCircle(accent, 8f / view.k, c, style = Stroke(2f / view.k))
                    }
                }
                if (draft.size > 1) drawDraft(tool, draft, colorOf(color), ink, (penSize * view.k).toFloat(), accent, shapeKind)
            }
            when {
                missing -> Text(stringResource(R.string.drawing_missing), Modifier.align(Alignment.Center), color = MaterialTheme.colorScheme.onSurfaceVariant)
                !active && board?.nodes?.isEmpty() == true -> Text(stringResource(R.string.tap_to_draw), Modifier.align(Alignment.Center), color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            if (!active && onOpenFull != null && !missing) {
                IconButton(onClick = onOpenFull, modifier = Modifier.align(Alignment.TopEnd).size(40.dp)) {
                    Icon(Icons.Outlined.OpenInFull, stringResource(R.string.open_full_size), Modifier.size(18.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }

    if (askDelete && onDelete != null) {
        AlertDialog(
            onDismissRequest = { askDelete = false },
            title = { Text(stringResource(R.string.delete_drawing_q)) },
            text = { Text(stringResource(R.string.delete_drawing_text)) },
            confirmButton = {
                TextButton(onClick = {
                    askDelete = false
                    saveJob?.cancel()
                    setActive(false)
                    onDelete()
                }) { Text(stringResource(R.string.delete), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { askDelete = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }

    asking?.let { done ->
        var text by remember(done) { mutableStateOf(askText) }
        AlertDialog(
            onDismissRequest = { asking = null },
            title = { Text(stringResource(if (askWords) R.string.add_words else R.string.shape_label)) },
            text = { OutlinedTextField(text, { text = it }, placeholder = { Text(stringResource(if (askWords) R.string.add_words else R.string.label_optional)) }) },
            confirmButton = { TextButton(onClick = { asking = null; done(text) }) { Text(stringResource(R.string.ok)) } },
            dismissButton = { TextButton(onClick = { asking = null }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
private fun Toolbar(
    tool: DrawTool, setTool: (DrawTool) -> Unit,
    color: String, setColor: (String) -> Unit,
    penSize: Double, setPenSize: (Double) -> Unit,
    shapeKind: String, setShape: (String) -> Unit,
    fill: String, setFill: (String) -> Unit,
    selection: CanvasNode?, onDelete: () -> Unit, onEditWords: () -> Unit,
    onDeleteDrawing: (() -> Unit)?,
    canUndo: Boolean, onUndo: () -> Unit, onDone: () -> Unit,
) {
    Surface(color = MaterialTheme.colorScheme.surfaceContainer, shape = RoundedCornerShape(12.dp), modifier = Modifier.fillMaxWidth().padding(bottom = 6.dp)) {
        Column(Modifier.padding(horizontal = 4.dp, vertical = 2.dp)) {
            // Every tool shows: on a narrow screen they wrap onto a second line.
            FlowRow(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.Center, itemVerticalAlignment = Alignment.CenterVertically) {
                ToolButton(Icons.Outlined.PanTool, stringResource(R.string.select_move), tool == DrawTool.SELECT) { setTool(DrawTool.SELECT) }
                ToolButton(Icons.Outlined.Draw, stringResource(R.string.pen), tool == DrawTool.PEN) { setTool(DrawTool.PEN) }
                ToolButton(Icons.Outlined.BorderColor, stringResource(R.string.highlighter), tool == DrawTool.HIGHLIGHT) { setTool(DrawTool.HIGHLIGHT) }
                // One button for every shape: a tap lists them.
                Box {
                    var picking by remember { mutableStateOf(false) }
                    ToolButton(shapeIcon(shapeKind), stringResource(R.string.shapes), tool == DrawTool.SHAPE) { picking = true }
                    DropdownMenu(picking, { picking = false }) {
                        SHAPES.forEach { g ->
                            DropdownMenuItem(
                                text = { Text(shapeName(g)) },
                                leadingIcon = { Icon(shapeIcon(g.kind), null, Modifier.size(22.dp)) },
                                onClick = {
                                    picking = false
                                    setShape(g.kind)
                                    setTool(DrawTool.SHAPE)
                                },
                            )
                        }
                    }
                }
                ToolButton(LineIcon, stringResource(R.string.line), tool == DrawTool.LINE) { setTool(DrawTool.LINE) }
                ToolButton(Icons.AutoMirrored.Outlined.ArrowRightAlt, stringResource(R.string.arrow), tool == DrawTool.ARROW) { setTool(DrawTool.ARROW) }
                ToolButton(DoubleArrowIcon, stringResource(R.string.double_arrow), tool == DrawTool.DOUBLE) { setTool(DrawTool.DOUBLE) }
                ToolButton(Icons.Outlined.TextFields, stringResource(R.string.words), tool == DrawTool.TEXT) { setTool(DrawTool.TEXT) }
                ToolButton(EraserIcon, stringResource(R.string.eraser), tool == DrawTool.ERASE) { setTool(DrawTool.ERASE) }
                IconButton(onClick = onUndo, enabled = canUndo) { Icon(Icons.AutoMirrored.Outlined.Undo, stringResource(R.string.undo)) }
                if (onDeleteDrawing != null) {
                    IconButton(onClick = onDeleteDrawing) { Icon(Icons.Outlined.Delete, stringResource(R.string.delete_drawing), tint = MaterialTheme.colorScheme.error) }
                }
                TextButton(onClick = onDone) {
                    Icon(Icons.Outlined.Check, null, Modifier.size(18.dp))
                    Spacer(Modifier.width(6.dp))
                    Text(stringResource(R.string.done))
                }
            }
            FlowRow(Modifier.padding(start = 8.dp, bottom = 6.dp), horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(6.dp), itemVerticalAlignment = Alignment.CenterVertically) {
                Swatch(null, color == "") { setColor("") }
                PALETTE.forEach { (id, c) -> Swatch(c, color == id) { setColor(id) } }
                if (selection != null) {
                    Spacer(Modifier.width(4.dp))
                    if (selection.ink == null) FillChip(stringResource(R.string.edit_words), false, onEditWords)
                    FillChip(stringResource(R.string.delete), false, onDelete)
                }
                if (tool == DrawTool.SHAPE || selection?.shape?.isNotEmpty() == true) {
                    Spacer(Modifier.width(4.dp))
                    FillChip(stringResource(R.string.fill_none), fill == "none") { setFill("none") }
                    FillChip(stringResource(R.string.fill_light), fill == "") { setFill("") }
                    FillChip(stringResource(R.string.fill_solid), fill == "solid") { setFill("solid") }
                }
                if (tool == DrawTool.PEN) {
                    Spacer(Modifier.width(4.dp))
                    PEN_SIZES.forEach { s ->
                        Box(
                            Modifier.size(30.dp).clip(CircleShape).background(if (s == penSize) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent).clickable { setPenSize(s) },
                            contentAlignment = Alignment.Center,
                        ) { Box(Modifier.size((s + 3).dp).clip(CircleShape).background(MaterialTheme.colorScheme.onSurface)) }
                    }
                }
            }
        }
    }
}

@Composable
private fun ToolButton(icon: ImageVector, label: String, on: Boolean, onClick: () -> Unit) {
    IconButton(onClick = onClick) {
        Box(
            Modifier.size(36.dp).clip(RoundedCornerShape(8.dp)).background(if (on) MaterialTheme.colorScheme.primary else Color.Transparent),
            contentAlignment = Alignment.Center,
        ) {
            Icon(icon, label, Modifier.size(20.dp), tint = if (on) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface)
        }
    }
}

@Composable
private fun Swatch(c: Color?, on: Boolean, onClick: () -> Unit) {
    val ring = MaterialTheme.colorScheme.onSurface
    Box(
        Modifier.size(28.dp).clip(CircleShape)
            .border(if (on) BorderStroke(2.dp, ring) else BorderStroke(1.dp, ring.copy(alpha = 0.2f)), CircleShape)
            .padding(4.dp).clip(CircleShape)
            .background(c ?: MaterialTheme.colorScheme.background)
            .clickable(onClick = onClick),
    ) {
        if (c == null) Canvas(Modifier.fillMaxSize()) { drawLine(Color(0xFFC33333), Offset(size.width, 0f), Offset(0f, size.height), 2.dp.toPx()) }
    }
}

@Composable
private fun FillChip(label: String, on: Boolean, onClick: () -> Unit) {
    Box(
        Modifier.height(30.dp).clip(RoundedCornerShape(15.dp))
            .background(if (on) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent)
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(15.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp),
        contentAlignment = Alignment.Center,
    ) { Text(label, style = MaterialTheme.typography.labelLarge) }
}

@Composable
private fun shapeName(g: ShapeGeometry): String = when (g.kind) {
    "rect" -> stringResource(R.string.rectangle)
    "ellipse" -> stringResource(R.string.ellipse)
    "diamond" -> stringResource(R.string.diamond)
    "rounded" -> stringResource(R.string.shape_rounded)
    "triangle" -> stringResource(R.string.shape_triangle)
    "hexagon" -> stringResource(R.string.shape_hexagon)
    "parallelogram" -> stringResource(R.string.shape_parallelogram)
    "star" -> stringResource(R.string.shape_star)
    "cylinder" -> stringResource(R.string.shape_cylinder)
    "bubble" -> stringResource(R.string.shape_bubble)
    "cloud" -> stringResource(R.string.shape_cloud)
    else -> g.name
}

/** An outline icon from path data in a `box` × `box` square. */
private fun pathIcon(name: String, box: Float, width: Float, vararg paths: String): ImageVector {
    val b = ImageVector.Builder(name, 24.dp, 24.dp, box + 12f, box + 12f)
    b.addGroup(translationX = 6f, translationY = 6f)
    paths.filter { it.isNotEmpty() }.forEach {
        b.addPath(PathParser().parsePathString(it).toNodes(), fill = null, stroke = SolidColor(Color.Black), strokeLineWidth = width, strokeLineCap = StrokeCap.Round, strokeLineJoin = StrokeJoin.Round)
    }
    b.clearGroup()
    return b.build()
}

private val shapeIcons = mutableMapOf<String, ImageVector>()
private fun shapeIcon(kind: String): ImageVector = shapeIcons.getOrPut(kind) {
    val g = shapeOf(kind)
    pathIcon("Shape $kind", 100f, 8f, g?.outline ?: "M0,0 L100,0 L100,100 L0,100 Z", g?.extra.orEmpty())
}
private val EraserIcon: ImageVector by lazy { pathIcon("Eraser", 12f, 1.3f, "M4.5,11 L0.8,7.3 L7.3,0.8 L11.2,4.7 L4.9,11 Z M2.6,5.5 L6.5,9.4 M4.5,11 L12,11") }
private val LineIcon: ImageVector by lazy { pathIcon("Line", 12f, 1.6f, "M0,12 L12,0") }
private val DoubleArrowIcon: ImageVector by lazy { pathIcon("Double arrow", 12f, 1.4f, "M0,6 L12,6 M3,3 L0,6 L3,9 M9,3 L12,6 L9,9") }

/**
 * A shape, filled lightly (see-through), solid, or not at all ("none": what's
 * under it shows), with its outline in `line` (or its colour).
 */
internal fun DrawScope.drawShape(g: ShapeGeometry?, fill: String, c: Color?, ink: Color, paper: Color, tl: Offset, sz: Size, width: Float, line: Color? = null) {
    val stroke = line ?: c ?: ink.copy(alpha = 0.7f)
    val path = g?.let { placePath(it.outline, tl, sz) } ?: Path().apply { addRect(androidx.compose.ui.geometry.Rect(tl, sz)) }
    when (fill) {
        "none" -> {}
        "solid" -> drawPath(path, c ?: mix(ink, paper, 0.78f), style = Fill)
        else -> drawPath(path, c?.copy(alpha = 0.16f) ?: paper.copy(alpha = 0.82f), style = Fill)
    }
    drawPath(path, stroke, style = Stroke(width, join = StrokeJoin.Round))
    g?.extra?.takeIf { it.isNotEmpty() }?.let { drawPath(placePath(it, tl, sz), stroke, style = Stroke(width)) }
}

private fun mix(c: Color, bg: Color, a: Float) = Color(c.red * a + bg.red * (1 - a), c.green * a + bg.green * (1 - a), c.blue * a + bg.blue * (1 - a), 1f)

/** Draws the cards, in world units (1 = 1 dp at 100%). */
internal fun DrawScope.drawBoard(b: Board, measurer: TextMeasurer, ink: Color, paper: Color) {
    val byId = b.nodes.associateBy { it.id }
    b.edges.forEach { e ->
        val a = byId[e.fromNode] ?: return@forEach
        val z = byId[e.toNode] ?: return@forEach
        val p1 = Offset((a.x + a.width / 2).toFloat(), (a.y + a.height / 2).toFloat())
        val p2 = Offset((z.x + z.width / 2).toFloat(), (z.y + z.height / 2).toFloat())
        val c = colorOf(e.color) ?: ink.copy(alpha = 0.5f)
        drawLine(c, p1, p2, 1.5f, StrokeCap.Round)
        arrowHead(p1, p2, c, 1.5f)
    }
    // Groups underneath.
    b.nodes.sortedBy { if (it.kind == "group") 0 else 1 }.forEach { n ->
        val c = colorOf(n.color)
        val tl = Offset(n.x.toFloat(), n.y.toFloat())
        val sz = Size(n.width.toFloat(), n.height.toFloat())
        val ink2 = n.ink
        when {
            ink2 != null -> {
                val sx = sz.width / ink2.w.toFloat()
                val sy = sz.height / ink2.h.toFloat()
                val pts = ink2.points.chunked(2).filter { it.size == 2 }.map { Offset(tl.x + it[0].toFloat() * sx, tl.y + it[1].toFloat() * sy) }
                if (pts.size < 2) return@forEach
                val color = if (ink2.highlight) (c ?: HIGHLIGHT).copy(alpha = 0.4f) else c ?: ink.copy(alpha = 0.85f)
                val stroke = Stroke(ink2.size.toFloat(), cap = StrokeCap.Round, join = StrokeJoin.Round)
                if (ink2.arrow || ink2.startArrow) {
                    drawLine(color, pts.first(), pts.last(), stroke.width, StrokeCap.Round)
                    if (ink2.arrow) arrowHead(pts.first(), pts.last(), color, stroke.width)
                    if (ink2.startArrow) arrowHead(pts.last(), pts.first(), color, stroke.width)
                } else drawPath(smooth(pts), color, style = stroke)
            }
            n.shape.isNotEmpty() -> {
                val g = shapeOf(n.shape)
                drawShape(g, n.fill, c, ink, paper, tl, sz, 2f)
                val lb = g?.label?.takeIf { it.size == 4 }?.map { it.toFloat() / 100f } ?: listOf(0.04f, 0.04f, 0.96f, 0.96f)
                val ltl = Offset(tl.x + lb[0] * sz.width, tl.y + lb[1] * sz.height)
                val lsz = Size((lb[2] - lb[0]) * sz.width, (lb[3] - lb[1]) * sz.height)
                label(measurer, n.text, ltl, lsz, if (n.fill == "solid") paper else ink, 17f, 0f, 0f)
            }
            n.kind == "group" -> {
                val col = c ?: Color(0xFF0B8CB5)
                drawRoundRect(col.copy(alpha = 0.07f), tl, sz, CornerRadius(12f))
                drawRoundRect(col.copy(alpha = 0.35f), tl, sz, CornerRadius(12f), style = Stroke(1f))
            }
            else -> {
                // A text card, note or link card: a soft card with its words.
                val bg = if (c != null) mix(c, paper, 0.14f) else mix(Color(0xFFE03E3E), paper, 0.06f)
                drawRoundRect(bg, tl, sz, CornerRadius(10f))
                val words = when (n.kind) { "file" -> n.file.substringAfterLast('/').removeSuffix(".md"); "link" -> n.url; else -> n.text }
                label(measurer, words, tl, sz, ink, 15f, 0.03f, 0.03f, center = false)
            }
        }
    }
}

private fun DrawScope.label(measurer: TextMeasurer, text: String, tl: Offset, sz: Size, ink: Color, size: Float, insetX: Float, insetY: Float, center: Boolean = true) {
    if (text.isBlank()) return
    val px = sz.width * insetX + 6f
    val py = sz.height * insetY + 4f
    val w = (sz.width - 2 * px).coerceAtLeast(1f)
    val h = (sz.height - 2 * py).coerceAtLeast(1f)
    val layout = measurer.measure(
        text,
        TextStyle(color = ink, fontSize = size.sp, textAlign = if (center) TextAlign.Center else TextAlign.Start, lineHeight = (size * 1.3f).sp),
        overflow = TextOverflow.Ellipsis,
        constraints = Constraints(maxWidth = w.toInt().coerceAtLeast(1), maxHeight = h.toInt().coerceAtLeast(1)),
    )
    val top = if (center) tl.y + (sz.height - layout.size.height) / 2 else tl.y + py
    val left = if (center) tl.x + (sz.width - layout.size.width) / 2 else tl.x + px
    drawText(layout, topLeft = Offset(left, top))
}

private fun smooth(pts: List<Offset>): Path = Path().apply {
    moveTo(pts[0].x, pts[0].y)
    for (i in 1 until pts.size) {
        val mid = (pts[i - 1] + pts[i]) / 2f
        quadraticTo(pts[i - 1].x, pts[i - 1].y, mid.x, mid.y)
    }
    lineTo(pts.last().x, pts.last().y)
}

private fun DrawScope.arrowHead(from: Offset, to: Offset, color: Color, width: Float) {
    val a = atan2((to.y - from.y).toDouble(), (to.x - from.x).toDouble())
    val len = 10f + width * 2
    for (side in listOf(-0.45, 0.45)) {
        val end = Offset((to.x - len * cos(a + side)).toFloat(), (to.y - len * sin(a + side)).toFloat())
        drawLine(color, to, end, width, StrokeCap.Round)
    }
}

/** What's being drawn right now, in screen pixels. */
private fun DrawScope.drawDraft(tool: DrawTool, pts: List<Offset>, picked: Color?, ink: Color, penPx: Float, accent: Color, shapeKind: String) {
    val a = pts.first()
    val z = pts.last()
    val color = picked ?: ink
    when (tool) {
        DrawTool.PEN -> drawPath(smooth(pts), color, style = Stroke(penPx.coerceAtLeast(1.5f), cap = StrokeCap.Round, join = StrokeJoin.Round))
        DrawTool.HIGHLIGHT -> drawPath(smooth(pts), (picked ?: HIGHLIGHT).copy(alpha = 0.4f), style = Stroke(max(penPx, 14f * density), cap = StrokeCap.Round, join = StrokeJoin.Round))
        DrawTool.LINE -> drawLine(color, a, z, max(penPx, 1.5f), StrokeCap.Round)
        DrawTool.ARROW, DrawTool.DOUBLE -> {
            drawLine(color, a, z, 2.5f * density, StrokeCap.Round)
            arrowHead(a, z, color, 2.5f * density)
            if (tool == DrawTool.DOUBLE) arrowHead(z, a, color, 2.5f * density)
        }
        DrawTool.SHAPE -> {
            val tl = Offset(min(a.x, z.x), min(a.y, z.y))
            val sz = Size(abs(z.x - a.x), abs(z.y - a.y))
            val dash = Stroke(1.5f * density, pathEffect = androidx.compose.ui.graphics.PathEffect.dashPathEffect(floatArrayOf(10f, 8f)))
            val g = shapeOf(shapeKind)
            if (g != null) drawPath(placePath(g.outline, tl, sz), accent, style = dash) else drawRect(accent, tl, sz, style = dash)
        }
        DrawTool.ERASE -> drawCircle(accent.copy(alpha = 0.3f), 14f * density, z)
        DrawTool.TEXT, DrawTool.SELECT -> {}
    }
}
