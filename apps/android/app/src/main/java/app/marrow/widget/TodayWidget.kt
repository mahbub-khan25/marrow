package app.marrow.widget

import android.content.Context
import android.content.Intent
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.ColorFilter
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.LocalSize
import androidx.glance.action.ActionParameters
import androidx.glance.action.actionParametersOf
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.cornerRadius
import androidx.glance.appwidget.provideContent
import androidx.glance.appwidget.updateAll
import androidx.glance.background
import androidx.glance.color.ColorProvider
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import app.marrow.MainActivity
import app.marrow.MarrowApp
import app.marrow.R
import app.marrow.data.Engine
import app.marrow.data.Reminders
import app.marrow.ui.reminders.reminderText
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import java.time.LocalDate
import java.time.format.DateTimeFormatter

/**
 * Today's tasks on the home screen: what's due today and overdue, each with
 * a tick box, and a button to add a task. The widget shows a copy of the
 * tasks kept in preferences (written whenever notes change), so it never
 * waits for the vault to open.
 */
object TodayTasks {
    private const val PREFS = "marrow.widget"
    private const val KEY = "tasks"
    /** How many tasks are kept for the widget (it shows what fits). */
    private const val KEEP = 40
    private val json = Json { ignoreUnknownKeys = true }

    @Serializable
    data class Item(val path: String, val line: Int, val text: String, val due: String, val remind: String? = null)

    @Serializable
    data class Snapshot(val ready: Boolean, val items: List<Item>)

    fun read(ctx: Context): Snapshot =
        ctx.getSharedPreferences(PREFS, 0).getString(KEY, null)
            ?.let { runCatching { json.decodeFromString<Snapshot>(it) }.getOrNull() }
            ?: Snapshot(ready = false, items = emptyList())

    internal fun write(ctx: Context, s: Snapshot) {
        ctx.getSharedPreferences(PREFS, 0).edit().putString(KEY, json.encodeToString(s)).apply()
    }

    /** Copies the open tasks with a date from the vault, and redraws the widgets. */
    suspend fun refresh(ctx: Context, engine: Engine) {
        val tasks = runCatching { engine.run { tasks(false) } }.getOrNull() ?: return
        val items = tasks
            .filter { !it.done && it.due != null }
            .sortedWith(compareBy({ it.due }, { it.remind ?: "99" }))
            .take(KEEP)
            .map { Item(it.path, it.line.toInt(), it.text, it.due!!, it.remind) }
        write(ctx, Snapshot(ready = true, items = items))
        runCatching { TodayWidget().updateAll(ctx) }
    }

    /** Takes a ticked task off at once, before the note is changed. */
    suspend fun drop(ctx: Context, path: String, line: Int) {
        val s = read(ctx)
        write(ctx, s.copy(items = s.items.filterNot { it.path == path && it.line == line }))
        runCatching { TodayWidget().updateAll(ctx) }
    }

    /** What the widget shows on [day]: due that day or before. */
    fun due(s: Snapshot, day: String) = s.items.filter { it.due <= day }
}

class TodayWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = TodayWidget()
}

private val paper = ColorProvider(Color(0xFFFBFAF7), Color(0xFF1C1B19))
private val ink = ColorProvider(Color(0xFF1D1C1A), Color(0xFFECE8E2))
private val faint = ColorProvider(Color(0xFF5C5850), Color(0xFFB8B2A8))
private val accent = ColorProvider(Color(0xFF0A7EA4), Color(0xFF4FB6DC))
private val late = ColorProvider(Color(0xFFB3261E), Color(0xFFF2B8B5))

private val pathKey = ActionParameters.Key<String>("path")
private val lineKey = ActionParameters.Key<Int>("line")
private val textKey = ActionParameters.Key<String>("text")

