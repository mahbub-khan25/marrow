package app.marrow.ui.today

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowForward
import androidx.compose.material.icons.outlined.AddTask
import androidx.compose.material.icons.outlined.Archive
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.Cloud
import androidx.compose.material.icons.outlined.CloudOff
import androidx.compose.material.icons.outlined.CloudSync
import androidx.compose.material.icons.outlined.EditNote
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.Mic
import androidx.compose.material.icons.outlined.PhotoCamera
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.WbSunny
import androidx.compose.material3.Checkbox
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.marrow.R
import app.marrow.core.SyncProblem
import app.marrow.core.TaskItem
import app.marrow.data.words
import app.marrow.ui.CalendarRoute
import app.marrow.ui.SettingsRoute
import app.marrow.ui.SyncRoute
import app.marrow.ui.TasksRoute
import app.marrow.ui.common.FileRow
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.SectionTitle
import app.marrow.ui.common.ago
import app.marrow.ui.common.load
import app.marrow.ui.common.today
import app.marrow.ui.theme.LocalReading
import kotlinx.coroutines.launch
import java.time.LocalDate
import java.time.format.TextStyle
import java.util.Locale

@Composable
fun TodayScreen(onCapture: () -> Unit) {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    val scope = rememberCoroutineScope()
    val day = today()
    val date = LocalDate.parse(day)
    var offset by remember { mutableIntStateOf(0) }
    val todayNote = load(day) { run { dailyNote(day)?.let { p -> p to runCatching { readNote(p) }.getOrDefault("") } } }.value
    val tasks = load(day) { run { tasks(false) } }.value.orEmpty()
    val due = remember(tasks, day) { tasks.filter { it.due != null && it.due!! <= day }.sortedBy { it.due } }
    val week = load(day) { run { daysWithNotes(day.substring(0, 7)) + daysWithNotes(date.minusDays(6).toString().substring(0, 7)) } }.value.orEmpty()
    val resurfaced = load(day, offset) { run { resurface(day, System.currentTimeMillis(), offset.toUInt()) } }.value
    val recent = load(day) { run { recentNotes(6u) } }.value.orEmpty()

    fun openDayNote(d: String) = scope.launch {
        runCatching { engine.run { openDay(d) } }.onSuccess { nav.note(it, edit = true) }.onFailure { engine.say(it.words()) }
    }

    LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = 32.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        item { Header(date) }
        item {
            Column(Modifier.widthIn(max = 720.dp).padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                CaptureCard(onCapture)
                TodayCard(todayNote) { openDayNote(day) }
                Week(date, week.map { it.toInt() }.toSet()) { openDayNote(it) }
            }
        }
        if (due.isNotEmpty()) {
            item { Wide { SectionTitle(stringResource(R.string.due_today_and_overdue)) { TextButton(onClick = { nav.go(TasksRoute) }) { Text(stringResource(R.string.see_all)) } } } }
            items(due.take(6), key = { "${it.path}:${it.line}" }) { t ->
                Wide { TaskLine(t, day) }
            }
        }
        val r = resurfaced
        if (r != null && (r.forgotten != null || r.onThisDay.isNotEmpty())) {
            item { Wide { SectionTitle(stringResource(R.string.worth_another_look)) } }
            r.forgotten?.let { f ->
                item {
                    Wide {
                        Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.padding(horizontal = 16.dp).fillMaxWidth()) {
                            Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                Label(Icons.Outlined.Archive, stringResource(R.string.from_your_archive))
                                Text(f.title, style = MaterialTheme.typography.titleLarge, modifier = Modifier.clickable { nav.note(f.path) })
                                Text(stringResource(R.string.not_opened_since, ago(f.lastMs)), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.outline)
                                if (f.preview.isNotEmpty()) {
                                    Text(f.preview, style = LocalReading.current.body.copy(fontSize = LocalReading.current.body.fontSize * 0.9f), color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 4, overflow = TextOverflow.Ellipsis)
                                }
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    OutlinedButton(onClick = { nav.note(f.path) }) { Text(stringResource(R.string.open)) }
                                    if (r.forgottenCount > 1u) TextButton(onClick = { offset++ }) { Text(stringResource(R.string.another)) }
                                    TextButton(onClick = { scope.launch { engine.run { skipResurfacing(f.path) }; offset++ } }) { Text(stringResource(R.string.not_this_one)) }
                                }
                            }
                        }
                    }
                }
            }
            if (r.onThisDay.isNotEmpty()) {
                item {
                    Wide {
                        Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp).fillMaxWidth()) {
                            Column(Modifier.padding(vertical = 14.dp)) {
                                Box(Modifier.padding(horizontal = 18.dp)) { Label(Icons.Outlined.History, stringResource(R.string.on_this_day)) }
                                r.onThisDay.forEach { o ->
                                    Column(Modifier.fillMaxWidth().clickable { nav.note(o.path) }.padding(horizontal = 18.dp, vertical = 10.dp)) {
                                        Text(pluralStringResource(R.plurals.years_ago, o.yearsAgo.toInt(), o.yearsAgo.toInt()), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                                        Text(o.preview.ifEmpty { o.day }, style = MaterialTheme.typography.bodyMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if (recent.isNotEmpty()) {
            item { Wide { SectionTitle(stringResource(R.string.recent)) } }
            items(recent, key = { "r:" + it.path }) { f -> Wide { FileRow(f) { nav.open(f) } } }
        }
    }
}

@Composable
private fun Wide(content: @Composable () -> Unit) {
    Box(Modifier.widthIn(max = 720.dp).fillMaxWidth()) { content() }
}

@Composable
private fun Header(date: LocalDate) {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    val sync by engine.sync.collectAsStateWithLifecycle()
    val locale = Locale.getDefault()
    Row(Modifier.widthIn(max = 720.dp).fillMaxWidth().statusBarsPadding().padding(start = 24.dp, end = 8.dp, top = 18.dp, bottom = 16.dp), verticalAlignment = Alignment.Top) {
        Column(Modifier.weight(1f)) {
            Text(
                date.dayOfWeek.getDisplayName(TextStyle.FULL, locale).uppercase(locale),
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.primary,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                "${date.dayOfMonth} ${date.month.getDisplayName(TextStyle.FULL, locale)}",
                style = MaterialTheme.typography.displaySmall,
            )
        }
        val s = sync
        if (s != null) {
            IconButton(onClick = { nav.go(SyncRoute) }) {
                Icon(
                    when {
                        s.syncing -> Icons.Outlined.CloudSync
                        s.problem != null && s.problem != SyncProblem.OFFLINE -> Icons.Outlined.CloudOff
                        else -> Icons.Outlined.Cloud
                    },
                    stringResource(R.string.sync),
                    tint = if (s.problem != null && s.problem != SyncProblem.OFFLINE) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        IconButton(onClick = { nav.go(SettingsRoute) }) { Icon(Icons.Outlined.Settings, stringResource(R.string.settings)) }
    }
}

@Composable
private fun CaptureCard(onCapture: () -> Unit) {
    Surface(onClick = onCapture, shape = RoundedCornerShape(24.dp), color = MaterialTheme.colorScheme.surfaceContainerHigh, modifier = Modifier.fillMaxWidth()) {
        Row(Modifier.padding(start = 20.dp, end = 8.dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Outlined.EditNote, null, tint = MaterialTheme.colorScheme.primary)
            Text(stringResource(R.string.whats_on_your_mind), style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f).padding(start = 14.dp))
            IconButton(onClick = onCapture) { Icon(Icons.Outlined.AddTask, stringResource(R.string.add_task), tint = MaterialTheme.colorScheme.onSurfaceVariant) }
            IconButton(onClick = onCapture) { Icon(Icons.Outlined.PhotoCamera, stringResource(R.string.add_photo), tint = MaterialTheme.colorScheme.onSurfaceVariant) }
            IconButton(onClick = onCapture) { Icon(Icons.Outlined.Mic, stringResource(R.string.dictate), tint = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
    }
}

@Composable
private fun TodayCard(note: Pair<String, String>?, onOpen: () -> Unit) {
    val body = note?.second?.let { app.marrow.core.preview(it, 140u) }.orEmpty()
    Surface(onClick = onOpen, shape = RoundedCornerShape(24.dp), color = MaterialTheme.colorScheme.primaryContainer, modifier = Modifier.fillMaxWidth()) {
        Row(Modifier.padding(20.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(44.dp).background(MaterialTheme.colorScheme.primary, CircleShape), contentAlignment = Alignment.Center) {
                Icon(Icons.Outlined.WbSunny, null, tint = MaterialTheme.colorScheme.onPrimary)
            }
            Column(Modifier.weight(1f).padding(horizontal = 16.dp)) {
                Text(stringResource(if (note == null) R.string.start_todays_note else R.string.todays_note), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onPrimaryContainer)
                Text(
                    body.ifEmpty { stringResource(R.string.todays_note_hint) },
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.8f),
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            Icon(Icons.AutoMirrored.Outlined.ArrowForward, null, tint = MaterialTheme.colorScheme.onPrimaryContainer)
        }
    }
}

/** This week: seven days, a dot under days with a note. */
@Composable
private fun Week(today: LocalDate, withNotes: Set<Int>, onDay: (String) -> Unit) {
    val nav = LocalNav.current
    val locale = Locale.getDefault()
    val days = (6 downTo 0).map { today.minusDays(it.toLong()) }
    Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth()) {
        Row(Modifier.padding(horizontal = 8.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            days.forEach { d ->
                val isToday = d == today
                val has = d.dayOfMonth in withNotes && (d.month == today.month || d.dayOfMonth > 7)
                Column(
                    Modifier.weight(1f).clickable { onDay(d.toString()) }.padding(vertical = 4.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    Text(d.dayOfWeek.getDisplayName(TextStyle.NARROW, locale), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.outline)
                    Box(
                        Modifier.size(34.dp).background(if (isToday) MaterialTheme.colorScheme.primary else androidx.compose.ui.graphics.Color.Transparent, CircleShape),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(
                            "${d.dayOfMonth}",
                            style = MaterialTheme.typography.titleSmall,
                            color = if (isToday) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface,
                        )
                    }
                    Box(Modifier.size(5.dp).background(if (has) MaterialTheme.colorScheme.primary else androidx.compose.ui.graphics.Color.Transparent, CircleShape))
                }
            }
            IconButton(onClick = { nav.go(CalendarRoute) }) { Icon(Icons.Outlined.CalendarMonth, stringResource(R.string.calendar), tint = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
    }
}

@Composable
fun TaskLine(t: TaskItem, today: String, showNote: Boolean = true) {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    val scope = rememberCoroutineScope()
    Row(Modifier.fillMaxWidth().clickable { nav.note(t.path, t.line.toInt()) }.padding(start = 8.dp, end = 20.dp, top = 2.dp, bottom = 2.dp), verticalAlignment = Alignment.CenterVertically) {
        Checkbox(t.done, onCheckedChange = {
            scope.launch { runCatching { engine.run { toggleTask(t.path, t.line) } }.onFailure { engine.say(it.words()) } }
        })
        Column(Modifier.weight(1f)) {
            // The task's own Markdown: links, bold, code…
            val spans = remember(t.text) {
                // Links in a task aren't looked up here; show them all as links.
                app.marrow.core.renderMarkdown(t.text).firstOrNull()?.spans.orEmpty().map { sp ->
                    if (sp.link?.kind == app.marrow.core.LinkKind.MISSING) sp.copy(link = sp.link!!.copy(kind = app.marrow.core.LinkKind.NOTE)) else sp
                }
            }
            app.marrow.ui.note.Rich(spans, MaterialTheme.typography.bodyLarge, app.marrow.ui.note.BlockActions(onLink = { nav.note(t.path, t.line.toInt()) }))
            val overdue = t.due != null && t.due!! < today
            // A task in a daily note is due that day: say which note once.
            val daily = Regex("\\d{4}-\\d{2}-\\d{2}").matches(t.title)
            val note = when {
                !showNote -> null
                t.title == today -> stringResource(R.string.todays_note)
                daily -> app.marrow.ui.common.longDate(t.title)
                else -> t.title
            }
            val meta = listOfNotNull(
                t.due?.takeIf { it != t.title }?.let { if (it == today) stringResource(R.string.today) else app.marrow.ui.common.mediumDate(it) },
                note,
            ).joinToString(" · ")
            if (meta.isNotEmpty()) {
                Text(meta, style = MaterialTheme.typography.labelMedium, color = if (overdue) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.outline)
            }
        }
    }
}

@Composable
private fun Label(icon: ImageVector, text: String) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Icon(icon, null, Modifier.size(18.dp), tint = MaterialTheme.colorScheme.primary)
        Text(text, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
    }
}
