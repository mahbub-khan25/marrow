package app.marrow.data

import android.Manifest
import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import app.marrow.MainActivity
import app.marrow.MarrowApp
import app.marrow.R
import app.marrow.core.Reminder
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.time.LocalDateTime
import java.time.ZoneId
import java.time.format.DateTimeFormatter

/**
 * Task reminders (`- [ ] Call Sam ⏰ 2026-11-03 09:00`). The notes are the
 * source of truth: after the vault opens and whenever notes change (here or
 * by sync), the reminders still ahead are scheduled with the system's alarm
 * clock, so they arrive even when Marrow is closed. The schedule is also kept
 * in preferences to set it again after the phone restarts.
 */
object Reminders {
    const val CHANNEL = "reminders"
    private const val PREFS = "marrow.reminders"
    private const val KEY = "scheduled"
    private const val ACTION_FIRE = "app.marrow.REMINDER"
    const val ACTION_DONE = "app.marrow.REMINDER_DONE"
    const val ACTION_SNOOZE = "app.marrow.REMINDER_SNOOZE"
    /** Alarms further ahead than this are set later (the list is refreshed often). */
    private const val HORIZON_DAYS = 60L
    const val SNOOZE_MINUTES = 10L

    private val json = Json { ignoreUnknownKeys = true }
    val FORMAT: DateTimeFormatter = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm")

    @Serializable
    data class Item(val id: String, val path: String, val title: String, val line: Int, val text: String, val at: String)

    fun Reminder.item() = Item(id, path, title, line.toInt(), text, at)

    fun millisOf(at: String): Long? = runCatching {
        LocalDateTime.parse(at, FORMAT).atZone(ZoneId.systemDefault()).toInstant().toEpochMilli()
    }.getOrNull()

    fun channel(ctx: Context) {
        if (Build.VERSION.SDK_INT >= 26) {
            val nm = ctx.getSystemService(NotificationManager::class.java)
            nm.createNotificationChannel(NotificationChannel(CHANNEL, ctx.getString(R.string.reminders), NotificationManager.IMPORTANCE_HIGH))
        }
    }

    fun canNotify(ctx: Context): Boolean =
        Build.VERSION.SDK_INT < 33 || ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED

    /** Whether reminders can ring at the exact minute (Android 12+ asks for this separately). */
    fun exact(ctx: Context): Boolean {
        val am = ctx.getSystemService(AlarmManager::class.java)
        return Build.VERSION.SDK_INT < 31 || am.canScheduleExactAlarms()
    }

    private fun saved(ctx: Context): List<Item> =
        runCatching { json.decodeFromString<List<Item>>(ctx.getSharedPreferences(PREFS, 0).getString(KEY, "[]")!!) }.getOrDefault(emptyList())

    private fun save(ctx: Context, items: List<Item>) {
        ctx.getSharedPreferences(PREFS, 0).edit().putString(KEY, json.encodeToString(items)).apply()
    }

    private fun fireIntent(ctx: Context, item: Item): PendingIntent {
        val i = Intent(ctx, ReminderReceiver::class.java).setAction(ACTION_FIRE).putExtra("item", json.encodeToString(item))
        return PendingIntent.getBroadcast(ctx, item.id.hashCode(), i, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    }

    /** Sets the alarms for [items], replacing the previous ones. */
    fun schedule(ctx: Context, items: List<Item>) {
        val am = ctx.getSystemService(AlarmManager::class.java)
        val now = System.currentTimeMillis()
        val until = now + HORIZON_DAYS * 86_400_000L
        val wanted = items.filter { r -> millisOf(r.at)?.let { it > now - 60_000 && it < until } == true }
        // Cancel the ones that went away (done, deleted, moved in time).
        val keep = wanted.map { it.id }.toSet()
        for (old in saved(ctx)) if (old.id !in keep) am.cancel(fireIntent(ctx, old))
        for (r in wanted) {
            val at = maxOf(millisOf(r.at)!!, now + 1_000)
            val pi = fireIntent(ctx, r)
            if (exact(ctx)) am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi)
            else am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, pi)
        }
        save(ctx, wanted)
    }

    /** Sets the alarms again from what was saved (after a restart). */
    fun restore(ctx: Context) = schedule(ctx, saved(ctx))

    /** Reads the reminders from the open vault and schedules them. */
    suspend fun refresh(ctx: Context, engine: Engine) {
        val list = runCatching { engine.run { reminders() } }.getOrNull() ?: return
        schedule(ctx, list.map { it.item() })
    }

    fun show(ctx: Context, item: Item) {
        if (!canNotify(ctx)) return
        channel(ctx)
        val code = item.id.hashCode()
        val open = Intent(ctx, MainActivity::class.java)
            .setAction("app.marrow.OPEN_NOTE")
            .putExtra("path", item.path)
            .putExtra("line", item.line)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        fun action(a: String) = PendingIntent.getBroadcast(
            ctx, code xor a.hashCode(),
            Intent(ctx, ReminderReceiver::class.java).setAction(a).putExtra("item", json.encodeToString(item)),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val n = NotificationCompat.Builder(ctx, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(item.text)
            .setContentText(item.title)
            .setCategory(NotificationCompat.CATEGORY_REMINDER)
            .setPriority(NotificationCompat.PRIORITY_HIGH)
            .setAutoCancel(true)
            .setContentIntent(PendingIntent.getActivity(ctx, code, open, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE))
            .addAction(0, ctx.getString(R.string.reminder_done), action(ACTION_DONE))
            .addAction(0, ctx.getString(R.string.reminder_snooze), action(ACTION_SNOOZE))
            .build()
        @Suppress("MissingPermission")
        NotificationManagerCompat.from(ctx).notify(code, n)
    }

    internal fun decode(intent: Intent): Item? = intent.getStringExtra("item")?.let { runCatching { json.decodeFromString<Item>(it) }.getOrNull() }
}

/** The alarm went off, or a button on the notification was pressed. */
class ReminderReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        val item = Reminders.decode(intent) ?: return
        when (intent.action) {
            Reminders.ACTION_DONE, Reminders.ACTION_SNOOZE -> {
                NotificationManagerCompat.from(ctx).cancel(item.id.hashCode())
                val pending = goAsync()
                val engine = (ctx.applicationContext as MarrowApp).engine
                CoroutineScope(SupervisorJob() + Dispatchers.IO).launch {
                    try {
                        if (engine.vault.value == null) engine.openLast()
                        if (intent.action == Reminders.ACTION_DONE) {
                            engine.run { completeTask(item.path, item.line.toUInt(), item.text) }
                        } else {
                            val next = LocalDateTime.now().plusMinutes(Reminders.SNOOZE_MINUTES).format(Reminders.FORMAT)
                            engine.run { setReminder(item.path, item.line.toUInt(), item.text, next) }
                        }
                        Reminders.refresh(ctx, engine)
                    } catch (_: Exception) {
                    } finally {
                        pending.finish()
                    }
                }
            }
            else -> Reminders.show(ctx, item)
        }
    }
}

/** After the phone restarts (or Marrow is updated), set the alarms again. */
class ReminderBootReceiver : BroadcastReceiver() {
    override fun onReceive(ctx: Context, intent: Intent) {
        Reminders.restore(ctx)
    }
}
