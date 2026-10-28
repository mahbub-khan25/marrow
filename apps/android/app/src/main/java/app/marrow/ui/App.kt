package app.marrow.ui

import android.net.Uri
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material.icons.outlined.EditNote
import androidx.compose.material.icons.outlined.Menu
import androidx.compose.material3.IconButton
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.isMetaPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.unit.dp
import app.marrow.data.words
import app.marrow.ui.common.NameDialog
import app.marrow.ui.desk.DESK_WIDTH
import app.marrow.ui.desk.Place
import app.marrow.ui.desk.Sidebar
import app.marrow.ui.desk.todayPath
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
    /** In the desktop layout a note opened from the sidebar replaces the one showing. */
    var desk = false

    override fun note(path: String, line: Int, edit: Boolean) = nav.navigate(NoteRoute(path, line, edit)) {
        if (desk) popUpTo<NoteRoute> { inclusive = true }
    }
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

private fun goTab(nav: NavHostController, route: Any) = nav.navigate(route) {
    popUpTo(nav.graph.findStartDestination().id) { saveState = true }
    launchSingleTop = true
    restoreState = true
}

private fun goPlace(nav: NavHostController, place: Place) = when (place) {
    Place.Today -> goTab(nav, TodayRoute)
    Place.Tasks -> goTab(nav, TasksRoute)
    Place.Ask -> goTab(nav, AskRoute)
    Place.Search -> goTab(nav, SearchRoute())
    Place.Calendar -> nav.navigate(CalendarRoute) { launchSingleTop = true }
    Place.Graph -> nav.navigate(GraphRoute()) { launchSingleTop = true }
    Place.Settings -> nav.navigate(SettingsRoute) { launchSingleTop = true }
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
    var sidebar by rememberSaveable { mutableStateOf(true) }
    var naming by remember { mutableStateOf(false) }
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
        BoxWithConstraints(
            Modifier.fillMaxSize().onPreviewKeyEvent { e ->
                if (e.type != KeyEventType.KeyDown || !(e.isCtrlPressed || e.isMetaPressed)) return@onPreviewKeyEvent false
                when (e.key) {
                    Key.K, Key.P -> { goPlace(nav, Place.Search); true }
                    Key.D -> { scope.launch { engine.todayPath()?.let { appNav.note(it, edit = true) } }; true }
                    Key.N -> { naming = true; true }
                    Key.Comma -> { goPlace(nav, Place.Settings); true }
                    Key.Backslash -> { sidebar = !sidebar; true }
                    else -> false
                }
            },
        ) {
            val desk = maxWidth >= DESK_WIDTH
            appNav.desk = desk
            val place = when {
                dest?.hasRoute(TodayRoute::class) == true -> Place.Today
                dest?.hasRoute(TasksRoute::class) == true -> Place.Tasks
                dest?.hasRoute(AskRoute::class) == true -> Place.Ask
                dest?.hasRoute(SearchRoute::class) == true -> Place.Search
                dest?.hasRoute(CalendarRoute::class) == true -> Place.Calendar
                dest?.hasRoute(GraphRoute::class) == true -> Place.Graph
                dest?.hasRoute(SettingsRoute::class) == true -> Place.Settings
                else -> null
            }
            val openPath = when {
                dest?.hasRoute(NoteRoute::class) == true -> entry?.toRoute<NoteRoute>()?.path
                dest?.hasRoute(CanvasRoute::class) == true -> entry?.toRoute<CanvasRoute>()?.path
                dest?.hasRoute(FileRoute::class) == true -> entry?.toRoute<FileRoute>()?.path
                else -> null
            }
            NavigationSuiteScaffold(
                navigationSuiteItems = {
                    TABS.forEach { t ->
                        val selected = dest?.hasRoute(t.route::class) == true
                        item(
                            selected = selected,
                            onClick = { goTab(nav, t.route) },
                            icon = { Icon(if (selected) t.selected else t.icon, contentDescription = null) },
                            label = { Text(stringResource(t.label)) },
                        )
                    }
                },
                layoutType = when {
                    // The desktop layout has its own sidebar.
                    desk -> NavigationSuiteType.None
                    onTab -> NavigationSuiteScaffoldDefaults.calculateFromAdaptiveInfo(androidx.compose.material3.adaptive.currentWindowAdaptiveInfo())
                    else -> NavigationSuiteType.None
                },
                containerColor = MaterialTheme.colorScheme.background,
            ) {
                Row(Modifier.fillMaxSize()) {
                    if (desk && sidebar) {
                        Sidebar(
                            current = place,
                            currentPath = openPath,
                            onPlace = { goPlace(nav, it) },
                            onNewNote = { naming = true },
                            onHide = { sidebar = false },
                            modifier = Modifier.width(280.dp),
                        )
                        VerticalDivider()
                    } else if (desk) {
                        Column(
                            Modifier.fillMaxHeight().background(MaterialTheme.colorScheme.surfaceContainerLow).windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Start + WindowInsetsSides.Vertical)).padding(4.dp),
                        ) {
                            IconButton(onClick = { sidebar = true }) { Icon(Icons.Outlined.Menu, stringResource(R.string.show_sidebar)) }
                            IconButton(onClick = { naming = true }) { Icon(Icons.Outlined.EditNote, stringResource(R.string.new_note)) }
                            IconButton(onClick = { goPlace(nav, Place.Search) }) { Icon(Icons.Outlined.Search, stringResource(R.string.search)) }
                        }
                        VerticalDivider()
                    }
                    Box(Modifier.weight(1f).fillMaxHeight()) {
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
            }
        }
        if (naming) {
            NameDialog(stringResource(R.string.new_note), placeholder = stringResource(R.string.untitled), confirm = stringResource(R.string.create), onDismiss = { naming = false }) { name ->
                naming = false
                val folder = if (dest?.hasRoute(FolderRoute::class) == true) entry?.toRoute<FolderRoute>()?.path.orEmpty() else ""
                scope.launch {
                    runCatching { engine.run { createNote(folder, name, "") } }
                        .onSuccess { appNav.note(it, edit = true) }
                        .onFailure { engine.say(it.words()) }
                }
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
