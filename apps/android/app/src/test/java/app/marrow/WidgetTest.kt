package app.marrow

import android.content.Context
import android.view.View
import android.widget.FrameLayout
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.glance.appwidget.compose
import androidx.test.core.app.ApplicationProvider
import app.marrow.widget.TodayTasks
import app.marrow.widget.TodayWidget
import app.marrow.widget.plain
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.time.LocalDate
import java.time.LocalDateTime
import app.marrow.data.Reminders

/** The home-screen widget shows what's due today and overdue. */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w411dp-h891dp-xxhdpi", application = android.app.Application::class)
class WidgetTest {
    private val ctx: Context = ApplicationProvider.getApplicationContext()

    @Test
    fun dueTodayAndOverdue() {
        val today = LocalDate.now()
        val s = TodayTasks.Snapshot(
            ready = true,
            items = listOf(
                TodayTasks.Item("Projects/Garden plan.md", 9, "Order the seed potatoes", today.minusDays(2).toString()),
                TodayTasks.Item("Daily/$today.md", 4, "Call **Sam** about [[Garden plan]]", today.toString(), LocalDateTime.now().withHour(17).withMinute(30).format(Reminders.FORMAT)),
                TodayTasks.Item("Daily/$today.md", 5, "Water the seedlings", today.toString()),
                TodayTasks.Item("Projects/Garden plan.md", 11, "Turn the compost", today.plusDays(3).toString()),
            ),
        )
        assertEquals(3, TodayTasks.due(s, today.toString()).size)
        assertEquals("Call Sam about Garden plan", plain(s.items[1].text))
        TodayTasks.write(ctx, s)
        draw("screenshots/23-widget.png")
    }

    @Test
    fun beforeAVaultIsOpen() {
        TodayTasks.write(ctx, TodayTasks.Snapshot(ready = false, items = emptyList()))
        draw("screenshots/24-widget-empty.png", DpSize(300.dp, 140.dp))
    }

    private fun draw(file: String, size: DpSize = DpSize(320.dp, 260.dp)) {
        val rv = runBlocking { TodayWidget().compose(ctx, size = size) }
        val host = FrameLayout(ctx)
        val v: View = rv.apply(ctx, host)
        val d = ctx.resources.displayMetrics.density
        host.addView(v, FrameLayout.LayoutParams((size.width.value * d).toInt(), (size.height.value * d).toInt()))
        host.measure(View.MeasureSpec.makeMeasureSpec((size.width.value * d).toInt(), View.MeasureSpec.EXACTLY), View.MeasureSpec.makeMeasureSpec((size.height.value * d).toInt(), View.MeasureSpec.EXACTLY))
        host.layout(0, 0, host.measuredWidth, host.measuredHeight)
        // On a plain wallpaper-ish grey, with some room around it.
        val pad = (16 * d).toInt()
        val bmp = android.graphics.Bitmap.createBitmap(host.measuredWidth + 2 * pad, host.measuredHeight + 2 * pad, android.graphics.Bitmap.Config.ARGB_8888)
        val c = android.graphics.Canvas(bmp)
        c.drawColor(android.graphics.Color.rgb(0x5B, 0x6B, 0x73))
        c.translate(pad.toFloat(), pad.toFloat())
        host.draw(c)
        java.io.File(file).apply { parentFile?.mkdirs() }.outputStream().use { bmp.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
    }
}
