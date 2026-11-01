package app.marrow.ui.home

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentSize
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.horizontalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Undo
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.CheckBox
import androidx.compose.material.icons.outlined.CheckBoxOutlineBlank
import androidx.compose.material.icons.outlined.ChevronLeft
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material.icons.outlined.AutoAwesomeMosaic
import androidx.compose.material.icons.outlined.Done
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material.icons.outlined.GridView
import androidx.compose.material.icons.outlined.ViewAgenda
import androidx.compose.material.icons.outlined.Palette
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalFontFamilyResolver
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.marrow.R
import app.marrow.core.Board
import app.marrow.core.CanvasNode
import app.marrow.core.TaskFilter
import app.marrow.core.homeStarterBoard
import app.marrow.core.homeTemplateBoard
import app.marrow.core.homeTemplateRead
import app.marrow.core.homeTemplateWrite
import app.marrow.core.homeTemplates
import app.marrow.core.noteSection
import app.marrow.core.renderMarkdown
import app.marrow.ui.AskRoute
import app.marrow.ui.SearchRoute
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.load
import app.marrow.ui.common.longDate
import app.marrow.ui.common.nowTime
import app.marrow.ui.common.today
import app.marrow.ui.drawing.drawBoard
import app.marrow.ui.note.BlockActions
import app.marrow.ui.note.Blocks
import app.marrow.ui.theme.Garamond
import app.marrow.ui.theme.Literata
import app.marrow.data.words
import coil3.compose.AsyncImage
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.time.LocalDate
import java.time.LocalTime
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle as DayStyle
import java.time.temporal.ChronoUnit
import java.util.Locale
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt

/*
 * Home: the person's own board, the first thing the app shows. It's the
 * vault's `Home.canvas`, the same board as on the computer: live cards
 * (sticky notes, tasks picked by any filter, a note or part of one, lists of
 * notes, a greeting, a quick note, shortcuts, a calendar, today's note, a
 * countdown, a clock, rediscover), pictures, links, text and drawings.
 * Pinch and drag to look around; "Edit" moves, resizes, adds and restyles
 * cards; Templates swaps in a ready-made board or one someone shared.
 */

private const val HOME = "Home.canvas"

/** A card's live part: its kind, its settings and its look (from `extra`). */
private class Live(val widget: String, val cfg: JSONObject, val style: JSONObject)

private fun CanvasNode.live(): Live {
    val o = runCatching { JSONObject(extra) }.getOrDefault(JSONObject())
    return Live(o.optString("widget"), o.optJSONObject("cfg") ?: JSONObject(), o.optJSONObject("style") ?: JSONObject())
}

/** The same card with changed settings or look, kept with its other fields. */
private fun CanvasNode.with(cfg: JSONObject? = null, style: JSONObject? = null): CanvasNode {
    val o = runCatching { JSONObject(extra) }.getOrDefault(JSONObject())
    if (cfg != null) o.put("cfg", cfg)
    if (style != null) o.put("style", style)
    return copy(extra = o.toString())
}

private fun JSONObject.copyWith(k: String, v: Any?): JSONObject = JSONObject(toString()).apply { if (v == null || v == "") remove(k) else put(k, v) }

private fun newId() = java.util.UUID.randomUUID().toString().replace("-", "").take(16)

/** The board's own settings (`home` in the file). */
private fun Board.settings(): JSONObject = runCatching { JSONObject(extra).optJSONObject("home") }.getOrNull() ?: JSONObject()
private fun Board.withSettings(s: JSONObject): Board = copy(extra = runCatching { JSONObject(extra) }.getOrDefault(JSONObject()).put("home", s).toString())

private val WIDGETS = listOf("sticky", "tasks", "note", "notes", "greeting", "capture", "shortcuts", "calendar", "daily", "countdown", "clock", "rediscover", "welcome")
private val SIZES = mapOf("sticky" to (240 to 200), "tasks" to (380 to 360), "note" to (360 to 320), "notes" to (300 to 300), "greeting" to (480 to 130), "capture" to (440 to 96), "shortcuts" to (480 to 70), "calendar" to (300 to 300), "daily" to (360 to 320), "countdown" to (240 to 140), "clock" to (220 to 110), "rediscover" to (360 to 280), "welcome" to (520 to 300))

@Composable
private fun widgetName(w: String): String = stringResource(
    when (w) {
        "sticky" -> R.string.w_sticky; "tasks" -> R.string.w_tasks; "note" -> R.string.w_note; "notes" -> R.string.w_notes
        "greeting" -> R.string.w_greeting; "capture" -> R.string.w_capture; "shortcuts" -> R.string.w_shortcuts; "calendar" -> R.string.w_calendar
        "daily" -> R.string.w_daily; "countdown" -> R.string.w_countdown; "clock" -> R.string.w_clock; "rediscover" -> R.string.w_rediscover
        "welcome" -> R.string.w_welcome; "picture" -> R.string.w_picture; "link" -> R.string.w_link; else -> R.string.w_text
    },
)

/** A new live card of [kind], with the settings it starts with (as on the computer). */
private fun newCard(kind: String, x: Double, y: Double): CanvasNode {
    val (w, h) = SIZES[kind] ?: (300 to 200)
    val cfg = when (kind) {
        "tasks" -> JSONObject("""{"title":"Next 7 days","range":"days","days":7,"from":"","to":"","overdue":true,"undated":false,"done":false,"tag":"","folder":"","note":"","text":"","group":true,"showNote":true}""")
        "note" -> JSONObject("""{"path":"","section":""}""")
        "notes" -> JSONObject("""{"title":"Recently edited","kind":"recent","value":"","limit":8}""")
        "greeting" -> JSONObject("""{"name":"","date":true,"stats":true}""")
        "capture" -> JSONObject("""{"target":"daily","path":"","task":false}""")
        "shortcuts" -> JSONObject("""{"items":["new","today","search","ask"]}""")
        "countdown" -> JSONObject("""{"title":"","date":""}""")
        "clock" -> JSONObject("""{"h24":false}""")
        else -> JSONObject()
    }
    val style = when (kind) {
        "sticky" -> JSONObject("""{"bg":"#ffd84d","tint":45,"look":"paper"}""")
        "greeting", "shortcuts" -> JSONObject("""{"look":"none"}""")
        else -> null
    }
    val extra = JSONObject().put("widget", kind).put("cfg", cfg).apply { if (style != null) put("style", style) }
    val label = if (kind == "sticky") "" else "${kind.replaceFirstChar { it.uppercase() }} (Marrow Home card)"
    return CanvasNode(newId(), "text", x, y, w.toDouble(), h.toDouble(), label, "", "", "", "", "", "", null, extra.toString())
}

// ------------------------------------------------------------------ looks

private val PALETTE = listOf("#e03e3e", "#e8780c", "#ffd84d", "#2f9e44", "#0b8cb5", "#8e44ad", "#e64980", "#868e96")
private val CANVAS_COLORS = mapOf("1" to "#e03e3e", "2" to "#e8780c", "3" to "#d4a106", "4" to "#2f9e44", "5" to "#0b8cb5", "6" to "#8e44ad")

private fun hex(s: String): Color? = (CANVAS_COLORS[s] ?: s).let { if (Regex("#[0-9a-fA-F]{6}").matches(it)) Color(android.graphics.Color.parseColor(it)) else null }
private fun mix(c: Color, bg: Color, a: Float) = Color(c.red * a + bg.red * (1 - a), c.green * a + bg.green * (1 - a), c.blue * a + bg.blue * (1 - a), 1f)

private data class Look(val look: String, val bg: Color, val fg: Color, val font: FontFamily?, val scale: Float, val align: TextAlign, val radius: Dp, val alpha: Float)

