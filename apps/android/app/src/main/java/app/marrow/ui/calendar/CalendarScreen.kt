package app.marrow.ui.calendar

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.outlined.KeyboardArrowRight
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.data.words
import app.marrow.ui.common.BackBar
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.SectionTitle
import app.marrow.ui.common.load
import app.marrow.ui.common.longDate
import app.marrow.ui.common.today
import kotlinx.coroutines.launch
import java.time.LocalDate
import java.time.YearMonth
import java.time.format.TextStyle
import java.time.temporal.WeekFields
import java.util.Locale

/** Daily notes by month: tap a day to open (or start) its note. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CalendarScreen() {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    val scope = rememberCoroutineScope()
    val now = LocalDate.parse(today())
    var month by remember { mutableStateOf(YearMonth.from(now)) }
    val days = load(month) { run { daysWithNotes(month.toString()) } }.value.orEmpty().map { it.toInt() }.toSet()
    val recent = load { run { dailyNotes(40u) } }.value.orEmpty()
    val locale = Locale.getDefault()
    val first = WeekFields.of(locale).firstDayOfWeek

    fun open(d: LocalDate) = scope.launch {
        runCatching { engine.run { openDay(d.toString()) } }.onSuccess { nav.note(it) }.onFailure { engine.say(it.words()) }
    }

    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { BackBar(stringResource(R.string.calendar)) }) { pad ->
        Box(Modifier.padding(pad).fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            LazyColumn(Modifier.widthIn(max = 560.dp).fillMaxWidth(), contentPadding = PaddingValues(bottom = 40.dp)) {
                item {
                    Surface(shape = RoundedCornerShape(24.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.padding(16.dp)) {
                        Column(Modifier.padding(12.dp)) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                IconButton(onClick = { month = month.minusMonths(1) }) { Icon(Icons.AutoMirrored.Outlined.KeyboardArrowLeft, stringResource(R.string.previous_month)) }
                                Text(
                                    "${month.month.getDisplayName(TextStyle.FULL_STANDALONE, locale)} ${month.year}",
                                    style = MaterialTheme.typography.titleLarge,
                                    modifier = Modifier.weight(1f),
                                    textAlign = androidx.compose.ui.text.style.TextAlign.Center,
                                )
                                IconButton(onClick = { month = month.plusMonths(1) }) { Icon(Icons.AutoMirrored.Outlined.KeyboardArrowRight, stringResource(R.string.next_month)) }
                            }
                            Row(Modifier.padding(top = 8.dp)) {
                                (0 until 7).forEach { i ->
                                    val dow = first.plus(i.toLong())
                                    Text(dow.getDisplayName(TextStyle.SHORT, locale), Modifier.weight(1f), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.outline, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
                                }
                            }
                            val lead = (month.atDay(1).dayOfWeek.value - first.value + 7) % 7
                            val cells = lead + month.lengthOfMonth()
                            (0 until (cells + 6) / 7).forEach { row ->
                                Row {
                                    (0 until 7).forEach { col ->
                                        val n = row * 7 + col - lead + 1
                                        Box(Modifier.weight(1f).aspectRatio(1f).padding(2.dp), contentAlignment = Alignment.Center) {
                                            if (n in 1..month.lengthOfMonth()) {
                                                val d = month.atDay(n)
                                                val isToday = d == now
                                                val has = n in days
                                                Column(
                                                    Modifier.fillMaxSize().background(
                                                        when {
                                                            isToday -> MaterialTheme.colorScheme.primary
                                                            has -> MaterialTheme.colorScheme.primaryContainer
                                                            else -> Color.Transparent
                                                        },
                                                        CircleShape,
                                                    ).clickable { open(d) },
                                                    horizontalAlignment = Alignment.CenterHorizontally,
                                                    verticalArrangement = Arrangement.Center,
                                                ) {
                                                    Text(
                                                        "$n",
                                                        style = MaterialTheme.typography.bodyLarge,
                                                        fontWeight = if (has || isToday) FontWeight.SemiBold else FontWeight.Normal,
                                                        color = when {
                                                            isToday -> MaterialTheme.colorScheme.onPrimary
                                                            has -> MaterialTheme.colorScheme.onPrimaryContainer
                                                            d.isAfter(now) -> MaterialTheme.colorScheme.outline
                                                            else -> MaterialTheme.colorScheme.onSurface
                                                        },
                                                    )
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if (month != YearMonth.from(now)) {
                                TextButton(onClick = { month = YearMonth.from(now) }, modifier = Modifier.align(Alignment.CenterHorizontally)) { Text(stringResource(R.string.back_to_today)) }
                            }
                        }
                    }
                }
                if (recent.isNotEmpty()) {
                    item { SectionTitle(stringResource(R.string.daily_notes)) }
                    items(recent, key = { it.path }) { d ->
                        val text = load(d.path) { runCatching { app.marrow.core.preview(run { readNote(d.path) }, 120u) }.getOrDefault("") }.value.orEmpty()
                        Column(Modifier.fillMaxWidth().clickable { nav.note(d.path) }.padding(horizontal = 20.dp, vertical = 10.dp)) {
                            Text(longDate(d.day), style = MaterialTheme.typography.titleSmall)
                            if (text.isNotEmpty()) Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
        }
    }
}
