import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/*
 * Home: the person's own board, the first thing the app shows. It's the
 * vault's `Home.canvas`, the same board as on the computer and on Android:
 * live cards (sticky notes, tasks picked by any filter, a note or part of
 * one, lists of notes, a greeting, a quick note, shortcuts, a calendar,
 * today's note, a countdown, a clock, rediscover), pictures, links, text and
 * drawings. Pinch and drag to look around; "Edit" moves, resizes, adds and
 * restyles cards; Templates swaps in a ready-made board or one someone shared.
 */

private let homePath = "Home.canvas"

// MARK: - JSON

private typealias Obj = [String: Any]

private func jsonObj(_ s: String) -> Obj {
    guard let d = s.data(using: .utf8), let o = (try? JSONSerialization.jsonObject(with: d)) as? Obj else { return [:] }
    return o
}

private func jsonText(_ o: Obj) -> String {
    guard let d = try? JSONSerialization.data(withJSONObject: o, options: [.sortedKeys]), let s = String(data: d, encoding: .utf8) else { return "{}" }
    return s
}

private extension Dictionary where Key == String, Value == Any {
    func str(_ k: String, _ d: String = "") -> String { self[k] as? String ?? d }
    func int(_ k: String, _ d: Int) -> Int { (self[k] as? NSNumber)?.intValue ?? d }
    func bool(_ k: String, _ d: Bool = false) -> Bool { (self[k] as? NSNumber)?.boolValue ?? d }
    func sub(_ k: String) -> [String: Any] { self[k] as? [String: Any] ?? [:] }
    func strings(_ k: String) -> [String]? { self[k] as? [String] }
    /// A copy with `k` set (or removed, for nothing or "").
    func with(_ k: String, _ v: Any?) -> [String: Any] {
        var o = self
        if let v, !((v as? String)?.isEmpty ?? false) { o[k] = v } else { o.removeValue(forKey: k) }
        return o
    }
}

/// A card's live part: its kind, its settings and its look (from `extra`).
private struct HLive {
    let widget: String
    let cfg: Obj
    let style: Obj
}

private extension CanvasNode {
    var live: HLive {
        let o = jsonObj(extra)
        return HLive(widget: o.str("widget"), cfg: o.sub("cfg"), style: o.sub("style"))
    }

    /// The same card with changed settings or look, kept with its other fields.
    func with(cfg: Obj? = nil, style: Obj? = nil) -> CanvasNode {
        var o = jsonObj(extra)
        if let cfg { o["cfg"] = cfg }
        if let style { o["style"] = style }
        var n = self
        n.extra = jsonText(o)
        return n
    }
}

private func homeId() -> String { String(UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(16)).lowercased() }

/// The board's own settings (`home` in the file).
private func settingsOf(_ b: Board) -> Obj { jsonObj(b.extra).sub("home") }

private func withSettings(_ b: Board, _ s: Obj) -> Board {
    var o = jsonObj(b.extra)
    o["home"] = s
    var nb = b
    nb.extra = jsonText(o)
    return nb
}

private let widgetKinds = ["sticky", "tasks", "note", "notes", "greeting", "capture", "shortcuts", "calendar", "daily", "countdown", "clock", "rediscover", "welcome"]

private let cardSizes: [String: (Double, Double)] = [
    "sticky": (240, 200), "tasks": (380, 360), "note": (360, 320), "notes": (300, 300), "greeting": (480, 130), "capture": (440, 96),
    "shortcuts": (480, 70), "calendar": (300, 300), "daily": (360, 320), "countdown": (240, 140), "clock": (220, 110), "rediscover": (360, 280), "welcome": (520, 300),
]

private func widgetName(_ w: String) -> LocalizedStringKey {
    switch w {
    case "sticky": return "Sticky note"
    case "tasks": return "Tasks"
    case "note": return "A note"
    case "notes": return "List of notes"
    case "greeting": return "Greeting"
    case "capture": return "Quick note"
    case "shortcuts": return "Shortcuts"
    case "calendar": return "Calendar"
    case "daily": return "Today's note"
    case "countdown": return "Countdown"
    case "clock": return "Clock"
    case "rediscover": return "Rediscover"
    case "welcome": return "Getting started"
    case "picture": return "Picture"
    case "link": return "Link"
    default: return "Text card"
    }
}

