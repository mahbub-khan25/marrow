package app.marrow.ui.history

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.RestorePage
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.core.NoteThen
import app.marrow.core.ThenNote
import app.marrow.core.Version
import app.marrow.core.renderMarkdown
import app.marrow.data.words
import app.marrow.ui.common.BackBar
import app.marrow.ui.common.ConfirmDialog
import app.marrow.ui.common.Empty
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.SectionTitle
import app.marrow.ui.common.load
import app.marrow.ui.common.parentOf
import app.marrow.ui.note.BlockActions
import app.marrow.ui.note.Blocks
import kotlinx.coroutines.launch
import java.text.DateFormat
import java.util.Date

private fun stamp(ms: Long): String = DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(ms))

/** Earlier versions of a note: pick one to read, and put it back if you want. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HistoryScreen(path: String) {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val versions = load(path) { run { versions(path) } }
    var open by remember { mutableStateOf<Version?>(null) }
    var restoring by remember { mutableStateOf(false) }

    val v = open
    if (v != null) {
        val text = load(v.timeMs) { run { readVersion(path, v.timeMs) } }
        Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { BackBar(stamp(v.timeMs)) }) { pad ->
            Box(Modifier.padding(pad).fillMaxSize(), contentAlignment = Alignment.TopCenter) {
                Column(Modifier.widthIn(max = 720.dp).verticalScroll(rememberScrollState()).padding(horizontal = 22.dp)) {
                    Button(onClick = { restoring = true }, modifier = Modifier.padding(vertical = 8.dp)) {
                        Icon(Icons.Outlined.RestorePage, null)
                        Text("  " + stringResource(R.string.restore_this_version))
                    }
                    text.value?.let { Blocks(remember(it) { renderMarkdown(it) }, BlockActions()) }
                    text.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                    Spacer(Modifier.height(80.dp))
                }
            }
        }
        androidx.activity.compose.BackHandler { open = null }
        if (restoring) ConfirmDialog(
            stringResource(R.string.restore_q),
            stringResource(R.string.restore_text),
            stringResource(R.string.restore),
            onDismiss = { restoring = false },
        ) {
            restoring = false
            scope.launch {
                runCatching { engine.run { restoreVersion(path, v.timeMs) } }
                    .onSuccess { engine.say(ctx.getString(R.string.restored)); nav.back() }
                    .onFailure { engine.say(it.words()) }
            }
        }
        return
    }

    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { BackBar(stringResource(R.string.version_history)) }) { pad ->
        LazyColumn(Modifier.padding(pad).fillMaxSize(), contentPadding = PaddingValues(bottom = 40.dp)) {
            item {
                Text(stringResource(R.string.history_explain), Modifier.padding(horizontal = 20.dp, vertical = 8.dp), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            if (versions.value?.isEmpty() == true) {
                item { Empty(Icons.Outlined.History, stringResource(R.string.no_versions), stringResource(R.string.no_versions_text)) }
            }
            items(versions.value.orEmpty(), key = { it.timeMs }) { ver ->
                Row(Modifier.fillMaxWidth().clickable { open = ver }.padding(horizontal = 20.dp, vertical = 14.dp), verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Outlined.History, null, tint = MaterialTheme.colorScheme.primary)
                    Column(Modifier.padding(start = 16.dp)) {
                        Text(stamp(ver.timeMs), style = MaterialTheme.typography.titleMedium)
                        Text(app.marrow.ui.common.ago(ver.timeMs) + " · " + android.text.format.Formatter.formatShortFileSize(ctx, ver.size.toLong()), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.outline)
                    }
                }
            }
        }
    }
}

/** The whole vault as it was on a day: what was there, what changed since, what's gone. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TimeMachineScreen() {
    val nav = LocalNav.current
    val timeline = load { run { timeline() } }.value
    val day = 86_400_000L
    val now = System.currentTimeMillis()
    val first = (timeline?.firstMs ?: (now - 30 * day)).coerceAtMost(now - day)
    var pos by remember { mutableFloatStateOf(1f) }
    val at = first + ((now - first) * pos).toLong()
    val notes = load(at / (60_000)) { run { vaultAt(at) } }
    var reading by remember { mutableStateOf<ThenNote?>(null) }

    reading?.let { n ->
        val text = load(n.path, n.version) {
            run { if (n.version != null) readVersion(n.path, n.version!!) else readNote(n.path) }
        }
        Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { BackBar(n.title) }) { pad ->
            Column(Modifier.padding(pad).verticalScroll(rememberScrollState()).padding(horizontal = 22.dp)) {
                Text(stamp(at), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                text.value?.let { Blocks(remember(it) { renderMarkdown(it) }, BlockActions()) }
                if (n.state != NoteThen.GONE) {
                    Button(onClick = { reading = null; nav.note(n.path) }, modifier = Modifier.padding(vertical = 16.dp)) { Text(stringResource(R.string.open_as_it_is_now)) }
                }
                if (n.version != null) {
                    Button(onClick = { reading = null; nav.go(app.marrow.ui.HistoryRoute(n.path)) }, modifier = Modifier.padding(bottom = 40.dp)) { Text(stringResource(R.string.version_history)) }
                }
            }
        }
        androidx.activity.compose.BackHandler { reading = null }
        return
    }

    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { BackBar(stringResource(R.string.time_machine)) }) { pad ->
        Column(Modifier.padding(pad).fillMaxSize()) {
            Surface(color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(stringResource(R.string.your_vault_on), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(DateFormat.getDateInstance(DateFormat.FULL).format(Date(at)), style = MaterialTheme.typography.headlineSmall)
                    Slider(pos, { pos = it })
                    val list = notes.value.orEmpty()
                    val changed = list.count { it.state == NoteThen.CHANGED }
                    val gone = list.count { it.state == NoteThen.GONE }
                    Text(stringResource(R.string.tm_summary, list.size, changed, gone), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            LazyColumn(contentPadding = PaddingValues(bottom = 40.dp)) {
                val list = notes.value.orEmpty().sortedBy { it.state == NoteThen.SAME }
                val groups = list.groupBy { it.state }
                listOf(NoteThen.GONE to R.string.deleted_since, NoteThen.CHANGED to R.string.changed_since, NoteThen.SAME to R.string.unchanged).forEach { (state, label) ->
                    val g = groups[state].orEmpty()
                    if (g.isNotEmpty()) {
                        item(key = "h$state") { SectionTitle("${stringResource(label)} · ${g.size}") }
                        items(g, key = { "${state}:${it.path}" }) { n ->
                            Row(Modifier.fillMaxWidth().clickable { reading = n }.padding(horizontal = 20.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                                Box(Modifier.size(10.dp).padding(1.dp)) {
                                    Surface(shape = CircleShape, color = when (state) {
                                        NoteThen.GONE -> MaterialTheme.colorScheme.error
                                        NoteThen.CHANGED -> MaterialTheme.colorScheme.tertiary
                                        NoteThen.SAME -> MaterialTheme.colorScheme.outlineVariant
                                    }, modifier = Modifier.fillMaxSize()) {}
                                }
                                Column(Modifier.padding(start = 16.dp)) {
                                    Text(n.title, style = MaterialTheme.typography.titleMedium)
                                    val folder = parentOf(n.path)
                                    if (folder.isNotEmpty()) Text(folder, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.outline)
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
