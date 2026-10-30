import Foundation

/// Things shared to Marrow from other apps. The share extension can't open
/// the vault, so it leaves each one in the app group's Inbox folder, and the
/// app files them into the vault the next time it opens.
struct SharedItem: Codable {
    /// A web page's address, when one was shared.
    var url: String?
    /// The page as Safari showed it (so it isn't downloaded again).
    var html: String?
    /// The part of the page that was selected (HTML).
    var selection: String?
    /// Words that were shared, or typed in the share sheet.
    var text: String
    /// Save the whole page in Clippings (not just its link in today's note).
    var savePage: Bool
    /// Picture files beside the item, by name.
    var images: [String]
}

enum ShareInbox {
    private static let itemFile = "item.json"

    static var folder: URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: TodaySnapshot.group)?
            .appendingPathComponent("Inbox", isDirectory: true)
    }

    /// Leaves an item (with its pictures: data and file extension) in the inbox.
    static func add(_ item: SharedItem, pictures: [(Data, String)]) throws {
        guard let folder else { throw CocoaError(.fileNoSuchFile) }
        let stamp = Int(Date().timeIntervalSince1970 * 1000)
        let dir = folder.appendingPathComponent("\(stamp)-\(UUID().uuidString.prefix(8))", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        var item = item
        item.images = []
        for (i, p) in pictures.enumerated() {
            let name = "picture-\(i + 1).\(p.1)"
            try p.0.write(to: dir.appendingPathComponent(name))
            item.images.append(name)
        }
        try JSONEncoder().encode(item).write(to: dir.appendingPathComponent(itemFile), options: .atomic)
    }

    /// What's waiting, oldest first: each item's folder and the item.
    static func waiting() -> [(URL, SharedItem)] {
        guard let folder, let dirs = try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil) else { return [] }
        return dirs.sorted { $0.lastPathComponent < $1.lastPathComponent }.compactMap { dir in
            guard let data = try? Data(contentsOf: dir.appendingPathComponent(itemFile)),
                  let item = try? JSONDecoder().decode(SharedItem.self, from: data) else { return nil }
            return (dir, item)
        }
    }

    static func remove(_ dir: URL) {
        try? FileManager.default.removeItem(at: dir)
    }
}
