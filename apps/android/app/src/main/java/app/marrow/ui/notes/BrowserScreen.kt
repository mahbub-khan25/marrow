package app.marrow.ui.notes

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.DriveFileMove
import androidx.compose.material.icons.automirrored.outlined.Sort
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.CreateNewFolder
import androidx.compose.material.icons.outlined.Dashboard
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.DriveFileRenameOutline
import androidx.compose.material.icons.outlined.EditNote
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.Hub
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.MoreVert
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.Star
import androidx.compose.material.icons.outlined.StarBorder
import androidx.compose.material.icons.outlined.Tag
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LargeTopAppBar
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.rememberTopAppBarState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.marrow.R
import app.marrow.core.FileItem
import app.marrow.core.FileKind
import app.marrow.data.words
import app.marrow.ui.CalendarRoute
import app.marrow.ui.GraphRoute
import app.marrow.ui.SettingsRoute
import app.marrow.ui.TimeMachineRoute
import app.marrow.ui.common.ConfirmDialog
import app.marrow.ui.common.Empty
import app.marrow.ui.common.FileRow
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.NameDialog
import app.marrow.ui.common.SectionTitle
import app.marrow.ui.common.load
import app.marrow.ui.common.nameOf
import app.marrow.ui.common.nowTime
import app.marrow.ui.common.parentOf
import app.marrow.ui.common.today
import app.marrow.ui.note.LockDialog
import kotlinx.coroutines.launch

