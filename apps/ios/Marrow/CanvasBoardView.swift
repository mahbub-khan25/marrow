import SwiftUI

/// JSON Canvas colours "1"–"6".
private let palette: [(String, Color)] = [("1", Color(light: 0xE03E3E, dark: 0xF07070)), ("2", Color(light: 0xE8780C, dark: 0xF5A04B)), ("3", Color(light: 0xD4A106, dark: 0xE8C040)), ("4", Color(light: 0x2F9E44, dark: 0x5CC071)), ("5", Color(light: 0x0B8CB5, dark: 0x4FB6DC)), ("6", Color(light: 0x8E44AD, dark: 0xB57BD0))]

private func colorOf(_ c: String) -> Color? {
    if let p = palette.first(where: { $0.0 == c }) { return p.1 }
    if c.hasPrefix("#"), c.count == 7, let v = UInt32(c.dropFirst(), radix: 16) { return Color(uiColor: UIColor(hex: v)) }
    return nil
}

private func newId() -> String { String(UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(16)).lowercased() }

/// A board of cards, notes and arrows: pinch to zoom, drag to move a card,
/// double-tap a card to open or edit it.
struct CanvasBoardView: View {
    let path: String
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var board: Board?
    @State private var onDisk: Board?
    @State private var error: String?
    @State private var scale: CGFloat = 1
    @State private var lastScale: CGFloat = 1
    @State private var pan: CGSize = .zero
    @State private var lastPan: CGSize = .zero
    @State private var selected: String?
    @State private var connecting = false
    @State private var editing: CanvasNode?
    @State private var editText = ""
    @State private var picking = false
    @State private var size: CGSize = .zero
    @State private var saveTask: Task<Void, Never>?
    @State private var dragStart: [String: CGPoint] = [:]

    var body: some View {
        VStack(spacing: 0) {
            if let error {
                Text(verbatim: error).foregroundStyle(Color.danger).padding(24)
                Spacer()
            } else if let b = board {
                GeometryReader { geo in
                    ZStack(alignment: .topLeading) {
                        Color(light: 0xFFFFFF, dark: 0x141311)
                            .contentShape(Rectangle())
                            .onTapGesture { selected = nil; connecting = false }
                        world(b)
                            .scaleEffect(scale, anchor: .topLeading)
                            .offset(pan)
                    }
                    .clipped()
                    .gesture(
                        SimultaneousGesture(
                            MagnificationGesture()
                                .onChanged { v in scale = min(4, max(0.2, lastScale * v)) }
                                .onEnded { _ in lastScale = scale },
                            DragGesture()
                                .onChanged { v in pan = CGSize(width: lastPan.width + v.translation.width, height: lastPan.height + v.translation.height) }
                                .onEnded { _ in lastPan = pan }
                        )
                    )
                    .onAppear { size = geo.size; showAll() }
                    .onChange(of: geo.size) { _, s in size = s }
                }
                .overlay(alignment: .top) {
                    if connecting {
                        Text("Tap another card to connect").font(.subheadline)
                            .padding(.horizontal, 16).padding(.vertical, 8)
                            .background(Color.ink, in: Capsule()).foregroundStyle(Color.paper).padding(12)
                    }
                }
                toolbar
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(Text(verbatim: nameOf(path)))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { showAll() } label: { Image(systemName: "arrow.up.left.and.arrow.down.right") }.accessibilityLabel(Text("Show everything"))
            }
        }
        .task {
            do {
                let b = try await store.run { [p = path] m in try m.readCanvas(path: p) }
                board = b
                onDisk = b
                showAll()
            } catch { self.error = words(error) }
        }
        .alert("Card", isPresented: Binding(get: { editing != nil }, set: { if !$0 { editing = nil } })) {
            TextField("Write something…", text: $editText, axis: .vertical)
            Button("Cancel", role: .cancel) { editing = nil }
            Button("Done") {
                if let n = editing { change { b in b.nodes = b.nodes.map { var x = $0; if x.id == n.id { x.text = editText }; return x } } }
                editing = nil
            }
        }
        .sheet(isPresented: $picking) { NotePicker { hit in add(file: hit.path) } }
    }

