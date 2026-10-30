package app.marrow.ui.reminders

import android.Manifest
import android.content.Intent
import android.os.Build
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Alarm
import androidx.compose.material.icons.outlined.AlarmOff
import androidx.compose.material.icons.outlined.CalendarMonth
import androidx.compose.material.icons.outlined.Schedule
import androidx.compose.material.icons.outlined.WbSunny
import androidx.compose.material.icons.outlined.WbTwilight
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.data.Reminders
import java.time.Instant
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/** "Tue, 3 Nov, 09:00", or "Today, 09:00", for a `YYYY-MM-DD HH:MM` reminder. */
@Composable
fun reminderLabel(at: String): String = reminderText(LocalContext.current, at)

fun reminderText(ctx: android.content.Context, at: String): String {
    val t = runCatching { LocalDateTime.parse(at, Reminders.FORMAT) }.getOrNull() ?: return at
    val time = t.toLocalTime().format(DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT))
    val today = LocalDate.now()
    val day = when (t.toLocalDate()) {
        today -> ctx.getString(R.string.today)
        today.plusDays(1) -> ctx.getString(R.string.tomorrow)
        else -> t.toLocalDate().format(DateTimeFormatter.ofPattern(if (t.year == today.year) "EEE d MMM" else "d MMM yyyy"))
    }
    return "$day, $time"
}

/**
 * Picks when to be reminded: soon, this evening, tomorrow morning, or any
 * date and time; or removes the reminder. Asks for permission to notify the
 * first time.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RemindSheet(current: String?, onDismiss: () -> Unit, onPick: (String?) -> Unit) {
    val ctx = LocalContext.current
    var picking by remember { mutableStateOf(false) }
    var date by remember { mutableStateOf<LocalDate?>(null) }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}
    LaunchedEffect(Unit) {
        if (Build.VERSION.SDK_INT >= 33 && !Reminders.canNotify(ctx)) ask.launch(Manifest.permission.POST_NOTIFICATIONS)
    }
    val now = LocalDateTime.now()
    fun fmt(t: LocalDateTime) = t.format(Reminders.FORMAT)
    val inAnHour = now.plusHours(1).withSecond(0).withNano(0)
    val evening = now.toLocalDate().atTime(18, 0)
    val tomorrow = now.toLocalDate().plusDays(1).atTime(9, 0)

    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.navigationBarsPadding().padding(bottom = 16.dp)) {
            Text(stringResource(R.string.remind_me), style = MaterialTheme.typography.titleLarge, modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp))
            if (current != null) {
                Text(stringResource(R.string.reminder_set_for, reminderLabel(current)), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(horizontal = 24.dp))
            }
            Option(Icons.Outlined.Schedule, stringResource(R.string.in_an_hour), reminderLabel(fmt(inAnHour))) { onPick(fmt(inAnHour)) }
            if (evening.isAfter(now.plusMinutes(15))) {
                Option(Icons.Outlined.WbTwilight, stringResource(R.string.this_evening), reminderLabel(fmt(evening))) { onPick(fmt(evening)) }
            }
            Option(Icons.Outlined.WbSunny, stringResource(R.string.tomorrow_morning), reminderLabel(fmt(tomorrow))) { onPick(fmt(tomorrow)) }
            Option(Icons.Outlined.CalendarMonth, stringResource(R.string.pick_date_time), null) { picking = true }
            if (current != null) {
                HorizontalDivider(Modifier.padding(vertical = 4.dp))
                Option(Icons.Outlined.AlarmOff, stringResource(R.string.remove_reminder), null) { onPick(null) }
            }
            if (!Reminders.exact(ctx)) {
                // Android 12 and later ask apps to get leave to ring on the dot.
                Text(
                    stringResource(R.string.exact_alarms_hint),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
                )
                TextButton(onClick = {
                    if (Build.VERSION.SDK_INT >= 31) {
                        runCatching { ctx.startActivity(Intent(Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM).setData(android.net.Uri.parse("package:" + ctx.packageName))) }
                    }
                }, modifier = Modifier.padding(horizontal = 12.dp)) { Text(stringResource(R.string.allow_exact_alarms)) }
            }
        }
    }

    if (picking && date == null) {
        val start = current?.let { runCatching { LocalDateTime.parse(it, Reminders.FORMAT).toLocalDate() }.getOrNull() } ?: LocalDate.now()
        val state = rememberDatePickerState(initialSelectedDateMillis = start.atStartOfDay().toInstant(ZoneOffset.UTC).toEpochMilli())
        DatePickerDialog(
            onDismissRequest = { picking = false },
            confirmButton = {
                TextButton(onClick = {
                    date = state.selectedDateMillis?.let { Instant.ofEpochMilli(it).atZone(ZoneOffset.UTC).toLocalDate() }
                    if (date == null) picking = false
                }) { Text(stringResource(R.string.next)) }
            },
            dismissButton = { TextButton(onClick = { picking = false }) { Text(stringResource(R.string.cancel)) } },
        ) { DatePicker(state) }
    }
    date?.let { d ->
        val start = current?.let { runCatching { LocalDateTime.parse(it, Reminders.FORMAT).toLocalTime() }.getOrNull() } ?: LocalTime.of(9, 0)
        val state = rememberTimePickerState(initialHour = start.hour, initialMinute = start.minute, is24Hour = android.text.format.DateFormat.is24HourFormat(ctx))
        AlertDialog(
            onDismissRequest = { date = null; picking = false },
            title = { Text(stringResource(R.string.pick_time)) },
            text = { TimePicker(state) },
            confirmButton = {
                TextButton(onClick = {
                    val at = d.atTime(state.hour, state.minute)
                    date = null
                    picking = false
                    onPick(fmt(at))
                }) { Text(stringResource(R.string.ok)) }
            },
            dismissButton = { TextButton(onClick = { date = null; picking = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

@Composable
private fun Option(icon: ImageVector, label: String, detail: String?, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 24.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Icon(icon, null, Modifier.size(22.dp), tint = MaterialTheme.colorScheme.primary)
        Text(label, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
        if (detail != null) Text(detail, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** Shown on a task that has a reminder: the alarm icon and when. */
@Composable
fun ReminderChip(at: String, modifier: Modifier = Modifier) {
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Icon(Icons.Outlined.Alarm, null, Modifier.size(14.dp), tint = MaterialTheme.colorScheme.primary)
        Text(reminderLabel(at), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary)
    }
}
