import Foundation

/// Today's tasks, copied by the app for the home-screen widget (which can't
/// open the vault itself). Kept in the app group both share.
struct TodaySnapshot: Codable, Equatable {
    struct Item: Codable, Hashable {
        let path: String
        let line: Int
        let text: String
        let due: String
        var remind: String?
    }

    var items: [Item]

    static let group = "group.app.marrow.notes"
    private static let file = "today.json"

    private static var url: URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: group)?.appendingPathComponent(file)
    }

    /// nil until the app has written one (no vault yet, or no app group).
    static func read() -> TodaySnapshot? {
        guard let url, let data = try? Data(contentsOf: url) else { return nil }
        return try? JSONDecoder().decode(TodaySnapshot.self, from: data)
    }

    func write() {
        guard let url = Self.url, let data = try? JSONEncoder().encode(self) else { return }
        try? data.write(to: url, options: .atomic)
    }

    static func clear() {
        if let url { try? FileManager.default.removeItem(at: url) }
    }

    /// What's due on `day` (`YYYY-MM-DD`) or before.
    func due(on day: String) -> [Item] { items.filter { $0.due <= day } }

    /// `marrow://` links the widget opens the app with.
    static func link(_ action: String, _ query: [String: String] = [:]) -> URL {
        var c = URLComponents()
        c.scheme = "marrow"
        c.host = action
        if !query.isEmpty { c.queryItems = query.sorted { $0.key < $1.key }.map { URLQueryItem(name: $0.key, value: $0.value) } }
        return c.url!
    }
}

/// `YYYY-MM-DD` for a local date.
func dayString(_ d: Date) -> String {
    let c = Calendar.current.dateComponents([.year, .month, .day], from: d)
    return String(format: "%04d-%02d-%02d", c.year!, c.month!, c.day!)
}

/// A task's Markdown as plain words (links and marks dropped).
func plainTask(_ md: String) -> String {
    var s = md
    for (pattern, template) in [
        (#"\[\[([^\]|]+)\|([^\]]+)\]\]"#, "$2"),
        (#"\[\[([^\]]+)\]\]"#, "$1"),
        (#"\[([^\]]+)\]\([^)]*\)"#, "$1"),
        (#"(\*\*|__|~~|`|==)"#, ""),
    ] {
        s = s.replacingOccurrences(of: pattern, with: template, options: .regularExpression)
    }
    return s.trimmingCharacters(in: .whitespaces)
}
