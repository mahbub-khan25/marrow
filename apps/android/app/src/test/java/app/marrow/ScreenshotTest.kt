package app.marrow

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onRoot
import androidx.test.core.app.ApplicationProvider
import app.marrow.data.Engine
import app.marrow.data.Look
import app.marrow.data.MemorySecrets
import app.marrow.ui.ask.AskScreen
import app.marrow.ui.calendar.CalendarScreen
import app.marrow.ui.canvas.CanvasScreen
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.Nav
import app.marrow.ui.graph.GraphScreen
import app.marrow.ui.history.TimeMachineScreen
import app.marrow.ui.note.NoteScreen
import app.marrow.ui.notes.BrowserScreen
import app.marrow.ui.onboarding.WelcomeScreen
import app.marrow.ui.search.SearchScreen
import app.marrow.ui.settings.SettingsScreen
import app.marrow.ui.settings.SyncScreen
import app.marrow.ui.tasks.TasksScreen
import app.marrow.ui.theme.MarrowTheme
import app.marrow.ui.today.TodayScreen
import app.marrow.core.FileItem
import com.github.takahirom.roborazzi.captureRoboImage
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File

/**
 * Pictures of every main screen, drawn on this computer with the real
 * engine and the demo vault (apps/demo-vault). They land in
 * app/screenshots/ for a person to look at; nothing is compared.
 *
 *   ./gradlew :app:testDebugUnitTest --tests '*ScreenshotTest*' -Proborazzi.test.record=true
 */
@RunWith(RobolectricTestRunner::class)
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(sdk = [35], qualifiers = "w412dp-h915dp-xxhdpi", application = android.app.Application::class)
class ScreenshotTest {
    @get:Rule val compose = createComposeRule()

    private lateinit var engine: Engine

    companion object {
        object NoNav : Nav {
            override fun note(path: String, line: Int, edit: Boolean) {}
            override fun folder(path: String) {}
            override fun tag(tag: String) {}
            override fun search(query: String) {}
            override fun open(item: FileItem) {}
            override fun openPath(path: String, line: Int) {}
            override fun go(route: Any) {}
            override fun back() {}
        }
        val out = File(System.getProperty("marrow.shots") ?: "screenshots").apply { mkdirs() }
        /** One copy of the demo vault for all the pictures. */
        val tmp: File by lazy {
            kotlin.io.path.createTempDirectory("marrow").toFile().also {
                File(System.getProperty("marrow.demo")!!).copyRecursively(File(it, "Demo vault"))
            }
        }
    }

    @Before
    fun open() {
        engine = Engine(ApplicationProvider.getApplicationContext(), File(tmp, "data"), MemorySecrets())
        runBlocking { engine.open(File(tmp, "Demo vault").absolutePath) }
    }

    @After
    fun close() = engine.close()


    private fun shot(name: String, look: Look = Look(), content: @Composable () -> Unit) {
        compose.setContent {
            MarrowTheme(look) {
                CompositionLocalProvider(LocalEngine provides engine, LocalNav provides NoNav) {
                    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) { content() }
                }
            }
        }
        // Let the engine calls (off the main thread) finish and the screen settle.
        repeat(12) {
            Thread.sleep(60)
            compose.mainClock.advanceTimeBy(120)
        }
        compose.waitForIdle()
        compose.mainClock.advanceTimeBy(1000)
        compose.onRoot().captureRoboImage(File(out, "$name.png").path)
    }

    @Test fun today() = shot("01-today") { TodayScreen(onCapture = {}) }
    @Test fun notes() = shot("02-notes") { BrowserScreen("") }
    @Test fun folder() = shot("03-folder") { BrowserScreen("Projects") }
    @Test fun noteReading() = shot("04-note") { NoteScreen("Projects/Garden plan.md", -1, false) }
    @Test fun noteEditing() = shot("05-editor") { NoteScreen("Projects/Garden plan.md", -1, true) }
    @Test fun noteDark() = shot("06-note-dark", Look(theme = "dark")) { NoteScreen("Reading/Notes on memory.md", -1, false) }
    @Test fun search() = shot("07-search") { SearchScreen("garden") }
    @Test fun tasks() = shot("08-tasks") { TasksScreen() }
    @Test fun ask() = shot("09-ask") { AskScreen() }
    @Test fun calendar() = shot("10-calendar") { CalendarScreen() }
    @Test fun graph() = shot("11-graph") { GraphScreen("") }
    @Test fun canvas() = shot("12-canvas") { CanvasScreen("Ideas.canvas") }
    @Test fun timeMachine() = shot("13-time-machine") { TimeMachineScreen() }
    @Test fun settings() = shot("14-settings") { SettingsScreen() }
    @Test fun sync() = shot("15-sync") { SyncScreen() }
    @Test fun welcome() = shot("16-welcome") { WelcomeScreen() }
    @Test fun todayDark() = shot("17-today-dark", Look(theme = "dark")) { TodayScreen(onCapture = {}) }
    @Test fun bangla() = shot("18-note-bangla", Look(readFont = "book")) { NoteScreen("Reading/বাংলা কবিতা.md", -1, false) }
}
