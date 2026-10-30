import SwiftUI
import UIKit
import UserNotifications

@main
struct MarrowApp: App {
    @State private var store: Store
    @State private var router = Router()
    @State private var awaySince: Date?
    @Environment(\.scenePhase) private var phase
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    /// Screenshot runs: a copy of a demo vault, and the screen to show.
    private let demo: Demo?

    init() {
        let d = Demo.fromArguments()
        demo = d
        if let d {
            _store = State(initialValue: Store(dataDir: d.dataDir, secrets: MemorySecrets()))
        } else {
            _store = State(initialValue: Store())
        }
    }

    var body: some Scene {
        WindowGroup {
            Root()
                .toasts()
                .environment(store)
                .environment(router)
                .environment(\.reading, Reading(prefs: store.prefs))
                .preferredColorScheme(store.prefs.colorScheme)
                .tint(.accent)
                .task { await start() }
                .onReceive(NotificationCenter.default.publisher(for: QuickActions.notice)) { _ in handleQuickAction() }
                .onReceive(NotificationCenter.default.publisher(for: ReminderTap.notice)) { _ in handleReminderTap() }
                .task(id: store.tick) {
                    // Notes changed (here or by sync): schedule their reminders again.
                    guard demo == nil, store.vault != nil else { return }
                    try? await Task.sleep(nanoseconds: 1_500_000_000)
                    await Reminders.refresh(store.marrow)
                    await WidgetData.refresh(store.marrow)
                }
                .onOpenURL { url in OpenedLink.pending = url }
                .onReceive(NotificationCenter.default.publisher(for: OpenedLink.notice)) { _ in handleLink() }
        }
        .onChange(of: phase) { _, now in
            switch now {
            case .background:
                awaySince = Date()
            case .active:
                if let away = awaySince, Date().timeIntervalSince(away) >= Double(store.prefs.lockAfter * 60) {
                    store.marrow.lockAll()
                }
                awaySince = nil
                // Catch up with changes made elsewhere while away.
                Task.detached { [store] in
                    try? store.marrow.rescan()
                    store.marrow.syncNow(force: false)
                }
                handleQuickAction()
                Task { await ShareImport.run(store) }
            default: break
            }
        }
    }

    @MainActor
    private func start() async {
        if let demo {
            store.prefs.theme = demo.theme ?? "light"
            if demo.screen != "welcome" {
                _ = try? await store.open(demo.vault.path)
            }
            store.starting = false
            demo.show(router)
            return
        }
        Bookmarks.restore()
        ReminderCenter.shared.marrow = store.marrow
        await store.openLast()
        if store.prefs.afterOpen == "sync" {
            router.tab = .today
            router.push(.sync)
        }
        store.prefs.afterOpen = nil
        handleQuickAction()
        handleReminderTap()
        handleLink()
        await ShareImport.run(store)
    }

    /// `marrow://today`, `capture`, `note?path=&line=` and `done?path=&line=&text=`.
    @MainActor
    private func handleLink() {
        guard store.vault != nil, let url = OpenedLink.pending, url.scheme == "marrow" else { return }
        OpenedLink.pending = nil
        let q = Dictionary(URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.compactMap { i in i.value.map { (i.name, $0) } } ?? [], uniquingKeysWith: { a, _ in a })
        let line = q["line"].flatMap(Int.init) ?? -1
        switch url.host {
        case "capture":
            router.capture = true
        case "note":
            guard let p = q["path"] else { return }
            router.tab = .tasks
            router.note(p, line: line)
        case "done":
            guard let p = q["path"], let text = q["text"], line >= 0 else { return }
            Task {
                do {
                    try await store.run { try $0.completeTask(path: p, line: UInt32(line), text: text) }
                    store.say(String(localized: "Done: \(plainTask(text))"))
                } catch { store.say(error) }
            }
        default:
            Task {
                if let p = try? await store.run({ try $0.openDay(day: todayString()) }) {
                    router.tab = .today
                    router.note(p, edit: false)
                }
            }
        }
    }

    @MainActor
    private func handleReminderTap() {
        guard store.vault != nil, let tap = ReminderTap.pending else { return }
        ReminderTap.pending = nil
        router.tab = .tasks
        router.note(tap.path, line: tap.line)
    }

    @MainActor
    private func handleQuickAction() {
        guard store.vault != nil, let action = QuickActions.pending else { return }
        QuickActions.pending = nil
        switch action {
        case "app.marrow.capture":
            router.capture = true
        case "app.marrow.today":
            Task {
                if let p = try? await store.run({ try $0.openDay(day: todayString()) }) {
                    router.tab = .today
                    router.note(p, edit: true)
                }
            }
        default: break
        }
    }
}

/// The first screen, or the app.
struct Root: View {
    @Environment(Store.self) private var store

    var body: some View {
        Group {
            if store.starting {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity).background(Color.paper)
            } else if store.vault == nil {
                WelcomeView()
            } else {
                MainTabs()
            }
        }
    }
}

struct MainTabs: View {
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router

