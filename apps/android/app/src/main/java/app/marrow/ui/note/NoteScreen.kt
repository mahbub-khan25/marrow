package app.marrow.ui.note

import android.app.Activity
import android.content.Intent
import android.speech.RecognizerIntent
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.DriveFileMove
import androidx.compose.material.icons.outlined.AutoAwesome
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.ContentCopy
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.DriveFileRenameOutline
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.Hub
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.LockOpen
import androidx.compose.material.icons.outlined.MoreVert
import androidx.compose.material.icons.outlined.Share
import androidx.compose.material.icons.outlined.Star
import androidx.compose.material.icons.outlined.StarBorder
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.rememberTopAppBarState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.core.Block
import app.marrow.core.BlockKind
import app.marrow.core.FileHit
import app.marrow.core.LinkKind
import app.marrow.core.LockState
import app.marrow.core.MarrowException
import app.marrow.data.words
import app.marrow.ui.GraphRoute
import app.marrow.ui.HistoryRoute
import app.marrow.ui.common.BackBar
import app.marrow.ui.common.Centered
import app.marrow.ui.common.ConfirmDialog
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.NameDialog
import app.marrow.ui.common.PasswordDialog
import app.marrow.ui.common.ReadingWidth
import app.marrow.ui.common.nameOf
import app.marrow.ui.common.parentOf
import app.marrow.ui.notes.MoveDialog
import app.marrow.ui.theme.LocalReading
import kotlinx.coroutines.FlowPreview
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch

/** Where photos added to notes go. */
const val ATTACHMENTS = "Attachments"