    private func world(_ b: Board) -> some View {
        let byId = Dictionary(b.nodes.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        return ZStack(alignment: .topLeading) {
            ForEach(b.nodes.filter { $0.kind == "group" }, id: \.id) { n in card(n) }
            Canvas { ctx, _ in
                for e in b.edges {
                    guard let a = byId[e.fromNode], let z = byId[e.toNode] else { continue }
                    let p1 = anchor(a, e.fromSide, z), p2 = anchor(z, e.toSide, a)
                    let c = colorOf(e.color) ?? Color.faint
                    var line = Path()
                    line.move(to: p1)
                    line.addLine(to: p2)
                    ctx.stroke(line, with: .color(c), style: StrokeStyle(lineWidth: 2, lineCap: .round))
                    ctx.stroke(arrowHead(p1, p2), with: .color(c), style: StrokeStyle(lineWidth: 2, lineCap: .round))
                }
            }
            .frame(width: 1, height: 1)
            .allowsHitTesting(false)
            ForEach(b.nodes.filter { $0.kind != "group" }, id: \.id) { n in card(n) }
        }
    }

    private func card(_ n: CanvasNode) -> some View {
        let accent = colorOf(n.color)
        let isGroup = n.kind == "group"
        let isSelected = selected == n.id
        return Group {
            if let ink = n.ink {
                InkShape(ink: ink).stroke(accent ?? Color.ink, style: StrokeStyle(lineWidth: CGFloat(ink.size), lineCap: .round, lineJoin: .round))
                    .overlay { if isSelected { RoundedRectangle(cornerRadius: 6).stroke(Color.accent, lineWidth: 2) } }
            } else {
                CardContent(node: n)
                    .frame(width: CGFloat(n.width), height: CGFloat(n.height), alignment: .topLeading)
                    .background(
                        isGroup ? (accent ?? Color.faint).opacity(0.06) : accent.map { $0.opacity(0.14) } ?? Color(light: 0xFBFAF7, dark: 0x262421),
                        in: RoundedRectangle(cornerRadius: isGroup ? 16 : 12, style: .continuous)
                    )
                    .overlay(RoundedRectangle(cornerRadius: isGroup ? 16 : 12, style: .continuous).stroke(isSelected ? Color.accent : (accent ?? Color.line), lineWidth: isSelected ? 2 : 1))
                    .shadow(color: .black.opacity(isGroup ? 0 : 0.05), radius: 2, y: 1)
            }
        }
        .frame(width: CGFloat(n.width), height: CGFloat(n.height))
        .offset(x: CGFloat(n.x), y: CGFloat(n.y))
        .onTapGesture(count: 2) {
            if n.kind == "file", !n.file.isEmpty { router.open(path: n.file) }
            else if n.kind == "text", n.ink == nil { editText = n.text; editing = n }
        }
        .onTapGesture {
            if connecting, let from = selected, from != n.id {
                change { b in b.edges.append(CanvasEdge(id: newId(), fromNode: from, toNode: n.id, fromSide: "", toSide: "", label: "", color: "", extra: "")) }
                connecting = false
            }
            selected = n.id
        }
        .gesture(
            DragGesture(minimumDistance: 4)
                .onChanged { v in
                    selected = n.id
                    let start = dragStart[n.id] ?? CGPoint(x: n.x, y: n.y)
                    if dragStart[n.id] == nil { dragStart[n.id] = start }
                    move(n.id, to: CGPoint(x: start.x + v.translation.width, y: start.y + v.translation.height), group: isGroup)
                }
                .onEnded { _ in dragStart[n.id] = nil }
        )
    }

    private var toolbar: some View {
        HStack {
            tool("plus.rectangle", "Card") {
                let c = center()
                let n = CanvasNode(id: newId(), kind: "text", x: Double(c.x - 120), y: Double(c.y - 60), width: 240, height: 120, text: "", file: "", url: "", label: "", color: "", shape: "", ink: nil, extra: "")
                change { $0.nodes.append(n) }
                selected = n.id
                editText = ""
                editing = n
            }
            tool("doc.badge.plus", "Note") { picking = true }
            tool("arrow.triangle.branch", "Connect", enabled: selected != nil, active: connecting) { connecting.toggle() }
            Menu {
                Button("None") { setColor("") }
                ForEach(palette, id: \.0) { p in Button { setColor(p.0) } label: { Label { Text(verbatim: p.0) } icon: { Image(systemName: "circle.fill").foregroundStyle(p.1) } } }
            } label: {
                VStack(spacing: 2) { Image(systemName: "paintpalette"); Text("Colour").font(.caption2) }.frame(maxWidth: .infinity)
            }
            .disabled(selected == nil)
            tool("trash", "Delete", enabled: selected != nil) {
                guard let id = selected else { return }
                change { b in
                    b.nodes.removeAll { $0.id == id }
                    b.edges.removeAll { $0.fromNode == id || $0.toNode == id }
                }
                selected = nil
            }
        }
        .padding(.vertical, 8)
        .background(.bar)
    }

    private func tool(_ symbol: String, _ label: LocalizedStringKey, enabled: Bool = true, active: Bool = false, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            VStack(spacing: 2) { Image(systemName: symbol); Text(label).font(.caption2) }
                .frame(maxWidth: .infinity)
                .foregroundStyle(active ? Color.accent : enabled ? Color.ink : Color.faint)
        }
        .disabled(!enabled)
    }

