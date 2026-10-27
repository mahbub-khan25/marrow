package app.marrow.ui

import android.net.Uri
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AutoAwesome
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Folder
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.WbSunny
import androidx.compose.material.icons.outlined.AutoAwesome
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.WbSunny
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.adaptive.navigationsuite.NavigationSuiteScaffold
import androidx.compose.material3.adaptive.navigationsuite.NavigationSuiteScaffoldDefaults
import androidx.compose.material3.adaptive.navigationsuite.NavigationSuiteType
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.navigation.NavDestination.Companion.hasRoute
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.navigation.toRoute
import app.marrow.R
import app.marrow.core.FileItem
import app.marrow.core.FileKind
import app.marrow.data.Engine
import app.marrow.ui.ask.AskScreen
import app.marrow.ui.calendar.CalendarScreen
import app.marrow.ui.canvas.CanvasScreen
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.LocalSnackbar
import app.marrow.ui.common.Nav
import app.marrow.ui.common.today
import app.marrow.ui.files.FileScreen
import app.marrow.ui.graph.GraphScreen
import app.marrow.ui.history.HistoryScreen
import app.marrow.ui.history.TimeMachineScreen
import app.marrow.ui.note.NoteScreen
import app.marrow.ui.notes.BrowserScreen
import app.marrow.ui.notes.TagScreen
import app.marrow.ui.onboarding.WelcomeScreen
import app.marrow.ui.search.SearchScreen
import app.marrow.ui.settings.AiSettingsScreen
import app.marrow.ui.settings.AppearanceScreen
import app.marrow.ui.settings.ConflictsScreen
import app.marrow.ui.settings.ImportScreen
import app.marrow.ui.settings.SettingsScreen
import app.marrow.ui.settings.SyncScreen
import app.marrow.ui.settings.VaultScreen
import app.marrow.ui.tasks.TasksScreen
import app.marrow.ui.today.CaptureSheet
import app.marrow.ui.today.TodayScreen
import kotlinx.coroutines.launch
import kotlinx.serialization.Serializable

/** What arrived from outside: a shortcut, or something shared from another app. */
sealed interface Incoming {
    data object Capture : Incoming
    data object Today : Incoming
    data class Shared(val text: String, val images: List<Uri>) : Incoming
}

// The places in the app. The first five are the main tabs.
@Serializable object TodayRoute
@Serializable object NotesRoute
@Serializable object AskRoute
@Serializable object TasksRoute
@Serializable data class SearchRoute(val query: String = "")
@Serializable data class FolderRoute(val path: String)
@Serializable data class NoteRoute(val path: String, val line: Int = -1, val edit: Boolean = false)
@Serializable data class TagRoute(val tag: String)
@Serializable data class CanvasRoute(val path: String)
@Serializable data class FileRoute(val path: String)
@Serializable object CalendarRoute
@Serializable data class GraphRoute(val around: String = "")
@Serializable data class HistoryRoute(val path: String)
@Serializable object TimeMachineRoute
@Serializable object SettingsRoute
@Serializable object SyncRoute
@Serializable object AiRoute
@Serializable object AppearanceRoute
@Serializable object VaultRoute
@Serializable object ImportRoute
@Serializable object ConflictsRoute

private data class Tab(val route: Any, val label: Int, val icon: ImageVector, val selected: ImageVector)

private val TABS = listOf(
    Tab(TodayRoute, R.string.today, Icons.Outlined.WbSunny, Icons.Filled.WbSunny),
    Tab(NotesRoute, R.string.notes, Icons.Outlined.Folder, Icons.Filled.Folder),
    Tab(AskRoute, R.string.ask, Icons.Outlined.AutoAwesome, Icons.Filled.AutoAwesome),
    Tab(TasksRoute, R.string.tasks, Icons.Outlined.CheckCircle, Icons.Filled.CheckCircle),
    Tab(SearchRoute(), R.string.search, Icons.Outlined.Search, Icons.Filled.Search),
)

private class AppNav(private val nav: NavHostController) : Nav {
    override fun note(path: String, line: Int, edit: Boolean) = nav.navigate(NoteRoute(path, line, edit))
    override fun folder(path: String) = nav.navigate(FolderRoute(path))
    override fun tag(tag: String) = nav.navigate(TagRoute(tag))
    override fun search(query: String) = nav.navigate(SearchRoute(query))
    override fun go(route: Any) = nav.navigate(route)
    override fun back() {
        nav.popBackStack()
    }

    override fun open(item: FileItem) = when (item.kind) {
        FileKind.FOLDER -> folder(item.path)
        FileKind.NOTE -> note(item.path)
        FileKind.CANVAS -> nav.navigate(CanvasRoute(item.path))
        else -> nav.navigate(FileRoute(item.path))
    }

    override fun openPath(path: String, line: Int) {
        val lower = path.lowercase()
        when {
            lower.endsWith(".md") -> note(path, line)
            lower.endsWith(".canvas") -> nav.navigate(CanvasRoute(path))
            else -> nav.navigate(FileRoute(path))
        }
    }
}

