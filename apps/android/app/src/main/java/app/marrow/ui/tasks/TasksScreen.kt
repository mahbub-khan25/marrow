package app.marrow.ui.tasks

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.TaskAlt
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.LargeTopAppBar
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.rememberTopAppBarState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.core.TaskItem
import app.marrow.ui.common.Empty
import app.marrow.ui.common.SectionTitle
import app.marrow.ui.common.load
import app.marrow.ui.common.today
import app.marrow.ui.today.TaskLine
import java.time.LocalDate

/** Every to-do in every note: overdue, today, coming up, and the rest. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TasksScreen() {
    var showDone by remember { mutableStateOf(false) }
    val tasks = load(showDone) { run { tasks(showDone) } }
    val day = today()
    val week = LocalDate.parse(day).plusDays(7).toString()
    val groups = remember(tasks.value, day) {
        val all = tasks.value.orEmpty()
        val open = all.filter { !it.done }
        listOf(
            R.string.overdue to open.filter { it.due != null && it.due!! < day }.sortedBy { it.due },
            R.string.today to open.filter { it.due == day },
            R.string.next_7_days to open.filter { it.due != null && it.due!! > day && it.due!! <= week }.sortedBy { it.due },
            R.string.later to open.filter { it.due != null && it.due!! > week }.sortedBy { it.due },
            R.string.no_date to open.filter { it.due == null },
            R.string.done_tasks to all.filter { it.done },
        ).filter { it.second.isNotEmpty() }
    }
    val scroll = TopAppBarDefaults.exitUntilCollapsedScrollBehavior(rememberTopAppBarState())
    Scaffold(
        modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
        containerColor = MaterialTheme.colorScheme.background,
        topBar = {
            LargeTopAppBar(
                title = { Text(stringResource(R.string.tasks)) },
                actions = {
                    FilterChip(showDone, { showDone = !showDone }, label = { Text(stringResource(R.string.show_done)) }, modifier = Modifier.padding(end = 12.dp))
                },
                colors = TopAppBarDefaults.largeTopAppBarColors(containerColor = MaterialTheme.colorScheme.background, scrolledContainerColor = MaterialTheme.colorScheme.surfaceContainer),
                scrollBehavior = scroll,
            )
        },
    ) { pad ->
        Box(Modifier.padding(pad).fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            LazyColumn(Modifier.widthIn(max = 720.dp).fillMaxWidth(), contentPadding = PaddingValues(bottom = 40.dp)) {
                if (tasks.value?.isEmpty() == true) {
                    item { Empty(Icons.Outlined.TaskAlt, stringResource(R.string.no_tasks), stringResource(R.string.no_tasks_text)) }
                }
                groups.forEach { (label, list: List<TaskItem>) ->
                    item(key = "h$label") { SectionTitle("${stringResource(label)} · ${list.size}") }
                    items(list, key = { "${it.path}:${it.line}:${it.done}" }) { TaskLine(it, day) }
                }
            }
        }
    }
}
