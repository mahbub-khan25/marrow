import Foundation

/// Files what was shared to Marrow (left in the inbox by the share
/// extension): pages into Clippings, words and pictures into today's note.
enum ShareImport {
    @MainActor
    static func run(_ store: Store) async {
        guard store.vault != nil else { return }
        let waiting = ShareInbox.waiting()
        guard !waiting.isEmpty else { return }
        var pages: [String] = []
        var others = 0
        for (dir, item) in waiting {
            do {
                let page = try await store.run { m in try file(m, dir: dir, item: item) }
                ShareInbox.remove(dir)
                if let page { pages.append(nameOf(page)) } else { others += 1 }
            } catch {
                // Kept for next time (offline, say); tell why once.
                store.say(error)
                return
            }
        }
        if pages.count == 1 && others == 0 {
            store.say(String(localized: "Saved “\(pages[0])” in Clippings"))
        } else {
            store.say(String(localized: "Added what you shared."))
        }
    }

    /// Files one item; returns the clipped page's path, if it was a page.
    private static func file(_ m: Marrow, dir: URL, item: SharedItem) throws -> String? {
        if item.savePage, let url = item.url {
            do {
                return try m.clipPage(url: url, html: item.html, selection: item.selection, folder: nil, today: todayString())
            } catch {
                // Can't get the page (offline): keep the link in today's note.
                let words = [item.text, url].filter { !$0.isEmpty }.joined(separator: " ")
                _ = try m.capture(today: todayString(), time: nowTime(), text: words, asTask: false, extra: [])
                return nil
            }
        }
        var embeds: [String] = []
        for (i, name) in item.images.enumerated() {
            guard let data = try? Data(contentsOf: dir.appendingPathComponent(name)) else { continue }
            let ext = (name as NSString).pathExtension
            let file = "Photo \(todayString()) \(nowTime().replacingOccurrences(of: ":", with: ""))\(item.images.count > 1 ? "-\(i + 1)" : "").\(ext)"
            let saved = try m.saveAttachment(folder: "Attachments", name: file, bytes: data)
            embeds.append("![[\(nameOf(saved))]]")
        }
        let words = item.text.trimmingCharacters(in: .whitespacesAndNewlines)
        let body = words.isEmpty ? (embeds.isEmpty ? "" : String(localized: "Photo")) : words
        guard !body.isEmpty else { return nil }
        _ = try m.capture(today: todayString(), time: nowTime(), text: body, asTask: false, extra: embeds)
        return nil
    }
}
