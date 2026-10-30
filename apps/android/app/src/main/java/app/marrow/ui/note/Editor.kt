package app.marrow.ui.note

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.FormatListBulleted
import androidx.compose.material.icons.outlined.AddPhotoAlternate
import androidx.compose.material.icons.outlined.AlarmAdd
import androidx.compose.material.icons.outlined.CheckBox
import androidx.compose.material.icons.outlined.Code
import androidx.compose.material.icons.outlined.FormatBold
import androidx.compose.material.icons.outlined.FormatItalic
import androidx.compose.material.icons.outlined.FormatQuote
import androidx.compose.material.icons.outlined.KeyboardHide
import androidx.compose.material.icons.outlined.Link
import androidx.compose.material.icons.outlined.Mic
import androidx.compose.material.icons.outlined.Tag
import androidx.compose.material.icons.outlined.Title
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.core.FileHit
import app.marrow.core.MarkKind
import app.marrow.core.highlight
import app.marrow.ui.theme.LocalNoteColors
import app.marrow.ui.theme.LocalReading
import app.marrow.ui.theme.NoteColors

/** Colours the Markdown as it's typed (the engine finds what to colour). */
class MarkdownColors(private val notes: NoteColors, private val text: Color, private val primary: Color) : VisualTransformation {
    override fun filter(text: AnnotatedString): TransformedText {
        val s = text.text
        if (s.length > 300_000) return TransformedText(text, OffsetMapping.Identity)
        val b = AnnotatedString.Builder(s)
        for (m in highlight(s)) {
            val start = m.start.toInt().coerceIn(0, s.length)
            val end = m.end.toInt().coerceIn(start, s.length)
            val style = when (m.kind) {
                MarkKind.HEADING -> SpanStyle(fontWeight = FontWeight.Bold, fontSize = when (m.level.toInt()) {
                    1 -> 1.35f
                    2 -> 1.2f
                    3 -> 1.1f
                    else -> 1f
                }.let { androidx.compose.ui.unit.TextUnit(it, androidx.compose.ui.unit.TextUnitType.Em) })
                MarkKind.BOLD -> SpanStyle(fontWeight = FontWeight.Bold)
                MarkKind.ITALIC -> SpanStyle(fontStyle = FontStyle.Italic)
                MarkKind.STRIKE -> SpanStyle(textDecoration = TextDecoration.LineThrough)
                MarkKind.CODE, MarkKind.CODE_BLOCK -> SpanStyle(fontFamily = FontFamily.Monospace, background = notes.codeBg)
                MarkKind.LINK -> SpanStyle(color = primary)
                MarkKind.TAG -> SpanStyle(color = notes.tag, background = notes.tagBg)
                MarkKind.QUOTE -> SpanStyle(fontStyle = FontStyle.Italic)
                MarkKind.LIST_MARKER -> SpanStyle(color = primary, fontWeight = FontWeight.Bold)
                MarkKind.TASK_OPEN -> SpanStyle(color = primary, fontFamily = FontFamily.Monospace)
                MarkKind.TASK_DONE -> if (m.level.toInt() == 1) SpanStyle(color = notes.faint, textDecoration = TextDecoration.LineThrough) else SpanStyle(color = primary, fontFamily = FontFamily.Monospace)
                MarkKind.HIGHLIGHT -> SpanStyle(background = notes.highlight)
                MarkKind.FRONTMATTER -> SpanStyle(color = notes.faint, fontFamily = FontFamily.Monospace)
                MarkKind.SYNTAX -> SpanStyle(color = notes.faint)
            }
            b.addStyle(style, start, end)
        }
        return TransformedText(b.toAnnotatedString(), OffsetMapping.Identity)
    }

    override fun equals(other: Any?): Boolean = other is MarkdownColors && other.notes == notes && other.text == text && other.primary == primary
    override fun hashCode(): Int = notes.hashCode() * 31 + text.hashCode()
}

/** The `[[` being typed before the cursor, if any: the text after `[[`. */
fun linkQuery(v: TextFieldValue): String? {
    val before = v.text.substring(0, v.selection.start.coerceIn(0, v.text.length))
    val open = before.lastIndexOf("[[")
    if (open < 0) return null
    val q = before.substring(open + 2)
    return if (q.contains("]]") || q.contains('\n') || q.length > 60) null else q
}

