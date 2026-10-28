package app.marrow.ui.common

import android.text.format.DateUtils
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
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
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.outlined.Article
import androidx.compose.material.icons.outlined.AudioFile
import androidx.compose.material.icons.outlined.Dashboard
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.Image
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.PictureAsPdf
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.TopAppBarScrollBehavior
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.ui.desk.onSecondaryClick
import app.marrow.core.FileItem
import app.marrow.core.FileKind
import app.marrow.data.Engine
import app.marrow.data.words
import java.time.LocalDate
import java.time.LocalTime
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

val LocalEngine = staticCompositionLocalOf<Engine> { error("No engine") }
val LocalNav = staticCompositionLocalOf<Nav> { error("No navigation") }
val LocalSnackbar = staticCompositionLocalOf { SnackbarHostState() }

/** Moving around the app. */
interface Nav {
    fun note(path: String, line: Int = -1, edit: Boolean = false)
    fun folder(path: String)
    fun tag(tag: String)
    fun search(query: String)
    fun open(item: FileItem)
    /** Opens a note, canvas or other file by its path. */
    fun openPath(path: String, line: Int = -1)
    fun go(route: Any)
    fun back()
}

/** Something loaded from the engine: its value, or why it failed. */
class Loaded<T>(val value: T?, val error: String?, val loading: Boolean)

/**
 * Loads with [block] and loads again when the vault changes (or [reloadOn]
 * says a change matters), or when [keys] change.
 */
@Composable
fun <T> load(vararg keys: Any?, reloadOn: (List<String>) -> Boolean = { true }, block: suspend Engine.() -> T): Loaded<T> {
    val engine = LocalEngine.current
    var tick by remember { mutableIntStateOf(0) }
    var state by remember(*keys) { mutableStateOf(Loaded<T>(null, null, true)) }
    LaunchedEffect(*keys) {
        engine.changes.collect { if (reloadOn(it)) tick++ }
    }
    LaunchedEffect(*keys, tick) {
        state = try {
            Loaded(engine.block(), null, false)
        } catch (e: Exception) {
            Loaded(state.value, e.words(), false)
        }
    }
    return state
}

/** Today in the person's time zone, as YYYY-MM-DD. */
fun today(): String = LocalDate.now().toString()

fun nowTime(): String = LocalTime.now().format(DateTimeFormatter.ofPattern("HH:mm"))

/** "Monday, 26 October" in the app's language (with the year when it isn't this year). */
fun longDate(day: String): String = runCatching {
    val d = LocalDate.parse(day)
    d.format(DateTimeFormatter.ofPattern(if (d.year == LocalDate.now().year) "EEEE, d MMMM" else "EEEE, d MMMM yyyy"))
}.getOrDefault(day)

fun mediumDate(day: String): String = runCatching {
    LocalDate.parse(day).format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM))
}.getOrDefault(day)

/** "Just now", "5 minutes ago", "Yesterday", "12 Mar". */
@Composable
fun ago(ms: Long): String = when {
    ms <= 0 -> ""
    System.currentTimeMillis() - ms < 60_000 -> stringResource(R.string.just_now)
    else -> DateUtils.getRelativeTimeSpanString(ms, System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS, DateUtils.FORMAT_ABBREV_RELATIVE).toString()
}

fun iconFor(kind: FileKind, locked: Boolean = false): ImageVector = when {
    locked -> Icons.Outlined.Lock
    else -> when (kind) {
        FileKind.FOLDER -> Icons.Outlined.Folder
        FileKind.NOTE -> Icons.Outlined.Description
        FileKind.CANVAS -> Icons.Outlined.Dashboard
        FileKind.IMAGE -> Icons.Outlined.Image
        FileKind.PDF -> Icons.Outlined.PictureAsPdf
        FileKind.AUDIO -> Icons.Outlined.AudioFile
        FileKind.OTHER -> Icons.Outlined.Article
    }
}

/** The folder part of a path: "Projects/Work" for "Projects/Work/Note.md". */
fun parentOf(path: String): String = path.substringBeforeLast('/', "")

/** The name shown for a path: "Note" for "Projects/Note.md". */
fun nameOf(path: String): String {
    val n = path.substringAfterLast('/')
    return when {
        n.endsWith(".md", true) -> n.dropLast(3)
        n.endsWith(".canvas", true) -> n.dropLast(7)
        else -> n
    }
}

