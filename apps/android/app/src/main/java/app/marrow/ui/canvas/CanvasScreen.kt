package app.marrow.ui.canvas

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.AddComment
import androidx.compose.material.icons.outlined.CenterFocusStrong
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.NoteAdd
import androidx.compose.material.icons.outlined.Palette
import androidx.compose.material.icons.outlined.Timeline
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.core.Board
import app.marrow.core.CanvasEdge
import app.marrow.core.CanvasNode
import app.marrow.core.MarrowException
import app.marrow.core.renderMarkdown
import app.marrow.data.words
import app.marrow.ui.common.BackBar
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.nameOf
import app.marrow.ui.note.BlockActions
import app.marrow.ui.note.Blocks
import kotlinx.coroutines.delay
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

/** JSON Canvas colours "1"–"6". */
private val PALETTE = listOf("1" to Color(0xFFE03E3E), "2" to Color(0xFFE8780C), "3" to Color(0xFFD4A106), "4" to Color(0xFF2F9E44), "5" to Color(0xFF0B8CB5), "6" to Color(0xFF8E44AD))

private fun colorOf(c: String): Color? = PALETTE.firstOrNull { it.first == c }?.second
    ?: if (c.matches(Regex("#[0-9a-fA-F]{6}"))) Color(android.graphics.Color.parseColor(c)) else null

