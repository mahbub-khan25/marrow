package app.marrow.ui.note

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Article
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Lightbulb
import androidx.compose.material.icons.outlined.ReportProblem
import androidx.compose.material.icons.outlined.HelpOutline
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.FormatQuote
import androidx.compose.material.icons.outlined.ErrorOutline
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import app.marrow.core.Block
import app.marrow.core.BlockKind
import app.marrow.core.SpanLink
import app.marrow.core.LinkKind
import app.marrow.core.Span
import app.marrow.ui.theme.LocalNoteColors
import app.marrow.ui.theme.LocalReading
import app.marrow.ui.theme.NoteColors
import app.marrow.ui.theme.calloutColor
import app.marrow.ui.theme.isDark
import coil3.compose.AsyncImage
import java.io.File

/** What tapping things in a rendered note does. */
class BlockActions(
    /** A link to a note, a missing note, a tag or a web page. */
    val onLink: (SpanLink) -> Unit = {},
    /** The task on this line was ticked or unticked. */
    val onToggle: (Int) -> Unit = {},
    /** Double-tap: edit the note at this line. */
    val onEdit: (Int) -> Unit = {},
    /** Opens an image or embedded file (vault path, or web address). */
    val onOpen: (String) -> Unit = {},
    /** Full path of a vault file, to show an image. */
    val fileOf: (String) -> String? = { null },
)

/** A note, drawn from the blocks the engine made of its Markdown. */
@Composable
fun Blocks(blocks: List<Block>, actions: BlockActions, modifier: Modifier = Modifier) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(0.dp)) {
        var i = 0
        var prev: Block? = null
        while (i < blocks.size) {
            val b = blocks[i]
            if (b.quote > 0u) {
                // A quote or callout: everything inside it, drawn in one box.
                val start = i
                while (i < blocks.size && blocks[i].quote > 0u) i++
                val inside = blocks.subList(start, i).map { it.copy(quote = (it.quote - 1u).toUByte()) }
                Gap(prev, b)
                Quote(inside, b.callout, actions)
                prev = b
                continue
            }
            Gap(prev, b)
            BlockView(b, actions)
            prev = b
            i++
        }
    }
}

/** Space above a block, by what it follows. */
@Composable
private fun Gap(prev: Block?, b: Block) {
    if (prev == null) return
    val list = setOf(BlockKind.BULLET, BlockKind.NUMBERED, BlockKind.TASK)
    val h = when {
        b.kind == BlockKind.HEADING -> if (b.level <= 2u) 26.dp else 18.dp
        b.kind in list && prev.kind in list -> 2.dp
        prev.kind == BlockKind.HEADING -> 8.dp
        else -> 12.dp
    }
    Box(Modifier.height(h))
}

@Composable
private fun Quote(inside: List<Block>, callout: String, actions: BlockActions) {
    val dark = isDark()
    val notes = LocalNoteColors.current
    val accent = if (callout.isEmpty()) notes.quoteBar else calloutColor(callout, dark)
    val bg = if (callout.isEmpty()) Color.Transparent else accent.copy(alpha = if (dark) 0.14f else 0.08f)
    Row(
        Modifier.fillMaxWidth().height(IntrinsicSize.Min).clip(RoundedCornerShape(if (callout.isEmpty()) 2.dp else 10.dp)).background(bg),
    ) {
        Box(Modifier.width(if (callout.isEmpty()) 3.dp else 4.dp).fillMaxHeight().background(accent))
        Column(Modifier.padding(start = 14.dp, end = 12.dp, top = if (callout.isEmpty()) 2.dp else 12.dp, bottom = if (callout.isEmpty()) 2.dp else 12.dp)) {
            Blocks(inside, actions)
        }
    }
}

private fun calloutIcon(kind: String): ImageVector = when (kind) {
    "tip", "hint", "important" -> Icons.Outlined.Lightbulb
    "warning", "caution", "attention" -> Icons.Outlined.ReportProblem
    "danger", "error", "bug", "failure", "fail", "missing" -> Icons.Outlined.ErrorOutline
    "question", "help", "faq" -> Icons.Outlined.HelpOutline
    "success", "check", "done" -> Icons.Outlined.CheckCircle
    "quote", "cite" -> Icons.Outlined.FormatQuote
    else -> Icons.Outlined.Info
}

