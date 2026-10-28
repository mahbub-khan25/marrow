package app.marrow.ui.desk

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.DriveFileMove
import androidx.compose.material.icons.automirrored.outlined.MenuOpen
import androidx.compose.material.icons.outlined.AutoAwesome
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.DriveFileRenameOutline
import androidx.compose.material.icons.outlined.EditNote
import androidx.compose.material.icons.outlined.ExpandMore
import androidx.compose.material.icons.outlined.Hub
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.WbSunny
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshots.SnapshotStateMap
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.isSecondaryPressed
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.marrow.R
import app.marrow.core.FileItem
import app.marrow.core.FileKind
import app.marrow.data.Engine
import app.marrow.data.words
import app.marrow.ui.common.ConfirmDialog
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.NameDialog
import app.marrow.ui.common.iconFor
import app.marrow.ui.notes.MoveDialog
import kotlinx.coroutines.launch

/** Windows at least this wide get the desktop layout: a sidebar beside the page. */
val DESK_WIDTH = 840.dp

/** The sidebar's places, besides the notes themselves. */
enum class Place { Today, Tasks, Ask, Search, Calendar, Graph, Settings }

/**
 * Right-click (a mouse's secondary button) runs [action], like a long press
 * does with a finger.
 */
fun Modifier.onSecondaryClick(action: () -> Unit): Modifier = pointerInput(action) {
    awaitPointerEventScope {
        while (true) {
            val e = awaitPointerEvent()
            if (e.type == PointerEventType.Press && e.buttons.isSecondaryPressed) {
                e.changes.forEach { it.consume() }
                action()
            }
        }
    }
}