/** Replaces the `[[query` before the cursor with a whole link. */
fun completeLink(v: TextFieldValue, linkText: String): TextFieldValue {
    val cur = v.selection.start
    val open = v.text.substring(0, cur).lastIndexOf("[[")
    val after = v.text.substring(cur)
    val close = if (after.startsWith("]]")) 2 else 0
    val insert = "[[$linkText]]"
    val text = v.text.substring(0, open) + insert + after.substring(close)
    return TextFieldValue(text, TextRange(open + insert.length))
}

private val LIST = Regex("""^(\s*)([-*+]|\d+[.)])( \[[ xX]\])? (.*)$""")

/**
 * After a new line: keeps a list going ("- ", "- [ ] ", "2. "), or ends it
 * when the item was left empty.
 */
fun continueList(old: TextFieldValue, new: TextFieldValue): TextFieldValue {
    if (new.text.length != old.text.length + 1) return new
    val at = new.selection.start - 1
    if (at < 0 || at >= new.text.length || new.text[at] != '\n' || new.selection.collapsed.not()) return new
    val lineStart = new.text.lastIndexOf('\n', at - 1) + 1
    val line = new.text.substring(lineStart, at)
    val m = LIST.find(line) ?: return new
    val (indent, marker, task, rest) = m.destructured
    if (rest.isBlank()) {
        // An empty item: Enter ends the list.
        val text = new.text.substring(0, lineStart) + new.text.substring(at + 1)
        return TextFieldValue(text, TextRange(lineStart))
    }
    // "3." goes on as "4."; bullets stay the same.
    val nextMarker = marker.dropLast(1).toIntOrNull()?.let { "${it + 1}${marker.last()}" } ?: marker
    val prefix = indent + nextMarker + (if (task.isNotEmpty()) " [ ]" else "") + " "
    val text = new.text.substring(0, at + 1) + prefix + new.text.substring(at + 1)
    return TextFieldValue(text, TextRange(at + 1 + prefix.length))
}

/** Wraps the selection in `mark` (or puts the cursor between two of them). */
fun wrap(v: TextFieldValue, mark: String): TextFieldValue {
    val s = v.selection.min
    val e = v.selection.max
    val text = v.text.substring(0, s) + mark + v.text.substring(s, e) + mark + v.text.substring(e)
    return TextFieldValue(text, if (s == e) TextRange(s + mark.length) else TextRange(s + mark.length, e + mark.length))
}

/** Sets or changes the start of the cursor's line ("# ", "- ", "- [ ] ", "> "). */
fun linePrefix(v: TextFieldValue, prefix: String): TextFieldValue {
    val cur = v.selection.start
    val start = v.text.lastIndexOf('\n', cur - 1) + 1
    val line = v.text.substring(start).substringBefore('\n')
    val existing = Regex("""^(#{1,6} |[-*+] \[[ xX]\] |[-*+] |\d+[.)] |> )""").find(line)?.value ?: ""
    val replace = when {
        prefix == "# " && existing.startsWith("#") -> if (existing.length >= 7) "" else "#$existing"
        existing == prefix -> ""
        else -> prefix
    }
    val text = v.text.substring(0, start) + replace + v.text.substring(start + existing.length)
    val shift = replace.length - existing.length
    return TextFieldValue(text, TextRange((v.selection.start + shift).coerceAtLeast(start), (v.selection.end + shift).coerceAtLeast(start)))
}

/** The cursor's line: where it starts, and its text. */
fun currentLine(v: TextFieldValue): Pair<Int, String> {
    val start = v.text.lastIndexOf('\n', (v.selection.start - 1).coerceAtLeast(-1)) + 1
    return start to v.text.substring(start).substringBefore('\n')
}

/** Makes the cursor's line a task if it isn't one, then sets its reminder (or removes it with null). */
fun withReminder(v: TextFieldValue, at: String?): TextFieldValue {
    val isTask = Regex("^\\s*(?:>\\s*)*(?:[-*+]|\\d+[.)]) \\[[ xX]\\] ")
    val v2 = if (isTask.containsMatchIn(currentLine(v).second)) v else linePrefix(v, "- [ ] ")
    val (start, line) = currentLine(v2)
    val next = app.marrow.core.lineWithReminder(line, at)
    val text = v2.text.substring(0, start) + next + v2.text.substring(start + line.length)
    return TextFieldValue(text, TextRange(start + next.length))
}

