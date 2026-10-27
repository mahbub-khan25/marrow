package app.marrow.ui.note

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Send
import androidx.compose.material.icons.outlined.AutoAwesome
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SuggestionChip
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.core.AiListener
import app.marrow.core.AiTask
import app.marrow.core.NoteAction
import app.marrow.core.Source
import app.marrow.core.renderMarkdown
import app.marrow.data.words
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.ago
import app.marrow.ui.common.load
import kotlinx.coroutines.launch

/** About a note: tags, outline, what links here and what it links to. */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun NoteInfoSheet(path: String, onDismiss: () -> Unit) {
    val nav = LocalNav.current
    val engine = LocalEngine.current
    val scope = rememberCoroutineScope()
    val d = load(path) { run { noteDetails(path) } }.value
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = false), containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
        if (d == null) {
            LinearProgressIndicator(Modifier.fillMaxWidth().padding(24.dp))
            return@ModalBottomSheet
        }
        LazyColumn(Modifier.fillMaxWidth().padding(bottom = 24.dp)) {
            item {
                Column(Modifier.padding(horizontal = 24.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(d.title, style = MaterialTheme.typography.headlineSmall)
                    Text(
                        stringResource(R.string.words_and_changed, d.words.toInt(), ago(d.modifiedMs)),
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            if (d.tags.isNotEmpty()) {
                item {
                    FlowRow(Modifier.padding(horizontal = 20.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        d.tags.forEach { t -> AssistChip(onClick = { onDismiss(); nav.tag(t) }, label = { Text(t) }) }
                    }
                }
            }
            if (d.outline.isNotEmpty()) {
                item { Head(stringResource(R.string.outline)) }
                items(d.outline) { h ->
                    Text(
                        h.text,
                        style = if (h.level.toInt() <= 2) MaterialTheme.typography.titleSmall else MaterialTheme.typography.bodyMedium,
                        modifier = Modifier.fillMaxWidth().clickable { onDismiss(); nav.note(path, h.line.toInt()) }.padding(start = (24 + (h.level.toInt() - 1) * 14).dp, end = 24.dp, top = 8.dp, bottom = 8.dp),
                    )
                }
            }
            item { Head(stringResource(R.string.linked_from_n, d.backlinks.size)) }
            if (d.backlinks.isEmpty()) item { Faint(stringResource(R.string.no_backlinks)) }
            items(d.backlinks) { b ->
                Column(Modifier.fillMaxWidth().clickable { onDismiss(); nav.note(b.source, b.lines.firstOrNull()?.line?.toInt() ?: -1) }.padding(horizontal = 24.dp, vertical = 10.dp)) {
                    Text(b.title, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
                    b.lines.take(2).forEach { Text(it.text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2) }
                }
            }
            if (d.mentions.isNotEmpty()) {
                item { Head(stringResource(R.string.mentioned_in)) }
                items(d.mentions) { m ->
                    Row(Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f).clickable { onDismiss(); nav.note(m.source, m.line.toInt()) }) {
                            Text(m.title, style = MaterialTheme.typography.titleSmall)
                            Text(m.text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2)
                        }
                        TextButton(onClick = {
                            scope.launch { runCatching { engine.run { linkMention(path, m.source, m.line) } }.onFailure { engine.say(it.words()) } }
                        }) { Text(stringResource(R.string.link_it)) }
                    }
                }
            }
            if (d.outgoing.isNotEmpty() || d.unresolved.isNotEmpty()) {
                item { Head(stringResource(R.string.links_to)) }
                item {
                    FlowRow(Modifier.padding(horizontal = 20.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        d.outgoing.forEach { o -> SuggestionChip(onClick = { onDismiss(); nav.openPath(o) }, label = { Text(app.marrow.ui.common.nameOf(o)) }) }
                        d.unresolved.forEach { u -> SuggestionChip(onClick = {}, label = { Text("$u ·") }, enabled = false) }
                    }
                }
            }
        }
    }
}

@Composable
private fun Head(text: String) {
    Column {
        HorizontalDivider(Modifier.padding(top = 14.dp), color = MaterialTheme.colorScheme.outlineVariant)
        Text(text, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(start = 24.dp, top = 14.dp, bottom = 4.dp))
    }
}

@Composable
private fun Faint(text: String) {
    Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.outline, modifier = Modifier.padding(horizontal = 24.dp, vertical = 6.dp))
}

/** Streams an AI answer into Compose state. */
class Answer {
    var text by mutableStateOf("")
    var sources by mutableStateOf<List<Source>>(emptyList())
    var done by mutableStateOf(false)
    var error by mutableStateOf<String?>(null)
    var task: AiTask? = null

    val listener = object : AiListener {
        override fun sources(sources: List<Source>) {
            this@Answer.sources = sources
        }

        override fun answer(text: String) {
            this@Answer.text = text
        }

        override fun done(text: String) {
            this@Answer.text = text
            this@Answer.done = true
        }

        override fun failed(message: String) {
            this@Answer.error = message
            this@Answer.done = true
        }
    }
}

/** AI actions on a note: summarize, find action items, translate… */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun NoteAiSheet(path: String, text: String, onDismiss: () -> Unit, onReplace: (String) -> Unit, onAppend: (String) -> Unit) {
    val engine = LocalEngine.current
    val ctx = LocalContext.current
    val actions = remember { engine.marrow.noteActions() }
    var chosen by remember { mutableStateOf<NoteAction?>(null) }
    var answer by remember { mutableStateOf<Answer?>(null) }
    var ask by remember { mutableStateOf("") }
    DisposableEffect(Unit) { onDispose { answer?.task?.cancel() } }

    fun run(action: NoteAction?, instruction: String) {
        chosen = action
        val a = Answer()
        answer = a
        try {
            a.task = engine.marrow.runNoteAction(action?.id ?: "", instruction, path, text, null, a.listener)
        } catch (e: Exception) {
            a.error = e.words()
            a.done = true
        }
    }

    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(bottom = 28.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Outlined.AutoAwesome, null, tint = MaterialTheme.colorScheme.primary)
                Text(chosen?.label ?: stringResource(R.string.ask_ai_about_note), style = MaterialTheme.typography.titleLarge, modifier = Modifier.padding(start = 10.dp))
            }
            val a = answer
            if (a == null) {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    actions.forEach { act -> AssistChip(onClick = { run(act, "") }, label = { Text(act.label) }) }
                }
                OutlinedTextField(
                    ask,
                    { ask = it },
                    placeholder = { Text(stringResource(R.string.or_tell_ai)) },
                    modifier = Modifier.fillMaxWidth(),
                    trailingIcon = {
                        IconButton(onClick = { if (ask.isNotBlank()) run(null, ask) }, enabled = ask.isNotBlank()) { Icon(Icons.AutoMirrored.Outlined.Send, stringResource(R.string.send)) }
                    },
                )
            } else {
                if (!a.done) LinearProgressIndicator(Modifier.fillMaxWidth())
                Column(Modifier.heightIn(max = 460.dp).verticalScroll(rememberScrollState())) {
                    if (a.error != null) {
                        Text(a.error!!, color = MaterialTheme.colorScheme.error)
                    } else {
                        Blocks(remember(a.text) { renderMarkdown(a.text) }, BlockActions())
                    }
                }
                if (a.done && a.error == null) {
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        if (chosen?.replaces == true) {
                            Button(onClick = { onReplace(a.text); onDismiss() }) { Text(stringResource(R.string.replace_note)) }
                        }
                        FilledTonalButton(onClick = { onAppend(a.text); onDismiss() }) { Text(stringResource(R.string.add_to_note)) }
                        OutlinedButton(onClick = {
                            (ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("Marrow", a.text))
                            engine.say(ctx.getString(R.string.copied))
                        }) { Text(stringResource(R.string.copy)) }
                    }
                }
            }
        }
    }
}

/** Sets a password (and optional hint) to lock a note or folder. */
@Composable
fun LockDialog(folder: Boolean = false, onDismiss: () -> Unit, onDone: (String, String?) -> Unit) {
    var pw by remember { mutableStateOf("") }
    var again by remember { mutableStateOf("") }
    var hint by remember { mutableStateOf("") }
    val ok = pw.length >= 6 && pw == again
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(if (folder) R.string.lock_folder else R.string.lock_note)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.lock_explain), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                OutlinedTextField(pw, { pw = it }, label = { Text(stringResource(R.string.password)) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Next), supportingText = { Text(stringResource(R.string.at_least_6)) })
                OutlinedTextField(again, { again = it }, label = { Text(stringResource(R.string.password_again)) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), isError = again.isNotEmpty() && again != pw)
                OutlinedTextField(hint, { hint = it }, label = { Text(stringResource(R.string.hint_optional)) }, singleLine = true)
            }
        },
        confirmButton = { TextButton(onClick = { onDone(pw, hint.ifBlank { null }) }, enabled = ok) { Text(stringResource(R.string.lock)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}