/// A new live card of `kind`, with the settings it starts with (as on the computer).
private func newCard(_ kind: String, x: Double, y: Double) -> CanvasNode {
    let (w, h) = cardSizes[kind] ?? (300, 200)
    let cfg: Obj
    switch kind {
    case "tasks": cfg = jsonObj(#"{"title":"Next 7 days","range":"days","days":7,"from":"","to":"","overdue":true,"undated":false,"done":false,"tag":"","folder":"","note":"","text":"","group":true,"showNote":true}"#)
    case "note": cfg = ["path": "", "section": ""]
    case "notes": cfg = jsonObj(#"{"title":"Recently edited","kind":"recent","value":"","limit":8}"#)
    case "greeting": cfg = ["name": "", "date": true, "stats": true]
    case "capture": cfg = ["target": "daily", "path": "", "task": false]
    case "shortcuts": cfg = ["items": ["new", "today", "search", "ask"]]
    case "countdown": cfg = ["title": "", "date": ""]
    case "clock": cfg = ["h24": false]
    default: cfg = [:]
    }
    var extra: Obj = ["widget": kind, "cfg": cfg]
    switch kind {
    case "sticky": extra["style"] = ["bg": "#ffd84d", "tint": 45, "look": "paper"] as Obj
    case "greeting", "shortcuts": extra["style"] = ["look": "none"] as Obj
    default: break
    }
    let label = kind == "sticky" ? "" : "\(kind.prefix(1).uppercased() + kind.dropFirst()) (Marrow Home card)"
    return CanvasNode(id: homeId(), kind: "text", x: x, y: y, width: w, height: h, text: label, file: "", url: "", label: "", color: "", shape: "", fill: "", ink: nil, extra: jsonText(extra))
}

private func isImage(_ f: String) -> Bool {
    f.range(of: #"\.(png|jpe?g|gif|webp|bmp|heic|avif)$"#, options: [.regularExpression, .caseInsensitive]) != nil
}

/// Shown on the board as a card of its own (not drawn with the drawings).
private func isCard(_ n: CanvasNode) -> Bool { !n.live.widget.isEmpty || n.kind == "link" || (n.kind == "file" && isImage(n.file)) }

// MARK: - Looks

private let homePalette = ["#e03e3e", "#e8780c", "#ffd84d", "#2f9e44", "#0b8cb5", "#8e44ad", "#e64980", "#868e96"]
private let canvasColors = ["1": "#e03e3e", "2": "#e8780c", "3": "#d4a106", "4": "#2f9e44", "5": "#0b8cb5", "6": "#8e44ad"]

private func hexColor(_ s: String) -> Color? {
    let h = canvasColors[s] ?? s
    guard h.count == 7, h.hasPrefix("#"), let v = UInt32(h.dropFirst(), radix: 16) else { return nil }
    return Color(uiColor: UIColor(hex: v))
}

private struct HLook {
    var look: String
    var color: Color?
    var tint: Double
    var fg: Color
    var fontName: String
    var scale: CGFloat
    var align: TextAlignment
    var frameAlign: Alignment
    var radius: CGFloat
    var alpha: Double

    init(_ st: Obj) {
        look = st.str("look").isEmpty ? "card" : st.str("look")
        color = hexColor(st.str("bg"))
        tint = Double(st.int("tint", 18)) / 100
        fg = hexColor(st.str("fg")) ?? Color.ink
        fontName = st.str("font")
        switch st.str("size") {
        case "s": scale = 0.87
        case "l": scale = 1.2
        case "xl": scale = 1.6
        default: scale = 1
        }
        switch st.str("align") {
        case "center": align = .center; frameAlign = .top
        case "right": align = .trailing; frameAlign = .topTrailing
        default: align = .leading; frameAlign = .topLeading
        }
        let paperish = ["paper", "lined", "grid", "tape", "pin", "fold"].contains(look)
        radius = CGFloat(st.int("radius", paperish ? 3 : 12))
        alpha = Double(st.int("opacity", 100)) / 100
    }

    func font(_ base: CGFloat = 16) -> Font {
        let s = base * scale
        switch fontName {
        case "read": return .custom("Literata", size: s)
        case "classic": return .custom("EB Garamond", size: s)
        case "mono": return .system(size: s, design: .monospaced)
        case "hand": return .custom("Noteworthy", size: s)
        default: return .system(size: s)
        }
    }

    var shape: AnyShape { look == "fold" ? AnyShape(FoldShape()) : AnyShape(RoundedRectangle(cornerRadius: radius, style: .continuous)) }
}

private struct FoldShape: Shape {
    func path(in r: CGRect) -> Path {
        let f: CGFloat = 22
        var p = Path()
        p.move(to: r.origin)
        p.addLine(to: CGPoint(x: r.maxX, y: r.minY))
        p.addLine(to: CGPoint(x: r.maxX, y: r.maxY - f))
        p.addLine(to: CGPoint(x: r.maxX - f, y: r.maxY))
        p.addLine(to: CGPoint(x: r.minX, y: r.maxY))
        p.closeSubpath()
        return p
    }
}

/// A card's frame, its look's extras (tape, pin, lines) and its text style.
private struct CardBody<Content: View>: View {
    let l: HLook
    let framed: Bool
    @ViewBuilder let content: () -> Content

    var body: some View {
        ZStack(alignment: .top) {
            Group {
                if framed {
                    content()
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                        .background { fill.overlay { lines } }
                        .clipShape(l.shape)
                        .overlay {
                            if l.look == "outline" { l.shape.stroke(Color.accent, lineWidth: 2) }
                            else if l.look == "glass" { l.shape.stroke(l.fg.opacity(0.12), lineWidth: 1) }
                        }
                        .shadow(color: .black.opacity(["none", "outline", "glass"].contains(l.look) ? 0 : 0.13), radius: l.look == "card" ? 3 : 5, y: 2)
                } else {
                    content().frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
            }
            .opacity(l.alpha)
            if l.look == "tape" {
                Rectangle().fill(Color(red: 0.96, green: 0.94, blue: 0.88).opacity(0.85)).frame(width: 84, height: 20).rotationEffect(.degrees(-3)).offset(y: -9)
            } else if l.look == "pin" {
                Circle().fill(Color(red: 0.78, green: 0.16, blue: 0.16)).frame(width: 16, height: 16).shadow(radius: 1).offset(y: -7)
            }
        }
        .foregroundStyle(l.fg)
        .font(l.font())
        .multilineTextAlignment(l.align)
    }

    @ViewBuilder private var fill: some View {
        switch l.look {
        case "none", "outline": Color.clear
        case "glass": Rectangle().fill(.ultraThinMaterial)
        default:
            if let c = l.color { ZStack { Color.paper; c.opacity(l.tint) } }
            else if l.look == "paper" { ZStack { Color.paper; Color(uiColor: UIColor(hex: 0xFFD84D)).opacity(0.45) } }
            else { Color.card }
        }
    }

    @ViewBuilder private var lines: some View {
        if l.look == "lined" || l.look == "grid" {
            let look = l.look
            let ink = l.fg
            Canvas { c, size in
                if look == "lined" {
                    var y: CGFloat = 34
                    while y < size.height {
                        var p = Path(); p.move(to: CGPoint(x: 0, y: y)); p.addLine(to: CGPoint(x: size.width, y: y))
                        c.stroke(p, with: .color(Color(red: 0.29, green: 0.56, blue: 0.85).opacity(0.35)), lineWidth: 1)
                        y += 22
                    }
                } else {
                    var p = Path()
                    var x: CGFloat = 16
                    while x < size.width { p.move(to: CGPoint(x: x, y: 0)); p.addLine(to: CGPoint(x: x, y: size.height)); x += 16 }
                    var y: CGFloat = 16
                    while y < size.height { p.move(to: CGPoint(x: 0, y: y)); p.addLine(to: CGPoint(x: size.width, y: y)); y += 16 }
                    c.stroke(p, with: .color(ink.opacity(0.09)), lineWidth: 1)
                }
            }
            .allowsHitTesting(false)
        }
    }
}

// MARK: - Screen

private enum HSheet: String, Identifiable {
    case add, card, board, templates, export
    var id: String { rawValue }
}

private struct HPending {
    let name: String
    let board: Board
}

private let marrowHomeType = UTType(filenameExtension: "marrowhome", conformingTo: .json) ?? .json

private struct HomeTemplateDoc: FileDocument {
    static var readableContentTypes: [UTType] { [marrowHomeType, .json] }
    var text: String
    init(text: String) { self.text = text }
    init(configuration: ReadConfiguration) throws { text = String(decoding: configuration.file.regularFileContents ?? Data(), as: UTF8.self) }
    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper { FileWrapper(regularFileWithContents: Data(text.utf8)) }
}

struct HomeView: View {
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var board: Board?
    @State private var edit = false
    @State private var listChoice: Bool?
    @State private var selected: String?
    @State private var sheet: HSheet?
    @State private var past: [Board] = []
    @State private var saving: Task<Void, Never>?
    @State private var asking: Int?
    @State private var pending: HPending?
    @State private var importing = false
    @State private var exportName = "My Home"
    @State private var exportWords = false
    @State private var exportDoc: HomeTemplateDoc?
    @State private var exportShown = false

    /// Phones show the cards as a list; the board itself is a tap away (and for editing).
    private var asList: Bool { !edit && (listChoice ?? (sizeClass == .compact)) }

    var body: some View {
        VStack(spacing: 0) {
            bar
            if edit {
                Text("Drag cards to move them, drag the corner to resize, tap one to change it.")
                    .font(.footnote).foregroundStyle(Color.inkSoft)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 16).padding(.bottom, 4)
            }
            ZStack(alignment: .bottomTrailing) {
                if let b = board {
                    if asList {
                        HomeList(board: b, onUpdate: { id, f in update(id, undoable: false, f) })
                    } else {
                        HomeBoard(
                            board: b, edit: edit, selected: selected,
                            onSelect: { id in selected = id; if id != nil { sheet = .card } },
                            onMoved: { n in update(n.id) { $0 = n } },
                            onUpdate: { id, f in update(id, undoable: false, f) }
                        )
                    }
                } else {
                    Color.paper
                }
                if edit {
                    Button { sheet = .add } label: {
                        Image(systemName: "plus").font(.title2.weight(.semibold)).foregroundStyle(.white)
                            .frame(width: 56, height: 56).background(Color.accent, in: Circle()).shadow(radius: 4, y: 2)
                    }
                    .accessibilityLabel(Text("Add"))
                    .padding(20)
                }
            }
        }
        .background(Color.paper)
        .toolbar(.hidden, for: .navigationBar)
        .task(id: store.vault?.root) { await load() }
        .onChange(of: store.tick) {
            // Changed elsewhere (sync, the computer): show what's on disk, unless editing here.
            guard store.changed.contains(homePath), saving == nil else { return }
            Task { if let b = try? await store.run({ try $0.readCanvas(path: homePath) }), saving == nil { board = b } }
        }
        .sheet(item: $sheet) { s in
            ScrollView {
                VStack(alignment: .leading, spacing: 14) { sheetContent(s) }
                    .padding(20)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .presentationDetents(s == .card || s == .add ? [.medium, .large] : [.large])
            .presentationDragIndicator(.visible)
        }
        .alert(Text(verbatim: String(localized: "Use “\(askName)”?")), isPresented: Binding(get: { asking != nil || pending != nil }, set: { if !$0 { asking = nil; pending = nil } })) {
            Button("Replace") { choose(replace: true) }
            Button("Add next to it") { choose(replace: false) }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Replace your board with it, or put it next to what's there. Undo brings your board back.")
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [marrowHomeType, .json, .item]) { result in
            guard case .success(let url) = result else { return }
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            let text = (try? String(contentsOf: url, encoding: .utf8)) ?? ""
            if let t = try? homeTemplateRead(text: text) {
                pending = HPending(name: t.name.isEmpty ? String(localized: "Templates") : t.name, board: t.board)
            } else {
                store.say(String(localized: "That file isn't a Marrow Home template."))
            }
        }
        .fileExporter(isPresented: $exportShown, document: exportDoc, contentType: marrowHomeType, defaultFilename: exportName) { result in
            switch result {
            case .success: store.say(String(localized: "Template saved. Share the file with anyone; they import it from Templates."))
            case .failure(let e): store.say(e)
            }
        }
    }

    private var askName: String {
        if let pending { return pending.name }
        if let asking, asking < homeTemplates().count { return homeTemplates()[asking].name }
        return ""
    }

    // MARK: the bar

    private var bar: some View {
        HStack(spacing: 2) {
            Text("Home").font(.display(28)).foregroundStyle(Color.ink)
            Spacer()
            if edit && !past.isEmpty {
                tool("arrow.uturn.backward", "Undo") {
                    let b = past.removeLast()
                    board = b
                    save(b)
                }
            }
            if edit { tool("paintpalette", "Board") { selected = nil; sheet = .board } }
            if !edit {
                tool(asList ? "square.grid.2x2" : "rectangle.grid.1x2", asList ? LocalizedStringKey("Show as a board") : LocalizedStringKey("Show as a list")) { listChoice = !asList }
            }
            tool("rectangle.3.group", "Templates") { sheet = .templates }
            if !edit, let s = store.sync {
                tool(s.syncing ? "arrow.triangle.2.circlepath.icloud" : (s.problem != nil && s.problem != .offline) ? "exclamationmark.icloud" : "checkmark.icloud", "Sync") { router.push(.sync) }
            }
            if !edit { tool("gearshape", "Settings") { router.push(.settings) } }
            Button {
                edit.toggle()
                selected = nil
            } label: {
                Label(edit ? LocalizedStringKey("Done") : LocalizedStringKey("Edit"), systemImage: edit ? "checkmark" : "pencil")
                    .font(.subheadline.weight(.semibold))
            }
            .buttonStyle(.bordered)
            .tint(Color.accent)
            .padding(.leading, 4)
        }
        .padding(.leading, 16).padding(.trailing, 10).padding(.top, 8).padding(.bottom, 6)
    }

    private func tool(_ symbol: String, _ label: LocalizedStringKey, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).font(.title3).foregroundStyle(Color.inkSoft).frame(width: 38, height: 38)
        }
        .accessibilityLabel(Text(label))
    }

    // MARK: loading and saving

    private func load() async {
        guard store.vault != nil else { return }
        let few = (store.vault?.notes ?? 0) < 15
        do {
            let b = try await store.run { m -> Board in
                // Only a missing Home is made anew; one that can't be read is left alone.
                if (try? m.readNote(path: homePath)) != nil { return try m.readCanvas(path: homePath) }
                let s = homeStarterBoard(rediscover: true, welcome: few)
                try m.writeCanvas(path: homePath, canvas: s, expected: nil)
                return s
            }
            board = b
            past = []
        } catch { store.say(error) }
    }

    private func save(_ b: Board) {
        saving?.cancel()
        saving = Task { @MainActor in
            try? await Task.sleep(nanoseconds: 500_000_000)
            if Task.isCancelled { return }
            do { try await store.run { try $0.writeCanvas(path: homePath, canvas: b, expected: nil) } } catch { store.say(error) }
            saving = nil
        }
    }

    private func change(_ next: Board, undoable: Bool = true) {
        if undoable, let b = board {
            past.append(b)
            if past.count > 60 { past.removeFirst() }
        }
        board = next
        save(next)
    }

    private func update(_ id: String, undoable: Bool = true, _ f: (inout CanvasNode) -> Void) {
        guard var b = board, let i = b.nodes.firstIndex(where: { $0.id == id }) else { return }
        f(&b.nodes[i])
        change(b, undoable: undoable)
    }

    /// A template's board, replacing this one or added to its right.
    private func choose(replace: Bool) {
        let name = askName
        var t: Board?
        if let pending { t = pending.board } else if let asking { t = homeTemplateBoard(index: UInt32(asking)) }
        asking = nil
        pending = nil
        guard let t, let now = board else { return }
        if replace {
            change(t)
        } else {
            let right = (now.nodes.map { $0.x + $0.width }.max() ?? 0) + 120
            let left = t.nodes.map(\.x).min() ?? 0
            var b = now
            b.nodes += t.nodes.map { var n = $0; n.x = n.x - left + right; return n }
            b.edges += t.edges
            change(b)
        }
        store.say(String(localized: "“\(name)” is on your Home."))
    }

    // MARK: sheets

    @ViewBuilder
    private func sheetContent(_ s: HSheet) -> some View {
        switch s {
        case .add:
            Text("Add").font(.title2.weight(.semibold))
            WrapLayout(spacing: 8, lineSpacing: 8) {
                ForEach(widgetKinds, id: \.self) { w in
                    Button { add(w) } label: { Text(widgetName(w)) }.buttonStyle(.bordered)
                }
            }
        case .card:
            if let b = board, let n = b.nodes.first(where: { $0.id == selected }) {
                CardSheet(
                    n: n,
                    onChange: { next in update(n.id) { $0 = next } },
                    onFront: { var nb = b; nb.nodes = b.nodes.filter { $0.id != n.id } + [n]; change(nb) },
                    onBack: { var nb = b; nb.nodes = [n] + b.nodes.filter { $0.id != n.id }; change(nb) },
                    onCopy: {
                        var c = n
                        c.id = homeId()
                        c.x += 30
                        c.y += 30
                        var nb = b
                        nb.nodes.append(c)
                        change(nb)
                        selected = c.id
                    },
                    onRemove: {
                        var nb = b
                        nb.nodes.removeAll { $0.id == n.id }
                        nb.edges.removeAll { $0.fromNode == n.id || $0.toNode == n.id }
                        change(nb)
                        selected = nil
                        sheet = nil
                    }
                )
            }
        case .board:
            if let b = board { BoardSheet(s: settingsOf(b)) { change(withSettings(b, $0)) } }
        case .templates:
            Text("Start from a template").font(.title2.weight(.semibold))
            ForEach(Array(homeTemplates().enumerated()), id: \.offset) { i, t in
                Button {
                    sheet = nil
                    asking = i
                } label: {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(t.name).font(.body.weight(.semibold)).foregroundStyle(Color.ink)
                        Text(t.about).font(.footnote).foregroundStyle(Color.inkSoft)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(14)
                    .background(Color.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                }
                .buttonStyle(.plain)
            }
            Divider()
            Button("Import a template…") {
                sheet = nil
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { importing = true }
            }
            Button("Save this board as a template…") { sheet = .export }
        case .export:
            Text("Save this board as a template…").font(.title2.weight(.semibold))
            TextField("Name for the template", text: $exportName).textFieldStyle(.roundedBorder)
            Toggle("Keep the words on sticky notes", isOn: $exportWords)
            Button {
                guard let b = board else { return }
                let name = exportName.trimmingCharacters(in: .whitespaces).isEmpty ? "My Home" : exportName.trimmingCharacters(in: .whitespaces)
                exportName = name
                exportDoc = HomeTemplateDoc(text: homeTemplateWrite(board: b, name: name, words: exportWords, today: todayString()))
                sheet = nil
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.45) { exportShown = true }
            } label: {
                Text("OK").frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
        }
    }

    private func add(_ kind: String) {
        guard var b = board else { return }
        // Below everything, at the left: easy to find, then move.
        let y = (b.nodes.map { $0.y + $0.height }.max() ?? 0) + 30
        let x = b.nodes.map(\.x).min() ?? 0
        let n = newCard(kind, x: x, y: y)
        b.nodes.append(n)
        change(b)
        selected = n.id
        sheet = .card
    }
}

// MARK: - The board

private struct HomeBoard: View {
    let board: Board
    let edit: Bool
    let selected: String?
    let onSelect: (String?) -> Void
    let onMoved: (CanvasNode) -> Void
    let onUpdate: (String, (inout CanvasNode) -> Void) -> Void
    @Environment(Store.self) private var store
    @State private var k: CGFloat = 0
    @State private var tx: CGFloat = 0
    @State private var ty: CGFloat = 0
    @State private var panBase: CGPoint?
    @State private var zoomBase: (k: CGFloat, tx: CGFloat, ty: CGFloat)?

    private var minX: CGFloat { CGFloat(board.nodes.map(\.x).min() ?? 0) }
    private var minY: CGFloat { CGFloat(board.nodes.map(\.y).min() ?? 0) }
    private var width: CGFloat { CGFloat(board.nodes.map { $0.x + $0.width }.max() ?? 400) - minX }
    private var height: CGFloat { CGFloat(board.nodes.map { $0.y + $0.height }.max() ?? 400) - minY }

    var body: some View {
        let s = settingsOf(board)
        let pattern = s.str("bg").isEmpty ? "dots" : s.str("bg")
        let kk = k == 0 ? 1 : k
        GeometryReader { geo in
            backdrop(s, pattern)
                .contentShape(Rectangle())
                .onTapGesture { if edit { onSelect(nil) } }
                .overlay(alignment: .topLeading) {
                    world
                        .frame(width: width, height: height, alignment: .topLeading)
                        .scaleEffect(kk, anchor: .topLeading)
                        .offset(x: tx, y: ty)
                }
                .frame(width: geo.size.width, height: geo.size.height, alignment: .topLeading)
                .clipped()
                .coordinateSpace(name: "home")
                .gesture(pan.simultaneously(with: zoom))
                .onAppear { fit(geo.size.width) }
                .onChange(of: geo.size.width) { fit(geo.size.width) }
        }
        // The board's top-left moves when a card goes past it; the view moves with it, so nothing jumps.
        .onChange(of: minX) { old, new in tx += (new - old) * kk }
        .onChange(of: minY) { old, new in ty += (new - old) * kk }
    }

    private func fit(_ w: CGFloat) {
        guard k == 0, !board.nodes.isEmpty, w > 0 else { return }
        // The board's width (not smaller than readable), from the top.
        let kk = min(1, max(0.5, w / (width + 32)))
        k = kk
        let spare = w - width * kk
        // Narrower than the screen: centred; wider: from its left edge.
        tx = spare > 0 ? spare / 2 : 12
        ty = 12
    }

    private var pan: some Gesture {
        DragGesture(minimumDistance: 5)
            .onChanged { v in
                guard zoomBase == nil else { return }
                if panBase == nil { panBase = CGPoint(x: tx, y: ty) }
                tx = panBase!.x + v.translation.width
                ty = panBase!.y + v.translation.height
            }
            .onEnded { _ in panBase = nil }
    }

    private var zoom: some Gesture {
        MagnifyGesture()
            .onChanged { v in
                if zoomBase == nil { zoomBase = (k == 0 ? 1 : k, tx, ty) }
                guard let b = zoomBase else { return }
                let nk = min(2.5, max(0.25, b.k * v.magnification))
                let c = v.startLocation
                tx = c.x - (c.x - b.tx) * nk / b.k
                ty = c.y - (c.y - b.ty) * nk / b.k
                k = nk
            }
            .onEnded { _ in
                zoomBase = nil
                panBase = nil
            }
    }

    @ViewBuilder
    private func backdrop(_ s: Obj, _ pattern: String) -> some View {
        let base: Color = hexColor(s.str("color")) ?? { () -> Color in
            switch pattern {
            case "paper": return Color(light: 0xF6EFDF, dark: 0x2A2620)
            case "cork": return Color(red: 0.79, green: 0.64, blue: 0.42)
            default: return Color.paper
            }
        }()
        let k = self.k == 0 ? 1 : self.k
        let ox = tx, oy = ty
        ZStack {
            base
            if !s.str("image").isEmpty { HomePicture(path: s.str("image"), fill: true) }
            if ["dots", "grid", "lines", "paper"].contains(pattern) {
                Canvas { c, size in
                    let step = 22 * max(k, 0.3)
                    let line = Color.ink.opacity(0.1)
                    var y = (oy.truncatingRemainder(dividingBy: step) + step).truncatingRemainder(dividingBy: step)
                    var p = Path()
                    while y < size.height {
                        if pattern == "lines" || pattern == "grid" { p.move(to: CGPoint(x: 0, y: y)); p.addLine(to: CGPoint(x: size.width, y: y)) }
                        if pattern == "dots" || pattern == "paper" {
                            var x = (ox.truncatingRemainder(dividingBy: step) + step).truncatingRemainder(dividingBy: step)
                            while x < size.width {
                                c.fill(Path(ellipseIn: CGRect(x: x - 1.2, y: y - 1.2, width: 2.4, height: 2.4)), with: .color(Color.ink.opacity(0.18)))
                                x += step
                            }
                        }
                        y += step
                    }
                    if pattern == "grid" {
                        var x = (ox.truncatingRemainder(dividingBy: step) + step).truncatingRemainder(dividingBy: step)
                        while x < size.width { p.move(to: CGPoint(x: x, y: 0)); p.addLine(to: CGPoint(x: x, y: size.height)); x += step }
                    }
                    c.stroke(p, with: .color(line), lineWidth: 1)
                }
                .allowsHitTesting(false)
            }
        }
    }

    private var world: some View {
        let plain = board.nodes.filter { !isCard($0) }
        let edges = board.edges
        let ox = minX, oy = minY
        return ZStack(alignment: .topLeading) {
            // Drawings, shapes, text and note cards: drawn as on the drawing pad.
            if !plain.isEmpty || !edges.isEmpty {
                Canvas { c, _ in
                    c.translateBy(x: -ox, y: -oy)
                    drawBoard(&c, Board(nodes: plain, edges: edges, extra: ""))
                }
                .allowsHitTesting(false)
            }
            ForEach(board.nodes, id: \.id) { n in
                if isCard(n) || edit {
                    HomeCard(n: n, minX: ox, minY: oy, k: k == 0 ? 1 : k, edit: edit, picked: n.id == selected, onSelect: onSelect, onMoved: onMoved, onUpdate: onUpdate)
                }
            }
        }
    }
}

/// One card on the board: its frame and look; in edit mode, dragged to move and by its corner to resize.
private struct HomeCard: View {
    let n: CanvasNode
    let minX: CGFloat
    let minY: CGFloat
    let k: CGFloat
    let edit: Bool
    let picked: Bool
    let onSelect: (String?) -> Void
    let onMoved: (CanvasNode) -> Void
    let onUpdate: (String, (inout CanvasNode) -> Void) -> Void
    @State private var drag: CGSize = .zero
    @State private var grow: CGSize = .zero

    var body: some View {
        let live = n.live
        let l = HLook(live.style)
        let w = max(60, CGFloat(n.width) + grow.width)
        let h = max(40, CGFloat(n.height) + grow.height)
        Group {
            if isCard(n) {
                CardBody(l: l, framed: true) { HomeCardContent(n: n, live: live, edit: edit, onUpdate: onUpdate) }
                    .allowsHitTesting(!edit)
            } else {
                // A drawing or shape: drawn underneath; here only its outline while editing.
                Color.clear
            }
        }
        .frame(width: w, height: h)
        .overlay {
            if edit {
                RoundedRectangle(cornerRadius: max(l.radius, 4))
                    .stroke(Color.accent.opacity(picked ? 1 : 0.25), lineWidth: picked ? 2 : 1)
                    .contentShape(Rectangle())
                    .onTapGesture { onSelect(n.id) }
                    .gesture(
                        DragGesture(minimumDistance: 3, coordinateSpace: .named("home"))
                            .onChanged { v in drag = CGSize(width: v.translation.width / k, height: v.translation.height / k) }
                            .onEnded { _ in
                                let d = drag
                                drag = .zero
                                guard d != .zero else { return }
                                var m = n
                                m.x = (n.x + Double(d.width)).rounded()
                                m.y = (n.y + Double(d.height)).rounded()
                                onMoved(m)
                            }
                    )
            }
        }
        .overlay(alignment: .bottomTrailing) {
            if edit {
                Circle().fill(Color.paper)
                    .overlay(Circle().stroke(Color.accent, lineWidth: 2))
                    .frame(width: 24, height: 24)
                    .padding(10)
                    .contentShape(Rectangle())
                    .offset(x: 18, y: 18)
                    .gesture(
                        DragGesture(minimumDistance: 1, coordinateSpace: .named("home"))
                            .onChanged { v in grow = CGSize(width: v.translation.width / k, height: v.translation.height / k) }
                            .onEnded { _ in
                                let g = grow
                                grow = .zero
                                guard g != .zero else { return }
                                var m = n
                                m.width = max(60, (n.width + Double(g.width)).rounded())
                                m.height = max(40, (n.height + Double(g.height)).rounded())
                                onMoved(m)
                            }
                    )
                    .accessibilityLabel(Text(verbatim: "resize"))
            }
        }
        .offset(x: CGFloat(n.x) - minX + drag.width, y: CGFloat(n.y) - minY + drag.height)
    }
}

/// Phones: the same cards one under another, full width, in reading order
/// (each column of the board top to bottom, then the next column).
private struct HomeList: View {
    let board: Board
    let onUpdate: (String, (inout CanvasNode) -> Void) -> Void

    var body: some View {
        let shown = board.nodes.filter { n in isCard(n) || (n.kind == "text" && n.shape.isEmpty && n.ink == nil && !n.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty) }
        // Columns: cards whose left edges are close together.
        var starts: [Double] = []
        for x in shown.map(\.x).sorted() where starts.isEmpty || x - starts.last! > 150 { starts.append(x) }
        let column = { (n: CanvasNode) -> Int in starts.lastIndex { $0 <= n.x + 1 } ?? 0 }
        let ordered = shown.sorted { a, b in column(a) != column(b) ? column(a) < column(b) : a.y < b.y }
        return ScrollView {
            VStack(spacing: 14) {
                ForEach(ordered, id: \.id) { n in
                    let live = n.live
                    CardBody(l: HLook(live.style), framed: true) {
                        if isCard(n) {
                            HomeCardContent(n: n, live: live, edit: false, onUpdate: onUpdate)
                        } else {
                            Text(n.text).padding(14)
                        }
                    }
                    .frame(height: min(520, max(56, CGFloat(n.height))))
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .readingWidth()
        }
    }
}

private struct HomePicture: View {
    let path: String
    var fill = true
    @Environment(Store.self) private var store
    @State private var image: UIImage?

    var body: some View {
        GeometryReader { geo in
            if let image {
                Image(uiImage: image).resizable().aspectRatio(contentMode: fill ? .fill : .fit)
                    .frame(width: geo.size.width, height: geo.size.height).clipped()
            }
        }
        .task(id: path) {
            let m = store.marrow
            let p = path
            image = await Task.detached { (try? m.filePath(path: p)).flatMap { UIImage(contentsOfFile: $0) } }.value
        }
    }
}

// MARK: - Cards

private struct HomeCardContent: View {
    let n: CanvasNode
    let live: HLive
    let edit: Bool
    let onUpdate: (String, (inout CanvasNode) -> Void) -> Void

    var body: some View {
        if !live.widget.isEmpty {
            HomeWidget(n: n, live: live, edit: edit, onUpdate: onUpdate)
        } else if n.kind == "file" {
            HomePicture(path: n.file)
        } else if n.kind == "link", let url = URL(string: n.url) {
            Link(n.url, destination: url).foregroundStyle(Color.accent).underline().padding(14)
        } else {
            Color.clear
        }
    }
}

private struct Head<Trailing: View>: View {
    let title: String
    let trailing: () -> Trailing

    init(_ title: String, @ViewBuilder trailing: @escaping () -> Trailing) {
        self.title = title
        self.trailing = trailing
    }

    var body: some View {
        HStack {
            Text(title).bold().lineLimit(1)
            Spacer(minLength: 4)
            trailing()
        }
        .padding(.bottom, 6)
    }
}

private extension Head where Trailing == EmptyView {
    init(_ title: String) {
        self.title = title
        trailing = { EmptyView() }
    }
}

private struct Faint: View {
    let text: LocalizedStringKey
    init(_ text: LocalizedStringKey) { self.text = text }
    var body: some View { Text(text).opacity(0.6) }
}

private func dayDate(_ s: String) -> Date? {
    let f = DateFormatter()
    f.calendar = Calendar(identifier: .gregorian)
    f.locale = Locale(identifier: "en_US_POSIX")
    f.dateFormat = "yyyy-MM-dd"
    return f.date(from: s)
}

private func dayLabel(_ day: String, _ today: String) -> String {
    guard let d = dayDate(day), let t = dayDate(today) else { return day }
    let diff = Calendar.current.dateComponents([.day], from: t, to: d).day ?? 99
    if diff == 0 { return String(localized: "Today") }
    if diff == 1 { return String(localized: "Tomorrow") }
    if (2...6).contains(diff) { return d.formatted(.dateTime.weekday(.wide)) }
    return d.formatted(.dateTime.day().month(.abbreviated))
}

private struct HomeWidget: View {
    let n: CanvasNode
    let live: HLive
    let edit: Bool
    let onUpdate: (String, (inout CanvasNode) -> Void) -> Void

    var body: some View {
        Group {
            switch live.widget {
            case "sticky": StickyCard(n: n, edit: edit, onUpdate: onUpdate)
            case "tasks": TasksCard(cfg: live.cfg, edit: edit)
            case "note", "daily": NoteCard(cfg: live.cfg, daily: live.widget == "daily", edit: edit)
            case "notes": NotesCard(cfg: live.cfg, edit: edit)
            case "greeting": GreetingCard(cfg: live.cfg, look: HLook(live.style))
            case "capture": CaptureCard(cfg: live.cfg, edit: edit)
            case "shortcuts": ShortcutsCard(cfg: live.cfg, edit: edit)
            case "calendar": CalendarCard(edit: edit)
            case "countdown": CountdownCard(cfg: live.cfg, look: HLook(live.style))
            case "clock": ClockCard(cfg: live.cfg, look: HLook(live.style))
            case "rediscover": RediscoverCard(edit: edit)
            case "welcome": WelcomeCard()
            default: Faint("This card needs a newer Marrow.")
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

private struct StickyCard: View {
    let n: CanvasNode
    let edit: Bool
    let onUpdate: (String, (inout CanvasNode) -> Void) -> Void
    @State private var text = ""
    @State private var ready = false

    var body: some View {
        TextField("Write something…", text: $text, axis: .vertical)
            .textFieldStyle(.plain)
            .lineSpacing(4)
            .disabled(edit)
            .onAppear { text = n.text; ready = true }
            .onChange(of: n.text) { if n.text != text { text = n.text } }
            .onChange(of: text) {
                guard ready, text != n.text else { return }
                let t = text
                onUpdate(n.id) { $0.text = t }
            }
    }
}

private struct TasksCard: View {
    let cfg: Obj
    let edit: Bool
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var tasks: [TaskItem]?

    var body: some View {
        let day = todayString()
        let open = tasks?.filter { !$0.done }.count ?? 0
        VStack(alignment: .leading, spacing: 0) {
            Head(cfg.str("title").isEmpty ? String(localized: "Tasks") : cfg.str("title")) {
                if open > 0 { Text("\(open)").bold().foregroundStyle(Color.accent) }
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 2) {
                    if let tasks, tasks.isEmpty { Faint("Nothing to do here. 🎉") }
                    ForEach(Array(rows(day).enumerated()), id: \.offset) { _, row in
                        if let g = row.group {
                            Text(g.uppercased())
                                .font(.caption.weight(.bold))
                                .foregroundStyle(g == String(localized: "Overdue") ? Color.danger : Color.inkSoft)
                                .padding(.top, 8).padding(.bottom, 2)
                        }
                        taskRow(row.task)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollDisabled(edit)
        }
        .task(id: "\(store.tick)-\(jsonText(cfg))-\(day)") { await load(day) }
    }

    private func rows(_ day: String) -> [(group: String?, task: TaskItem)] {
        var out: [(group: String?, task: TaskItem)] = []
        var last = ""
        for t in tasks ?? [] {
            let g: String
            if let due = t.due {
                g = due < day && !t.done ? String(localized: "Overdue") : dayLabel(due, day)
            } else {
                g = String(localized: "No date")
            }
            let show = cfg.bool("group", true) && g != last
            if show { last = g }
            out.append((show ? g : nil, t))
        }
        return out
    }

    private func taskRow(_ t: TaskItem) -> some View {
        HStack(alignment: .top, spacing: 8) {
            Button {
                Task {
                    do { _ = try await store.run { try $0.toggleTask(path: t.path, line: t.line) } } catch { store.say(error) }
                }
            } label: {
                Image(systemName: t.done ? "checkmark.square.fill" : "square").font(.title3)
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text(t.done ? "Done" : "Tasks"))
            Button { router.note(t.path, line: Int(t.line)) } label: {
                VStack(alignment: .leading, spacing: 1) {
                    Text(t.text).strikethrough(t.done).lineLimit(3)
                    if cfg.bool("showNote", true) { Text(t.title).font(.caption).opacity(0.55) }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .buttonStyle(.plain)
        }
        .padding(.vertical, 3)
    }

    private func load(_ day: String) async {
        let f = TaskFilter(
            range: cfg.str("range", "all"), days: UInt32(max(1, cfg.int("days", 7))), from: cfg.str("from"), to: cfg.str("to"),
            overdue: cfg.bool("overdue"), undated: cfg.bool("undated"), done: cfg.bool("done"),
            tag: cfg.str("tag"), folder: cfg.str("folder"), note: cfg.str("note"), text: cfg.str("text")
        )
        tasks = try? await store.run { try $0.taskView(today: day, filter: f) }
    }
}

private struct NoteCard: View {
    let cfg: Obj
    let daily: Bool
    let edit: Bool
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @Environment(\.openURL) private var openURL
    @State private var path: String?
    @State private var text: String?
    @State private var gone = false
    @State private var loaded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Head(daily ? String(localized: "Today's note") : path.map(nameOf) ?? String(localized: "A note")) {
                if let path {
                    Button { router.open(path: path) } label: { Image(systemName: "chevron.right") }.buttonStyle(.plain)
                }
            }
            if !loaded {
                EmptyView()
            } else if daily && path == nil {
                Faint("No note for today yet.")
                Button("Start today's note") {
                    Task {
                        do {
                            let p = try await store.run { try $0.openDay(day: todayString()) }
                            router.note(p, edit: true)
                        } catch { store.say(error) }
                    }
                }
                .buttonStyle(.bordered)
                .padding(.top, 6)
            } else if path == nil {
                Faint("Pick a note in Customize.")
            } else if gone {
                Faint("This note was moved or deleted.")
            } else if let text {
                ScrollView {
                    Blocks(blocks: renderMarkdown(text: text), actions: BlockActions(onLink: { l in
                        switch l.kind {
                        case .note: router.open(path: l.target)
                        case .url: if let u = URL(string: l.target) { openURL(u) }
                        case .tag: router.push(.tag(l.target))
                        default: break
                        }
                    }, onOpen: { router.open(path: $0) }, fileOf: { try? store.marrow.filePath(path: $0) }))
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .scrollDisabled(edit)
            } else {
                Faint("No such heading in this note.")
            }
        }
        .task(id: "\(store.tick)-\(jsonText(cfg))-\(daily)") { await load() }
    }

    private func load() async {
        let day = todayString()
        let want = cfg.str("path")
        let section = daily ? "" : cfg.str("section")
        let isDaily = daily
        let r = try? await store.run { m -> (String?, String?, Bool) in
            let found: String?
            if isDaily { found = try m.dailyNote(day: day) } else { found = want.isEmpty ? nil : want }
            guard let p = found else { return (nil, nil, false) }
            guard let t = try? m.readNote(path: p) else { return (p, nil, true) }
            let body: String? = section.isEmpty ? t : noteSection(text: t, heading: section)
            return (p, body?.replacingOccurrences(of: #"^---\n[\s\S]*?\n---\n"#, with: "", options: .regularExpression), false)
        }
        path = r?.0
        text = r?.1
        gone = r?.2 ?? false
        loaded = true
    }
}

private struct NotesCard: View {
    let cfg: Obj
    let edit: Bool
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var items: [String]?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Head(cfg.str("title").isEmpty ? String(localized: "List of notes") : cfg.str("title"))
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    if let items, items.isEmpty { Faint("No notes here yet.") }
                    ForEach(items ?? [], id: \.self) { p in
                        Button { router.open(path: p) } label: {
                            Text(nameOf(p)).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 6).contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            .scrollDisabled(edit)
        }
        .task(id: "\(store.tick)-\(jsonText(cfg))") { await load() }
    }

    private func load() async {
        let kind = cfg.str("kind", "recent")
        let value = cfg.str("value").trimmingCharacters(in: .whitespaces)
        let limit = max(1, cfg.int("limit", 8))
        items = try? await store.run { m -> [String] in
            let all: [String]
            switch kind {
            case "starred": all = try m.starred().map(\.path)
            case "folder":
                all = try m.listFolder(folder: value.trimmingCharacters(in: CharacterSet(charactersIn: "/"))).map(\.path).filter { $0.hasSuffix(".md") || $0.hasSuffix(".canvas") }
            case "tag", "search":
                let q = kind == "tag" ? "tag:#" + (value.hasPrefix("#") ? String(value.dropFirst()) : value) : value
                if value.isEmpty { all = [] } else { all = try m.search(query: q, limit: UInt32(limit)).hits.map(\.path) }
            default: all = try m.recentNotes(limit: UInt32(limit + 1)).map(\.path).filter { $0 != homePath }
            }
            return Array(all.prefix(limit))
        }
    }
}

private struct GreetingCard: View {
    let cfg: Obj
    let look: HLook

    var body: some View {
        let hr = Calendar.current.component(.hour, from: Date())
        let hello = (5...11).contains(hr) ? String(localized: "Good morning") : (12...17).contains(hr) ? String(localized: "Good afternoon") : String(localized: "Good evening")
        let name = cfg.str("name").trimmingCharacters(in: .whitespaces)
        VStack(alignment: .leading, spacing: 2) {
            Text(name.isEmpty ? hello : "\(hello), \(name)").font(.subheadline.weight(.semibold)).foregroundStyle(Color.accent)
            if cfg.bool("date", true) {
                Text(Date().formatted(.dateTime.weekday(.wide).day().month(.wide)))
                    .font(look.fontName.isEmpty ? .display(34 * look.scale) : look.font(34))
            }
        }
        .frame(maxHeight: .infinity, alignment: .leading)
    }
}

private struct CaptureCard: View {
    let cfg: Obj
    let edit: Bool
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var text = ""

    var body: some View {
        let task = cfg.bool("task")
        HStack(spacing: 8) {
            TextField(task ? LocalizedStringKey("Add a task…") : LocalizedStringKey("What's on your mind?"), text: $text)
                .textFieldStyle(.roundedBorder)
                .submitLabel(.send)
                .onSubmit(send)
                .disabled(edit)
            Button { router.captureText = ""; router.capture = true } label: { Image(systemName: "plus.circle").font(.title2) }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Quick capture"))
        }
        .frame(maxHeight: .infinity)
    }

    private func send() {
        let words = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !words.isEmpty else { return }
        text = ""
        let toNote = cfg.str("target") == "note" && !cfg.str("path").isEmpty
        let p = cfg.str("path")
        let task = cfg.bool("task")
        Task {
            do {
                try await store.run { m -> Void in
                    if toNote {
                        let old = try m.readNote(path: p)
                        let end = old.replacingOccurrences(of: #"\s+$"#, with: "", options: .regularExpression)
                        try m.writeNote(path: p, text: end + "\n" + (task ? "- [ ] " : "- ") + words + "\n", expected: nil)
                    } else {
                        _ = try m.capture(today: todayString(), time: nowTime(), text: words, asTask: task, extra: [])
                    }
                }
            } catch {
                text = words
                store.say(error)
            }
        }
    }
}

private struct ShortcutsCard: View {
    let cfg: Obj
    let edit: Bool
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 8) {
                ForEach(cfg.strings("items") ?? ["new", "today", "search", "ask"], id: \.self) { k in
                    if let label = shortcutName(k) {
                        Button { run(k) } label: { Text(label) }.buttonStyle(.bordered).disabled(edit)
                    }
                }
            }
        }
        .frame(maxHeight: .infinity)
    }

    private func run(_ k: String) {
        switch k {
        case "new", "today":
            Task {
                do {
                    let p = try await store.run { m -> String in try k == "new" ? m.createNote(folder: "", name: nil, text: "") : m.openDay(day: todayString()) }
                    router.note(p, edit: true)
                } catch { store.say(error) }
            }
        case "search": router.tab = .search
        case "ask": router.tab = .ask
        case "capture": router.captureText = ""; router.capture = true
        default: break
        }
    }
}

private func shortcutName(_ k: String) -> LocalizedStringKey? {
    switch k {
    case "new": return "New note"
    case "today": return "Today's note"
    case "search": return "Search"
    case "ask": return "Ask your notes"
    case "capture": return "Quick capture"
    default: return nil
    }
}

private struct CalendarCard: View {
    let edit: Bool
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var month = Calendar.current.date(from: Calendar.current.dateComponents([.year, .month], from: Date())) ?? Date()
    @State private var withNotes: Set<Int> = []
    @State private var due: Set<Int> = []

    var body: some View {
        let cal = Calendar.current
        let ym = String(todayString(month).prefix(7))
        let lead = (cal.component(.weekday, from: month) + 5) % 7
        let days = cal.range(of: .day, in: .month, for: month)?.count ?? 30
        let now = Date()
        let thisMonth = cal.isDate(now, equalTo: month, toGranularity: .month)
        let todayN = cal.component(.day, from: now)
        let syms = cal.veryShortWeekdaySymbols
        let names = (0..<7).map { syms[($0 + 1) % 7] }
        VStack(spacing: 4) {
            HStack {
                Button { shift(-1) } label: { Image(systemName: "chevron.left") }.buttonStyle(.plain)
                Text(month.formatted(.dateTime.month(.wide).year())).bold().frame(maxWidth: .infinity)
                Button { shift(1) } label: { Image(systemName: "chevron.right") }.buttonStyle(.plain)
            }
            HStack(spacing: 0) {
                ForEach(0..<7, id: \.self) { i in Text(names[i]).font(.caption2).opacity(0.55).frame(maxWidth: .infinity) }
            }
            ForEach(0..<((lead + days + 6) / 7), id: \.self) { w in
                HStack(spacing: 2) {
                    ForEach(0..<7, id: \.self) { c in
                        let d = w * 7 + c - lead + 1
                        ZStack(alignment: .bottom) {
                            if d >= 1 && d <= days {
                                let isToday = thisMonth && d == todayN
                                Button { open(d) } label: {
                                    Text("\(d)")
                                        .font(.footnote.weight(withNotes.contains(d) ? .bold : .regular))
                                        .foregroundStyle(isToday ? Color.white : Color.primary)
                                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                                        .background(isToday ? Color.accent : Color.clear, in: RoundedRectangle(cornerRadius: 8))
                                }
                                .buttonStyle(.plain)
                                if due.contains(d) { Circle().fill(Color(red: 0.91, green: 0.47, blue: 0.05)).frame(width: 4, height: 4).padding(.bottom, 3) }
                            }
                        }
                        .frame(maxWidth: .infinity, minHeight: 22, maxHeight: 40)
                    }
                }
            }
        }
        .task(id: "\(store.tick)-\(ym)") {
            let r = try? await store.run { m -> (Set<Int>, Set<Int>) in
                let n = Set(try m.daysWithNotes(month: ym).map { Int($0) })
                let d = Set(try m.tasks(includeDone: false).compactMap(\.due).filter { $0.hasPrefix(ym) }.compactMap { Int($0.suffix(2)) })
                return (n, d)
            }
            withNotes = r?.0 ?? []
            due = r?.1 ?? []
        }
    }

    private func shift(_ by: Int) {
        if let m = Calendar.current.date(byAdding: .month, value: by, to: month) { month = m }
    }

    private func open(_ d: Int) {
        guard let date = Calendar.current.date(byAdding: .day, value: d - 1, to: month) else { return }
        let day = todayString(date)
        Task {
            do {
                let p = try await store.run { try $0.openDay(day: day) }
                router.note(p, edit: true)
            } catch { store.say(error) }
        }
    }
}

private struct CountdownCard: View {
    let cfg: Obj
    let look: HLook

    var body: some View {
        let days = dayDate(cfg.str("date")).flatMap { Calendar.current.dateComponents([.day], from: Calendar.current.startOfDay(for: Date()), to: $0).day }
        VStack(spacing: 2) {
            if !cfg.str("title").isEmpty { Text(cfg.str("title")).fontWeight(.semibold) }
            Text(days == nil ? "—" : days == 0 ? String(localized: "Today!") : "\(abs(days!))")
                .font(look.fontName.isEmpty ? .custom("Literata", size: 48 * look.scale).weight(.bold) : look.font(48))
            if let days {
                if days > 0 { Text("days to go").font(.footnote).opacity(0.62) } else if days < 0 { Text("days ago").font(.footnote).opacity(0.62) }
            } else {
                Text("Pick a date in Customize").font(.footnote).opacity(0.62)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct ClockCard: View {
    let cfg: Obj
    let look: HLook

    var body: some View {
        TimelineView(.everyMinute) { ctx in
            VStack(spacing: 2) {
                Text(cfg.bool("h24") ? ctx.date.formatted(.dateTime.hour(.twoDigits(amPM: .omitted)).minute()) : ctx.date.formatted(date: .omitted, time: .shortened))
                    .font(look.fontName.isEmpty ? .custom("Literata", size: 40 * look.scale).weight(.semibold) : look.font(40))
                Text(ctx.date.formatted(.dateTime.weekday(.wide).day().month(.wide))).font(.footnote).opacity(0.62)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

private struct RediscoverCard: View {
    let edit: Bool
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var r: Resurfaced?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Head(String(localized: "Rediscover"))
            ScrollView {
                VStack(alignment: .leading, spacing: 6) {
                    if let r, r.forgotten == nil, r.onThisDay.isEmpty { Faint("Older notes and earlier days show up here as your vault grows.") }
                    if let f = r?.forgotten {
                        Button { router.note(f.path) } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("From your archive").font(.caption).foregroundStyle(Color.accent)
                                Text(f.title).fontWeight(.semibold)
                                if !f.preview.isEmpty { Text(f.preview).font(.subheadline).lineLimit(3) }
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .buttonStyle(.plain)
                    }
                    ForEach(r?.onThisDay ?? [], id: \.path) { o in
                        Button { router.note(o.path) } label: {
                            Text(o.preview.isEmpty ? o.day : o.preview).lineLimit(2).frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            .scrollDisabled(edit)
        }
        .task(id: "\(store.tick)-\(todayString())") {
            let d = todayString()
            r = try? await store.run { try $0.resurface(today: d, nowMs: Int64(Date().timeIntervalSince1970 * 1000), offset: 0) }
        }
    }
}

private struct WelcomeCard: View {
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 8) {
                Head(String(localized: "Getting started"))
                Text(verbatim: "1. ") + Text("Write: tap + in Notes and start typing. Everything saves by itself.")
                Text(verbatim: "2. ") + Text("Link: type [[ and part of a note's name to connect two notes.")
                Text(verbatim: "3. ") + Text("Make Home yours: tap Edit to move, resize and change cards, or add new ones.")
            }
        }
    }
}

// MARK: - Customize

private struct Choice: View {
    let label: LocalizedStringKey
    let options: [(String, LocalizedStringKey)]
    let value: String
    let onPick: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label).font(.subheadline.weight(.semibold))
            WrapLayout(spacing: 6, lineSpacing: 6) {
                ForEach(options.indices, id: \.self) { i in
                    let (v, l) = options[i]
                    Button { onPick(v) } label: {
                        Text(l).font(.subheadline)
                            .padding(.horizontal, 12).padding(.vertical, 7)
                            .foregroundStyle(v == value ? Color.onAccentSoft : Color.ink)
                            .background(v == value ? Color.accentSoft : Color.card, in: Capsule())
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }
}

private struct Field: View {
    let label: LocalizedStringKey
    let value: String
    let onDone: (String) -> Void
    @State private var text = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(.subheadline.weight(.semibold))
            TextField(label, text: $text)
                .textFieldStyle(.roundedBorder)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .onAppear { text = value }
                .onChange(of: text) { if text.trimmingCharacters(in: .whitespaces) != value { onDone(text.trimmingCharacters(in: .whitespaces)) } }
        }
    }
}

private struct Dial: View {
    let label: LocalizedStringKey
    let value: Int
    let range: ClosedRange<Double>
    let onChange: (Int) -> Void
    @State private var v: Double = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.subheadline.weight(.semibold))
            Slider(value: $v, in: range, onEditingChanged: { editing in if !editing { onChange(Int(v.rounded())) } })
        }
        .onAppear { v = Double(value) }
    }
}

private struct Colours: View {
    let label: LocalizedStringKey
    let value: String
    let onPick: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label).font(.subheadline.weight(.semibold))
            WrapLayout(spacing: 8, lineSpacing: 8) {
                Button { onPick("") } label: {
                    ZStack {
                        Circle().stroke(Color.ink.opacity(value.isEmpty ? 1 : 0.2), lineWidth: value.isEmpty ? 2 : 1)
                        Path { p in p.move(to: CGPoint(x: 25, y: 7)); p.addLine(to: CGPoint(x: 7, y: 25)) }.stroke(Color.danger, lineWidth: 2)
                    }
                    .frame(width: 32, height: 32)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("No colour"))
                ForEach(homePalette, id: \.self) { c in
                    Button { onPick(c) } label: {
                        Circle().fill(hexColor(c) ?? .gray)
                            .overlay(Circle().stroke(Color.ink, lineWidth: value.lowercased() == c ? 3 : 0))
                            .frame(width: 32, height: 32)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(Text(verbatim: c))
                }
            }
        }
    }
}

private struct Switch: View {
    let label: LocalizedStringKey
    let on: Bool
    let onChange: (Bool) -> Void

    var body: some View {
        Toggle(label, isOn: Binding(get: { on }, set: { onChange($0) }))
    }
}

private struct CardSheet: View {
    let n: CanvasNode
    let onChange: (CanvasNode) -> Void
    let onFront: () -> Void
    let onBack: () -> Void
    let onCopy: () -> Void
    let onRemove: () -> Void

    var body: some View {
        let live = n.live
        let cfg = live.cfg
        let st = live.style
        let conf = { (k: String, v: Any?) in onChange(n.with(cfg: cfg.with(k, v))) }
        let look = { (k: String, v: Any?) in onChange(n.with(style: st.with(k, v))) }
        let notePath = { (s: String) -> String in s.isEmpty || s.hasSuffix(".md") ? s : s + ".md" }
        VStack(alignment: .leading, spacing: 14) {
            Text(widgetName(live.widget.isEmpty ? (n.kind == "file" ? "picture" : n.kind) : live.widget)).font(.title2.weight(.semibold))
            switch live.widget {
            case "tasks":
                let r = cfg.str("range", "all")
                Field(label: "Title", value: cfg.str("title")) { conf("title", $0) }
                Choice(label: "Which dates", options: [("days", "The next few days"), ("today", "Today"), ("range", "Between two dates"), ("all", "Any date"), ("none", "No date only")], value: r) { conf("range", $0) }
                if r == "days" { Field(label: "How many days", value: "\(cfg.int("days", 7))") { if let d = Int($0) { conf("days", min(366, max(1, d))) } } }
                if r == "range" {
                    Field(label: "From (YYYY-MM-DD)", value: cfg.str("from")) { conf("from", $0) }
                    Field(label: "To (YYYY-MM-DD)", value: cfg.str("to")) { conf("to", $0) }
                }
                Switch(label: "Overdue too", on: cfg.bool("overdue")) { conf("overdue", $0) }
                Switch(label: "Tasks with no date", on: cfg.bool("undated")) { conf("undated", $0) }
                Switch(label: "Done tasks", on: cfg.bool("done")) { conf("done", $0) }
                Field(label: "Only with tag", value: cfg.str("tag")) { conf("tag", $0) }
                Field(label: "Only in folder", value: cfg.str("folder")) { conf("folder", $0) }
                Field(label: "Only in note (its path)", value: cfg.str("note")) { conf("note", $0) }
                Field(label: "Only with words", value: cfg.str("text")) { conf("text", $0) }
                Switch(label: "Group by day", on: cfg.bool("group", true)) { conf("group", $0) }
                Switch(label: "Show which note", on: cfg.bool("showNote", true)) { conf("showNote", $0) }
            case "note":
                Field(label: "Note (its path, like Projects/Plan.md)", value: cfg.str("path")) { conf("path", notePath($0)) }
                Field(label: "Only the part under heading", value: cfg.str("section")) { conf("section", $0) }
            case "notes":
                let kind = cfg.str("kind", "recent")
                Field(label: "Title", value: cfg.str("title")) { conf("title", $0) }
                Choice(label: "Which notes", options: [("recent", "Recently edited"), ("starred", "Starred"), ("tag", "With a tag"), ("folder", "In a folder"), ("search", "Matching a search")], value: kind) { conf("kind", $0) }
                if kind == "tag" { Field(label: "Tag", value: cfg.str("value")) { conf("value", $0) } }
                if kind == "folder" { Field(label: "Folder", value: cfg.str("value")) { conf("value", $0) } }
                if kind == "search" { Field(label: "Search for", value: cfg.str("value")) { conf("value", $0) } }
                Field(label: "How many", value: "\(cfg.int("limit", 8))") { if let d = Int($0) { conf("limit", min(50, max(1, d))) } }
            case "greeting":
                Field(label: "Your name", value: cfg.str("name")) { conf("name", $0) }
                Switch(label: "Show the date", on: cfg.bool("date", true)) { conf("date", $0) }
            case "capture":
                Choice(label: "Where it goes", options: [("daily", "Today's note"), ("note", "A note I choose")], value: cfg.str("target", "daily")) { conf("target", $0) }
                if cfg.str("target") == "note" { Field(label: "Note (its path, like Projects/Plan.md)", value: cfg.str("path")) { conf("path", notePath($0)) } }
                Switch(label: "Add as a task", on: cfg.bool("task")) { conf("task", $0) }
            case "shortcuts":
                let items = cfg.strings("items") ?? []
                ForEach(["new", "today", "search", "ask", "capture"], id: \.self) { k in
                    Switch(label: shortcutName(k) ?? "", on: items.contains(k)) { on in conf("items", on ? items + [k] : items.filter { $0 != k }) }
                }
            case "countdown":
                Field(label: "What for", value: cfg.str("title")) { conf("title", $0) }
                Field(label: "Date (YYYY-MM-DD)", value: cfg.str("date")) { conf("date", $0) }
            case "clock":
                Switch(label: "24-hour clock", on: cfg.bool("h24")) { conf("h24", $0) }
            default:
                EmptyView()
            }
            Divider()
            Text("Look").font(.headline)
            Choice(label: "Style", options: [("", "Card"), ("paper", "Sticky paper"), ("lined", "Lined paper"), ("grid", "Squared paper"), ("tape", "Taped on"), ("pin", "Pinned"), ("fold", "Folded corner"), ("outline", "Outline"), ("glass", "Frosted glass"), ("none", "No background")], value: st.str("look") == "card" ? "" : st.str("look")) { look("look", $0) }
            Colours(label: "Colour", value: st.str("bg")) { look("bg", $0) }
            Dial(label: "Colour strength", value: st.int("tint", 18), range: 5...100) { look("tint", $0) }
            Colours(label: "Text colour", value: st.str("fg")) { look("fg", $0) }
            Choice(label: "Font", options: [("", "App font"), ("read", "Book"), ("classic", "Classic"), ("mono", "Typewriter"), ("hand", "Handwritten")], value: st.str("font")) { look("font", $0) }
            Choice(label: "Text size", options: [("s", "Small"), ("", "Normal"), ("l", "Large"), ("xl", "Huge")], value: st.str("size")) { look("size", $0) }
            Choice(label: "Text alignment", options: [("", "Left"), ("center", "Centre"), ("right", "Right")], value: st.str("align")) { look("align", $0) }
            Dial(label: "Corners", value: st.int("radius", 12), range: 0...40) { look("radius", $0) }
            Dial(label: "See-through", value: st.int("opacity", 100), range: 20...100) { look("opacity", $0 == 100 ? nil : $0) }
            HStack(spacing: 8) {
                Button("Front", action: onFront).buttonStyle(.bordered)
                Button("Back", action: onBack).buttonStyle(.bordered)
                Button("Copy", action: onCopy).buttonStyle(.bordered)
            }
            Button("Remove", role: .destructive, action: onRemove)
        }
        .id(n.id)
    }
}

private struct BoardSheet: View {
    let s: Obj
    let onChange: (Obj) -> Void
    @Environment(Store.self) private var store
    @State private var pick: PhotosPickerItem?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Board").font(.title2.weight(.semibold))
            Choice(label: "Background", options: [("", "Dots"), ("grid", "Squares"), ("lines", "Lines"), ("paper", "Paper"), ("cork", "Corkboard"), ("plain", "Plain")], value: s.str("bg") == "dots" ? "" : s.str("bg")) { onChange(s.with("bg", $0)) }
            Colours(label: "Background colour", value: s.str("color")) { onChange(s.with("color", $0)) }
            Text("Background picture").font(.subheadline.weight(.semibold))
            HStack(spacing: 8) {
                PhotosPicker(selection: $pick, matching: .images) { Text("Choose…") }.buttonStyle(.bordered)
                if !s.str("image").isEmpty { Button("Remove", role: .destructive) { onChange(s.with("image", nil)) } }
            }
        }
        .onChange(of: pick) {
            guard let item = pick else { return }
            Task {
                defer { pick = nil }
                guard let data = try? await item.loadTransferable(type: Data.self) else { return }
                do {
                    let saved = try await store.run { try $0.saveAttachment(folder: "Attachments", name: "Home background.jpg", bytes: data) }
                    onChange(s.with("image", saved))
                } catch { store.say(error) }
            }
        }
    }
}