    var body: some View {
        @Bindable var router = router
        TabView(selection: $router.tab) {
            stack(.today) { TodayView() }
                .tabItem { Label("Today", systemImage: "sun.max") }.tag(AppTab.today)
            stack(.notes) { BrowserView(folder: "") }
                .tabItem { Label("Notes", systemImage: "folder") }.tag(AppTab.notes)
            stack(.ask) { AskView() }
                .tabItem { Label("Ask", systemImage: "sparkles") }.tag(AppTab.ask)
            stack(.tasks) { TasksView() }
                .tabItem { Label("Tasks", systemImage: "checklist") }.tag(AppTab.tasks)
            stack(.search) { SearchView() }
                .tabItem { Label("Search", systemImage: "magnifyingglass") }.tag(AppTab.search)
        }
        .sheet(isPresented: $router.capture) { CaptureSheet(initial: router.captureText) }
    }

    private func stack<V: View>(_ tab: AppTab, @ViewBuilder _ root: () -> V) -> some View {
        NavigationStack(path: router.binding(tab)) {
            root()
                .navigationDestination(for: Route.self) { destination($0) }
        }
    }

    @ViewBuilder
    private func destination(_ r: Route) -> some View {
        switch r {
        case .note(let path, let line, let edit): NoteView(path: path, line: line, startEditing: edit)
        case .folder(let path): BrowserView(folder: path)
        case .tag(let tag): TagView(tag: tag)
        case .canvas(let path): CanvasBoardView(path: path)
        case .file(let path): FileView(path: path)
        case .calendar: CalendarView()
        case .graph(let around): GraphView(around: around)
        case .history(let path): HistoryView(path: path)
        case .timeMachine: TimeMachineView()
        case .settings: SettingsView()
        case .sync: SyncView()
        case .ai: AiSettingsView()
        case .appearance: AppearanceView()
        case .vaultSettings: VaultSettingsView()
        case .importNotes: ImportView()
        case .conflicts: ConflictsView()
        }
    }
}

// MARK: - Home-screen quick actions

enum QuickActions {
    static let notice = Notification.Name("marrow.quickAction")
    static var pending: String? {
        didSet { if pending != nil { NotificationCenter.default.post(name: notice, object: nil) } }
    }
}

final class AppDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        UNUserNotificationCenter.current().delegate = ReminderCenter.shared
        Reminders.registerCategory()
        return true
    }

    func application(_ application: UIApplication, configurationForConnecting session: UISceneSession, options: UIScene.ConnectionOptions) -> UISceneConfiguration {
        if let item = options.shortcutItem { QuickActions.pending = item.type }
        let config = UISceneConfiguration(name: nil, sessionRole: session.role)
        config.delegateClass = SceneDelegate.self
        return config
    }
}

final class SceneDelegate: NSObject, UIWindowSceneDelegate {
    func windowScene(_ windowScene: UIWindowScene, performActionFor shortcutItem: UIApplicationShortcutItem, completionHandler: @escaping (Bool) -> Void) {
        QuickActions.pending = shortcutItem.type
        completionHandler(true)
    }
}

// MARK: - Screenshot runs

/// `-demoVault <folder> -screen <name> [-theme dark]`: opens a copy of the
/// folder and shows one screen, for the screenshot tests.
struct Demo {
    let vault: URL
    let dataDir: URL
    let screen: String
    let theme: String?

    static func fromArguments() -> Demo? {
        let args = ProcessInfo.processInfo.arguments
        func value(_ flag: String) -> String? {
            guard let i = args.firstIndex(of: flag), i + 1 < args.count else { return nil }
            return args[i + 1]
        }
        guard let source = value("-demoVault") else { return nil }
        let tmp = FileManager.default.temporaryDirectory.appendingPathComponent("demo-\(UUID().uuidString)", isDirectory: true)
        let vault = tmp.appendingPathComponent("Demo vault", isDirectory: true)
        try? FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: true)
        try? FileManager.default.copyItem(at: URL(fileURLWithPath: source), to: vault)
        return Demo(vault: vault, dataDir: tmp.appendingPathComponent("data"), screen: value("-screen") ?? "today", theme: value("-theme"))
    }

    @MainActor
    func show(_ router: Router) {
        let parts = screen.split(separator: ":", maxSplits: 1).map(String.init)
        let arg = parts.count > 1 ? parts[1] : ""
        switch parts[0] {
        case "notes": router.tab = .notes
        case "folder": router.tab = .notes; router.push(.folder(arg))
        case "note": router.tab = .notes; router.push(.note(arg))
        case "edit": router.tab = .notes; router.push(.note(arg, edit: true))
        case "search": router.tab = .search; router.searchText = arg
        case "tasks": router.tab = .tasks
        case "ask": router.tab = .ask
        case "calendar": router.tab = .today; router.push(.calendar)
        case "graph": router.tab = .notes; router.push(.graph())
        case "canvas": router.tab = .notes; router.push(.canvas(arg))
        case "timemachine": router.tab = .notes; router.push(.timeMachine)
        case "settings": router.tab = .today; router.push(.settings)
        case "sync": router.tab = .today; router.push(.sync)
        case "capture": router.capture = true
        default: router.tab = .today
        }
    }
}