/**
 * The desktop layout's sidebar: the vault, the main places, and every
 * folder and note as a tree. Right-click (or long-press) a note for more.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun Sidebar(
    current: Place?,
    currentPath: String?,
    onPlace: (Place) -> Unit,
    onNewNote: () -> Unit,
    onHide: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val engine = LocalEngine.current
    val vault by engine.vault.collectAsStateWithLifecycle()
    // Folder path → its items, for the folders that are open.
    val open = remember { mutableStateMapOf<String, List<FileItem>>() }
    var expanded by rememberSaveable { mutableStateOf(setOf("")) }
    var tick by remember { mutableStateOf(0) }
    LaunchedEffect(Unit) { engine.changes.collect { tick++ } }
    LaunchedEffect(expanded, tick) {
        for (path in expanded) {
            runCatching { engine.run { listFolder(path) } }.onSuccess { open[path] = it }
        }
        open.keys.retainAll(expanded)
    }

    Surface(color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = modifier.fillMaxHeight()) {
        Column(Modifier.windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Start + WindowInsetsSides.Vertical))) {
            Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    vault?.name ?: stringResource(R.string.notes),
                    style = MaterialTheme.typography.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                IconButton(onClick = onNewNote) { Icon(Icons.Outlined.EditNote, stringResource(R.string.new_note)) }
                IconButton(onClick = onHide) { Icon(Icons.AutoMirrored.Outlined.MenuOpen, stringResource(R.string.hide_sidebar)) }
            }
            LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(horizontal = 8.dp, vertical = 4.dp)) {
                item { PlaceRow(Icons.Outlined.Search, stringResource(R.string.search), current == Place.Search, shortcut = "Ctrl K") { onPlace(Place.Search) } }
                item { PlaceRow(Icons.Outlined.WbSunny, stringResource(R.string.today), current == Place.Today, shortcut = "Ctrl D") { onPlace(Place.Today) } }
                item { PlaceRow(Icons.Outlined.CheckCircle, stringResource(R.string.tasks), current == Place.Tasks) { onPlace(Place.Tasks) } }
                item { PlaceRow(Icons.Outlined.AutoAwesome, stringResource(R.string.ask), current == Place.Ask) { onPlace(Place.Ask) } }
                item { PlaceRow(Icons.Outlined.CalendarMonth, stringResource(R.string.calendar), current == Place.Calendar) { onPlace(Place.Calendar) } }
                item { PlaceRow(Icons.Outlined.Hub, stringResource(R.string.graph), current == Place.Graph) { onPlace(Place.Graph) } }
                item { HorizontalDivider(Modifier.padding(vertical = 8.dp, horizontal = 8.dp)) }
                tree(
                    folder = "",
                    depth = 0,
                    open = open,
                    expanded = expanded,
                    currentPath = currentPath,
                    onToggle = { p -> expanded = if (p in expanded) expanded - p else expanded + p },
                )
            }
            HorizontalDivider()
            Box(Modifier.padding(8.dp)) {
                PlaceRow(Icons.Outlined.Settings, stringResource(R.string.settings), current == Place.Settings, shortcut = "Ctrl ,") { onPlace(Place.Settings) }
            }
        }
    }
}

private fun LazyListScope.tree(
    folder: String,
    depth: Int,
    open: SnapshotStateMap<String, List<FileItem>>,
    expanded: Set<String>,
    currentPath: String?,
    onToggle: (String) -> Unit,
) {
    val items = open[folder] ?: return
    for (item in items) {
        item(key = "tree:" + item.path) {
            TreeRow(item, depth, item.kind == FileKind.FOLDER && item.path in expanded, item.path == currentPath) { onToggle(item.path) }
        }
        if (item.kind == FileKind.FOLDER && item.path in expanded) {
            tree(item.path, depth + 1, open, expanded, currentPath, onToggle)
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun PlaceRow(icon: ImageVector, label: String, selected: Boolean, shortcut: String? = null, onClick: () -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .background(if (selected) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent, RoundedCornerShape(10.dp))
            .combinedClickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 9.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Icon(icon, null, Modifier.size(20.dp), tint = if (selected) MaterialTheme.colorScheme.onSecondaryContainer else MaterialTheme.colorScheme.onSurfaceVariant)
        Text(label, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f), maxLines = 1)
        if (shortcut != null) Text(shortcut, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.outline)
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun TreeRow(item: FileItem, depth: Int, expanded: Boolean, selected: Boolean, onToggle: () -> Unit) {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    val scope = rememberCoroutineScope()
    var menu by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf(false) }
    var moving by remember { mutableStateOf(false) }
    var deleting by remember { mutableStateOf(false) }
    val isFolder = item.kind == FileKind.FOLDER
    fun fail(e: Throwable) = engine.say(e.words())

    Box {
        Row(
            Modifier
                .fillMaxWidth()
                .background(if (selected) MaterialTheme.colorScheme.secondaryContainer else Color.Transparent, RoundedCornerShape(8.dp))
                .onSecondaryClick { menu = true }
                .combinedClickable(onClick = { if (isFolder) onToggle() else nav.open(item) }, onLongClick = { menu = true })
                .padding(start = (8 + depth * 16).dp, end = 8.dp, top = 6.dp, bottom = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            if (isFolder) {
                Icon(if (expanded) Icons.Outlined.ExpandMore else Icons.Outlined.ChevronRight, null, Modifier.size(18.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
            } else {
                Spacer(Modifier.width(18.dp))
            }
            Icon(iconFor(item.kind, item.locked), null, Modifier.size(18.dp), tint = if (isFolder) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant)
            Text(
                item.name,
                style = MaterialTheme.typography.bodyMedium,
                color = if (selected) MaterialTheme.colorScheme.onSecondaryContainer else MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        DropdownMenu(menu, { menu = false }) {
            if (!isFolder) {
                DropdownMenuItem(text = { Text(stringResource(R.string.open)) }, onClick = { menu = false; nav.open(item) })
            }
            DropdownMenuItem(text = { Text(stringResource(R.string.rename)) }, leadingIcon = { Icon(Icons.Outlined.DriveFileRenameOutline, null) }, onClick = { menu = false; renaming = true })
            DropdownMenuItem(text = { Text(stringResource(R.string.move_to)) }, leadingIcon = { Icon(Icons.AutoMirrored.Outlined.DriveFileMove, null) }, onClick = { menu = false; moving = true })
            DropdownMenuItem(
                text = { Text(stringResource(R.string.delete), color = MaterialTheme.colorScheme.error) },
                leadingIcon = { Icon(Icons.Outlined.Delete, null, tint = MaterialTheme.colorScheme.error) },
                onClick = { menu = false; deleting = true },
            )
        }
    }
    if (renaming) {
        NameDialog(stringResource(R.string.rename), initial = item.name, confirm = stringResource(R.string.rename), onDismiss = { renaming = false }) { name ->
            renaming = false
            scope.launch { runCatching { engine.run { rename(item.path, name) } }.onFailure(::fail) }
        }
    }
    if (moving) {
        MoveDialog(item.path, onDismiss = { moving = false }) { to ->
            moving = false
            scope.launch { runCatching { engine.run { movePath(item.path, to) } }.onFailure(::fail) }
        }
    }
    if (deleting) {
        ConfirmDialog(
            stringResource(R.string.delete_note_q, item.name),
            stringResource(if (isFolder) R.string.delete_folder_text else R.string.delete_note_text),
            stringResource(R.string.delete),
            destructive = true,
            onDismiss = { deleting = false },
        ) {
            deleting = false
            scope.launch { runCatching { engine.run { trash(item.path) } }.onFailure(::fail) }
        }
    }
}

/** Opens today's note (from the Ctrl D shortcut and the sidebar). */
suspend fun Engine.todayPath(): String? = runCatching { run { openDay(app.marrow.ui.common.today()) } }.getOrNull()
