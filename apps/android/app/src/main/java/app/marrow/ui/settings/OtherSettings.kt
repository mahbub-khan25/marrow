package app.marrow.ui.settings

import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.CallMerge
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.EditNote
import androidx.compose.material.icons.outlined.FolderOpen
import androidx.compose.material.icons.outlined.Inventory2
import androidx.compose.material.icons.outlined.ViewDay
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.marrow.R
import app.marrow.core.ImportReport
import app.marrow.core.VaultSettings
import app.marrow.data.words
import app.marrow.ui.common.BackBar
import app.marrow.ui.common.ConfirmDialog
import app.marrow.ui.common.Empty
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.NameDialog
import app.marrow.ui.common.SectionTitle
import app.marrow.ui.common.load
import app.marrow.ui.common.nameOf
import app.marrow.ui.theme.LocalReading
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AppearanceScreen() {
    val engine = LocalEngine.current
    val look by engine.prefs.look.collectAsStateWithLifecycle()
    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { BackBar(stringResource(R.string.appearance)) }) { pad ->
        Box(Modifier.padding(pad).fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 640.dp).verticalScroll(rememberScrollState()).padding(bottom = 40.dp)) {
                Choice(R.string.theme, listOf("system" to R.string.same_as_phone, "light" to R.string.light, "dark" to R.string.dark), look.theme) { v -> engine.prefs.update { it.copy(theme = v) } }
                Choice(R.string.note_text, listOf("book" to R.string.font_book, "sans" to R.string.font_sans, "classic" to R.string.font_classic), look.readFont) { v -> engine.prefs.update { it.copy(readFont = v) } }
                Choice(R.string.text_size, listOf("0" to R.string.size_small, "1" to R.string.size_medium, "2" to R.string.size_large, "3" to R.string.size_larger), look.textSize.toString()) { v -> engine.prefs.update { it.copy(textSize = v.toInt()) } }
                Choice(R.string.open_notes_for, listOf("read" to R.string.reading, "edit" to R.string.editing), look.openIn) { v -> engine.prefs.update { it.copy(openIn = v) } }
                SectionTitle(stringResource(R.string.preview))
                Surface(shape = RoundedCornerShape(16.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.padding(horizontal = 16.dp).fillMaxWidth()) {
                    Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(stringResource(R.string.preview_title), style = LocalReading.current.h2)
                        Text(stringResource(R.string.preview_text), style = LocalReading.current.body)
                        Text("আমার সোনার বাংলা, আমি তোমায় ভালোবাসি।", style = LocalReading.current.body)
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Choice(title: Int, options: List<Pair<String, Int>>, selected: String, onPick: (String) -> Unit) {
    SectionTitle(stringResource(title))
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
        options.forEachIndexed { i, (key, label) ->
            SegmentedButton(selected = key == selected, onClick = { onPick(key) }, shape = SegmentedButtonDefaults.itemShape(i, options.size)) {
                Text(stringResource(label), maxLines = 1)
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun VaultScreen() {
    val engine = LocalEngine.current
    val scope = rememberCoroutineScope()
    val vault by engine.vault.collectAsStateWithLifecycle()
    var settings by remember { mutableStateOf<VaultSettings?>(null) }
    val loaded = load { run { vaultSettings() } }.value
    val s = settings ?: loaded
    var asking by remember { mutableStateOf<String?>(null) }
    var switching by remember { mutableStateOf(false) }

    fun save(next: VaultSettings) {
        settings = next
        scope.launch { runCatching { engine.run { setVaultSettings(next) } }.onFailure { engine.say(it.words()) } }
    }

    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { BackBar(stringResource(R.string.vault)) }) { pad ->
        Box(Modifier.padding(pad).fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 640.dp).verticalScroll(rememberScrollState()).padding(bottom = 40.dp)) {
                SettingRow(Icons.Outlined.Inventory2, vault?.name ?: "", vault?.root)
                Explain(stringResource(R.string.vault_explain))
                if (s != null) {
                    SectionTitle(stringResource(R.string.daily_notes))
                    SettingRow(Icons.Outlined.CalendarMonth, stringResource(R.string.daily_folder), s.dailyFolder.ifEmpty { stringResource(R.string.top_of_vault) }, onClick = { asking = "daily" })
                    SettingRow(Icons.Outlined.ViewDay, stringResource(R.string.daily_layout), stringResource(if (s.dailyLayout == "flat") R.string.layout_flat else R.string.layout_year_month), onClick = {
                        save(s.copy(dailyLayout = if (s.dailyLayout == "flat") "year-month" else "flat"))
                    })
                    SectionTitle(stringResource(R.string.templates))
                    SettingRow(Icons.Outlined.EditNote, stringResource(R.string.templates_folder), s.templateFolder.ifEmpty { "—" }, onClick = { asking = "templates" })
                }
                SectionTitle(stringResource(R.string.other_vaults))
                SettingRow(Icons.Outlined.FolderOpen, stringResource(R.string.open_another_vault), stringResource(R.string.open_another_vault_text), onClick = { switching = true })
            }
        }
    }
    when (asking) {
        "daily" -> NameDialog(stringResource(R.string.daily_folder), initial = s?.dailyFolder.orEmpty(), onDismiss = { asking = null }) { v ->
            asking = null
            s?.let { save(it.copy(dailyFolder = v)) }
        }
        "templates" -> NameDialog(stringResource(R.string.templates_folder), initial = s?.templateFolder.orEmpty(), onDismiss = { asking = null }) { v ->
            asking = null
            s?.let { save(it.copy(templateFolder = v)) }
        }
    }
    if (switching) ConfirmDialog(stringResource(R.string.open_another_vault), stringResource(R.string.open_another_vault_confirm), stringResource(R.string.continue_), onDismiss = { switching = false }) {
        switching = false
        engine.close()
    }
}

/** Brings in notes exported from Notion, Evernote, Word, web pages or Markdown files. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ImportScreen() {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf(false) }
    var report by remember { mutableStateOf<ImportReport?>(null) }
    var kind by remember { mutableStateOf("notion") }

    val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris: List<Uri> ->
        if (uris.isEmpty()) return@rememberLauncherForActivityResult
        busy = true
        scope.launch {
            try {
                // The engine reads files by path: copy them in first.
                val dir = File(ctx.cacheDir, "import").apply { deleteRecursively(); mkdirs() }
                val paths = withContext(Dispatchers.IO) {
                    uris.mapIndexed { i, u ->
                        val name = ctx.contentResolver.query(u, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c -> if (c.moveToFirst()) c.getString(0) else null } ?: "import-$i"
                        val f = File(dir, name)
                        ctx.contentResolver.openInputStream(u)?.use { input -> f.outputStream().use { input.copyTo(it) } }
                        f.absolutePath
                    }
                }
                val folder = when (kind) {
                    "notion" -> "Notion"
                    "evernote" -> "Evernote"
                    "word" -> "Word"
                    else -> "Imported"
                }
                report = engine.run { importFiles(kind, paths, folder) }
                dir.deleteRecursively()
            } catch (e: Exception) {
                engine.say(e.words())
            }
            busy = false
        }
    }

    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { BackBar(stringResource(R.string.import_notes)) }) { pad ->
        Box(Modifier.padding(pad).fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 640.dp).verticalScroll(rememberScrollState()).padding(bottom = 40.dp)) {
                Explain(stringResource(R.string.import_explain))
                listOf(
                    Triple("notion", R.string.import_notion, R.string.import_notion_how),
                    Triple("evernote", R.string.import_evernote, R.string.import_evernote_how),
                    Triple("word", R.string.import_word, R.string.import_word_how),
                    Triple("markdown", R.string.import_markdown, R.string.import_markdown_how),
                ).forEach { (k, title, how) ->
                    Surface(shape = RoundedCornerShape(16.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp).fillMaxWidth()) {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text(stringResource(title), style = MaterialTheme.typography.titleMedium)
                            Text(stringResource(how), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            OutlinedButton(enabled = !busy, onClick = {
                                kind = k
                                pick.launch(arrayOf("*/*"))
                            }) { Text(stringResource(R.string.choose_files)) }
                        }
                    }
                }
                if (busy) Row(Modifier.padding(20.dp), verticalAlignment = Alignment.CenterVertically) {
                    CircularProgressIndicator()
                    Text(stringResource(R.string.importing), Modifier.padding(start = 16.dp))
                }
                report?.let { r ->
                    Surface(shape = RoundedCornerShape(16.dp), color = MaterialTheme.colorScheme.primaryContainer, modifier = Modifier.padding(16.dp).fillMaxWidth()) {
                        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                            Text(pluralStringResource(R.plurals.imported_notes, r.notes.toInt(), r.notes.toInt(), r.folder), style = MaterialTheme.typography.titleMedium)
                            r.warnings.take(5).forEach { Text("• $it", style = MaterialTheme.typography.bodySmall) }
                            Button(onClick = { nav.folder(r.folder) }) { Text(stringResource(R.string.open_folder)) }
                        }
                    }
                }
            }
        }
    }
}