/** A screen's top bar with a back arrow. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun BackBar(title: String, scroll: TopAppBarScrollBehavior? = null, actions: @Composable () -> Unit = {}) {
    val nav = LocalNav.current
    TopAppBar(
        title = { Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        navigationIcon = {
            IconButton(onClick = { nav.back() }) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) }
        },
        actions = { actions() },
        colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.background, scrolledContainerColor = MaterialTheme.colorScheme.surfaceContainer),
        scrollBehavior = scroll,
    )
}

/** A friendly message where a list is empty. */
@Composable
fun Empty(icon: ImageVector, title: String, text: String, modifier: Modifier = Modifier, action: (@Composable () -> Unit)? = null) {
    Column(
        modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 48.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(Modifier.size(64.dp).background(MaterialTheme.colorScheme.surfaceContainerHigh, CircleShape), contentAlignment = Alignment.Center) {
            Icon(icon, null, Modifier.size(30.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Text(title, style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
        Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, textAlign = TextAlign.Center, modifier = Modifier.widthIn(max = 360.dp))
        if (action != null) {
            Spacer(Modifier.height(6.dp))
            action()
        }
    }
}

@Composable
fun Centered(content: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { content() }
}

/** A small heading over a group of rows. */
@Composable
fun SectionTitle(text: String, modifier: Modifier = Modifier, trailing: (@Composable () -> Unit)? = null) {
    Row(modifier.fillMaxWidth().padding(start = 20.dp, end = 12.dp, top = 20.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(
            text,
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            fontWeight = FontWeight.SemiBold,
            modifier = Modifier.weight(1f),
        )
        trailing?.invoke()
    }
}

/** A row for a note, folder or file: icon, name, preview and when it changed. */
@Composable
@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
fun FileRow(item: FileItem, modifier: Modifier = Modifier, starred: Boolean = false, subtitle: String? = null, onLongClick: (() -> Unit)? = null, onClick: () -> Unit) {
    val isFolder = item.kind == FileKind.FOLDER
    Row(
        modifier.fillMaxWidth()
            // With a mouse, right-click does what a long press does.
            .then(if (onLongClick != null) Modifier.onSecondaryClick(onLongClick) else Modifier)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick).padding(horizontal = 20.dp, vertical = 12.dp),
        verticalAlignment = if (item.preview.isNotEmpty()) Alignment.Top else Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Box(
            Modifier.size(36.dp).background(
                if (isFolder) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainerHigh,
                RoundedCornerShape(10.dp),
            ),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                iconFor(item.kind, item.locked),
                null,
                Modifier.size(20.dp),
                tint = if (isFolder) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(item.name, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                if (starred) {
                    Text("  ★", color = MaterialTheme.colorScheme.tertiary, style = MaterialTheme.typography.labelLarge)
                }
            }
            if (item.preview.isNotEmpty()) {
                Text(
                    item.preview,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            val meta = subtitle ?: when {
                isFolder -> pluralItems(item.items.toInt())
                item.locked -> stringResource(R.string.locked)
                else -> ago(item.modifiedMs)
            }
            if (meta.isNotEmpty()) {
                Text(meta, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.outline)
            }
        }
    }
}

@Composable
fun pluralItems(n: Int): String = if (n == 1) stringResource(R.string.one_item) else stringResource(R.string.n_items, n)

/** Asks for a name (new note, new folder, rename). */
@Composable
fun NameDialog(title: String, initial: String = "", confirm: String = stringResource(R.string.ok), placeholder: String = "", onDismiss: () -> Unit, onDone: (String) -> Unit) {
    var text by remember { mutableStateOf(initial) }
    val focus = remember { FocusRequester() }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            OutlinedTextField(
                text,
                { text = it },
                singleLine = true,
                placeholder = { Text(placeholder) },
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                modifier = Modifier.fillMaxWidth().focusRequester(focus),
            )
            LaunchedEffect(Unit) { focus.requestFocus() }
        },
        confirmButton = { TextButton(onClick = { onDone(text.trim()) }, enabled = text.isNotBlank()) { Text(confirm) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

/** Asks for a note's or folder's password. */
@Composable
fun PasswordDialog(title: String, hint: String?, error: String?, onDismiss: () -> Unit, onDone: (String) -> Unit) {
    var text by remember { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    AlertDialog(
        onDismissRequest = onDismiss,
        icon = { Icon(Icons.Outlined.Lock, null) },
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    text,
                    { text = it },
                    singleLine = true,
                    label = { Text(stringResource(R.string.password)) },
                    visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = ImeAction.Done),
                    isError = error != null,
                    supportingText = { if (error != null) Text(error) else if (!hint.isNullOrBlank()) Text(stringResource(R.string.hint_is, hint)) },
                    modifier = Modifier.fillMaxWidth().focusRequester(focus),
                )
                LaunchedEffect(Unit) { focus.requestFocus() }
            }
        },
        confirmButton = { TextButton(onClick = { onDone(text) }, enabled = text.isNotEmpty()) { Text(stringResource(R.string.unlock)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

@Composable
fun ConfirmDialog(title: String, text: String, confirm: String, destructive: Boolean = false, onDismiss: () -> Unit, onConfirm: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = { Text(text) },
        confirmButton = {
            TextButton(onClick = onConfirm) {
                Text(confirm, color = if (destructive) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary)
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

/** Reading width on tablets: text lines stay comfortable. */
val ReadingWidth = 720.dp

val ListPadding = PaddingValues(bottom = 96.dp)
