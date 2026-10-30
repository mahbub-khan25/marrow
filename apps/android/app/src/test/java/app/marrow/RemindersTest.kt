package app.marrow

import android.app.AlarmManager
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import androidx.test.core.app.ApplicationProvider
import app.marrow.data.ReminderReceiver
import app.marrow.data.Reminders
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import java.time.LocalDateTime

/** Alarms are set for reminders ahead, replaced when the notes change, and ring as notifications. */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], application = android.app.Application::class)
class RemindersTest {
    private val ctx: Context = ApplicationProvider.getApplicationContext()

    private fun at(minutes: Long) = LocalDateTime.now().plusMinutes(minutes).format(Reminders.FORMAT)

    @Test
    fun schedulesAndReplaces() {
        val am = shadowOf(ctx.getSystemService(AlarmManager::class.java))
        val soon = Reminders.Item("a", "Plan.md", "Plan", 1, "Call the dentist", at(30))
        val later = Reminders.Item("b", "Plan.md", "Plan", 2, "Buy bread", at(120))
        val past = Reminders.Item("c", "Plan.md", "Plan", 3, "Old", at(-120))
        Reminders.schedule(ctx, listOf(soon, later, past))
        assertEquals(2, am.scheduledAlarms.size)
        assertEquals(Reminders.millisOf(soon.at), am.scheduledAlarms.minOf { it.triggerAtTime })
        // "Buy bread" was ticked: its alarm goes.
        Reminders.schedule(ctx, listOf(soon))
        assertEquals(1, am.scheduledAlarms.size)
        // After a restart, the same alarms are set again.
        Reminders.restore(ctx)
        assertEquals(1, am.scheduledAlarms.size)
    }

    @Test
    fun ringsWithDoneAndSnooze() {
        shadowOf(ctx.getSystemService(android.app.Application::class.java) ?: ctx as android.app.Application)
            .grantPermissions(android.Manifest.permission.POST_NOTIFICATIONS)
        val item = Reminders.Item("a", "Plan.md", "Plan", 1, "Call the dentist", at(0))
        val alarm = Intent(ctx, ReminderReceiver::class.java).setAction("app.marrow.REMINDER")
            .putExtra("item", kotlinx.serialization.json.Json.encodeToString(Reminders.Item.serializer(), item))
        ReminderReceiver().onReceive(ctx, alarm)
        val posted = shadowOf(ctx.getSystemService(NotificationManager::class.java)).allNotifications
        assertEquals(1, posted.size)
        val n = posted[0]
        assertEquals("Call the dentist", n.extras.getCharSequence("android.title").toString())
        assertEquals("Plan", n.extras.getCharSequence("android.text").toString())
        assertTrue(n.actions.map { it.title.toString() }.containsAll(listOf("Done", "Snooze 10 min")))
    }
}