/** Copies of notes that sync apps made when both sides changed. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ConflictsScreen() {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val list = load { run { conflicts() } }
    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { BackBar(stringResource(R.string.conflicts)) }) { pad ->
        LazyColumn(Modifier.padding(pad).fillMaxSize(), contentPadding = PaddingValues(bottom = 40.dp)) {
            item { Explain(stringResource(R.string.conflicts_explain)) }
            if (list.value?.isEmpty() == true) item { Empty(Icons.AutoMirrored.Outlined.CallMerge, stringResource(R.string.no_conflicts), stringResource(R.string.no_conflicts_text)) }
            items(list.value.orEmpty(), key = { it.copy }) { c ->
                Surface(shape = RoundedCornerShape(16.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp).fillMaxWidth()) {
                    Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                        Text(nameOf(c.original), style = MaterialTheme.typography.titleMedium)
                        Text(stringResource(R.string.copy_named, nameOf(c.copy)), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Button(onClick = {
                                scope.launch {
                                    runCatching { engine.run { resolveConflict(c.copy, c.original) } }
                                        .onSuccess { n -> engine.say(if (n == 0u) ctx.getString(R.string.merged) else ctx.getString(R.string.merged_both_kept)) }
                                        .onFailure { engine.say(it.words()) }
                                }
                            }) { Text(stringResource(R.string.merge)) }
                            OutlinedButton(onClick = { nav.note(c.copy) }) { Text(stringResource(R.string.see_copy)) }
                        }
                    }
                }
            }
        }
    }
}