@Composable
private fun lookOf(style: JSONObject): Look {
    val paper = MaterialTheme.colorScheme.background
    val ink = MaterialTheme.colorScheme.onBackground
    val look = style.optString("look").ifEmpty { "card" }
    val color = hex(style.optString("bg"))
    val tint = style.optInt("tint", 18) / 100f
    val bg = when {
        look == "none" || look == "outline" -> Color.Transparent
        look == "glass" -> paper.copy(alpha = 0.6f)
        color != null -> mix(color, paper, tint)
        look == "paper" -> mix(Color(0xFFFFD84D), paper, 0.45f)
        else -> MaterialTheme.colorScheme.surfaceContainerLow
    }
    val font = when (style.optString("font")) { "read" -> Literata; "classic" -> Garamond; "mono" -> FontFamily.Monospace; "hand" -> FontFamily.Cursive; else -> null }
    val scale = when (style.optString("size")) { "s" -> 0.87f; "l" -> 1.2f; "xl" -> 1.6f; else -> 1f }
    val align = when (style.optString("align")) { "center" -> TextAlign.Center; "right" -> TextAlign.End; else -> TextAlign.Start }
    val radius = style.optInt("radius", if (look in setOf("paper", "lined", "grid", "tape", "pin", "fold")) 3 else 12).dp
    return Look(look, bg, hex(style.optString("fg")) ?: ink, font, scale, align, radius, style.optInt("opacity", 100) / 100f)
}

private class FoldShape(private val r: Dp, private val fold: Dp) : Shape {
    override fun createOutline(size: Size, layoutDirection: androidx.compose.ui.unit.LayoutDirection, density: Density): androidx.compose.ui.graphics.Outline {
        val f = with(density) { fold.toPx() }
        return androidx.compose.ui.graphics.Outline.Generic(Path().apply {
            moveTo(0f, 0f); lineTo(size.width, 0f); lineTo(size.width, size.height - f); lineTo(size.width - f, size.height); lineTo(0f, size.height); close()
        })
    }
}

/** The card's frame: background, shadow, border and its look's extras (tape, pin, lines). */
@Composable
private fun Modifier.framed(l: Look, edge: Color): Modifier {
    val shape: Shape = if (l.look == "fold") FoldShape(l.radius, 22.dp) else RoundedCornerShape(l.radius)
    val lines = l.fg.copy(alpha = 0.09f)
    return this
        .alpha(l.alpha)
        .then(if (l.look in setOf("none", "outline", "glass")) Modifier else Modifier.shadow(if (l.look == "card") 3.dp else 5.dp, shape, clip = false))
        .clip(shape)
        .background(l.bg)
        .then(if (l.look == "outline") Modifier.border(BorderStroke(2.dp, edge), shape) else if (l.look == "glass") Modifier.border(BorderStroke(1.dp, l.fg.copy(alpha = 0.12f)), shape) else Modifier)
        .drawBehind {
            when (l.look) {
                "lined" -> { var y = 34.dp.toPx(); while (y < size.height) { drawLine(Color(0x594A90D9), Offset(0f, y), Offset(size.width, y), 1f); y += 22.dp.toPx() } }
                "grid" -> {
                    val s = 16.dp.toPx(); var x = s; while (x < size.width) { drawLine(lines, Offset(x, 0f), Offset(x, size.height), 1f); x += s }
                    var y = s; while (y < size.height) { drawLine(lines, Offset(0f, y), Offset(size.width, y), 1f); y += s }
                }
            }
        }
}

// ------------------------------------------------------------------ screen

private enum class Sheet { NONE, ADD, CARD, BOARD, TEMPLATES }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HomeScreen(onCapture: () -> Unit) {
    val engine = LocalEngine.current
    val scope = rememberCoroutineScope()
    val ctx = LocalContext.current
    var board by remember { mutableStateOf<Board?>(null) }
    var edit by remember { mutableStateOf(false) }
    // Phones show the cards as a list; the board itself is a tap away (and for editing).
    val narrow = androidx.compose.ui.platform.LocalConfiguration.current.screenWidthDp < 600
    var asList by remember(narrow) { mutableStateOf(narrow) }
    var selected by remember { mutableStateOf<String?>(null) }
    var sheet by remember { mutableStateOf(Sheet.NONE) }
    val past = remember { mutableStateListOf<Board>() }
    var saveJob by remember { mutableStateOf<Job?>(null) }
    var asking by remember { mutableStateOf<Int?>(null) }
    var exporting by remember { mutableStateOf(false) }
    var pendingImport by remember { mutableStateOf<Board?>(null) }
    var importName by remember { mutableStateOf("") }
    val vault by engine.vault.collectAsStateWithLifecycle()

    LaunchedEffect(vault?.root) {
        if (vault == null) return@LaunchedEffect
        val b = runCatching { engine.run { readCanvas(HOME) } }.getOrNull()
        board = b ?: homeStarterBoard(true, (vault?.notes ?: 0u) < 15u).also { s ->
            runCatching { engine.run { writeCanvas(HOME, s, null) } }.onFailure { engine.say(it.words()) }
        }
        past.clear()
    }
    // Changed elsewhere (sync, the computer): show what's on disk, unless editing here.
    LaunchedEffect(vault?.root) {
        engine.changes.collect { paths ->
            if (HOME in paths && saveJob?.isActive != true) runCatching { engine.run { readCanvas(HOME) } }.onSuccess { board = it }
        }
    }

    fun save(b: Board) {
        saveJob?.cancel()
        saveJob = scope.launch {
            delay(500)
            runCatching { engine.run { writeCanvas(HOME, b, null) } }.onFailure { engine.say(it.words()) }
        }
    }
    fun change(next: Board, undoable: Boolean = true) {
        board?.let { if (undoable) { past.add(it); if (past.size > 60) past.removeAt(0) } }
        board = next
        save(next)
    }
    fun update(id: String, f: (CanvasNode) -> CanvasNode, undoable: Boolean = true) {
        val b = board ?: return
        change(b.copy(nodes = b.nodes.map { if (it.id == id) f(it) else it }), undoable)
    }