@OptIn(ExperimentalMaterial3Api::class, FlowPreview::class)
@Composable
fun NoteScreen(path: String, line: Int, startEditing: Boolean) {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    val ctx = LocalContext.current
    val uri = LocalUriHandler.current
    val scope = rememberCoroutineScope()
    val look by engine.prefs.look.collectAsStateWithLifecycle()
    val locksTick by engine.locks.collectAsStateWithLifecycle()

    var current by remember { mutableStateOf(path) }
    var loaded by remember { mutableStateOf(false) }
    var lock by remember { mutableStateOf<LockState?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    /** The text as last read from or written to disk. */
    var saved by remember { mutableStateOf("") }
    var value by remember { mutableStateOf(TextFieldValue("")) }
    var blocks by remember { mutableStateOf<List<Block>>(emptyList()) }
    var editing by remember { mutableStateOf(startEditing) }
    var starred by remember { mutableStateOf(false) }
    var menu by remember { mutableStateOf(false) }
    var info by remember { mutableStateOf(false) }
    var ai by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf(false) }
    var moving by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf(false) }
    var unlocking by remember { mutableStateOf(false) }
    var unlockError by remember { mutableStateOf<String?>(null) }
    var locking by remember { mutableStateOf(false) }
    var suggestions by remember { mutableStateOf<List<FileHit>>(emptyList()) }
    val focus = remember { FocusRequester() }
    val title = nameOf(current)

    suspend fun render(text: String) {
        blocks = runCatching { engine.run { renderNote(current, text) } }.getOrDefault(emptyList())
    }

    suspend fun reload() {
        try {
            val text = engine.run { readNote(current) }
            lock = null
            if (text != saved || !loaded) {
                saved = text
                if (value.text != text) {
                    val sel = if (line >= 0 && !loaded) TextRange(lineOffset(text, line)) else value.selection
                    value = TextFieldValue(text, TextRange(sel.start.coerceAtMost(text.length), sel.end.coerceAtMost(text.length)))
                }
                render(text)
            }
            if (!loaded) {
                editing = startEditing || look.openIn == "edit" || text.isBlank()
                starred = engine.run { vaultSettings().starred.contains(current) }
                engine.run { markSeen(current, System.currentTimeMillis()) }
            }
            loaded = true
            error = null
        } catch (e: MarrowException.Locked) {
            lock = runCatching { engine.run { lockState(current) } }.getOrNull()
            loaded = true
        } catch (e: Exception) {
            error = e.words()
            loaded = true
        }
    }

    /** Saves what's typed; merges if the note changed elsewhere meanwhile. */
    suspend fun save() {
        val text = value.text
        if (text == saved || lock != null) return
        try {
            engine.run { writeNote(current, text, saved) }
            saved = text
        } catch (e: MarrowException.ChangedElsewhere) {
            val disk = engine.run { readNote(current) }
            val merged = engine.run { mergeText(saved, text, disk) }
            engine.run { writeNote(current, merged.text, disk) }
            saved = merged.text
            value = value.copy(text = merged.text, selection = TextRange(value.selection.start.coerceAtMost(merged.text.length)))
            if (merged.conflicts > 0u) engine.say(ctx.getString(R.string.merged_with_changes))
        } catch (e: Exception) {
            engine.say(e.words())
        }
    }

    LaunchedEffect(current, locksTick) { reload() }
    // Changes from sync or other apps.
    LaunchedEffect(current) {
        engine.changes.collect { paths -> if (current in paths && value.text == saved) reload() }
    }
    // Save a moment after typing stops.
    LaunchedEffect(current) {
        snapshotFlow { value.text }.distinctUntilChanged().debounce(700).collect { save() }
    }
    // [[ suggestions while typing a link.
    LaunchedEffect(current) {
        snapshotFlow { linkQuery(value) }.distinctUntilChanged().collect { q ->
            suggestions = if (q == null) emptyList() else runCatching { engine.run { findFiles(q, current, 8u) }.filter { it.isNote } }.getOrDefault(emptyList())
        }
    }
    val saveNow by rememberUpdatedState(suspend { save() })
    DisposableEffect(current) {
        onDispose { engine.background { saveNow() } }
    }
    BackHandler(enabled = editing && !startEditing) {
        editing = false
        scope.launch {
            save()
            render(value.text)
        }
    }

    val photo = rememberLauncherForActivityResult(ActivityResultContracts.PickVisualMedia()) { picked ->
        if (picked == null) return@rememberLauncherForActivityResult
        scope.launch {
            try {
                val bytes = ctx.contentResolver.openInputStream(picked)?.use { it.readBytes() } ?: return@launch
                val type = ctx.contentResolver.getType(picked) ?: "image/jpeg"
                val ext = type.substringAfter('/').replace("jpeg", "jpg").substringBefore('+')
                val name = "Photo ${java.time.LocalDateTime.now().format(java.time.format.DateTimeFormatter.ofPattern("yyyy-MM-dd HHmmss"))}.$ext"
                val saved = engine.run { saveAttachment(ATTACHMENTS, name, bytes) }
                value = insert(value, "![[${saved.substringAfterLast('/')}]]\n")
            } catch (e: Exception) {
                engine.say(e.words())
            }
        }
    }
    val dictate = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        val said = r.data?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)?.firstOrNull()
        if (r.resultCode == Activity.RESULT_OK && !said.isNullOrBlank()) value = insert(value, said)
    }

    val barState = rememberTopAppBarState()
    val scroll = if (editing) TopAppBarDefaults.pinnedScrollBehavior(barState) else TopAppBarDefaults.enterAlwaysScrollBehavior(barState)
    Scaffold(
        modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
        containerColor = MaterialTheme.colorScheme.background,
        topBar = {
            BackBar(if (editing) title else "", scroll) {
                if (lock == null && loaded) {
                    if (editing) {
                        IconButton(onClick = {
                            editing = false
                            scope.launch {
                                save()
                                render(value.text)
                            }
                        }) { Icon(Icons.Outlined.Check, stringResource(R.string.done)) }
                    } else {
                        IconButton(onClick = { editing = true }) { Icon(Icons.Outlined.Edit, stringResource(R.string.edit)) }
                    }
                    IconButton(onClick = { info = true }) { Icon(Icons.Outlined.Info, stringResource(R.string.note_info)) }
                }
                Box {
                    IconButton(onClick = { menu = true }) { Icon(Icons.Outlined.MoreVert, stringResource(R.string.more)) }
                    DropdownMenu(menu, { menu = false }) {
                        if (engine.marrow.aiReady() && lock == null) {
                            Item(R.string.ask_ai_about_note, Icons.Outlined.AutoAwesome) { menu = false; ai = true }
                        }
                        Item(if (starred) R.string.unstar else R.string.star, if (starred) Icons.Outlined.Star else Icons.Outlined.StarBorder) {
                            menu = false
                            scope.launch {
                                engine.run { setStarred(current, !starred) }
                                starred = !starred
                            }
                        }
                        Item(R.string.share, Icons.Outlined.Share) {
                            menu = false
                            val send = Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_SUBJECT, title).putExtra(Intent.EXTRA_TEXT, value.text)
                            ctx.startActivity(Intent.createChooser(send, null))
                        }
                        Item(R.string.version_history, Icons.Outlined.History) { menu = false; nav.go(HistoryRoute(current)) }
                        Item(R.string.local_graph, Icons.Outlined.Hub) { menu = false; nav.go(GraphRoute(current)) }
                        HorizontalDivider()
                        Item(R.string.rename, Icons.Outlined.DriveFileRenameOutline) { menu = false; renaming = true }
                        Item(R.string.move_to, Icons.AutoMirrored.Outlined.DriveFileMove) { menu = false; moving = true }
                        Item(R.string.duplicate, Icons.Outlined.ContentCopy) {
                            menu = false
                            scope.launch { runCatching { engine.run { duplicate(current) } }.onSuccess { nav.note(it) }.onFailure { engine.say(it.words()) } }
                        }
                        if (lock == null) {
                            Item(R.string.lock_with_password, Icons.Outlined.Lock) { menu = false; locking = true }
                        } else if (lock?.unlocked == true) {
                            Item(R.string.remove_lock, Icons.Outlined.LockOpen) {
                                menu = false
                                scope.launch { runCatching { engine.run { removeLock(current) } }.onFailure { engine.say(it.words()) } }
                            }
                        }
                        HorizontalDivider()
                        Item(R.string.delete, Icons.Outlined.Delete, danger = true) { menu = false; deleting = true }
                    }
                }
            }
        },
        bottomBar = {
            if (editing && lock == null) {
                EditorBar(
                    value,
                    { value = it },
                    suggestions,
                    onPhoto = { photo.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) },
                    onDictate = {
                        val i = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                        runCatching { dictate.launch(i) }.onFailure { engine.say(ctx.getString(R.string.no_dictation)) }
                    },
                    modifier = Modifier.navigationBarsPadding().imePadding(),
                )
            }
        },
    ) { pad ->
        val l = lock
        when {
            !loaded -> Unit
            error != null -> Centered { Text(error!!, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(24.dp)) }
            l != null -> LockedNote(title, l) { unlocking = true }
            editing -> Box(Modifier.padding(pad).fillMaxSize(), contentAlignment = Alignment.TopCenter) {
                Column(Modifier.widthIn(max = ReadingWidth).fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 8.dp)) {
                    MarkdownEditor(
                        value,
                        { value = it },
                        Modifier.fillMaxWidth().focusRequester(focus),
                        placeholder = stringResource(R.string.start_writing),
                    )
                    Spacer(Modifier.height(240.dp))
                }
                LaunchedEffect(Unit) {
                    delay(150)
                    runCatching { focus.requestFocus() }
                }
            }
            else -> Reader(
                title = title,
                blocks = blocks,
                path = current,
                pad = pad,
                onEdit = { at ->
                    if (at >= 0) value = value.copy(selection = TextRange(lineOffset(value.text, at)))
                    editing = true
                },
                actions = BlockActions(
                    onLink = { link ->
                        when (link.kind) {
                            LinkKind.URL -> runCatching { uri.openUri(link.target) }
                            LinkKind.TAG -> nav.tag(link.target)
                            LinkKind.NOTE -> nav.openPath(link.target.substringBefore("#L"), link.target.substringAfter("#L", "-1").toIntOrNull() ?: -1)
                            LinkKind.MISSING -> scope.launch {
                                runCatching { engine.run { followLink(link.target, current) } }.onSuccess { nav.note(it, edit = true) }.onFailure { engine.say(it.words()) }
                            }
                        }
                    },
                    onToggle = { at ->
                        scope.launch {
                            try {
                                save()
                                val next = engine.run { toggleTask(current, at.toUInt()) }
                                saved = next
                                value = value.copy(text = next)
                                render(next)
                            } catch (e: Exception) {
                                engine.say(e.words())
                            }
                        }
                    },
                    onEdit = { at ->
                        value = value.copy(selection = TextRange(lineOffset(value.text, at)))
                        editing = true
                    },
                    onOpen = { target -> if (target.startsWith("http")) runCatching { uri.openUri(target) } else nav.openPath(target) },
                    fileOf = { p -> runCatching { engine.marrow.filePath(p) }.getOrNull() },
                ),
            )
        }
    }

    if (info) NoteInfoSheet(current, onDismiss = { info = false })
    if (ai) NoteAiSheet(current, value.text, onDismiss = { ai = false }, onReplace = { text ->
        value = TextFieldValue(text, TextRange(text.length))
        scope.launch {
            save()
            render(text)
        }
    }, onAppend = { add ->
        val text = value.text.trimEnd() + "\n\n" + add.trim() + "\n"
        value = TextFieldValue(text, TextRange(text.length))
        scope.launch {
            save()
            render(text)
        }
    })
    if (renaming) NameDialog(stringResource(R.string.rename), initial = title, confirm = stringResource(R.string.rename), onDismiss = { renaming = false }) { name ->
        renaming = false
        scope.launch {
            save()
            runCatching { engine.run { rename(current, name) } }
                .onSuccess {
                    current = it.newPath
                    if (it.linksUpdated > 0u) engine.say(ctx.resources.getQuantityString(R.plurals.links_updated, it.linksUpdated.toInt(), it.linksUpdated.toInt()))
                }
                .onFailure { engine.say(it.words()) }
        }
    }
    if (moving) MoveDialog(current, onDismiss = { moving = false }) { folder ->
        moving = false
        scope.launch {
            save()
            runCatching { engine.run { movePath(current, folder) } }.onSuccess { current = it.newPath }.onFailure { engine.say(it.words()) }
        }
    }
    if (deleting) ConfirmDialog(
        stringResource(R.string.delete_note_q, title),
        stringResource(R.string.delete_note_text),
        stringResource(R.string.delete),
        destructive = true,
        onDismiss = { deleting = false },
    ) {
        deleting = false
        scope.launch {
            runCatching { engine.run { trash(current) } }.onSuccess { nav.back() }.onFailure { engine.say(it.words()) }
        }
    }
    if (unlocking) PasswordDialog(stringResource(R.string.unlock_title, title), lock?.hint, unlockError, onDismiss = {
        unlocking = false
        unlockError = null
    }) { pw ->
        scope.launch {
            try {
                engine.run { unlock(current, pw) }
                unlocking = false
                unlockError = null
            } catch (e: Exception) {
                unlockError = ctx.getString(R.string.wrong_password)
            }
        }
    }
    if (locking) LockDialog(onDismiss = { locking = false }) { pw, hint ->
        locking = false
        scope.launch {
            save()
            runCatching { engine.run { lockNote(current, pw, hint) } }
                .onSuccess { warn -> engine.say(warn ?: ctx.getString(R.string.note_locked)) }
                .onFailure { engine.say(it.words()) }
        }
    }
}

