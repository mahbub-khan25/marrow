package app.marrow

import android.app.Application
import android.os.SystemClock
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import app.marrow.data.Engine
import app.marrow.data.Reminders
import kotlinx.coroutines.flow.debounce
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first

class MarrowApp : Application() {
    lateinit var engine: Engine
        private set

    /** When the app last went to the background. */
    private var awaySince = 0L

    override fun onCreate() {
        super.onCreate()
        engine = Engine(this)
        Reminders.channel(this)
        // Keep the reminder alarms in step with the notes (edits here, sync,
        // other apps): once the vault opens, then after changes settle.
        @OptIn(kotlinx.coroutines.FlowPreview::class)
        engine.background {
            engine.vault.filterNotNull().first()
            Reminders.refresh(this, engine)
            engine.changes.debounce(1500).collect { Reminders.refresh(this, engine) }
        }
        ProcessLifecycleOwner.get().lifecycle.addObserver(object : DefaultLifecycleObserver {
            override fun onStop(owner: LifecycleOwner) {
                awaySince = SystemClock.elapsedRealtime()
            }

            override fun onStart(owner: LifecycleOwner) {
                if (awaySince == 0L) return
                val away = SystemClock.elapsedRealtime() - awaySince
                val limit = engine.prefs.look.value.lockAfter * 60_000L
                if (away >= limit) engine.marrow.lockAll()
                // Catch up with changes made elsewhere while away.
                engine.background {
                    runCatching { engine.run { rescan() } }
                    engine.marrow.syncNow(false)
                }
            }
        })
    }
}