    private func setColor(_ c: String) {
        guard let id = selected else { return }
        change { b in b.nodes = b.nodes.map { var x = $0; if x.id == id { x.color = c }; return x } }
    }

    private func center() -> CGPoint {
        CGPoint(x: (size.width / 2 - pan.width) / scale, y: (size.height / 2 - pan.height) / scale)
    }

    private func add(file: String) {
        let c = center()
        let n = CanvasNode(id: newId(), kind: "file", x: Double(c.x - 150), y: Double(c.y - 100), width: 300, height: 200, text: "", file: file, url: "", label: "", color: "", shape: "", ink: nil, extra: "")
        change { $0.nodes.append(n) }
        selected = n.id
    }

    private func move(_ id: String, to p: CGPoint, group: Bool) {
        guard let b = board, let n = b.nodes.first(where: { $0.id == id }) else { return }
        let dx = Double(p.x) - n.x, dy = Double(p.y) - n.y
        // A group carries the cards inside it.
        let inside: Set<String> = group ? Set(b.nodes.filter { $0.id != id && $0.x >= n.x && $0.y >= n.y && $0.x + $0.width <= n.x + n.width && $0.y + $0.height <= n.y + n.height }.map(\.id)) : []
        change { b in b.nodes = b.nodes.map { var x = $0; if x.id == id || inside.contains(x.id) { x.x += dx; x.y += dy }; return x } }
    }

    /// Changes the board and saves a moment later.
    private func change(_ f: (inout Board) -> Void) {
        guard var b = board else { return }
        f(&b)
        board = b
        saveTask?.cancel()
        saveTask = Task {
            try? await Task.sleep(nanoseconds: 600_000_000)
            guard !Task.isCancelled, let now = board else { return }
            let expected = onDisk
            do {
                try await store.run { [p = path] m in try m.writeCanvas(path: p, canvas: now, expected: expected) }
                onDisk = now
            } catch MarrowError.ChangedElsewhere {
                store.say(String(localized: "The canvas changed elsewhere, so it was opened again."))
                if let fresh = try? await store.run({ [p = path] m in try m.readCanvas(path: p) }) { board = fresh; onDisk = fresh }
            } catch { store.say(error) }
        }
    }

    private func showAll() {
        guard let b = board, size.width > 0 else { return }
        guard !b.nodes.isEmpty else { scale = 1; pan = CGSize(width: size.width / 2, height: size.height / 2); lastPan = pan; lastScale = 1; return }
        let minX = b.nodes.map(\.x).min() ?? 0, minY = b.nodes.map(\.y).min() ?? 0
        let maxX = b.nodes.map { $0.x + $0.width }.max() ?? 1, maxY = b.nodes.map { $0.y + $0.height }.max() ?? 1
        let w = max(1, maxX - minX), h = max(1, maxY - minY)
        let s = min(1.25, max(0.2, min(Double(size.width) / w, Double(size.height) / h) * 0.9))
        scale = CGFloat(s)
        lastScale = scale
        pan = CGSize(width: (Double(size.width) - w * s) / 2 - minX * s, height: (Double(size.height) - h * s) / 2 - minY * s)
        lastPan = pan
    }
}

/// Where an edge leaves a card: the middle of the named side, or the side facing the other card.
private func anchor(_ n: CanvasNode, _ side: String, _ other: CanvasNode) -> CGPoint {
    let cx = n.x + n.width / 2, cy = n.y + n.height / 2
    var s = side
    if s.isEmpty {
        let dx = (other.x + other.width / 2) - cx, dy = (other.y + other.height / 2) - cy
        s = abs(dx) * n.height > abs(dy) * n.width ? (dx > 0 ? "right" : "left") : (dy > 0 ? "bottom" : "top")
    }
    switch s {
    case "left": return CGPoint(x: n.x, y: cy)
    case "right": return CGPoint(x: n.x + n.width, y: cy)
    case "top": return CGPoint(x: cx, y: n.y)
    default: return CGPoint(x: cx, y: n.y + n.height)
    }
}

