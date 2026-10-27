package app.marrow.ui.ask

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.Send
import androidx.compose.material.icons.outlined.AutoAwesome
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Stop
import androidx.compose.material3.Button
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.core.AiMessage
import app.marrow.core.LinkKind
import app.marrow.core.renderAnswer
import app.marrow.data.words
import app.marrow.ui.AiRoute
import app.marrow.ui.common.Empty
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.note.Answer
import app.marrow.ui.note.BlockActions
import app.marrow.ui.note.Blocks

private class Turn(val question: String, val answer: Answer)

/** Holds the conversation while the app runs, so switching tabs keeps it. */
private object Conversation {
    val turns = mutableStateListOf<Turn>()
}

@Composable
fun AskScreen() {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    var ready by remember { mutableStateOf(engine.marrow.aiReady()) }
    LaunchedEffect(Unit) { ready = engine.marrow.aiReady() }
    var input by rememberSaveable { mutableStateOf("") }
    val turns = Conversation.turns
    val list = rememberLazyListState()
    val busy = turns.lastOrNull()?.answer?.done == false
    LaunchedEffect(turns.size, turns.lastOrNull()?.answer?.text?.length) {
        if (turns.isNotEmpty()) list.animateScrollToItem(turns.size)
    }

    fun ask(q: String) {
        val question = q.trim()
        if (question.isEmpty() || busy) return
        input = ""
        engine.prefs.recentQuestions = (listOf(question) + engine.prefs.recentQuestions.filter { it != question })
        val history = turns.takeLast(3).flatMap { listOf(AiMessage("user", it.question), AiMessage("assistant", it.answer.text)) }
        val a = Answer()
        turns.add(Turn(question, a))
        try {
            a.task = engine.marrow.askNotes(question, history, null, a.listener)
        } catch (e: Exception) {
            a.error = e.words()
            a.done = true
        }
    }

    Column(Modifier.fillMaxSize().statusBarsPadding().imePadding(), horizontalAlignment = Alignment.CenterHorizontally) {
        Row(Modifier.widthIn(max = 760.dp).fillMaxWidth().padding(start = 24.dp, end = 8.dp, top = 16.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(stringResource(R.string.ask_your_notes), style = MaterialTheme.typography.headlineMedium, modifier = Modifier.weight(1f))
            if (turns.isNotEmpty() && !busy) {
                IconButton(onClick = { turns.clear() }) { Icon(Icons.Outlined.Delete, stringResource(R.string.new_conversation)) }
            }
        }
        if (!ready) {
            Empty(Icons.Outlined.AutoAwesome, stringResource(R.string.ask_your_notes), stringResource(R.string.ask_setup_text)) {
                Button(onClick = { nav.go(AiRoute) }) { Text(stringResource(R.string.set_up_ai)) }
            }
            return@Column
        }
        LazyColumn(Modifier.weight(1f).widthIn(max = 760.dp).fillMaxWidth(), state = list, contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            if (turns.isEmpty()) {
                item {
                    Column(Modifier.padding(top = 24.dp, start = 8.dp, end = 8.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Text(stringResource(R.string.ask_intro), style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        val examples = engine.prefs.recentQuestions.ifEmpty {
                            listOf(stringResource(R.string.ask_example_1), stringResource(R.string.ask_example_2), stringResource(R.string.ask_example_3))
                        }
                        examples.take(4).forEach { q ->
                            Surface(onClick = { ask(q) }, shape = RoundedCornerShape(16.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth()) {
                                Text(q, Modifier.padding(16.dp), style = MaterialTheme.typography.bodyLarge)
                            }
                        }
                    }
                }
            }
            itemsIndexed(turns) { _, t ->
                Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.CenterEnd) {
                        Surface(shape = RoundedCornerShape(20.dp, 20.dp, 6.dp, 20.dp), color = MaterialTheme.colorScheme.primaryContainer, modifier = Modifier.widthIn(max = 520.dp)) {
                            Text(t.question, Modifier.padding(horizontal = 16.dp, vertical = 12.dp), style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onPrimaryContainer)
                        }
                    }
                    AnswerView(t.answer)
                }
            }
            item { Box(Modifier.size(8.dp)) }
        }
        Row(Modifier.widthIn(max = 760.dp).fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            TextField(
                input,
                { input = it },
                placeholder = { Text(stringResource(R.string.ask_placeholder)) },
                shape = RoundedCornerShape(28.dp),
                maxLines = 5,
                colors = TextFieldDefaults.colors(
                    focusedIndicatorColor = Color.Transparent,
                    unfocusedIndicatorColor = Color.Transparent,
                    focusedContainerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
                    unfocusedContainerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
                ),
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                keyboardActions = KeyboardActions(onSend = { ask(input) }),
                modifier = Modifier.weight(1f),
            )
            if (busy) {
                FilledIconButton(onClick = { turns.lastOrNull()?.answer?.task?.cancel(); turns.lastOrNull()?.answer?.done = true }, modifier = Modifier.padding(start = 8.dp).size(52.dp)) {
                    Icon(Icons.Outlined.Stop, stringResource(R.string.stop))
                }
            } else {
                FilledIconButton(onClick = { ask(input) }, enabled = input.isNotBlank(), modifier = Modifier.padding(start = 8.dp).size(52.dp)) {
                    Icon(Icons.AutoMirrored.Outlined.Send, stringResource(R.string.send))
                }
            }
        }
    }
}

@Composable
private fun AnswerView(a: Answer) {
    val nav = LocalNav.current
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (!a.done && a.text.isEmpty()) {
            Text(stringResource(R.string.thinking), color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodyMedium)
            LinearProgressIndicator(Modifier.widthIn(max = 160.dp))
        }
        if (a.error != null) {
            Text(a.error!!, color = MaterialTheme.colorScheme.error)
        } else if (a.text.isNotEmpty()) {
            val blocks = remember(a.text, a.sources) { renderAnswer(a.text, a.sources) }
            Blocks(
                blocks,
                BlockActions(onLink = { l ->
                    if (l.kind == LinkKind.NOTE) nav.note(l.target.substringBefore("#L"), l.target.substringAfter("#L", "-1").toIntOrNull() ?: -1)
                }),
            )
        }
        if (a.sources.isNotEmpty() && a.done) {
            Text(stringResource(R.string.from_your_notes), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
            LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                itemsIndexed(a.sources) { i, s ->
                    Surface(onClick = { nav.note(s.path, s.line.toInt()) }, shape = RoundedCornerShape(14.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.widthIn(max = 220.dp)) {
                        Column(Modifier.padding(12.dp)) {
                            Text("${i + 1}  ${s.title}", style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis, color = MaterialTheme.colorScheme.primary)
                            if (s.heading.isNotEmpty()) Text(s.heading, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.outline, maxLines = 1)
                            Text(s.text, style = MaterialTheme.typography.bodySmall, maxLines = 3, overflow = TextOverflow.Ellipsis, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
            }
        }
    }
}