@Composable
private fun Item(label: Int, icon: androidx.compose.ui.graphics.vector.ImageVector, danger: Boolean = false, onClick: () -> Unit) {
    val c = if (danger) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface
    DropdownMenuItem(
        text = { Text(stringResource(label), color = c) },
        leadingIcon = { Icon(icon, null, tint = if (danger) c else MaterialTheme.colorScheme.onSurfaceVariant) },
        onClick = onClick,
    )
}

/** A note, read: its title, then its blocks, then what links here. */
@Composable
private fun Reader(title: String, blocks: List<Block>, path: String, pad: androidx.compose.foundation.layout.PaddingValues, onEdit: (Int) -> Unit, actions: BlockActions) {
    val type = LocalReading.current
    // The first heading repeats the title: don't show it twice.
    val shown = remember(blocks, title) {
        val i = blocks.indexOfFirst { it.kind != BlockKind.PROPERTIES }
        val b = blocks.getOrNull(i)
        if (b != null && b.kind == BlockKind.HEADING && b.level.toInt() == 1 && b.spans.joinToString("") { it.text }.trim().equals(title.trim(), true)) blocks.filterIndexed { j, _ -> j != i } else blocks
    }
    Box(Modifier.padding(pad).fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        Column(
            Modifier.widthIn(max = ReadingWidth).fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 22.dp),
        ) {
            Text(title, style = type.title, color = MaterialTheme.colorScheme.onBackground, modifier = Modifier.padding(top = 4.dp, bottom = 14.dp))
            if (shown.isEmpty()) {
                Text(
                    stringResource(R.string.empty_note_tap),
                    style = type.body.copy(color = MaterialTheme.colorScheme.outline),
                    modifier = Modifier.padding(vertical = 8.dp).clickableNoRipple { onEdit(-1) },
                )
            }
            Blocks(shown, actions)
            Linked(path)
            Spacer(Modifier.height(120.dp))
        }
    }
}