@Composable
fun App(engine: Engine, incoming: Incoming?, onHandled: () -> Unit) {
    val vault by engine.vault.collectAsStateWithLifecycle()
    var starting by remember { mutableStateOf(true) }
    LaunchedEffect(Unit) {
        engine.openLast()
        starting = false
    }
    val snackbar = remember { SnackbarHostState() }
    LaunchedEffect(Unit) { engine.messages.collect { snackbar.showSnackbar(it) } }
    CompositionLocalProvider(LocalEngine provides engine, LocalSnackbar provides snackbar) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            when {
                starting -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
                vault == null -> WelcomeScreen()
                else -> Main(engine, incoming, onHandled)
            }
        }
    }
}

@Composable
private fun Main(engine: Engine, incoming: Incoming?, onHandled: () -> Unit) {
    val nav = rememberNavController()
    val appNav = remember(nav) { AppNav(nav) }
    val entry by nav.currentBackStackEntryAsState()
    val dest = entry?.destination
    val onTab = TABS.any { t -> dest?.hasRoute(t.route::class) == true }
    var capture by remember { mutableStateOf<Incoming.Shared?>(null) }
    var captureOpen by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val snackbar = LocalSnackbar.current

    LaunchedEffect(Unit) {
        if (engine.prefs.afterOpen == "sync") appNav.go(SyncRoute)
        engine.prefs.afterOpen = null
    }
    LaunchedEffect(incoming) {
        when (incoming) {
            null -> {}
            Incoming.Capture -> captureOpen = true
            Incoming.Today -> {
                val path = runCatching { engine.run { openDay(today()) } }.getOrNull()
                if (path != null) appNav.note(path, edit = true)
            }
            is Incoming.Shared -> {
                capture = incoming
                captureOpen = true
            }
        }
        if (incoming != null) onHandled()
    }

    CompositionLocalProvider(LocalNav provides appNav) {
        NavigationSuiteScaffold(
            navigationSuiteItems = {
                TABS.forEach { t ->
                    val selected = dest?.hasRoute(t.route::class) == true
                    item(
                        selected = selected,
                        onClick = {
                            nav.navigate(t.route) {
                                popUpTo(nav.graph.findStartDestination().id) { saveState = true }
                                launchSingleTop = true
                                restoreState = true
                            }
                        },
                        icon = { Icon(if (selected) t.selected else t.icon, contentDescription = null) },
                        label = { Text(stringResource(t.label)) },
                    )
                }
            },
            layoutType = if (onTab) NavigationSuiteScaffoldDefaults.calculateFromAdaptiveInfo(androidx.compose.material3.adaptive.currentWindowAdaptiveInfo()) else NavigationSuiteType.None,
            containerColor = MaterialTheme.colorScheme.background,
        ) {
            Box(Modifier.fillMaxSize()) {
                NavHost(
                    nav,
                    startDestination = TodayRoute,
                    enterTransition = { fadeIn() },
                    exitTransition = { fadeOut() },
                ) {
                    composable<TodayRoute> { TodayScreen(onCapture = { captureOpen = true }) }
                    composable<NotesRoute> { BrowserScreen("") }
                    composable<AskRoute> { AskScreen() }
                    composable<TasksRoute> { TasksScreen() }
                    composable<SearchRoute> { SearchScreen(it.toRoute<SearchRoute>().query) }
                    composable<FolderRoute> { BrowserScreen(it.toRoute<FolderRoute>().path) }
                    composable<NoteRoute> {
                        val r = it.toRoute<NoteRoute>()
                        NoteScreen(r.path, r.line, r.edit)
                    }
                    composable<TagRoute> { TagScreen(it.toRoute<TagRoute>().tag) }
                    composable<CanvasRoute> { CanvasScreen(it.toRoute<CanvasRoute>().path) }
                    composable<FileRoute> { FileScreen(it.toRoute<FileRoute>().path) }
                    composable<CalendarRoute> { CalendarScreen() }
                    composable<GraphRoute> { GraphScreen(it.toRoute<GraphRoute>().around) }
                    composable<HistoryRoute> { HistoryScreen(it.toRoute<HistoryRoute>().path) }
                    composable<TimeMachineRoute> { TimeMachineScreen() }
                    composable<SettingsRoute> { SettingsScreen() }
                    composable<SyncRoute> { SyncScreen() }
                    composable<AiRoute> { AiSettingsScreen() }
                    composable<AppearanceRoute> { AppearanceScreen() }
                    composable<VaultRoute> { VaultScreen() }
                    composable<ImportRoute> { ImportScreen() }
                    composable<ConflictsRoute> { ConflictsScreen() }
                }
                SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter))
            }
        }
        if (captureOpen) {
            CaptureSheet(
                shared = capture,
                onDismiss = {
                    captureOpen = false
                    capture = null
                },
                onSaved = { msg ->
                    captureOpen = false
                    capture = null
                    scope.launch { snackbar.showSnackbar(msg) }
                },
            )
        }
    }
}