    val importer = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val text = runCatching { ctx.contentResolver.openInputStream(uri)?.use { it.readBytes().decodeToString() } }.getOrNull()
            runCatching { homeTemplateRead(text ?: "") }
                .onSuccess { t -> pendingImport = t.board; importName = t.name.ifEmpty { "Template" } }
                .onFailure { engine.say(ctx.getString(R.string.home_not_template)) }
        }
    }
    var exportText by remember { mutableStateOf("") }
    val exporter = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/json")) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        runCatching { ctx.contentResolver.openOutputStream(uri)?.use { it.write(exportText.toByteArray()) } }
            .onSuccess { engine.say(ctx.getString(R.string.home_template_saved)) }
            .onFailure { engine.say(it.words()) }
    }

    /** A template's board, replacing this one or added to its right. */
    fun useBoard(t: Board, replace: Boolean, name: String) {
        val now = board ?: return
        if (replace) change(t)
        else {
            val right = (now.nodes.maxOfOrNull { it.x + it.width } ?: 0.0) + 120
            val left = t.nodes.minOfOrNull { it.x } ?: 0.0
            change(now.copy(nodes = now.nodes + t.nodes.map { it.copy(x = it.x - left + right) }, edges = now.edges + t.edges))
        }
        engine.say(ctx.getString(R.string.home_on_home, name))
    }

    Column(Modifier.fillMaxSize()) {
        // The bar: the date's greeting is on the board itself; here, the tools.
        Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp, top = 6.dp, bottom = 2.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(R.string.home), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
            if (edit && past.isNotEmpty()) IconButton(onClick = { change(past.removeAt(past.lastIndex), undoable = false) }) { Icon(Icons.AutoMirrored.Outlined.Undo, stringResource(R.string.home_undo)) }
            if (edit) IconButton(onClick = { selected = null; sheet = Sheet.BOARD }) { Icon(Icons.Outlined.Palette, stringResource(R.string.home_board)) }
            if (!edit) IconButton(onClick = { asList = !asList }) {
                Icon(if (asList) Icons.Outlined.GridView else Icons.Outlined.ViewAgenda, stringResource(if (asList) R.string.home_as_board else R.string.home_as_list))
            }
            IconButton(onClick = { sheet = Sheet.TEMPLATES }) { Icon(Icons.Outlined.AutoAwesomeMosaic, stringResource(R.string.home_templates)) }
            TextButton(onClick = { edit = !edit; selected = null; if (edit) asList = false else if (narrow) asList = true }) {
                Icon(if (edit) Icons.Outlined.Done else Icons.Outlined.Edit, null, Modifier.size(18.dp))
                Spacer(Modifier.width(6.dp))
                Text(stringResource(if (edit) R.string.home_done_editing else R.string.home_edit))
            }
        }
        if (edit) Text(stringResource(R.string.home_tip_edit), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(horizontal = 16.dp, vertical = 2.dp))
        Box(Modifier.weight(1f).fillMaxWidth()) {
            val b = board
            if (b != null && asList && !edit) ListView(b, onUpdate = { id, f -> update(id, f, undoable = false) }, onCapture = onCapture)
            else if (b != null) {
                BoardView(
                    b, edit, selected,
                    onSelect = { selected = it; if (it != null) sheet = Sheet.CARD },
                    onMoved = { before, after -> past.add(before); board = after; save(after) },
                    onLive = { next -> board = next },
                    onUpdate = { id, f -> update(id, f, undoable = false) },
                    onCapture = onCapture,
                )
            }
            if (edit) {
                FloatingActionButton(onClick = { sheet = Sheet.ADD }, modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp)) {
                    Icon(Icons.Outlined.Add, stringResource(R.string.home_add))
                }
            }
        }
    }

    if (sheet != Sheet.NONE) {
        val state = rememberModalBottomSheetState(skipPartiallyExpanded = sheet == Sheet.CARD)
        ModalBottomSheet(onDismissRequest = { sheet = Sheet.NONE }, sheetState = state) {
            Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp).padding(bottom = 28.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                val b = board
                when (sheet) {
                    Sheet.ADD -> AddSheet { kind ->
                        val nb = board ?: return@AddSheet
                        // Below everything, at the left: easy to find, then move.
                        val y = (nb.nodes.maxOfOrNull { it.y + it.height } ?: 0.0) + 30
                        val x = nb.nodes.minOfOrNull { it.x } ?: 0.0
                        val n = newCard(kind, x, y)
                        change(nb.copy(nodes = nb.nodes + n))
                        selected = n.id
                        sheet = Sheet.CARD
                    }
                    Sheet.CARD -> {
                        val n = b?.nodes?.firstOrNull { it.id == selected }
                        if (n == null) sheet = Sheet.NONE
                        else CardSheet(
                            n,
                            onChange = { next -> update(n.id, { next }) },
                            onFront = { change(b.copy(nodes = b.nodes.filter { it.id != n.id } + n)) },
                            onBack = { change(b.copy(nodes = listOf(n) + b.nodes.filter { it.id != n.id })) },
                            onCopy = { val c = n.copy(id = newId(), x = n.x + 30, y = n.y + 30); change(b.copy(nodes = b.nodes + c)); selected = c.id },
                            onRemove = { change(b.copy(nodes = b.nodes.filter { it.id != n.id }, edges = b.edges.filter { it.fromNode != n.id && it.toNode != n.id })); selected = null; sheet = Sheet.NONE },
                        )
                    }
                    Sheet.BOARD -> if (b != null) BoardSheet(b.settings()) { s -> change(b.withSettings(s)) }
                    Sheet.TEMPLATES -> TemplatesSheet(
                        onPick = { asking = it; sheet = Sheet.NONE },
                        onImport = { sheet = Sheet.NONE; importer.launch(arrayOf("application/json", "application/octet-stream", "*/*")) },
                        onExport = { sheet = Sheet.NONE; exporting = true },
                    )
                    Sheet.NONE -> {}
                }
            }
        }
    }

    asking?.let { i ->
        val name = homeTemplates().getOrNull(i)?.name ?: ""
        ChooseDialog(name, onDismiss = { asking = null }) { replace ->
            asking = null
            homeTemplateBoard(i.toUInt())?.let { useBoard(it, replace, name) }
        }
    }
    pendingImport?.let { t ->
        ChooseDialog(importName, onDismiss = { pendingImport = null }) { replace ->
            pendingImport = null
            useBoard(t, replace, importName)
        }
    }
    if (exporting) {
        var name by remember { mutableStateOf("My Home") }
        var words by remember { mutableStateOf(false) }
        AlertDialog(
            onDismissRequest = { exporting = false },
            title = { Text(stringResource(R.string.home_export).trimEnd('…')) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    OutlinedTextField(name, { name = it }, label = { Text(stringResource(R.string.home_template_name)) }, singleLine = true)
                    Row(verticalAlignment = Alignment.CenterVertically) { Switch(words, { words = it }); Spacer(Modifier.width(10.dp)); Text(stringResource(R.string.home_keep_words)) }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    exporting = false
                    val b = board ?: return@TextButton
                    exportText = homeTemplateWrite(b, name.trim().ifEmpty { "My Home" }, words, today())
                    exporter.launch("${name.trim().ifEmpty { "My Home" }}.marrowhome")
                }) { Text(stringResource(R.string.ok)) }
            },
            dismissButton = { TextButton(onClick = { exporting = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

@Composable
private fun ChooseDialog(name: String, onDismiss: () -> Unit, onChoose: (Boolean) -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.home_use_q, name)) },
        text = { Text(stringResource(R.string.home_use_text)) },
        confirmButton = { TextButton(onClick = { onChoose(true) }) { Text(stringResource(R.string.home_replace)) } },
        dismissButton = {
            Row {
                TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
                TextButton(onClick = { onChoose(false) }) { Text(stringResource(R.string.home_add_next)) }
            }
        },
    )
}

// ------------------------------------------------------------------ board