private func arrowHead(_ from: CGPoint, _ to: CGPoint) -> Path {
    let a = atan2(to.y - from.y, to.x - from.x)
    var p = Path()
    for side in [-0.45, 0.45] {
        p.move(to: to)
        p.addLine(to: CGPoint(x: to.x - 12 * cos(a + side), y: to.y - 12 * sin(a + side)))
    }
    return p
}

/// A pen stroke or free arrow, scaled to its card.
private struct InkShape: Shape {
    let ink: Ink
    func path(in rect: CGRect) -> Path {
        let sx = rect.width / CGFloat(ink.w), sy = rect.height / CGFloat(ink.h)
        var pts: [CGPoint] = []
        var i = 0
        while i + 1 < ink.points.count {
            pts.append(CGPoint(x: CGFloat(ink.points[i]) * sx, y: CGFloat(ink.points[i + 1]) * sy))
            i += 2
        }
        var p = Path()
        guard pts.count >= 2 else { return p }
        if ink.arrow {
            p.move(to: pts[0])
            p.addLine(to: pts[pts.count - 1])
            p.addPath(arrowHead(pts[0], pts[pts.count - 1]))
            return p
        }
        p.move(to: pts[0])
        for j in 1..<pts.count {
            let mid = CGPoint(x: (pts[j - 1].x + pts[j].x) / 2, y: (pts[j - 1].y + pts[j].y) / 2)
            p.addQuadCurve(to: mid, control: pts[j - 1])
        }
        p.addLine(to: pts[pts.count - 1])
        return p
    }
}

private struct CardContent: View {
    let node: CanvasNode
    @Environment(Store.self) private var store
    @State private var text = ""

    var body: some View {
        switch node.kind {
        case "group":
            Text(verbatim: node.label).font(.subheadline.weight(.semibold)).foregroundStyle(colorOf(node.color) ?? Color.inkSoft).padding(12)
        case "file":
            VStack(alignment: .leading, spacing: 6) {
                Label { Text(verbatim: nameOf(node.file)).font(.subheadline.weight(.semibold)).lineLimit(1) } icon: { Image(systemName: "doc.text").foregroundStyle(Color.accent) }
                Text(verbatim: text).font(.caption).foregroundStyle(Color.inkSoft)
            }
            .padding(12)
            .task(id: node.file) {
                let f = node.file
                text = (try? await store.run { m in preview(text: try m.readNote(path: f), max: 400) }) ?? ""
            }
        case "link":
            Text(verbatim: node.url).font(.subheadline).foregroundStyle(Color.accent).padding(12)
        default:
            Blocks(blocks: renderMarkdown(text: node.text), actions: BlockActions())
                .padding(.horizontal, 12).padding(.vertical, 8)
        }
    }
}

/// Finds a note to put on a canvas.
struct NotePicker: View {
    let onPick: (FileHit) -> Void
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var hits: [FileHit] = []

    var body: some View {
        NavigationStack {
            List(hits, id: \.path) { h in
                Button { onPick(h); dismiss() } label: {
                    Label { VStack(alignment: .leading) { Text(verbatim: h.name); if !h.folder.isEmpty { Text(verbatim: h.folder).font(.caption).foregroundStyle(Color.faint) } } } icon: { Image(systemName: "doc.text") }
                }
            }
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: Text("Find a note"))
            .navigationTitle(Text("Note"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .task(id: query) { hits = ((try? await store.run { [q = query] m in try m.findFiles(query: q, source: nil, limit: 20) }) ?? []).filter(\.isNote) }
        }
    }
}

/// Earlier versions of a note: pick one to read, and put it back if you want.
struct HistoryView: View {
    let path: String
    @Environment(Store.self) private var store
    @State private var versions: [Version]?

    var body: some View {
        List {
            Section {
                Text("Marrow keeps earlier versions of each note as you write, every few minutes, and thins them out over time.").font(.callout).foregroundStyle(Color.inkSoft)
            }
            if versions?.isEmpty == true {
                EmptyState(symbol: "clock.arrow.circlepath", title: "No earlier versions", text: "Versions appear here once the note has been changed a few times.")
            }
            ForEach(versions ?? [], id: \.timeMs) { v in
                NavigationLink {
                    VersionView(path: path, version: v)
                } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(verbatim: stamp(v.timeMs)).font(.headline)
                        Text(verbatim: "\(ago(v.timeMs)) · \(ByteCountFormatter.string(fromByteCount: Int64(v.size), countStyle: .file))").font(.caption).foregroundStyle(Color.faint)
                    }
                }
            }
        }
        .navigationTitle(Text("Version history"))
        .task(id: store.tick) { versions = try? await store.run { [p = path] m in try m.versions(path: p) } }
    }
}