private fun newId(): String = (1..16).map { "0123456789abcdef".random() }.joinToString("")

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CanvasScreen(path: String) {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    val ctx = androidx.compose.ui.platform.LocalContext.current
    var viewport by remember { mutableStateOf(androidx.compose.ui.unit.IntSize.Zero) }
    var board by remember { mutableStateOf<Board?>(null) }
    var onDisk by remember { mutableStateOf<Board?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var scale by remember { mutableFloatStateOf(1f) }
    var pan by remember { mutableStateOf(Offset.Zero) }
    var selected by remember { mutableStateOf<String?>(null) }
    var connecting by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf<CanvasNode?>(null) }
    var picking by remember { mutableStateOf(false) }
    var colors by remember { mutableStateOf(false) }
    var dirty by remember { mutableStateOf(0) }
    val density = LocalDensity.current

    LaunchedEffect(path) {
        runCatching { engine.run { readCanvas(path) } }
            .onSuccess {
                board = it
                onDisk = it
            }
            .onFailure { error = it.words() }
    }
    // Save a moment after each change.
    LaunchedEffect(dirty) {
        if (dirty == 0) return@LaunchedEffect
        delay(600)
        val b = board ?: return@LaunchedEffect
        try {
            engine.run { writeCanvas(path, b, onDisk) }
            onDisk = b
        } catch (e: MarrowException.ChangedElsewhere) {
            engine.say(ctx.getString(R.string.canvas_reloaded))
            board = engine.run { readCanvas(path) }
            onDisk = board
        } catch (e: Exception) {
            engine.say(e.words())
        }
    }

    fun change(f: (Board) -> Board) {
        board = board?.let(f)
        dirty++
    }

    fun center(): Offset = Offset((viewport.width / 2f - pan.x) / scale / density.density, (viewport.height / 2f - pan.y) / scale / density.density)

    fun showAll() {
        val b = board ?: return
        val (s, p) = fit(b, viewport.width.toFloat(), viewport.height.toFloat(), density.density)
        scale = s
        pan = p
    }
    // Show the whole board once it's loaded and laid out.
    LaunchedEffect(board != null, viewport) { if (board != null && viewport.width > 0 && onDisk === board) showAll() }

    fun add(node: CanvasNode) {
        change { it.copy(nodes = it.nodes + node) }
        selected = node.id
    }

    Scaffold(
        containerColor = MaterialTheme.colorScheme.surfaceContainerLowest,
        topBar = {
            BackBar(nameOf(path)) {
                IconButton(onClick = { showAll() }) { Icon(Icons.Outlined.CenterFocusStrong, stringResource(R.string.fit_to_screen)) }
            }
        },
        bottomBar = {
            Surface(color = MaterialTheme.colorScheme.surfaceContainer, modifier = Modifier.navigationBarsPadding()) {
                Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp), horizontalArrangement = Arrangement.SpaceEvenly) {
                    Tool(Icons.Outlined.AddComment, stringResource(R.string.add_card)) {
                        val c = center()
                        val n = CanvasNode(newId(), "text", (c.x - 120).toDouble(), (c.y - 60).toDouble(), 240.0, 120.0, "", "", "", "", "", "", null, "")
                        add(n)
                        editing = n
                    }
                    Tool(Icons.Outlined.NoteAdd, stringResource(R.string.add_note)) { picking = true }
                    Tool(Icons.Outlined.Timeline, stringResource(R.string.connect), enabled = selected != null, active = connecting) { connecting = !connecting }
                    Tool(Icons.Outlined.Palette, stringResource(R.string.color), enabled = selected != null) { colors = true }
                    Tool(Icons.Outlined.Delete, stringResource(R.string.delete), enabled = selected != null) {
                        val id = selected ?: return@Tool
                        change { b -> b.copy(nodes = b.nodes.filter { it.id != id }, edges = b.edges.filter { it.fromNode != id && it.toNode != id }) }
                        selected = null
                    }
                }
            }
        },
    ) { pad ->
        val b = board
        when {
            error != null -> Text(error!!, Modifier.padding(pad).padding(24.dp), color = MaterialTheme.colorScheme.error)
            b == null -> Unit
            else -> Box(
                Modifier.padding(pad).fillMaxSize().clipToBounds().onSizeChanged { viewport = it }
                    .pointerInput(Unit) {
                        detectTransformGestures { centroid, p, zoom, _ ->
                            val s = (scale * zoom).coerceIn(0.2f, 4f)
                            // Zoom around the fingers.
                            pan = centroid - (centroid - pan) * (s / scale) + p
                            scale = s
                        }
                    }
                    .pointerInput(Unit) { detectTapGestures { selected = null; connecting = false } },
            ) {
                val d = density.density
                Box(
                    Modifier.wrapContentSize(Alignment.TopStart, unbounded = true).graphicsLayer {
                        translationX = pan.x
                        translationY = pan.y
                        scaleX = scale
                        scaleY = scale
                        transformOrigin = TransformOrigin(0f, 0f)
                    },
                ) {
                    val nodes = b.nodes
                    val byId = nodes.associateBy { it.id }
                    // Groups first, under everything.
                    nodes.filter { it.kind == "group" }.forEach { n -> Card(n, selected == n.id, d, onTap = { selected = n.id }, onDrag = { dx, dy -> change { it.moved(n.id, dx, dy, true) } }) { GroupLabel(n) } }
                    Edges(b.edges, byId)
                    nodes.filter { it.kind != "group" }.forEach { n ->
                        Card(
                            n,
                            selected == n.id,
                            d,
                            onTap = {
                                val from = selected
                                if (connecting && from != null && from != n.id) {
                                    change { it.copy(edges = it.edges + CanvasEdge(newId(), from, n.id, "", "", "", "", "")) }
                                    connecting = false
                                }
                                selected = n.id
                            },
                            onOpen = {
                                when {
                                    n.kind == "file" && n.file.isNotEmpty() -> nav.openPath(n.file)
                                    n.kind == "text" && n.ink == null -> editing = n
                                }
                            },
                            onDrag = { dx, dy -> change { it.moved(n.id, dx, dy, false) } },
                        ) { CardContent(n) }
                    }
                }
                if (connecting) {
                    Surface(shape = RoundedCornerShape(50), color = MaterialTheme.colorScheme.inverseSurface, modifier = Modifier.align(Alignment.TopCenter).padding(12.dp)) {
                        Text(stringResource(R.string.tap_card_to_connect), Modifier.padding(horizontal = 16.dp, vertical = 8.dp), color = MaterialTheme.colorScheme.inverseOnSurface)
                    }
                }
            }
        }
    }

    editing?.let { n ->
        var text by remember(n.id) { mutableStateOf(n.text) }
        AlertDialog(
            onDismissRequest = { editing = null },
            title = { Text(stringResource(R.string.card)) },
            text = { OutlinedTextField(text, { text = it }, modifier = Modifier.fillMaxWidth().heightIn(min = 160.dp), placeholder = { Text(stringResource(R.string.write_something)) }) },
            confirmButton = {
                TextButton(onClick = {
                    change { b -> b.copy(nodes = b.nodes.map { if (it.id == n.id) it.copy(text = text) else it }) }
                    editing = null
                }) { Text(stringResource(R.string.done)) }
            },
        )
    }
    if (picking) {
        var q by remember { mutableStateOf("") }
        var hits by remember { mutableStateOf(emptyList<app.marrow.core.FileHit>()) }
        LaunchedEffect(q) { hits = runCatching { engine.run { findFiles(q, null, 20u) } }.getOrDefault(emptyList()).filter { it.isNote } }
        AlertDialog(
            onDismissRequest = { picking = false },
            title = { Text(stringResource(R.string.add_note)) },
            text = {
                Column {
                    OutlinedTextField(q, { q = it }, placeholder = { Text(stringResource(R.string.find_a_note)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                    LazyColumn(Modifier.heightIn(max = 320.dp)) {
                        items(hits) { h ->
                            ListItem(
                                headlineContent = { Text(h.name) },
                                supportingContent = { if (h.folder.isNotEmpty()) Text(h.folder) },
                                leadingContent = { Icon(Icons.Outlined.Description, null) },
                                modifier = Modifier.clickable {
                                    picking = false
                                    val c = center()
                                    add(CanvasNode(newId(), "file", (c.x - 150).toDouble(), (c.y - 100).toDouble(), 300.0, 200.0, "", h.path, "", "", "", "", null, ""))
                                },
                            )
                        }
                    }
                }
            },
            confirmButton = { TextButton(onClick = { picking = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
    if (colors) {
        AlertDialog(
            onDismissRequest = { colors = false },
            title = { Text(stringResource(R.string.color)) },
            text = {
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    (listOf("" to MaterialTheme.colorScheme.surfaceContainerHigh) + PALETTE).forEach { (key, c) ->
                        Box(Modifier.size(36.dp).clip(CircleShape).background(c).border(1.dp, MaterialTheme.colorScheme.outline, CircleShape).clickable {
                            val id = selected
                            change { b -> b.copy(nodes = b.nodes.map { if (it.id == id) it.copy(color = key) else it }) }
                            colors = false
                        })
                    }
                }
            },
            confirmButton = {},
        )
    }
}

/** Moves a card (and, for a group, the cards inside it) by dx, dy. */
private fun Board.moved(id: String, dx: Double, dy: Double, group: Boolean): Board {
    val g = nodes.firstOrNull { it.id == id } ?: return this
    val inside = if (group) nodes.filter { it.id != id && it.x >= g.x && it.y >= g.y && it.x + it.width <= g.x + g.width && it.y + it.height <= g.y + g.height }.map { it.id }.toSet() else emptySet()
    return copy(nodes = nodes.map { if (it.id == id || it.id in inside) it.copy(x = it.x + dx, y = it.y + dy) else it })
}

/** Scale and offset that show the whole board in a view of `vw` × `vh` pixels. */
private fun fit(b: Board, vw: Float, vh: Float, d: Float): Pair<Float, Offset> {
    if (b.nodes.isEmpty() || vw <= 0f) return 1f to Offset(vw / 2, vh / 2)
    val minX = b.nodes.minOf { it.x }.toFloat()
    val minY = b.nodes.minOf { it.y }.toFloat()
    val w = (b.nodes.maxOf { it.x + it.width }.toFloat() - minX).coerceAtLeast(1f)
    val h = (b.nodes.maxOf { it.y + it.height }.toFloat() - minY).coerceAtLeast(1f)
    val s = (minOf(vw / (w * d), vh / (h * d)) * 0.9f).coerceIn(0.2f, 1.25f)
    return s to Offset((vw - w * d * s) / 2 - minX * d * s, (vh - h * d * s) / 2 - minY * d * s)
}

@Composable
private fun Card(n: CanvasNode, selected: Boolean, density: Float, onTap: () -> Unit, onOpen: () -> Unit = {}, onDrag: (Double, Double) -> Unit, content: @Composable () -> Unit) {
    val accent = colorOf(n.color)
    val isGroup = n.kind == "group"
    val isInk = n.ink != null
    Box(
        Modifier
            .offset { IntOffset((n.x * density).roundToInt(), (n.y * density).roundToInt()) }
            .size(n.width.toFloat().dp, n.height.toFloat().dp)
            .pointerInput(n.id) { detectTapGestures(onTap = { onTap() }, onDoubleTap = { onOpen() }) }
            .pointerInput(n.id) {
                detectDragGestures(onDragStart = { onTap() }) { change, amount ->
                    change.consume()
                    onDrag((amount.x / density).toDouble(), (amount.y / density).toDouble())
                }
            },
    ) {
        if (isInk) {
            Ink(n, accent ?: MaterialTheme.colorScheme.onSurface)
        } else {
            Surface(
                shape = RoundedCornerShape(if (isGroup) 16.dp else 12.dp),
                color = when {
                    isGroup -> (accent ?: MaterialTheme.colorScheme.outline).copy(alpha = 0.06f)
                    accent != null -> accent.copy(alpha = 0.12f).compositeOver(MaterialTheme.colorScheme.surface)
                    else -> MaterialTheme.colorScheme.surface
                },
                border = BorderStroke(if (selected) 2.dp else 1.dp, if (selected) MaterialTheme.colorScheme.primary else accent ?: MaterialTheme.colorScheme.outlineVariant),
                shadowElevation = if (isGroup) 0.dp else 1.dp,
                modifier = Modifier.fillMaxSize(),
            ) { content() }
        }
        if (selected && isInk) {
            Box(Modifier.fillMaxSize().border(2.dp, MaterialTheme.colorScheme.primary, RoundedCornerShape(6.dp)))
        }
    }
}

private fun Color.compositeOver(bg: Color): Color = Color(
    red = red * alpha + bg.red * (1 - alpha),
    green = green * alpha + bg.green * (1 - alpha),
    blue = blue * alpha + bg.blue * (1 - alpha),
)

@Composable
private fun GroupLabel(n: CanvasNode) {
    Text(n.label, Modifier.padding(12.dp), style = MaterialTheme.typography.titleSmall, color = colorOf(n.color) ?: MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable
private fun CardContent(n: CanvasNode) {
    val engine = LocalEngine.current
    when (n.kind) {
        "file" -> Column(Modifier.padding(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Icon(Icons.Outlined.Description, null, Modifier.size(16.dp), tint = MaterialTheme.colorScheme.primary)
                Text(nameOf(n.file), style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            val text = remember(n.file) { runCatching { app.marrow.core.preview(engine.marrow.readNote(n.file), 400u) }.getOrDefault("") }
            Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(top = 6.dp))
        }
        "link" -> Column(Modifier.padding(12.dp)) {
            Text(n.url, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.primary, overflow = TextOverflow.Ellipsis)
        }
        else -> Box(Modifier.padding(horizontal = 12.dp, vertical = 8.dp)) {
            val blocks = remember(n.text) { renderMarkdown(n.text) }
            Blocks(blocks, BlockActions())
        }
    }
}

@Composable
private fun Ink(n: CanvasNode, color: Color) {
    val ink = n.ink ?: return
    Canvas(Modifier.fillMaxSize()) {
        val sx = size.width / ink.w.toFloat()
        val sy = size.height / ink.h.toFloat()
        val pts = ink.points.chunked(2).filter { it.size == 2 }.map { Offset(it[0].toFloat() * sx, it[1].toFloat() * sy) }
        if (pts.size < 2) return@Canvas
        val stroke = Stroke(ink.size.toFloat() * density, cap = StrokeCap.Round, join = StrokeJoin.Round)
        if (ink.arrow) {
            drawLine(color, pts.first(), pts.last(), stroke.width, StrokeCap.Round)
            arrowHead(pts.first(), pts.last(), color, stroke.width)
        } else {
            val p = Path().apply {
                moveTo(pts[0].x, pts[0].y)
                for (i in 1 until pts.size) {
                    val mid = (pts[i - 1] + pts[i]) / 2f
                    quadraticTo(pts[i - 1].x, pts[i - 1].y, mid.x, mid.y)
                }
                lineTo(pts.last().x, pts.last().y)
            }
            drawPath(p, color, style = stroke)
        }
    }
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.arrowHead(from: Offset, to: Offset, color: Color, width: Float) {
    val a = atan2((to.y - from.y).toDouble(), (to.x - from.x).toDouble())
    val len = 10f * density + width
    for (side in listOf(-0.45, 0.45)) {
        val end = Offset((to.x - len * cos(a + side)).toFloat(), (to.y - len * sin(a + side)).toFloat())
        drawLine(color, to, end, width, StrokeCap.Round)
    }
}

/** Where an edge leaves a card: the middle of the named side, or the side facing the other card. */
private fun anchor(n: CanvasNode, side: String, other: CanvasNode): Offset {
    val cx = n.x + n.width / 2
    val cy = n.y + n.height / 2
    val s = side.ifEmpty {
        val dx = (other.x + other.width / 2) - cx
        val dy = (other.y + other.height / 2) - cy
        if (abs(dx) * n.height > abs(dy) * n.width) (if (dx > 0) "right" else "left") else (if (dy > 0) "bottom" else "top")
    }
    return when (s) {
        "left" -> Offset(n.x.toFloat(), cy.toFloat())
        "right" -> Offset((n.x + n.width).toFloat(), cy.toFloat())
        "top" -> Offset(cx.toFloat(), n.y.toFloat())
        else -> Offset(cx.toFloat(), (n.y + n.height).toFloat())
    }
}

@Composable
private fun Edges(edges: List<CanvasEdge>, byId: Map<String, CanvasNode>) {
    val line = MaterialTheme.colorScheme.outline
    // Drawn from the board's origin; nothing is clipped, so edges anywhere show.
    Canvas(Modifier.size(1.dp)) {
        val d = density
        edges.forEach { e ->
            val a = byId[e.fromNode] ?: return@forEach
            val b = byId[e.toNode] ?: return@forEach
            val p1 = anchor(a, e.fromSide, b) * d
            val p2 = anchor(b, e.toSide, a) * d
            val c = colorOf(e.color) ?: line
            drawLine(c, p1, p2, 2f * d, StrokeCap.Round)
            arrowHead(p1, p2, c, 2f * d)
        }
    }
}

@Composable
private fun Tool(icon: androidx.compose.ui.graphics.vector.ImageVector, label: String, enabled: Boolean = true, active: Boolean = false, onClick: () -> Unit) {
    Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.clickable(enabled = enabled, onClick = onClick).padding(horizontal = 6.dp, vertical = 4.dp)) {
        Icon(icon, label, tint = when {
            active -> MaterialTheme.colorScheme.primary
            enabled -> MaterialTheme.colorScheme.onSurface
            else -> MaterialTheme.colorScheme.outline
        })
        Text(label, style = MaterialTheme.typography.labelSmall, color = if (enabled) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.outline)
    }
}