/** "Linked from": notes that link here, at the end of a note. */
@Composable
private fun Linked(path: String) {
    val nav = LocalNav.current
    val d = app.marrow.ui.common.load(path) { run { noteDetails(path) } }.value ?: return
    if (d.backlinks.isEmpty()) return
    Column(Modifier.padding(top = 36.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
        Text(
            pluralStringResource(R.plurals.linked_from, d.backlinks.size, d.backlinks.size),
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(top = 10.dp),
        )
        d.backlinks.forEach { b ->
            androidx.compose.material3.Surface(
                onClick = { nav.note(b.source, b.lines.firstOrNull()?.line?.toInt() ?: -1) },
                shape = androidx.compose.foundation.shape.RoundedCornerShape(12.dp),
                color = MaterialTheme.colorScheme.surfaceContainerLow,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    Text(b.title, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
                    b.lines.take(2).forEach {
                        Text(it.text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2)
                    }
                }
            }
        }
    }
}

@Composable
private fun LockedNote(title: String, state: LockState, onUnlock: () -> Unit) {
    app.marrow.ui.common.Empty(
        Icons.Outlined.Lock,
        title,
        stringResource(if (state.folder != null) R.string.locked_folder_text else R.string.locked_note_text),
        Modifier.padding(top = 80.dp),
    ) {
        Button(onClick = onUnlock) { Text(stringResource(R.string.unlock)) }
    }
}

/** Character offset where `line` (zero-based) starts. */
fun lineOffset(text: String, line: Int): Int {
    var at = 0
    repeat(line) {
        val n = text.indexOf('\n', at)
        if (n < 0) return text.length
        at = n + 1
    }
    return at
}

@Composable
fun Modifier.clickableNoRipple(onClick: () -> Unit): Modifier =
    clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = onClick)