fun insert(v: TextFieldValue, s: String): TextFieldValue {
    val text = v.text.substring(0, v.selection.min) + s + v.text.substring(v.selection.max)
    return TextFieldValue(text, TextRange(v.selection.min + s.length))
}

@Composable
fun MarkdownEditor(value: TextFieldValue, onChange: (TextFieldValue) -> Unit, modifier: Modifier = Modifier, placeholder: String = "") {
    val type = LocalReading.current
    val notes = LocalNoteColors.current
    val onBg = MaterialTheme.colorScheme.onBackground
    val primary = MaterialTheme.colorScheme.primary
    val colors = remember(notes, onBg, primary) { MarkdownColors(notes, onBg, primary) }
    BasicTextField(
        value = value,
        onValueChange = { onChange(continueList(value, it)) },
        textStyle = type.editor.copy(color = onBg),
        cursorBrush = SolidColor(primary),
        visualTransformation = colors,
        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
        modifier = modifier,
        decorationBox = { inner ->
            if (value.text.isEmpty() && placeholder.isNotEmpty()) {
                Text(placeholder, style = type.editor.copy(color = MaterialTheme.colorScheme.outline))
            }
            inner()
        },
    )
}

/** The bar above the keyboard: formatting, links, tags, photos, dictation. */
@Composable
fun EditorBar(
    value: TextFieldValue,
    onChange: (TextFieldValue) -> Unit,
    suggestions: List<FileHit>,
    onPhoto: () -> Unit,
    onDictate: () -> Unit,
    modifier: Modifier = Modifier,
    onRemind: () -> Unit = {},
) {
    val focus = LocalFocusManager.current
    Surface(color = MaterialTheme.colorScheme.surfaceContainer, modifier = modifier.fillMaxWidth()) {
        Column {
            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
            if (suggestions.isNotEmpty() && linkQuery(value) != null) {
                Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    suggestions.forEach { hit ->
                        Surface(
                            onClick = { onChange(completeLink(value, hit.linkText)) },
                            shape = RoundedCornerShape(50),
                            color = MaterialTheme.colorScheme.primaryContainer,
                        ) {
                            Text(hit.name, Modifier.padding(horizontal = 14.dp, vertical = 8.dp), style = MaterialTheme.typography.labelLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
            Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 4.dp).height(48.dp), verticalAlignment = Alignment.CenterVertically) {
                Tool(Icons.Outlined.Title, stringResource(R.string.heading)) { onChange(linePrefix(value, "# ")) }
                Tool(Icons.Outlined.FormatBold, stringResource(R.string.bold)) { onChange(wrap(value, "**")) }
                Tool(Icons.Outlined.FormatItalic, stringResource(R.string.italic)) { onChange(wrap(value, "*")) }
                Tool(Icons.AutoMirrored.Outlined.FormatListBulleted, stringResource(R.string.list)) { onChange(linePrefix(value, "- ")) }
                Tool(Icons.Outlined.CheckBox, stringResource(R.string.task)) { onChange(linePrefix(value, "- [ ] ")) }
                Tool(Icons.Outlined.AlarmAdd, stringResource(R.string.remind_me), onRemind)
                Tool(Icons.Outlined.Link, stringResource(R.string.link_to_note)) { onChange(insert(value, "[[").let { TextFieldValue(it.text.substring(0, it.selection.start) + "]]" + it.text.substring(it.selection.start), it.selection) }) }
                Tool(Icons.Outlined.Tag, stringResource(R.string.tag)) { onChange(insert(value, "#")) }
                Tool(Icons.Outlined.FormatQuote, stringResource(R.string.quote)) { onChange(linePrefix(value, "> ")) }
                Tool(Icons.Outlined.Code, stringResource(R.string.code)) { onChange(wrap(value, "`")) }
                Tool(Icons.Outlined.AddPhotoAlternate, stringResource(R.string.add_photo), onPhoto)
                Tool(Icons.Outlined.Mic, stringResource(R.string.dictate), onDictate)
                Tool(Icons.Outlined.KeyboardHide, stringResource(R.string.done)) { focus.clearFocus() }
            }
        }
    }
}

@Composable
private fun Tool(icon: androidx.compose.ui.graphics.vector.ImageVector, label: String, onClick: () -> Unit) {
    IconButton(onClick = onClick, modifier = Modifier.size(44.dp)) {
        Icon(icon, label, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
