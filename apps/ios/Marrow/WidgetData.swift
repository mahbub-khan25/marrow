import Foundation
import WidgetKit

/// Keeps the home-screen widget's copy of today's tasks up to date.
enum WidgetData {
    /// How many dated tasks are copied (the widget shows what fits).
    private static let keep = 40

    static func refresh(_ marrow: Marrow) async {
        guard let tasks = try? marrow.tasks(includeDone: false) else { return }
        let items = tasks
            .filter { !$0.done && $0.due != nil }
            .sorted { ($0.due ?? "", $0.remind ?? "99") < ($1.due ?? "", $1.remind ?? "99") }
            .prefix(keep)
            .map { TodaySnapshot.Item(path: $0.path, line: Int($0.line), text: $0.text, due: $0.due ?? "", remind: $0.remind) }
        let next = TodaySnapshot(items: Array(items))
        // Widgets have a daily budget of reloads: only when something changed.
        guard next != TodaySnapshot.read() else { return }
        next.write()
        WidgetCenter.shared.reloadTimelines(ofKind: "today")
    }

    /// The vault was closed: the widget goes back to "Open Marrow".
    static func clear() {
        TodaySnapshot.clear()
        WidgetCenter.shared.reloadTimelines(ofKind: "today")
    }
}

/// A `marrow://` link that opened the app (from the widget), waiting for
/// the vault to be open.
enum OpenedLink {
    static let notice = Notification.Name("marrow.openedLink")
    static var pending: URL? {
        didSet { if pending != nil { NotificationCenter.default.post(name: notice, object: nil) } }
    }
}