/** A folder's notes and folders ("" for the top of the vault). */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalFoundationApi::class)
@Composable
fun BrowserScreen(folder: String) {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val look by engine.prefs.look.collectAsStateWithLifecycle()
    val vault by engine.vault.collectAsStateWithLifecycle()
    val items = load(folder) { run { listFolder(folder) } }
    val starredList = load(folder) { if (folder.isEmpty()) run { starred() } else emptyList() }.value.orEmpty()
    val starredPaths = remember(starredList) { starredList.map { it.path }.toSet() }
    val templates = load(folder) { runCatching { run { templates() } }.getOrDefault(emptyList()) }.value.orEmpty()

    var newMenu by remember { mutableStateOf(false) }
    var sortMenu by remember { mutableStateOf(false) }
    var more by remember { mutableStateOf(false) }
    var asking by remember { mutableStateOf<String?>(null) } // "note", "folder", "canvas"
    var pickTemplate by remember { mutableStateOf(false) }
    var acting by remember { mutableStateOf<FileItem?>(null) }
    var renaming by remember { mutableStateOf<FileItem?>(null) }
    var moving by remember { mutableStateOf<FileItem?>(null) }
    var deleting by remember { mutableStateOf<FileItem?>(null) }
    var locking by remember { mutableStateOf<FileItem?>(null) }

    val title = if (folder.isEmpty()) vault?.name ?: stringResource(R.string.notes) else nameOf(folder)
    val sorted = remember(items.value, look.sort) {
        val list = items.value.orEmpty()
        when (look.sort) {
            "modified" -> list.sortedWith(compareBy<FileItem> { it.kind != FileKind.FOLDER }.thenByDescending { it.modifiedMs })
            else -> list
        }
    }
    val scroll = TopAppBarDefaults.exitUntilCollapsedScrollBehavior(rememberTopAppBarState())

    fun fail(e: Throwable) = engine.say(e.words())

    Scaffold(
        modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
        containerColor = MaterialTheme.colorScheme.background,
        topBar = {
            LargeTopAppBar(
                title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                navigationIcon = {
                    if (folder.isNotEmpty()) {
                        IconButton(onClick = { nav.back() }) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) }
                    }
                },
                actions = {
                    Box {
                        IconButton(onClick = { sortMenu = true }) { Icon(Icons.AutoMirrored.Outlined.Sort, stringResource(R.string.sort)) }
                        DropdownMenu(sortMenu, { sortMenu = false }) {
                            listOf("name" to R.string.sort_name, "modified" to R.string.sort_modified).forEach { (key, label) ->
                                DropdownMenuItem(
                                    text = { Text(stringResource(label)) },
                                    trailingIcon = { if (look.sort == key) Text("✓") },
                                    onClick = {
                                        sortMenu = false
                                        engine.prefs.update { it.copy(sort = key) }
                                    },
                                )
                            }
                        }
                    }
                    if (folder.isEmpty()) {
                        Box {
                            IconButton(onClick = { more = true }) { Icon(Icons.Outlined.MoreVert, stringResource(R.string.more)) }
                            DropdownMenu(more, { more = false }) {
                                DropdownMenuItem(text = { Text(stringResource(R.string.calendar)) }, leadingIcon = { Icon(Icons.Outlined.CalendarMonth, null) }, onClick = { more = false; nav.go(CalendarRoute) })
                                DropdownMenuItem(text = { Text(stringResource(R.string.graph)) }, leadingIcon = { Icon(Icons.Outlined.Hub, null) }, onClick = { more = false; nav.go(GraphRoute()) })
                                DropdownMenuItem(text = { Text(stringResource(R.string.time_machine)) }, leadingIcon = { Icon(Icons.Outlined.History, null) }, onClick = { more = false; nav.go(TimeMachineRoute) })
                                HorizontalDivider()
                                DropdownMenuItem(text = { Text(stringResource(R.string.settings)) }, leadingIcon = { Icon(Icons.Outlined.Settings, null) }, onClick = { more = false; nav.go(SettingsRoute) })
                            }
                        }
                    }
                },
                colors = TopAppBarDefaults.largeTopAppBarColors(containerColor = MaterialTheme.colorScheme.background, scrolledContainerColor = MaterialTheme.colorScheme.surfaceContainer),
                scrollBehavior = scroll,
            )
        },
        floatingActionButton = {
            Box {
                ExtendedFloatingActionButton(
                    onClick = { newMenu = true },
                    icon = { Icon(Icons.Outlined.EditNote, null) },
                    text = { Text(stringResource(R.string.new_)) },
                )
                DropdownMenu(newMenu, { newMenu = false }) {
                    DropdownMenuItem(text = { Text(stringResource(R.string.new_note)) }, leadingIcon = { Icon(Icons.Outlined.Description, null) }, onClick = { newMenu = false; asking = "note" })
                    if (templates.isNotEmpty()) {
                        DropdownMenuItem(text = { Text(stringResource(R.string.from_template)) }, leadingIcon = { Icon(Icons.Outlined.EditNote, null) }, onClick = { newMenu = false; pickTemplate = true })
                    }
                    DropdownMenuItem(text = { Text(stringResource(R.string.new_canvas)) }, leadingIcon = { Icon(Icons.Outlined.Dashboard, null) }, onClick = { newMenu = false; asking = "canvas" })
                    DropdownMenuItem(text = { Text(stringResource(R.string.new_folder)) }, leadingIcon = { Icon(Icons.Outlined.CreateNewFolder, null) }, onClick = { newMenu = false; asking = "folder" })
                }
            }
        },
    ) { pad ->
        Box(Modifier.padding(pad).fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            LazyColumn(Modifier.widthIn(max = 840.dp).fillMaxSize(), contentPadding = PaddingValues(bottom = 110.dp)) {
                if (folder.isEmpty()) {
                    item { Shortcuts() }
                    if (starredList.isNotEmpty()) {
                        item { SectionTitle(stringResource(R.string.starred)) }
                        item {
                            LazyRow(contentPadding = PaddingValues(horizontal = 20.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                                items(starredList, key = { it.path }) { s -> StarCard(s) { nav.open(s) } }
                            }
                        }
                    }
                    item { SectionTitle(stringResource(R.string.all_notes)) }
                }
                if (items.value?.isEmpty() == true) {
                    item {
                        Empty(Icons.Outlined.Folder, stringResource(R.string.empty_folder), stringResource(R.string.empty_folder_text))
                    }
                }
                items(sorted, key = { it.path }) { item ->
                    Box {
                        FileRow(item, starred = item.path in starredPaths, onLongClick = { acting = item }) { nav.open(item) }
                        DropdownMenu(acting == item, { acting = null }) {
                            if (item.kind != FileKind.FOLDER) {
                                val isStar = item.path in starredPaths
                                DropdownMenuItem(
                                    text = { Text(stringResource(if (isStar) R.string.unstar else R.string.star)) },
                                    leadingIcon = { Icon(if (isStar) Icons.Outlined.Star else Icons.Outlined.StarBorder, null) },
                                    onClick = {
                                        acting = null
                                        scope.launch { runCatching { engine.run { setStarred(item.path, !isStar) } }.onFailure(::fail) }
                                    },
                                )
                            }
                            DropdownMenuItem(text = { Text(stringResource(R.string.rename)) }, leadingIcon = { Icon(Icons.Outlined.DriveFileRenameOutline, null) }, onClick = { acting = null; renaming = item })
                            DropdownMenuItem(text = { Text(stringResource(R.string.move_to)) }, leadingIcon = { Icon(Icons.AutoMirrored.Outlined.DriveFileMove, null) }, onClick = { acting = null; moving = item })
                            if (item.kind == FileKind.FOLDER || (item.kind == FileKind.NOTE && !item.locked)) {
                                DropdownMenuItem(text = { Text(stringResource(R.string.lock_with_password)) }, leadingIcon = { Icon(Icons.Outlined.Lock, null) }, onClick = { acting = null; locking = item })
                            }
                            HorizontalDivider()
                            DropdownMenuItem(
                                text = { Text(stringResource(R.string.delete), color = MaterialTheme.colorScheme.error) },
                                leadingIcon = { Icon(Icons.Outlined.Delete, null, tint = MaterialTheme.colorScheme.error) },
                                onClick = { acting = null; deleting = item },
                            )
                        }
                    }
                }
            }
        }
    }

    when (asking) {
        "note" -> NameDialog(stringResource(R.string.new_note), placeholder = stringResource(R.string.untitled), confirm = stringResource(R.string.create), onDismiss = { asking = null }) { name ->
            asking = null
            scope.launch { runCatching { engine.run { createNote(folder, name, "") } }.onSuccess { nav.note(it, edit = true) }.onFailure(::fail) }
        }
        "canvas" -> NameDialog(stringResource(R.string.new_canvas), placeholder = stringResource(R.string.untitled), confirm = stringResource(R.string.create), onDismiss = { asking = null }) { name ->
            asking = null
            scope.launch { runCatching { engine.run { createCanvas(folder, name) } }.onSuccess { nav.openPath(it) }.onFailure(::fail) }
        }
        "folder" -> NameDialog(stringResource(R.string.new_folder), confirm = stringResource(R.string.create), onDismiss = { asking = null }) { name ->
            asking = null
            scope.launch { runCatching { engine.run { createFolder(folder, name) } }.onFailure(::fail) }
        }
    }
    if (pickTemplate) {
        AlertDialog(
            onDismissRequest = { pickTemplate = false },
            title = { Text(stringResource(R.string.from_template)) },
            text = {
                LazyColumn(Modifier.heightIn(max = 360.dp)) {
                    items(templates) { t ->
                        ListItem(
                            headlineContent = { Text(t.name) },
                            leadingContent = { Icon(Icons.Outlined.Description, null) },
                            colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh),
                            modifier = Modifier.clickable {
                                pickTemplate = false
                                scope.launch {
                                    runCatching {
                                        val name = ctx.getString(R.string.untitled)
                                        val text = engine.run { templateText(t.path, name, today(), nowTime()) }
                                        engine.run { createNote(folder, name, text) }
                                    }.onSuccess { nav.note(it, edit = true) }.onFailure(::fail)
                                }
                            },
                        )
                    }
                }
            },
            confirmButton = { TextButton(onClick = { pickTemplate = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
    renaming?.let { item ->
        NameDialog(stringResource(R.string.rename), initial = item.name, confirm = stringResource(R.string.rename), onDismiss = { renaming = null }) { name ->
            renaming = null
            scope.launch { runCatching { engine.run { rename(item.path, name) } }.onFailure(::fail) }
        }
    }
    moving?.let { item ->
        MoveDialog(item.path, onDismiss = { moving = null }) { to ->
            moving = null
            scope.launch { runCatching { engine.run { movePath(item.path, to) } }.onFailure(::fail) }
        }
    }
    deleting?.let { item ->
        ConfirmDialog(
            stringResource(R.string.delete_note_q, item.name),
            stringResource(if (item.kind == FileKind.FOLDER) R.string.delete_folder_text else R.string.delete_note_text),
            stringResource(R.string.delete),
            destructive = true,
            onDismiss = { deleting = null },
        ) {
            deleting = null
            scope.launch { runCatching { engine.run { trash(item.path) } }.onFailure(::fail) }
        }
    }
    locking?.let { item ->
        LockDialog(folder = item.kind == FileKind.FOLDER, onDismiss = { locking = null }) { pw, hint ->
            locking = null
            scope.launch {
                runCatching {
                    if (item.kind == FileKind.FOLDER) {
                        val n = engine.run { lockFolder(item.path, pw, hint) }
                        ctx.resources.getQuantityString(R.plurals.notes_locked, n.toInt(), n.toInt())
                    } else {
                        engine.run { lockNote(item.path, pw, hint) } ?: ctx.getString(R.string.note_locked)
                    }
                }.onSuccess { engine.say(it) }.onFailure(::fail)
            }
        }
    }
}

/** Quick ways in at the top of the vault. */
@Composable
private fun Shortcuts() {
    val nav = LocalNav.current
    LazyRow(contentPadding = PaddingValues(horizontal = 20.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        item { Shortcut(Icons.Outlined.CalendarMonth, stringResource(R.string.calendar)) { nav.go(CalendarRoute) } }
        item { Shortcut(Icons.Outlined.Tag, stringResource(R.string.tags)) { nav.search("#") } }
        item { Shortcut(Icons.Outlined.Hub, stringResource(R.string.graph)) { nav.go(GraphRoute()) } }
        item { Shortcut(Icons.Outlined.History, stringResource(R.string.time_machine)) { nav.go(TimeMachineRoute) } }
    }
}

@Composable
private fun Shortcut(icon: androidx.compose.ui.graphics.vector.ImageVector, label: String, onClick: () -> Unit) {
    Surface(onClick = onClick, shape = RoundedCornerShape(50), color = MaterialTheme.colorScheme.surfaceContainerHigh) {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 9.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Icon(icon, null, Modifier.size(18.dp), tint = MaterialTheme.colorScheme.primary)
            Text(label, style = MaterialTheme.typography.labelLarge)
        }
    }
}

@Composable
private fun StarCard(item: FileItem, onClick: () -> Unit) {
    Surface(onClick = onClick, shape = RoundedCornerShape(16.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.size(width = 168.dp, height = 112.dp)) {
        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(item.name, style = MaterialTheme.typography.titleSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(item.preview, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 3, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** Picks a folder to move something into. */
@Composable
fun MoveDialog(path: String, onDismiss: () -> Unit, onPick: (String) -> Unit) {
    val folders = load(path) { run { allFolders() } }.value.orEmpty().filter { it != path && !it.startsWith("$path/") && it != parentOf(path) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.move_name_to, nameOf(path))) },
        text = {
            LazyColumn(Modifier.heightIn(max = 420.dp)) {
                items(folders) { f ->
                    ListItem(
                        headlineContent = { Text(if (f.isEmpty()) stringResource(R.string.top_of_vault) else nameOf(f)) },
                        supportingContent = { if (f.contains('/')) Text(parentOf(f)) },
                        leadingContent = { Icon(Icons.Outlined.Folder, null) },
                        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh),
                        modifier = Modifier.fillMaxWidth().clickable { onPick(f) },
                    )
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

/** Notes with a tag. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TagScreen(tag: String) {
    val nav = LocalNav.current
    val notes = load(tag) { run { notesWithTag(tag) } }
    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { app.marrow.ui.common.BackBar(tag) }) { pad ->
        LazyColumn(Modifier.padding(pad).fillMaxSize(), contentPadding = PaddingValues(bottom = 40.dp)) {
            items(notes.value.orEmpty(), key = { it.path }) { FileRow(it) { nav.open(it) } }
            if (notes.value?.isEmpty() == true) item { Empty(Icons.Outlined.Tag, tag, stringResource(R.string.no_notes_with_tag)) }
        }
    }
}