@Composable
private fun BlockView(b: Block, actions: BlockActions) {
    val type = LocalReading.current
    val notes = LocalNoteColors.current
    val edit = Modifier.pointerInput(b.line) { detectTapGestures(onDoubleTap = { actions.onEdit(b.line.toInt()) }) }
    when (b.kind) {
        BlockKind.HEADING -> {
            val style = when (b.level.toInt()) {
                1 -> type.h1
                2 -> type.h2
                3 -> type.h3
                else -> type.h4
            }
            Rich(b.spans, style, actions, edit)
        }
        BlockKind.PARAGRAPH -> Rich(b.spans, type.body, actions, edit.padding(start = (b.level.toInt() * 22).dp))
        BlockKind.BULLET, BlockKind.NUMBERED, BlockKind.TASK -> ListItem(b, actions, edit)
        BlockKind.CODE -> Code(b.text, b.language, notes)
        BlockKind.MATH -> Text(
            b.text,
            style = type.code.copy(fontStyle = FontStyle.Italic),
            modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
            textAlign = androidx.compose.ui.text.style.TextAlign.Center,
        )
        BlockKind.TABLE -> Table(b, actions)
        BlockKind.RULE -> HorizontalDivider(Modifier.padding(vertical = 10.dp), color = MaterialTheme.colorScheme.outlineVariant)
        BlockKind.IMAGE -> Picture(b, actions)
        BlockKind.EMBED -> Surface(
            onClick = { actions.onOpen(b.target) },
            shape = RoundedCornerShape(12.dp),
            color = MaterialTheme.colorScheme.surfaceContainerLow,
            border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Row(Modifier.padding(14.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Icon(Icons.Outlined.Article, null, tint = MaterialTheme.colorScheme.primary)
                Text(b.text.ifEmpty { b.target }, style = MaterialTheme.typography.titleSmall)
            }
        }
        BlockKind.PROPERTIES -> Properties(b)
        BlockKind.CALLOUT_TITLE -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            val c = calloutColor(b.callout, isDark())
            Icon(calloutIcon(b.callout), null, tint = c, modifier = Modifier.size(20.dp))
            Rich(b.spans, type.body.copy(fontWeight = FontWeight.SemiBold, color = c), actions)
        }
    }
}

@Composable
private fun ListItem(b: Block, actions: BlockActions, edit: Modifier) {
    val type = LocalReading.current
    val indent = (b.level.toInt() * 22).dp
    Row(edit.fillMaxWidth().padding(start = indent), verticalAlignment = Alignment.Top) {
        when (b.kind) {
            BlockKind.TASK -> Box(Modifier.width(34.dp).heightIn(min = with(androidx.compose.ui.platform.LocalDensity.current) { type.body.lineHeight.toDp() }), contentAlignment = Alignment.CenterStart) {
                Checkbox(
                    checked = b.checked,
                    onCheckedChange = { actions.onToggle(b.line.toInt()) },
                    modifier = Modifier.size(24.dp).minimumInteractiveComponentSize(),
                    colors = CheckboxDefaults.colors(uncheckedColor = MaterialTheme.colorScheme.outline),
                )
            }
            BlockKind.NUMBERED -> Text("${b.number}.", style = type.body.copy(color = MaterialTheme.colorScheme.onSurfaceVariant), modifier = Modifier.width(30.dp))
            else -> Text(if (b.level.toInt() % 2 == 0) "•" else "◦", style = type.body.copy(color = MaterialTheme.colorScheme.onSurfaceVariant), modifier = Modifier.width(22.dp).padding(start = 4.dp))
        }
        val style = if (b.kind == BlockKind.TASK && b.checked) type.body.copy(color = MaterialTheme.colorScheme.onSurfaceVariant, textDecoration = TextDecoration.LineThrough) else type.body
        Rich(b.spans, style, actions, Modifier.weight(1f))
    }
}

@Composable
private fun Code(text: String, language: String, notes: NoteColors) {
    val type = LocalReading.current
    Surface(shape = RoundedCornerShape(10.dp), color = notes.codeBg, modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(horizontal = 14.dp, vertical = 12.dp)) {
            if (language.isNotEmpty()) {
                Text(language, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.outline)
            }
            Text(text, style = type.code, softWrap = false, modifier = Modifier.horizontalScroll(rememberScrollState()))
        }
    }
}