class TodayWidget : GlanceAppWidget() {
    override val sizeMode = SizeMode.Exact

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val snapshot = TodayTasks.read(context)
        provideContent { Content(context, snapshot) }
    }

    @Composable
    private fun Content(ctx: Context, s: TodayTasks.Snapshot) {
        val today = LocalDate.now()
        val day = today.toString()
        val due = TodayTasks.due(s, day)
        fun open(action: String) = Intent(ctx, MainActivity::class.java).setAction(action).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        Column(
            GlanceModifier.fillMaxSize().background(paper).cornerRadius(20.dp).padding(start = 14.dp, end = 8.dp, top = 10.dp, bottom = 6.dp),
        ) {
            Row(GlanceModifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                Column(GlanceModifier.defaultWeight().clickable(actionStartActivity(open("app.marrow.TODAY")))) {
                    Text(ctx.getString(R.string.today), style = TextStyle(color = ink, fontSize = 17.sp, fontWeight = FontWeight.Bold), maxLines = 1)
                    Text(today.format(DateTimeFormatter.ofPattern("EEEE d MMMM")), style = TextStyle(color = faint, fontSize = 12.sp), maxLines = 1)
                }
                Box(
                    GlanceModifier.size(40.dp).background(accent).cornerRadius(20.dp).clickable(actionStartActivity(open("app.marrow.CAPTURE"))),
                    contentAlignment = Alignment.Center,
                ) {
                    Image(ImageProvider(R.drawable.ic_widget_add), ctx.getString(R.string.add_task), GlanceModifier.size(22.dp), colorFilter = ColorFilter.tint(paper))
                }
            }
            Spacer(GlanceModifier.height(6.dp))
            when {
                !s.ready -> Note(ctx.getString(R.string.widget_open_app), open("app.marrow.TODAY"))
                due.isEmpty() -> Note(ctx.getString(R.string.widget_nothing_due), open("app.marrow.CAPTURE"))
                else -> {
                    // As many as fit; the rest are counted (tapping that opens Today).
                    val fit = maxOf(1, ((LocalSize.current.height.value - 64f) / 46f).toInt())
                    val shown = if (due.size > fit) due.take(maxOf(1, fit - 1)) else due
                    for (t in shown) TaskRow(ctx, t, day)
                    if (shown.size < due.size) {
                        Text(
                            ctx.getString(R.string.widget_more, due.size - shown.size),
                            style = TextStyle(color = accent, fontSize = 13.sp),
                            modifier = GlanceModifier.padding(start = 38.dp, top = 4.dp).clickable(actionStartActivity(open("app.marrow.TODAY"))),
                        )
                    }
                }
            }
        }
    }

    @Composable
    private fun Note(text: String, intent: Intent) {
        Box(GlanceModifier.fillMaxSize().clickable(actionStartActivity(intent)), contentAlignment = Alignment.CenterStart) {
            Text(text, style = TextStyle(color = faint, fontSize = 14.sp))
        }
    }

    @Composable
    private fun TaskRow(ctx: Context, t: TodayTasks.Item, day: String) {
        val openNote = Intent(ctx, MainActivity::class.java)
            .setAction("app.marrow.OPEN_NOTE")
            .putExtra("path", t.path)
            .putExtra("line", t.line)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        Row(GlanceModifier.fillMaxWidth().padding(vertical = 5.dp), verticalAlignment = Alignment.CenterVertically) {
            Image(
                ImageProvider(R.drawable.ic_widget_circle),
                ctx.getString(R.string.done),
                GlanceModifier.size(32.dp).padding(5.dp).clickable(
                    actionRunCallback<TickTask>(actionParametersOf(pathKey to t.path, lineKey to t.line, textKey to t.text)),
                ),
            )
            Spacer(GlanceModifier.width(6.dp))
            Column(GlanceModifier.defaultWeight().clickable(actionStartActivity(openNote))) {
                Text(plain(t.text), style = TextStyle(color = ink, fontSize = 14.sp), maxLines = 2)
                val meta = when {
                    t.remind != null -> reminderText(ctx, t.remind)
                    t.due < day -> app.marrow.ui.common.mediumDate(t.due)
                    else -> null
                }
                if (meta != null) {
                    Text(meta, style = TextStyle(color = if (t.due < day) late else accent, fontSize = 12.sp), maxLines = 1)
                }
            }
        }
    }
}

/** A task's Markdown as plain words (links and marks dropped). */
internal fun plain(md: String): String = md
    .replace(Regex("""\[\[([^\]|]+)\|([^\]]+)]]"""), "$2")
    .replace(Regex("""\[\[([^\]]+)]]"""), "$1")
    .replace(Regex("""\[([^\]]+)]\([^)]*\)"""), "$1")
    .replace(Regex("""(\*\*|__|~~|`|==)"""), "")
    .trim()

/** The tick box: marks the task done in its note. */
class TickTask : ActionCallback {
    override suspend fun onAction(context: Context, glanceId: GlanceId, parameters: ActionParameters) {
        val path = parameters[pathKey] ?: return
        val line = parameters[lineKey] ?: return
        val text = parameters[textKey] ?: return
        TodayTasks.drop(context, path, line)
        val engine = (context.applicationContext as MarrowApp).engine
        runCatching {
            if (engine.vault.value == null) engine.openLast()
            engine.run { completeTask(path, line.toUInt(), text) }
        }
        TodayTasks.refresh(context, engine)
        Reminders.refresh(context, engine)
    }
}