@Composable
private fun BoardView(
    b: Board,
    edit: Boolean,
    selected: String?,
    onSelect: (String?) -> Unit,
    onMoved: (before: Board, after: Board) -> Unit,
    onLive: (Board) -> Unit,
    onUpdate: (String, (CanvasNode) -> CanvasNode) -> Unit,
    onCapture: () -> Unit,
) {
    val settings = b.settings()
    val paper = MaterialTheme.colorScheme.background
    val ink = MaterialTheme.colorScheme.onBackground
    val bgColor = hex(settings.optString("color")) ?: when (settings.optString("bg")) { "paper" -> mix(Color(0xFFF3E9D2), paper, 0.7f); "cork" -> Color(0xFFC9A46B); else -> paper }
    val pattern = settings.optString("bg").ifEmpty { "dots" }
    val image = settings.optString("image")
    val engine = LocalEngine.current
    val density = LocalDensity.current.density
    val lowX = (b.nodes.minOfOrNull { it.x } ?: 0.0).toFloat()
    val lowY = (b.nodes.minOfOrNull { it.y } ?: 0.0).toFloat()
    val maxX = (b.nodes.maxOfOrNull { it.x + it.width } ?: 400.0).toFloat()
    val maxY = (b.nodes.maxOfOrNull { it.y + it.height } ?: 400.0).toFloat()
    var k by remember { mutableFloatStateOf(0f) }
    var tx by remember { mutableFloatStateOf(0f) }
    var ty by remember { mutableFloatStateOf(0f) }
    // The board's top-left only moves outward, and the view moves with it,
    // so cards don't jump when one is dragged past the edge.
    var minX by remember { mutableFloatStateOf(lowX) }
    var minY by remember { mutableFloatStateOf(lowY) }
    if (lowX < minX) { tx -= (minX - lowX) * k * density; minX = lowX }
    if (lowY < minY) { ty -= (minY - lowY) * k * density; minY = lowY }
    val fonts = LocalFontFamilyResolver.current
    val dir = LocalLayoutDirection.current
    val measurer = remember(fonts, dir) { TextMeasurer(fonts, Density(1f), dir) }

    BoxWithConstraints(
        Modifier.fillMaxSize().clipToBounds().background(bgColor)
            .drawBehind {
                val line = ink.copy(alpha = if (pattern == "cork") 0f else 0.1f)
                val step = 22.dp.toPx() * max(k, 0.3f)
                if (pattern in setOf("dots", "grid", "lines", "paper")) {
                    var y = (ty % step + step) % step
                    while (y < size.height) {
                        if (pattern == "lines" || pattern == "grid") drawLine(line, Offset(0f, y), Offset(size.width, y), 1f)
                        if (pattern == "dots" || pattern == "paper") { var x = (tx % step + step) % step; while (x < size.width) { drawCircle(line.copy(alpha = 0.18f), 1.2f * density, Offset(x, y)); x += step } }
                        y += step
                    }
                    if (pattern == "grid") { var x = (tx % step + step) % step; while (x < size.width) { drawLine(line, Offset(x, 0f), Offset(x, size.height), 1f); x += step } }
                }
            }
            .pointerInput(Unit) {
                detectTransformGestures { centroid, pan, zoom, _ ->
                    val nk = (k * zoom).coerceIn(0.25f, 2.5f)
                    tx = centroid.x - (centroid.x - tx) * nk / k + pan.x
                    ty = centroid.y - (centroid.y - ty) * nk / k + pan.y
                    k = nk
                }
            }
            .pointerInput(edit) { detectTapGestures { if (edit) onSelect(null) } },
    ) {
        val wPx = constraints.maxWidth.toFloat()
        // Fit the board's width (not smaller than readable), from the top.
        LaunchedEffect(wPx, b.nodes.isNotEmpty()) {
            if (k == 0f && b.nodes.isNotEmpty()) {
                val kk = (wPx / density / (maxX - minX + 32f)).coerceIn(0.5f, 1f)
                k = kk
                val spare = wPx - (maxX - minX) * kk * density
                // Narrower than the screen: centred; wider: from its left edge.
                tx = if (spare > 0) spare / 2f else 12f * density
                ty = 12f * density
            }
        }
        if (image.isNotEmpty()) {
            val file = remember(image) { runCatching { engine.marrow.filePath(image) }.getOrNull() }
            if (file != null) AsyncImage(model = File(file), contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
        }
        val kk = if (k == 0f) 1f else k
        Box(
            Modifier.wrapContentSize(Alignment.TopStart, unbounded = true)
                .requiredSize((maxX - minX).dp, (maxY - minY).dp)
                .graphicsLayer { transformOrigin = TransformOrigin(0f, 0f); scaleX = kk; scaleY = kk; translationX = tx; translationY = ty },
        ) {
            // Drawings, shapes, text and note cards: drawn as on the drawing pad.
            val plain = b.nodes.filter { it.live().widget.isEmpty() && !(it.kind == "file" && isImage(it.file)) && it.kind != "link" }
            if (plain.isNotEmpty() || b.edges.isNotEmpty()) {
                Canvas(Modifier.fillMaxSize()) {
                    val d = density
                    shifted(d, -minX, -minY) { drawBoard(Board(plain, b.edges, ""), measurer, ink, paper) }
                }
            }
            b.nodes.forEach { n ->
                val live = n.live()
                val isPicture = n.kind == "file" && isImage(n.file)
                val isLink = n.kind == "link"
                Card(n, live, minX, minY, edit, n.id == selected, kk, density, onSelect, onMoved = { after -> onMoved(b, after) }, board = b, onLive = onLive) {
                    when {
                        live.widget.isNotEmpty() -> Widget(n, live, edit, onUpdate, onCapture)
                        isPicture -> {
                            val file = remember(n.file) { runCatching { engine.marrow.filePath(n.file) }.getOrNull() }
                            if (file != null) AsyncImage(model = File(file), contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
                        }
                        isLink -> {
                            val uri = androidx.compose.ui.platform.LocalUriHandler.current
                            Text(n.url, color = MaterialTheme.colorScheme.primary, textDecoration = TextDecoration.Underline, modifier = Modifier.padding(14.dp).clickable(enabled = !edit) { runCatching { uri.openUri(n.url) } })
                        }
                        else -> Box(Modifier.fillMaxSize())
                    }
                }
            }
        }
    }
}

private fun isImage(f: String) = Regex("\\.(png|jpe?g|gif|webp|bmp|svg|avif)$", RegexOption.IGNORE_CASE).containsMatchIn(f)

private inline fun androidx.compose.ui.graphics.drawscope.DrawScope.shifted(d: Float, dx: Float, dy: Float, block: androidx.compose.ui.graphics.drawscope.DrawScope.() -> Unit) {
    drawContext.transform.scale(d, d, Offset.Zero)
    drawContext.transform.translate(dx, dy)
    block()
    drawContext.transform.translate(-dx, -dy)
    drawContext.transform.scale(1f / d, 1f / d, Offset.Zero)
}

/** One card on the board: its frame and look; in edit mode, dragged to move and by its corner to resize. */
@Composable
private fun Card(
    n: CanvasNode,
    live: Live,
    minX: Float,
    minY: Float,
    edit: Boolean,
    picked: Boolean,
    k: Float,
    density: Float,
    onSelect: (String?) -> Unit,
    onMoved: (Board) -> Unit,
    board: Board,
    onLive: (Board) -> Unit,
    content: @Composable () -> Unit,
) {
    val l = lookOf(live.style)
    val accent = MaterialTheme.colorScheme.primary
    val framed = live.widget.isNotEmpty() || n.kind == "link" || (n.kind == "file" && isImage(n.file))
    var drag by remember { mutableStateOf<Offset?>(null) }
    var grow by remember { mutableStateOf<Offset?>(null) }
    val dx = drag?.x ?: 0f
    val dy = drag?.y ?: 0f
    val gw = grow?.x ?: 0f
    val gh = grow?.y ?: 0f
    val w = max(60f, n.width.toFloat() + gw)
    val h = max(40f, n.height.toFloat() + gh)
    Box(
        Modifier.offset((n.x.toFloat() - minX + dx).dp, (n.y.toFloat() - minY + dy).dp).size(w.dp, h.dp)
            .semantics { contentDescription = live.widget.ifEmpty { n.kind } },
    ) {
        CardBody(l, framed, content)
        if (edit) {
            // Over the card: picks it, and drags it (nothing inside reacts while editing).
            Box(
                Modifier.fillMaxSize()
                    .border(if (picked) BorderStroke(2.dp, accent) else BorderStroke(1.dp, accent.copy(alpha = 0.25f)), RoundedCornerShape(max(l.radius.value, 4f).dp))
                    .pointerInput(n.id) { detectTapGestures { onSelect(n.id) } }
                    .pointerInput(n.id, board) {
                        detectDragGestures(
                            onDragStart = { drag = Offset.Zero },
                            onDragEnd = {
                                val d = drag ?: Offset.Zero
                                drag = null
                                if (d != Offset.Zero) onMoved(board.copy(nodes = board.nodes.map { if (it.id == n.id) it.copy(x = (n.x + d.x).roundToInt().toDouble(), y = (n.y + d.y).roundToInt().toDouble()) else it }))
                            },
                            onDragCancel = { drag = null },
                        ) { change, amount ->
                            change.consume()
                            drag = (drag ?: Offset.Zero) + amount / density
                        }
                    },
            )
            Box(
                Modifier.align(Alignment.BottomEnd).offset(8.dp, 8.dp).size(26.dp).clip(CircleShape).background(MaterialTheme.colorScheme.background).border(2.dp, accent, CircleShape)
                    .pointerInput(n.id, board) {
                        detectDragGestures(
                            onDragStart = { grow = Offset.Zero },
                            onDragEnd = {
                                val g = grow ?: Offset.Zero
                                grow = null
                                if (g != Offset.Zero) onMoved(board.copy(nodes = board.nodes.map { if (it.id == n.id) it.copy(width = max(60.0, (n.width + g.x).roundToInt().toDouble()), height = max(40.0, (n.height + g.y).roundToInt().toDouble())) else it }))
                            },
                            onDragCancel = { grow = null },
                        ) { change, amount ->
                            change.consume()
                            grow = (grow ?: Offset.Zero) + amount / density
                        }
                    },
            )
        }
    }
}

/** A card's frame, its look's extras (tape, pin) and its text style. */
@Composable
private fun androidx.compose.foundation.layout.BoxScope.CardBody(l: Look, framed: Boolean, content: @Composable () -> Unit) {
    val accent = MaterialTheme.colorScheme.primary
    Box(Modifier.fillMaxSize().then(if (framed) Modifier.framed(l, accent) else Modifier)) {
        androidx.compose.runtime.CompositionLocalProvider(
            androidx.compose.material3.LocalContentColor provides l.fg,
            androidx.compose.material3.LocalTextStyle provides MaterialTheme.typography.bodyMedium.copy(color = l.fg, fontFamily = l.font, fontSize = MaterialTheme.typography.bodyMedium.fontSize * l.scale, textAlign = l.align),
        ) { content() }
    }
    when (l.look) {
        "tape" -> Box(Modifier.align(Alignment.TopCenter).offset(y = (-9).dp).size(84.dp, 20.dp).graphicsLayer { rotationZ = -3f }.background(Color(0xCCF5F0E1)))
        "pin" -> Box(Modifier.align(Alignment.TopCenter).offset(y = (-7).dp).size(16.dp).clip(CircleShape).background(Color(0xFFC62828)))
    }
}

/**
 * Phones: the same cards one under another, full width, in reading order
 * (each column of the board top to bottom, then the next column).
 */
@Composable
private fun ListView(b: Board, onUpdate: (String, (CanvasNode) -> CanvasNode) -> Unit, onCapture: () -> Unit) {
    val engine = LocalEngine.current
    val shown = b.nodes.filter { n ->
        val w = n.live().widget
        w.isNotEmpty() || n.kind == "link" || (n.kind == "file" && isImage(n.file)) || (n.kind == "text" && n.shape.isEmpty() && n.ink == null && n.text.isNotBlank())
    }
    // Columns: cards whose left edges are close together.
    val lefts = shown.map { it.x }.sorted()
    val starts = mutableListOf<Double>()
    lefts.forEach { x -> if (starts.isEmpty() || x - starts.last() > 150) starts.add(x) }
    val ordered = shown.sortedWith(compareBy({ n -> starts.indexOfLast { it <= n.x + 1 } }, { it.y }))
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        ordered.forEach { n ->
            val live = n.live()
            val l = lookOf(live.style)
            Box(Modifier.fillMaxWidth().height(n.height.coerceIn(56.0, 520.0).dp).semantics { contentDescription = live.widget.ifEmpty { n.kind } }) {
                CardBody(l, true) {
                    when {
                        live.widget.isNotEmpty() -> Widget(n, live, false, onUpdate, onCapture)
                        n.kind == "file" -> {
                            val file = remember(n.file) { runCatching { engine.marrow.filePath(n.file) }.getOrNull() }
                            if (file != null) AsyncImage(model = File(file), contentDescription = null, contentScale = ContentScale.Crop, modifier = Modifier.fillMaxSize())
                        }
                        n.kind == "link" -> {
                            val uri = androidx.compose.ui.platform.LocalUriHandler.current
                            Text(n.url, color = MaterialTheme.colorScheme.primary, textDecoration = TextDecoration.Underline, modifier = Modifier.padding(14.dp).clickable { runCatching { uri.openUri(n.url) } })
                        }
                        else -> Text(n.text, modifier = Modifier.padding(14.dp))
                    }
                }
            }
        }
    }
}

// ------------------------------------------------------------------ cards

@Composable
private fun Head(title: String, trailing: (@Composable () -> Unit)? = null) {
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().padding(bottom = 6.dp)) {
        Text(title, fontWeight = FontWeight.Bold, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        trailing?.invoke()
    }
}

@Composable
private fun Empty(text: String) = Text(text, color = androidx.compose.material3.LocalContentColor.current.copy(alpha = 0.6f), style = androidx.compose.material3.LocalTextStyle.current)

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Widget(n: CanvasNode, live: Live, edit: Boolean, onUpdate: (String, (CanvasNode) -> CanvasNode) -> Unit, onCapture: () -> Unit) {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    val scope = rememberCoroutineScope()
    val cfg = live.cfg
    val style = androidx.compose.material3.LocalTextStyle.current
    val pad = Modifier.fillMaxSize().padding(horizontal = 14.dp, vertical = 12.dp)
    when (live.widget) {
        "sticky" -> {
            var text by remember(n.id) { mutableStateOf(n.text) }
            BasicTextField(
                text, { text = it; onUpdate(n.id) { c -> c.copy(text = it) } },
                enabled = !edit, textStyle = style.copy(lineHeight = style.fontSize * 1.5f),
                modifier = pad,
                decorationBox = { inner -> Box { if (text.isEmpty()) Empty(stringResource(R.string.home_write_something)); inner() } },
            )
        }
        "tasks" -> {
            val f = TaskFilter(cfg.optString("range", "all"), cfg.optInt("days", 7).toUInt(), cfg.optString("from"), cfg.optString("to"), cfg.optBoolean("overdue"), cfg.optBoolean("undated"), cfg.optBoolean("done"), cfg.optString("tag"), cfg.optString("folder"), cfg.optString("note"), cfg.optString("text"))
            val day = today()
            val tasks = load(cfg.toString(), day) { run { taskView(day, f) } }.value
            Column(pad) {
                Head(cfg.optString("title").ifEmpty { stringResource(R.string.w_tasks) }) { tasks?.count { !it.done }?.takeIf { it > 0 }?.let { Text("$it", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold) } }
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    if (tasks != null && tasks.isEmpty()) Empty(stringResource(R.string.home_nothing_to_do))
                    var group = ""
                    tasks?.forEach { t ->
                        val g = when { t.due == null -> stringResource(R.string.home_no_date); t.due!! < day && !t.done -> stringResource(R.string.home_overdue); else -> dayLabel(t.due!!, day) }
                        if (cfg.optBoolean("group", true) && g != group) {
                            group = g
                            Text(g.uppercase(), fontSize = style.fontSize * 0.75f, fontWeight = FontWeight.Bold, color = if (g == stringResource(R.string.home_overdue)) MaterialTheme.colorScheme.error else style.color.copy(alpha = 0.6f), modifier = Modifier.padding(top = 8.dp, bottom = 2.dp))
                        }
                        Row(verticalAlignment = Alignment.Top, modifier = Modifier.padding(vertical = 3.dp)) {
                            Icon(
                                if (t.done) Icons.Outlined.CheckBox else Icons.Outlined.CheckBoxOutlineBlank, null,
                                Modifier.size(22.dp).clickable(enabled = !edit) { scope.launch { runCatching { engine.run { toggleTask(t.path, t.line) } }.onFailure { engine.say(it.words()) } } },
                            )
                            Spacer(Modifier.width(8.dp))
                            Column(Modifier.clickable(enabled = !edit) { nav.note(t.path, t.line.toInt()) }) {
                                Text(t.text, textDecoration = if (t.done) TextDecoration.LineThrough else null, maxLines = 3, overflow = TextOverflow.Ellipsis)
                                if (cfg.optBoolean("showNote", true)) Text(t.title, fontSize = style.fontSize * 0.78f, color = style.color.copy(alpha = 0.55f))
                            }
                        }
                    }
                }
            }
        }
        "note", "daily" -> {
            val day = today()
            val path = if (live.widget == "daily") load(day) { run { dailyNote(day) } }.value else cfg.optString("path").ifEmpty { null }
            val section = if (live.widget == "note") cfg.optString("section") else ""
            val text = load(path, section) { if (path == null) null else run { readNote(path).let { if (section.isNotEmpty()) noteSection(it, section) else it } } }
            Column(pad) {
                Head(if (live.widget == "daily") stringResource(R.string.w_daily) else path?.substringAfterLast('/')?.removeSuffix(".md") ?: stringResource(R.string.w_note)) {
                    if (path != null) Icon(Icons.Outlined.ChevronRight, null, Modifier.clickable(enabled = !edit) { nav.note(path) })
                }
                when {
                    live.widget == "daily" && path == null -> {
                        Empty(stringResource(R.string.home_no_today))
                        OutlinedButton(onClick = { scope.launch { runCatching { engine.run { openDay(day) } }.onSuccess { nav.note(it, edit = true) } } }, enabled = !edit) { Text(stringResource(R.string.home_start_today)) }
                    }
                    path == null -> Empty(stringResource(R.string.home_pick_note))
                    text.error != null -> Empty(stringResource(R.string.home_note_gone))
                    text.value == null && !text.loading -> Empty(stringResource(R.string.home_no_heading))
                    text.value != null -> Column(Modifier.verticalScroll(rememberScrollState())) {
                        Blocks(remember(text.value) { renderMarkdown(text.value!!.replace(Regex("^---\\n[\\s\\S]*?\\n---\\n"), "")) }, BlockActions(onOpen = { nav.openPath(it) }))
                    }
                }
            }
        }
        "notes" -> {
            val kind = cfg.optString("kind", "recent")
            val value = cfg.optString("value").trim()
            val limit = cfg.optInt("limit", 8).coerceAtLeast(1)
            val items = load(kind, value, limit) {
                run {
                    when (kind) {
                        "starred" -> starred().map { it.path }
                        "folder" -> listFolder(value.trim('/')).filter { it.path.endsWith(".md") || it.path.endsWith(".canvas") }.map { it.path }
                        "tag" -> if (value.isEmpty()) emptyList() else search("tag:#${value.removePrefix("#")}", limit.toUInt()).hits.map { it.path }
                        "search" -> if (value.isEmpty()) emptyList() else search(value, limit.toUInt()).hits.map { it.path }
                        else -> recentNotes((limit + 1).toUInt()).map { it.path }.filter { it != HOME }
                    }.take(limit)
                }
            }.value
            Column(pad) {
                Head(cfg.optString("title").ifEmpty { stringResource(R.string.w_notes) })
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    if (items != null && items.isEmpty()) Empty(stringResource(R.string.home_no_notes))
                    items?.forEach { p ->
                        Text(p.substringAfterLast('/').removeSuffix(".md").removeSuffix(".canvas"), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.fillMaxWidth().clickable(enabled = !edit) { nav.openPath(p) }.padding(vertical = 6.dp))
                    }
                }
            }
        }
        "greeting" -> {
            val hr = LocalTime.now().hour
            val hello = stringResource(if (hr in 5..11) R.string.home_good_morning else if (hr in 12..17) R.string.home_good_afternoon else R.string.home_good_evening)
            val name = cfg.optString("name").trim()
            Column(pad, verticalArrangement = Arrangement.Center) {
                Text(if (name.isEmpty()) hello else "$hello, $name", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.SemiBold)
                if (cfg.optBoolean("date", true)) Text(longDate(today()), fontFamily = style.fontFamily ?: Literata, fontWeight = FontWeight.SemiBold, fontSize = style.fontSize * 2.3f, lineHeight = style.fontSize * 2.5f)
            }
        }
        "capture" -> {
            var text by remember { mutableStateOf("") }
            val toNote = cfg.optString("target") == "note" && cfg.optString("path").isNotEmpty()
            val task = cfg.optBoolean("task")
            fun send() {
                val words = text.trim()
                if (words.isEmpty()) return
                text = ""
                scope.launch {
                    runCatching {
                        engine.run {
                            if (toNote) {
                                val p = cfg.optString("path")
                                writeNote(p, readNote(p).trimEnd() + "\n" + (if (task) "- [ ] $words" else "- $words") + "\n", null)
                            } else capture(today(), nowTime(), words, task, emptyList())
                        }
                    }.onFailure { text = words; engine.say(it.words()) }
                }
            }
            Row(pad, verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    text, { text = it }, enabled = !edit, singleLine = true, modifier = Modifier.weight(1f),
                    placeholder = { Text(stringResource(if (task) R.string.home_add_task else R.string.home_whats_on_mind)) },
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send), keyboardActions = KeyboardActions(onSend = { send() }),
                )
                IconButton(onClick = onCapture, enabled = !edit) { Icon(Icons.Outlined.Add, stringResource(R.string.home_quick_capture)) }
            }
        }
        "shortcuts" -> {
            val items = cfg.optJSONArray("items")?.let { a -> (0 until a.length()).map { a.optString(it) } } ?: listOf("new", "today", "search", "ask")
            Row(pad.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                items.forEach { k ->
                    val label = when (k) { "new" -> R.string.home_new_note; "today" -> R.string.home_today_note; "search" -> R.string.home_search; "ask" -> R.string.home_ask; "capture" -> R.string.home_quick_capture; else -> null }
                    if (label != null) OutlinedButton(enabled = !edit, onClick = {
                        when (k) {
                            "new" -> scope.launch { runCatching { engine.run { createNote("", null, "") } }.onSuccess { nav.note(it, edit = true) } }
                            "today" -> scope.launch { runCatching { engine.run { openDay(today()) } }.onSuccess { nav.note(it, edit = true) } }
                            "search" -> nav.go(SearchRoute())
                            "ask" -> nav.go(AskRoute)
                            "capture" -> onCapture()
                        }
                    }) { Text(stringResource(label)) }
                }
            }
        }
        "calendar" -> {
            var month by remember { mutableStateOf(LocalDate.now().withDayOfMonth(1)) }
            val ym = month.toString().substring(0, 7)
            val withNotes = load(ym) { run { daysWithNotes(ym) } }.value.orEmpty().map { it.toInt() }.toSet()
            val due = load(ym) { run { tasks(false) } }.value.orEmpty().mapNotNull { it.due }.filter { it.startsWith(ym) }.map { it.substring(8).toInt() }.toSet()
            Column(pad) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Outlined.ChevronLeft, null, Modifier.clickable(enabled = !edit) { month = month.minusMonths(1) })
                    Text(month.format(DateTimeFormatter.ofPattern("MMMM yyyy")), fontWeight = FontWeight.Bold, textAlign = TextAlign.Center, modifier = Modifier.weight(1f))
                    Icon(Icons.Outlined.ChevronRight, null, Modifier.clickable(enabled = !edit) { month = month.plusMonths(1) })
                }
                val lead = month.dayOfWeek.value - 1
                val days = month.lengthOfMonth()
                val todayD = LocalDate.now()
                Row { (1..7).forEach { d -> Text(java.time.DayOfWeek.of(d).getDisplayName(DayStyle.NARROW, Locale.getDefault()), Modifier.weight(1f), textAlign = TextAlign.Center, fontSize = style.fontSize * 0.75f, color = style.color.copy(alpha = 0.55f)) } }
                (0 until (lead + days + 6) / 7).forEach { w ->
                    Row {
                        (0 until 7).forEach { c ->
                            val d = w * 7 + c - lead + 1
                            Box(Modifier.weight(1f).aspectRatio(1f).padding(1.dp), contentAlignment = Alignment.Center) {
                                if (d in 1..days) {
                                    val isToday = month.year == todayD.year && month.month == todayD.month && d == todayD.dayOfMonth
                                    Box(
                                        Modifier.fillMaxSize().clip(RoundedCornerShape(8.dp)).background(if (isToday) MaterialTheme.colorScheme.primary else Color.Transparent)
                                            .clickable(enabled = !edit) { scope.launch { runCatching { engine.run { openDay(month.withDayOfMonth(d).toString()) } }.onSuccess { nav.note(it, edit = true) } } },
                                        contentAlignment = Alignment.Center,
                                    ) {
                                        Text("$d", fontSize = style.fontSize * 0.9f, fontWeight = if (d in withNotes) FontWeight.Bold else null, color = if (isToday) MaterialTheme.colorScheme.onPrimary else style.color)
                                        if (d in due) Box(Modifier.align(Alignment.BottomCenter).padding(bottom = 3.dp).size(4.dp).clip(CircleShape).background(Color(0xFFE8780C)))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        "countdown" -> {
            val date = cfg.optString("date")
            val days = runCatching { ChronoUnit.DAYS.between(LocalDate.now(), LocalDate.parse(date)) }.getOrNull()
            Column(pad, horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                if (cfg.optString("title").isNotEmpty()) Text(cfg.optString("title"), fontWeight = FontWeight.SemiBold)
                Text(if (days == null) "—" else if (days == 0L) stringResource(R.string.home_today_bang) else "${kotlin.math.abs(days)}", fontFamily = Literata, fontWeight = FontWeight.Bold, fontSize = style.fontSize * 3f)
                Text(if (days == null) stringResource(R.string.home_pick_date) else if (days > 0) stringResource(R.string.home_days_to_go) else if (days < 0) stringResource(R.string.home_days_ago) else "", fontSize = style.fontSize * 0.85f, color = style.color.copy(alpha = 0.62f))
            }
        }
        "clock" -> {
            var now by remember { mutableStateOf(LocalTime.now()) }
            LaunchedEffect(Unit) { while (true) { delay(15_000); now = LocalTime.now() } }
            Column(pad, horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                Text(now.format(DateTimeFormatter.ofPattern(if (cfg.optBoolean("h24")) "HH:mm" else "h:mm a")), fontFamily = Literata, fontWeight = FontWeight.SemiBold, fontSize = style.fontSize * 2.6f)
                Text(longDate(today()), fontSize = style.fontSize * 0.85f, color = style.color.copy(alpha = 0.62f))
            }
        }
        "rediscover" -> {
            val r = load(today()) { run { resurface(today(), System.currentTimeMillis(), 0u) } }.value
            Column(pad) {
                Head(stringResource(R.string.w_rediscover))
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    val f = r?.forgotten
                    if (f == null && r?.onThisDay.isNullOrEmpty()) Empty(stringResource(R.string.home_rediscover_empty))
                    if (f != null) Column(Modifier.clickable(enabled = !edit) { nav.note(f.path) }.padding(vertical = 4.dp)) {
                        Text(stringResource(R.string.home_from_archive), fontSize = style.fontSize * 0.78f, color = MaterialTheme.colorScheme.primary)
                        Text(f.title, fontWeight = FontWeight.SemiBold)
                        if (f.preview.isNotEmpty()) Text(f.preview, maxLines = 3, overflow = TextOverflow.Ellipsis, fontSize = style.fontSize * 0.9f)
                    }
                    r?.onThisDay?.forEach { o -> Text(o.preview.ifEmpty { o.day }, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.clickable(enabled = !edit) { nav.note(o.path) }.padding(vertical = 6.dp)) }
                }
            }
        }
        "welcome" -> Column(pad.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Head(stringResource(R.string.w_welcome))
            listOf(R.string.home_step_write, R.string.home_step_link, R.string.home_step_home).forEachIndexed { i, s -> Text("${i + 1}. ${stringResource(s)}") }
        }
        else -> Box(pad) { Empty(stringResource(R.string.home_card_newer)) }
    }
}

@Composable
private fun dayLabel(day: String, today: String): String {
    val d = runCatching { ChronoUnit.DAYS.between(LocalDate.parse(today), LocalDate.parse(day)) }.getOrDefault(99)
    return when {
        d == 0L -> stringResource(R.string.range_today)
        d == 1L -> stringResource(R.string.tomorrow)
        d in 2..6 -> LocalDate.parse(day).dayOfWeek.getDisplayName(DayStyle.FULL, Locale.getDefault())
        else -> runCatching { LocalDate.parse(day).format(DateTimeFormatter.ofPattern("d MMM")) }.getOrDefault(day)
    }
}

// ------------------------------------------------------------------ sheets

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AddSheet(onAdd: (String) -> Unit) {
    Text(stringResource(R.string.home_add), style = MaterialTheme.typography.titleLarge)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        WIDGETS.forEach { w -> OutlinedButton(onClick = { onAdd(w) }) { Text(widgetName(w)) } }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Choice(label: String, options: List<Pair<String, String>>, value: String, onPick: (String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(label, style = MaterialTheme.typography.labelLarge)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            options.forEach { (v, l) -> FilterChip(selected = v == value, onClick = { onPick(v) }, label = { Text(l) }) }
        }
    }
}

@Composable
private fun Field(label: String, value: String, onDone: (String) -> Unit) {
    var t by remember(value) { mutableStateOf(value) }
    OutlinedTextField(
        t, { t = it; onDone(it.trim()) }, label = { Text(label) }, singleLine = true, modifier = Modifier.fillMaxWidth(),
    )
}

@Composable
private fun Toggle(label: String, on: Boolean, onChange: (Boolean) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().clickable { onChange(!on) }) {
        Text(label, modifier = Modifier.weight(1f))
        Switch(on, onChange)
    }
}

@Composable
private fun SliderRow(label: String, value: Int, range: IntRange, onChange: (Int) -> Unit) {
    var v by remember(value) { mutableFloatStateOf(value.toFloat()) }
    Column {
        Text(label, style = MaterialTheme.typography.labelLarge)
        Slider(v, { v = it }, valueRange = range.first.toFloat()..range.last.toFloat(), onValueChangeFinished = { onChange(v.roundToInt()) })
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Colours(label: String, value: String, onPick: (String) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(label, style = MaterialTheme.typography.labelLarge)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            val ring = MaterialTheme.colorScheme.onSurface
            Box(Modifier.size(32.dp).clip(CircleShape).border(if (value.isEmpty()) BorderStroke(2.dp, ring) else BorderStroke(1.dp, ring.copy(alpha = 0.2f)), CircleShape).clickable { onPick("") }.semantics { contentDescription = "none" }) {
                Canvas(Modifier.fillMaxSize()) { drawLine(Color(0xFFC33333), Offset(size.width * 0.8f, size.height * 0.2f), Offset(size.width * 0.2f, size.height * 0.8f), 2.dp.toPx()) }
            }
            PALETTE.forEach { c ->
                Box(Modifier.size(32.dp).clip(CircleShape).background(hex(c)!!).border(if (value.equals(c, true)) BorderStroke(3.dp, ring) else BorderStroke(0.dp, Color.Transparent), CircleShape).clickable { onPick(c) })
            }
        }
    }
}

@Composable
private fun CardSheet(n: CanvasNode, onChange: (CanvasNode) -> Unit, onFront: () -> Unit, onBack: () -> Unit, onCopy: () -> Unit, onRemove: () -> Unit) {
    val live = n.live()
    val cfg = live.cfg
    val st = live.style
    fun conf(k: String, v: Any?) = onChange(n.with(cfg = cfg.copyWith(k, v)))
    fun look(k: String, v: Any?) = onChange(n.with(style = st.copyWith(k, v)))
    Text(if (live.widget.isNotEmpty()) widgetName(live.widget) else widgetName(n.kind), style = MaterialTheme.typography.titleLarge)
    when (live.widget) {
        "tasks" -> {
            val r = cfg.optString("range", "all")
            Field(stringResource(R.string.home_title), cfg.optString("title")) { conf("title", it) }
            Choice(stringResource(R.string.home_which_dates), listOf("days" to stringResource(R.string.range_days), "today" to stringResource(R.string.range_today), "range" to stringResource(R.string.range_range), "all" to stringResource(R.string.range_all), "none" to stringResource(R.string.range_none)), r) { conf("range", it) }
            if (r == "days") Field(stringResource(R.string.home_how_many_days), cfg.optInt("days", 7).toString()) { it.toIntOrNull()?.let { d -> conf("days", d.coerceIn(1, 366)) } }
            if (r == "range") {
                Field(stringResource(R.string.home_from), cfg.optString("from")) { conf("from", it) }
                Field(stringResource(R.string.home_to), cfg.optString("to")) { conf("to", it) }
            }
            Toggle(stringResource(R.string.home_overdue_too), cfg.optBoolean("overdue")) { conf("overdue", it) }
            Toggle(stringResource(R.string.home_undated), cfg.optBoolean("undated")) { conf("undated", it) }
            Toggle(stringResource(R.string.home_done_tasks), cfg.optBoolean("done")) { conf("done", it) }
            Field(stringResource(R.string.home_only_tag), cfg.optString("tag")) { conf("tag", it) }
            Field(stringResource(R.string.home_only_folder), cfg.optString("folder")) { conf("folder", it) }
            Field(stringResource(R.string.home_only_note), cfg.optString("note")) { conf("note", it) }
            Field(stringResource(R.string.home_only_words), cfg.optString("text")) { conf("text", it) }
            Toggle(stringResource(R.string.home_group_day), cfg.optBoolean("group", true)) { conf("group", it) }
            Toggle(stringResource(R.string.home_show_note), cfg.optBoolean("showNote", true)) { conf("showNote", it) }
        }
        "note" -> {
            Field(stringResource(R.string.home_note_path), cfg.optString("path")) { conf("path", if (it.isEmpty() || it.endsWith(".md")) it else "$it.md") }
            Field(stringResource(R.string.home_section), cfg.optString("section")) { conf("section", it) }
        }
        "notes" -> {
            val kind = cfg.optString("kind", "recent")
            Field(stringResource(R.string.home_title), cfg.optString("title")) { conf("title", it) }
            Choice(stringResource(R.string.home_which_notes), listOf("recent" to stringResource(R.string.notes_recent), "starred" to stringResource(R.string.notes_starred), "tag" to stringResource(R.string.notes_tag), "folder" to stringResource(R.string.notes_folder), "search" to stringResource(R.string.notes_search)), kind) { conf("kind", it) }
            when (kind) {
                "tag" -> Field(stringResource(R.string.home_value_tag), cfg.optString("value")) { conf("value", it) }
                "folder" -> Field(stringResource(R.string.home_value_folder), cfg.optString("value")) { conf("value", it) }
                "search" -> Field(stringResource(R.string.home_value_search), cfg.optString("value")) { conf("value", it) }
            }
            Field(stringResource(R.string.home_how_many), cfg.optInt("limit", 8).toString()) { it.toIntOrNull()?.let { d -> conf("limit", d.coerceIn(1, 50)) } }
        }
        "greeting" -> {
            Field(stringResource(R.string.home_your_name), cfg.optString("name")) { conf("name", it) }
            Toggle(stringResource(R.string.home_show_date), cfg.optBoolean("date", true)) { conf("date", it) }
        }
        "capture" -> {
            Choice(stringResource(R.string.home_where_goes), listOf("daily" to stringResource(R.string.capture_daily), "note" to stringResource(R.string.capture_note)), cfg.optString("target", "daily")) { conf("target", it) }
            if (cfg.optString("target") == "note") Field(stringResource(R.string.home_note_path), cfg.optString("path")) { conf("path", if (it.isEmpty() || it.endsWith(".md")) it else "$it.md") }
            Toggle(stringResource(R.string.home_as_task), cfg.optBoolean("task")) { conf("task", it) }
        }
        "shortcuts" -> {
            val items = cfg.optJSONArray("items")?.let { a -> (0 until a.length()).map { a.optString(it) } } ?: emptyList()
            listOf("new" to R.string.home_new_note, "today" to R.string.home_today_note, "search" to R.string.home_search, "ask" to R.string.home_ask, "capture" to R.string.home_quick_capture).forEach { (k, l) ->
                Toggle(stringResource(l), k in items) { on -> conf("items", JSONArray(if (on) items + k else items - k)) }
            }
        }
        "countdown" -> {
            Field(stringResource(R.string.home_what_for), cfg.optString("title")) { conf("title", it) }
            Field(stringResource(R.string.home_date), cfg.optString("date")) { conf("date", it) }
        }
        "clock" -> Toggle(stringResource(R.string.home_24h), cfg.optBoolean("h24")) { conf("h24", it) }
    }
    HorizontalDivider()
    Text(stringResource(R.string.home_look), style = MaterialTheme.typography.titleMedium)
    Choice(stringResource(R.string.home_style), listOf("" to R.string.look_card, "paper" to R.string.look_paper, "lined" to R.string.look_lined, "grid" to R.string.look_grid, "tape" to R.string.look_tape, "pin" to R.string.look_pin, "fold" to R.string.look_fold, "outline" to R.string.look_outline, "glass" to R.string.look_glass, "none" to R.string.look_none).map { it.first to stringResource(it.second) }, st.optString("look").let { if (it == "card") "" else it }) { look("look", it) }
    Colours(stringResource(R.string.home_colour), st.optString("bg")) { look("bg", it) }
    SliderRow(stringResource(R.string.home_strength), st.optInt("tint", 18), 5..100) { look("tint", it) }
    Colours(stringResource(R.string.home_text_colour), st.optString("fg")) { look("fg", it) }
    Choice(stringResource(R.string.home_font), listOf("" to R.string.font_app, "read" to R.string.font_book, "classic" to R.string.font_classic, "mono" to R.string.font_mono, "hand" to R.string.font_hand).map { it.first to stringResource(it.second) }, st.optString("font")) { look("font", it) }
    Choice(stringResource(R.string.home_text_size), listOf("s" to R.string.size_s, "" to R.string.size_m, "l" to R.string.size_l, "xl" to R.string.size_xl).map { it.first to stringResource(it.second) }, st.optString("size")) { look("size", it) }
    Choice(stringResource(R.string.home_alignment), listOf("" to R.string.align_left, "center" to R.string.align_center, "right" to R.string.align_right).map { it.first to stringResource(it.second) }, st.optString("align")) { look("align", it) }
    SliderRow(stringResource(R.string.home_corners), st.optInt("radius", 12), 0..40) { look("radius", it) }
    SliderRow(stringResource(R.string.home_see_through), st.optInt("opacity", 100), 20..100) { look("opacity", if (it == 100) null else it) }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedButton(onClick = onFront) { Text(stringResource(R.string.home_front)) }
        OutlinedButton(onClick = onBack) { Text(stringResource(R.string.home_back)) }
        OutlinedButton(onClick = onCopy) { Text(stringResource(R.string.home_copy)) }
    }
    TextButton(onClick = onRemove) { Text(stringResource(R.string.home_remove), color = MaterialTheme.colorScheme.error) }
}

@Composable
private fun BoardSheet(s: JSONObject, onChange: (JSONObject) -> Unit) {
    val engine = LocalEngine.current
    val scope = rememberCoroutineScope()
    val ctx = LocalContext.current
    val picker = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri: Uri? ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            runCatching {
                val bytes = ctx.contentResolver.openInputStream(uri)!!.use { it.readBytes() }
                engine.run { saveAttachment("Attachments", "Home background.jpg", bytes) }
            }.onSuccess { onChange(s.copyWith("image", it)) }.onFailure { engine.say(it.words()) }
        }
    }
    Text(stringResource(R.string.home_board), style = MaterialTheme.typography.titleLarge)
    Choice(stringResource(R.string.home_background), listOf("" to R.string.bg_dots, "grid" to R.string.bg_grid, "lines" to R.string.bg_lines, "paper" to R.string.bg_paper, "cork" to R.string.bg_cork, "plain" to R.string.bg_plain).map { it.first to stringResource(it.second) }, s.optString("bg").let { if (it == "dots") "" else it }) { onChange(s.copyWith("bg", it)) }
    Colours(stringResource(R.string.home_bg_colour), s.optString("color")) { onChange(s.copyWith("color", it)) }
    Text(stringResource(R.string.home_bg_picture), style = MaterialTheme.typography.labelLarge)
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedButton(onClick = { picker.launch("image/*") }) { Text(stringResource(R.string.home_choose)) }
        if (s.optString("image").isNotEmpty()) TextButton(onClick = { onChange(s.copyWith("image", null)) }) { Text(stringResource(R.string.home_remove)) }
    }
}

@Composable
private fun TemplatesSheet(onPick: (Int) -> Unit, onImport: () -> Unit, onExport: () -> Unit) {
    Text(stringResource(R.string.home_start_from), style = MaterialTheme.typography.titleLarge)
    homeTemplates().forEachIndexed { i, t ->
        Surface(shape = RoundedCornerShape(14.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth().clickable { onPick(i) }) {
            Column(Modifier.padding(14.dp)) {
                Text(t.name, fontWeight = FontWeight.SemiBold)
                Text(t.about, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
    HorizontalDivider()
    TextButton(onClick = onImport) { Text(stringResource(R.string.home_import)) }
    TextButton(onClick = onExport) { Text(stringResource(R.string.home_export)) }
}