@Composable
private fun Table(b: Block, actions: BlockActions) {
    val type = LocalReading.current
    val line = MaterialTheme.colorScheme.outlineVariant
    val head = MaterialTheme.colorScheme.surfaceContainer
    val cols = b.rows.maxOfOrNull { it.cells.size } ?: 0
    if (cols == 0) return
    Box(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState())) {
        // Every cell of a column is as wide as its widest cell (up to a limit),
        // and every cell of a row as tall as its tallest.
        Layout(
            content = {
                b.rows.forEach { row ->
                    for (c in 0 until cols) {
                        Box(
                            Modifier
                                .background(if (row.header) head else Color.Transparent)
                                .drawBehind {
                                    val w = 1.dp.toPx()
                                    if (c < cols - 1) drawLine(line, Offset(size.width - w / 2, 0f), Offset(size.width - w / 2, size.height), w)
                                    drawLine(line, Offset(0f, size.height - w / 2), Offset(size.width, size.height - w / 2), w)
                                }
                                .padding(horizontal = 12.dp, vertical = 8.dp),
                        ) {
                            Rich(
                                row.cells.getOrNull(c) ?: emptyList(),
                                type.body.copy(fontSize = type.body.fontSize * 0.92f, lineHeight = type.body.lineHeight * 0.92f, fontWeight = if (row.header) FontWeight.SemiBold else FontWeight.Normal),
                                actions,
                            )
                        }
                    }
                }
            },
            modifier = Modifier.border(1.dp, line, RoundedCornerShape(8.dp)).clip(RoundedCornerShape(8.dp)),
        ) { cells, _ ->
            val most = 260.dp.roundToPx()
            val least = 64.dp.roundToPx()
            val widths = IntArray(cols)
            cells.forEachIndexed { i, m -> widths[i % cols] = maxOf(widths[i % cols], m.maxIntrinsicWidth(Constraints.Infinity).coerceIn(least, most)) }
            val rows = cells.size / cols
            val heights = IntArray(rows) { r -> (0 until cols).maxOf { c -> cells[r * cols + c].maxIntrinsicHeight(widths[c]) } }
            val placed = cells.mapIndexed { i, m -> m.measure(Constraints.fixed(widths[i % cols], heights[i / cols])) }
            layout(widths.sum(), heights.sum()) {
                var y = 0
                for (r in 0 until rows) {
                    var x = 0
                    for (c in 0 until cols) {
                        placed[r * cols + c].place(x, y)
                        x += widths[c]
                    }
                    y += heights[r]
                }
            }
        }
    }
}

@Composable
private fun Picture(b: Block, actions: BlockActions) {
    val model: Any? = if (b.local) actions.fileOf(b.target)?.let { File(it) } else if (b.target.startsWith("http")) b.target else null
    if (model == null) {
        Text("🖼  ${b.target}", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        return
    }
    AsyncImage(
        model = model,
        contentDescription = b.text.ifEmpty { null },
        contentScale = ContentScale.FillWidth,
        modifier = Modifier.fillMaxWidth().heightIn(max = 520.dp).clip(RoundedCornerShape(12.dp)).clickable { actions.onOpen(b.target) },
    )
}

@Composable
private fun Properties(b: Block) {
    Surface(shape = RoundedCornerShape(12.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth().padding(bottom = 4.dp)) {
        Column(Modifier.padding(horizontal = 14.dp, vertical = 10.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            b.props.forEach { p ->
                Row {
                    Text(p.key, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.width(110.dp))
                    Text(p.value, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
    }
}

/** Styled runs of text, with tappable links and tags. */
@Composable
fun Rich(spans: List<Span>, style: TextStyle, actions: BlockActions, modifier: Modifier = Modifier) {
    val notes = LocalNoteColors.current
    val primary = MaterialTheme.colorScheme.primary
    val text = remember(spans, notes, primary) { annotated(spans, notes, primary, actions) }
    Text(text, style = style.copy(color = if (style.color == Color.Unspecified) MaterialTheme.colorScheme.onBackground else style.color), modifier = modifier)
}

fun annotated(spans: List<Span>, notes: NoteColors, primary: Color, actions: BlockActions): AnnotatedString = buildAnnotatedString {
    for (s in spans) {
        val base = SpanStyle(
            fontWeight = if (s.bold) FontWeight.Bold else null,
            fontStyle = if (s.italic) FontStyle.Italic else null,
            textDecoration = if (s.strike) TextDecoration.LineThrough else null,
            fontFamily = if (s.code) FontFamily.Monospace else null,
            fontSize = if (s.code) 0.88.em else androidx.compose.ui.unit.TextUnit.Unspecified,
            background = when {
                s.highlight -> notes.highlight
                s.code -> notes.codeBg
                else -> Color.Unspecified
            },
        )
        val link = s.link
        if (link == null) {
            withStyle(base) { append(s.text) }
            continue
        }
        val linkStyle = when (link.kind) {
            LinkKind.TAG -> SpanStyle(color = notes.tag, background = notes.tagBg, fontWeight = FontWeight.Medium)
            LinkKind.MISSING -> SpanStyle(color = notes.missingLink, textDecoration = TextDecoration.Underline)
            LinkKind.URL -> SpanStyle(color = primary, textDecoration = TextDecoration.Underline)
            LinkKind.NOTE -> SpanStyle(color = primary, fontWeight = FontWeight.Medium)
        }
        withLink(LinkAnnotation.Clickable(link.target, TextLinkStyles(style = linkStyle)) { actions.onLink(link) }) {
            withStyle(base) { append(s.text) }
        }
    }
}

private val Double.em get() = androidx.compose.ui.unit.TextUnit(this.toFloat(), androidx.compose.ui.unit.TextUnitType.Em)