private struct VersionView: View {
    let path: String
    let version: Version
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var text: String?
    @State private var asking = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Button { asking = true } label: { Label("Restore this version", systemImage: "arrow.uturn.backward") }.buttonStyle(.borderedProminent)
                if let text { Blocks(blocks: renderMarkdown(text: text), actions: BlockActions()) }
            }
            .padding(22)
            .readingWidth()
        }
        .background(Color.paper)
        .navigationTitle(Text(verbatim: stamp(version.timeMs)))
        .navigationBarTitleDisplayMode(.inline)
        .task { text = try? await store.run { [p = path, t = version.timeMs] m in try m.readVersion(path: p, timeMs: t) } }
        .confirmationDialog(Text("Restore this version?"), isPresented: $asking, titleVisibility: .visible) {
            Button("Restore") {
                Task {
                    do {
                        try await store.run { [p = path, t = version.timeMs] m in try m.restoreVersion(path: p, timeMs: t) }
                        store.say(String(localized: "Restored."))
                        dismiss()
                    } catch { store.say(error) }
                }
            }
        } message: { Text("The note as it is now is kept as a version too, so you can switch back.") }
    }
}

/// The whole vault as it was on a day: what was there, what changed since, what's gone.
struct TimeMachineView: View {
    @Environment(Store.self) private var store
    @State private var timeline: Timeline?
    @State private var position: Double = 1
    @State private var notes: [ThenNote] = []

    private var at: Int64 {
        let now = Int64(Date().timeIntervalSince1970 * 1000)
        let day: Int64 = 86_400_000
        let first = min(timeline?.firstMs ?? (now - 30 * day), now - day)
        return first + Int64(Double(now - first) * position)
    }

    var body: some View {
        List {
            Section {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Your vault on").font(.subheadline).foregroundStyle(Color.inkSoft)
                    Text(Date(timeIntervalSince1970: TimeInterval(at) / 1000).formatted(date: .complete, time: .omitted)).font(.title3.weight(.semibold))
                    Slider(value: $position)
                    let changed = notes.filter { $0.state == .changed }.count, gone = notes.filter { $0.state == .gone }.count
                    Text("\(notes.count) notes · \(changed) changed since · \(gone) deleted since").font(.caption).foregroundStyle(Color.inkSoft)
                }
            }
            ForEach([NoteThen.gone, .changed, .same], id: \.self) { state in
                let group = notes.filter { $0.state == state }
                if !group.isEmpty {
                    Section {
                        ForEach(group, id: \.path) { n in
                            NavigationLink { ThenView(note: n, at: at) } label: {
                                HStack(spacing: 12) {
                                    Circle().fill(state == .gone ? Color.danger : state == .changed ? Color.orange : Color.line).frame(width: 9, height: 9)
                                    VStack(alignment: .leading) {
                                        Text(verbatim: n.title)
                                        if !parentOf(n.path).isEmpty { Text(verbatim: parentOf(n.path)).font(.caption).foregroundStyle(Color.faint) }
                                    }
                                }
                            }
                        }
                    } header: {
                        Text(state == .gone ? "Deleted since" : state == .changed ? "Changed since" : "Unchanged")
                    }
                }
            }
        }
        .navigationTitle(Text("Time machine"))
        .task { timeline = try? await store.run { try $0.timeline() } }
        .task(id: at / 60_000) {
            let t = at
            notes = (try? await store.run { try $0.vaultAt(timeMs: t) }) ?? []
        }
    }
}

private struct ThenView: View {
    let note: ThenNote
    let at: Int64
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var text: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                Text(verbatim: stamp(at)).font(.subheadline.weight(.semibold)).foregroundStyle(Color.accent)
                if let text { Blocks(blocks: renderMarkdown(text: text), actions: BlockActions()) }
                if note.state != .gone {
                    Button("Open as it is now") { router.note(note.path) }.buttonStyle(.bordered)
                }
            }
            .padding(22)
            .readingWidth()
        }
        .background(Color.paper)
        .navigationTitle(Text(verbatim: note.title))
        .task {
            let n = note
            text = try? await store.run { m -> String in
                if let v = n.version { return try m.readVersion(path: n.path, timeMs: v) }
                return try m.readNote(path: n.path)
            }
        }
    }
}
