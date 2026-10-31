import SwiftUI

// Drawings inside notes: a small canvas (a `.canvas` file) shown where its
// `![[Drawing ….canvas|360]]` line is. Tap it to draw right there with a
// finger or Apple Pencil: pen, shapes, arrows and words, in colour. The
// cards it makes are the same as on the desktop and in canvases (the
// engine's strokeNode / shapeNode), so a drawing looks the same everywhere.

/// JSON Canvas colours "1"–"6".
private let drawPalette: [(String, Color)] = [("1", Color(light: 0xE03E3E, dark: 0xF07070)), ("2", Color(light: 0xE8780C, dark: 0xF5A04B)), ("3", Color(light: 0xD4A106, dark: 0xE8C040)), ("4", Color(light: 0x2F9E44, dark: 0x5CC071)), ("5", Color(light: 0x0B8CB5, dark: 0x4FB6DC)), ("6", Color(light: 0x8E44AD, dark: 0xB57BD0))]

private func drawColor(_ c: String) -> Color? {
    if let p = drawPalette.first(where: { $0.0 == c }) { return p.1 }
    if c.hasPrefix("#"), c.count == 7, let v = UInt32(c.dropFirst(), radix: 16) { return Color(uiColor: UIColor(hex: v)) }
    return nil
}

enum DrawTool: CaseIterable { case pen, rect, ellipse, diamond, arrow, text, erase }

private let penSizes: [Double] = [2, 4, 8]

/// Tells the note's scroll view to hold still while a drawing is being drawn on.
struct DrawingActiveKey: PreferenceKey {
    static let defaultValue = false
    static func reduce(value: inout Bool, nextValue: () -> Bool) { value = value || nextValue() }
}

/// How the drawing maps onto the box: screen = world * k + (tx, ty).
private struct Xform: Equatable {
    var k: CGFloat, tx: CGFloat, ty: CGFloat
    func world(_ p: CGPoint) -> CGPoint { CGPoint(x: (p.x - tx) / k, y: (p.y - ty) / k) }
}

/// Fits everything drawn into the box, never larger than life.
private func fit(_ b: Board?, _ size: CGSize) -> Xform {
    let pad: CGFloat = 16
    guard let nodes = b?.nodes, !nodes.isEmpty, size.width > 0 else { return Xform(k: 1, tx: pad, ty: pad) }
    let x0 = CGFloat(nodes.map(\.x).min()!), y0 = CGFloat(nodes.map(\.y).min()!)
    let x1 = CGFloat(nodes.map { $0.x + $0.width }.max()!), y1 = CGFloat(nodes.map { $0.y + $0.height }.max()!)
    let k = max(0.05, min((size.width - 2 * pad) / max(1, x1 - x0), (size.height - 2 * pad) / max(1, y1 - y0), 1))
    return Xform(k: k, tx: (size.width - (x1 - x0) * k) / 2 - x0 * k, ty: (size.height - (y1 - y0) * k) / 2 - y0 * k)
}

/// A drawing in a note: shows it; a tap starts drawing on it in place.
struct InlineDrawing: View {
    let path: String
    let height: Int
    var onOpenFull: () -> Void
    @State private var active = false

    var body: some View {
        DrawingPad(path: path, active: $active, onOpenFull: onOpenFull)
            .frame(height: CGFloat(height) + (active ? 100 : 0))
            .preference(key: DrawingActiveKey.self, value: active)
            .padding(.vertical, 6)
    }
}

/// A drawing just put into a note, drawn on full size.
struct DrawingEditorSheet: View {
    let path: String
    @State private var active = true
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        DrawingPad(path: path, active: $active, onOpenFull: nil)
            .padding(12)
            .background(Color.paper)
            .onChange(of: active) { _, now in if !now { dismiss() } }
            .interactiveDismissDisabled()
    }
}

private enum Asking: Identifiable {
    case label(String)
    case words(CGPoint)
    var id: String {
        switch self {
        case .label(let id): return "label-\(id)"
        case .words(let p): return "words-\(p.x)-\(p.y)"
        }
    }
}

struct DrawingPad: View {
    let path: String
    @Binding var active: Bool
    var onOpenFull: (() -> Void)?

    @Environment(Store.self) private var store
    @State private var board: Board?
    @State private var missing = false
    @State private var past: [Board] = []
    @State private var tool: DrawTool = .pen
    @State private var color = ""
    @State private var penSize: Double = 4
    @State private var frozen: Xform?
    @State private var size: CGSize = .zero
    @State private var draft: [CGPoint] = []
    @State private var asking: Asking?
    @State private var words = ""
    @State private var saveTask: Task<Void, Never>?

    private var view: Xform { frozen ?? fit(board, size) }

    var body: some View {
        VStack(spacing: 6) {
            if active { toolbar }
            ZStack {
                Canvas { ctx, _ in
                    guard let b = board else { return }
                    var c = ctx
                    c.translateBy(x: view.tx, y: view.ty)
                    c.scaleBy(x: view.k, y: view.k)
                    drawBoard(&c, b)
                    if draft.count > 1 { drawDraft(ctx, draft) }
                }
                if missing {
                    Text("This drawing was moved or deleted.").font(.subheadline).foregroundStyle(Color.faint)
                } else if !active, board?.nodes.isEmpty == true {
                    Text("Tap to draw").font(.subheadline).foregroundStyle(Color.faint)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color.paper)
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(active ? Color.accent.opacity(0.7) : Color.line, lineWidth: active ? 2 : 1))
            .overlay(alignment: .topTrailing) {
                if !active, let open = onOpenFull, !missing {
                    Button(action: open) {
                        Image(systemName: "arrow.up.left.and.arrow.down.right").font(.footnote).foregroundStyle(Color.inkSoft).frame(width: 36, height: 36)
                    }
                    .accessibilityLabel(Text("Open full size"))
                }
            }
            .background(GeometryReader { g in Color.clear.onAppear { size = g.size }.onChange(of: g.size) { _, s in size = s } })
            .contentShape(Rectangle())
            .onTapGesture { if !active { active = true } }
            .gesture(drag, including: active ? .all : .none)
            .accessibilityLabel(Text("Drawing"))
        }
        .task(id: path) {
            let p = path
            do { board = try await store.run { try $0.readCanvas(path: p) } } catch { missing = true }
            freeze()
        }
        .onChange(of: active) { _, now in
            frozen = nil
            if now { freeze() }
        }
        .onChange(of: size) { _, _ in freeze() }
        .alert(alertTitle, isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } })) {
            TextField(String(localized: "Label (optional)"), text: $words)
            Button("OK") { answer() }
            Button("Cancel", role: .cancel) { asking = nil }
        }
    }

    private var alertTitle: String {
        if case .words? = asking { return String(localized: "Add words") }
        return String(localized: "Label")
    }

    /// While drawing, the view stays put (it would jump as the drawing grows).
    private func freeze() {
        if active, frozen == nil, size.width > 0, board != nil { frozen = fit(board, size) }
    }

    // MARK: drawing

    private var drag: some Gesture {
        DragGesture(minimumDistance: 0, coordinateSpace: .local)
            .onChanged { v in
                if draft.isEmpty { draft = [v.startLocation] }
                if let last = draft.last, hypot(v.location.x - last.x, v.location.y - last.y) > 1.5 { draft.append(v.location) }
            }
            .onEnded { v in
                var pts = draft.isEmpty ? [v.startLocation] : draft
                pts.append(v.location)
                draft = []
                finish(pts)
            }
    }

    private func finish(_ pts: [CGPoint]) {
        let w = pts.map { view.world($0) }
        guard let a = w.first, let z = w.last, let b = board else { return }
        switch tool {
        case .pen, .arrow:
            if let n = strokeNode(points: w.flatMap { [Double($0.x), Double($0.y)] }, size: penSize, arrow: tool == .arrow, color: color, zoom: Double(view.k)) { change { $0.nodes.append(n) } }
        case .rect, .ellipse, .diamond:
            let n = shapeNode(shape: tool == .ellipse ? "ellipse" : tool == .diamond ? "diamond" : "rect", x0: Double(a.x), y0: Double(a.y), x1: Double(z.x), y1: Double(z.y), color: color)
            change { $0.nodes.append(n) }
            words = ""
            asking = .label(n.id)
        case .text:
            words = ""
            asking = .words(a)
        case .erase:
            let tol = Double(10 / view.k)
            guard let hit = b.nodes.last(where: { n in w.contains { p in Double(p.x) >= n.x - tol && Double(p.x) <= n.x + n.width + tol && Double(p.y) >= n.y - tol && Double(p.y) <= n.y + n.height + tol } }) else { return }
            change { $0.nodes.removeAll { $0.id == hit.id }; $0.edges.removeAll { $0.fromNode == hit.id || $0.toNode == hit.id } }
        }
    }

    private func answer() {
        let t = words.trimmingCharacters(in: .whitespacesAndNewlines)
        defer { asking = nil }
        guard !t.isEmpty, let ask = asking else { return }
        switch ask {
        case .label(let id):
            change { b in if let i = b.nodes.firstIndex(where: { $0.id == id }) { b.nodes[i].text = t } }
        case .words(let p):
            let n = textNode(text: t, x: Double(p.x), y: Double(p.y), color: color)
            change { $0.nodes.append(n) }
        }
    }

    private func change(_ f: (inout Board) -> Void) {
        guard var b = board else { return }
        past.append(b)
        if past.count > 100 { past.removeFirst() }
        f(&b)
        board = b
        save(b)
    }

    private func save(_ b: Board) {
        saveTask?.cancel()
        let p = path
        saveTask = Task {
            try? await Task.sleep(nanoseconds: 400_000_000)
            if Task.isCancelled { return }
            do { try await store.run { try $0.writeCanvas(path: p, canvas: b, expected: nil) } } catch { await MainActor.run { store.say(error) } }
        }
    }

    // MARK: toolbar

    private var toolbar: some View {
        VStack(spacing: 4) {
            HStack(spacing: 2) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 2) {
                        toolButton(.pen, "scribble.variable", "Pen")
                        toolButton(.rect, "rectangle", "Rectangle")
                        toolButton(.ellipse, "circle", "Ellipse")
                        toolButton(.diamond, "diamond", "Diamond")
                        toolButton(.arrow, "arrow.right", "Arrow")
                        toolButton(.text, "textformat", "Words")
                        toolButton(.erase, "eraser", "Eraser")
                        Button {
                            if let prev = past.popLast() { board = prev; save(prev) }
                        } label: { Image(systemName: "arrow.uturn.backward").frame(width: 38, height: 34) }
                        .disabled(past.isEmpty)
                        .accessibilityLabel(Text("Undo"))
                    }
                }
                Button { active = false } label: { Label("Done", systemImage: "checkmark").font(.subheadline.weight(.semibold)) }
                    .padding(.horizontal, 8)
            }
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    swatch(nil, "")
                    ForEach(drawPalette, id: \.0) { p in swatch(p.1, p.0) }
                    if tool == .pen {
                        Divider().frame(height: 20)
                        ForEach(penSizes, id: \.self) { s in
                            Button { penSize = s } label: {
                                Circle().fill(Color.ink).frame(width: s + 3, height: s + 3)
                                    .frame(width: 30, height: 30)
                                    .background(s == penSize ? Color.line : Color.clear, in: Circle())
                            }
                            .accessibilityLabel(Text(verbatim: "\(Int(s))"))
                        }
                    }
                }
                .padding(.horizontal, 8)
            }
        }
        .padding(.vertical, 4)
        .background(Color.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .buttonStyle(.plain)
    }

    private func toolButton(_ t: DrawTool, _ symbol: String, _ label: LocalizedStringKey) -> some View {
        Button { tool = t } label: {
            Image(systemName: symbol)
                .font(.system(size: 17))
                .foregroundStyle(tool == t ? Color.paper : Color.ink)
                .frame(width: 38, height: 34)
                .background(tool == t ? Color.accent : Color.clear, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .accessibilityLabel(Text(label))
    }

    private func swatch(_ c: Color?, _ id: String) -> some View {
        Button { color = id } label: {
            ZStack {
                Circle().fill(c ?? Color.paper).padding(4)
                if c == nil { Path { p in p.move(to: CGPoint(x: 22, y: 6)); p.addLine(to: CGPoint(x: 6, y: 22)) }.stroke(Color.red, lineWidth: 2) }
                Circle().stroke(color == id ? Color.ink : Color.line, lineWidth: color == id ? 2 : 1)
            }
            .frame(width: 28, height: 28)
        }
    }

    // MARK: rendering (in world units: 1 = 1 point at 100%)

    private func drawBoard(_ c: inout GraphicsContext, _ b: Board) {
        let byId = Dictionary(b.nodes.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        for e in b.edges {
            guard let a = byId[e.fromNode], let z = byId[e.toNode] else { continue }
            let p1 = CGPoint(x: CGFloat(a.x + a.width / 2), y: CGFloat(a.y + a.height / 2)), p2 = CGPoint(x: CGFloat(z.x + z.width / 2), y: CGFloat(z.y + z.height / 2))
            var p = Path()
            p.move(to: p1)
            p.addLine(to: p2)
            p.addPath(head(p1, p2, 1.5))
            c.stroke(p, with: .color(drawColor(e.color) ?? Color.ink.opacity(0.5)), style: StrokeStyle(lineWidth: 1.5, lineCap: .round))
        }
        for n in b.nodes.sorted(by: { ($0.kind == "group" ? 0 : 1) < ($1.kind == "group" ? 0 : 1) }) {
            let r = CGRect(x: CGFloat(n.x), y: CGFloat(n.y), width: CGFloat(n.width), height: CGFloat(n.height))
            let col = drawColor(n.color)
            if let ink = n.ink {
                c.stroke(inkPath(ink, in: r), with: .color(col ?? Color.ink.opacity(0.85)), style: StrokeStyle(lineWidth: CGFloat(ink.size), lineCap: .round, lineJoin: .round))
            } else if !n.shape.isEmpty {
                let shape: Path
                switch n.shape {
                case "ellipse": shape = Path(ellipseIn: r)
                case "diamond":
                    var p = Path()
                    p.move(to: CGPoint(x: r.midX, y: r.minY)); p.addLine(to: CGPoint(x: r.maxX, y: r.midY))
                    p.addLine(to: CGPoint(x: r.midX, y: r.maxY)); p.addLine(to: CGPoint(x: r.minX, y: r.midY)); p.closeSubpath()
                    shape = p
                default: shape = Path(roundedRect: r, cornerRadius: 4)
                }
                c.fill(shape, with: .color(col.map { $0.opacity(0.13) } ?? Color.paper))
                c.stroke(shape, with: .color(col ?? Color.ink.opacity(0.7)), lineWidth: 2)
                let inset = n.shape == "diamond" ? CGSize(width: r.width * 0.2, height: r.height * 0.16) : n.shape == "ellipse" ? CGSize(width: r.width * 0.1, height: r.height * 0.1) : CGSize(width: 6, height: 4)
                label(&c, n.text, in: r.insetBy(dx: inset.width + 6, dy: inset.height + 4), size: 17, center: true)
            } else if n.kind == "group" {
                let g = col ?? Color.accent
                c.fill(Path(roundedRect: r, cornerRadius: 12), with: .color(g.opacity(0.07)))
                c.stroke(Path(roundedRect: r, cornerRadius: 12), with: .color(g.opacity(0.35)), lineWidth: 1)
            } else {
                c.fill(Path(roundedRect: r, cornerRadius: 10), with: .color((col ?? Color.red).opacity(col == nil ? 0.06 : 0.14)))
                let words = n.kind == "file" ? nameOf(n.file) : n.kind == "link" ? n.url : n.text
                label(&c, words, in: r.insetBy(dx: 10, dy: 8), size: 15, center: false)
            }
        }
    }

    private func label(_ c: inout GraphicsContext, _ text: String, in r: CGRect, size: CGFloat, center: Bool) {
        guard !text.trimmingCharacters(in: .whitespaces).isEmpty, r.width > 1, r.height > 1 else { return }
        let t = c.resolve(Text(verbatim: text).font(.system(size: size)).foregroundColor(Color.ink))
        let s = t.measure(in: r.size)
        let box = center ? CGRect(x: r.midX - s.width / 2, y: r.midY - s.height / 2, width: s.width, height: min(s.height, r.height)) : CGRect(origin: r.origin, size: CGSize(width: r.width, height: min(s.height, r.height)))
        c.draw(t, in: box)
    }

    private func inkPath(_ ink: Ink, in r: CGRect) -> Path {
        let sx = r.width / CGFloat(ink.w), sy = r.height / CGFloat(ink.h)
        var pts: [CGPoint] = []
        var i = 0
        while i + 1 < ink.points.count {
            pts.append(CGPoint(x: r.minX + CGFloat(ink.points[i]) * sx, y: r.minY + CGFloat(ink.points[i + 1]) * sy))
            i += 2
        }
        var p = Path()
        guard pts.count >= 2 else { return p }
        if ink.arrow {
            p.move(to: pts[0])
            p.addLine(to: pts[pts.count - 1])
            p.addPath(head(pts[0], pts[pts.count - 1], CGFloat(ink.size)))
            return p
        }
        return smooth(pts)
    }

    private func smooth(_ pts: [CGPoint]) -> Path {
        var p = Path()
        guard let first = pts.first else { return p }
        p.move(to: first)
        for j in 1..<max(1, pts.count) {
            let mid = CGPoint(x: (pts[j - 1].x + pts[j].x) / 2, y: (pts[j - 1].y + pts[j].y) / 2)
            p.addQuadCurve(to: mid, control: pts[j - 1])
        }
        if let last = pts.last { p.addLine(to: last) }
        return p
    }

    private func head(_ from: CGPoint, _ to: CGPoint, _ width: CGFloat) -> Path {
        let a = atan2(to.y - from.y, to.x - from.x)
        let len = 10 + width * 2
        var p = Path()
        for side in [-0.45, 0.45] {
            p.move(to: to)
            p.addLine(to: CGPoint(x: to.x - len * cos(a + side), y: to.y - len * sin(a + side)))
        }
        return p
    }

    /// What's being drawn right now, on screen.
    private func drawDraft(_ ctx: GraphicsContext, _ pts: [CGPoint]) {
        guard let a = pts.first, let z = pts.last else { return }
        let ink = drawColor(color) ?? Color.ink
        let dash = StrokeStyle(lineWidth: 1.5, dash: [6, 5])
        switch tool {
        case .pen: ctx.stroke(smooth(pts), with: .color(ink), style: StrokeStyle(lineWidth: max(1.5, penSize * view.k), lineCap: .round, lineJoin: .round))
        case .arrow:
            var p = Path(); p.move(to: a); p.addLine(to: z); p.addPath(head(a, z, 2.5))
            ctx.stroke(p, with: .color(ink), style: StrokeStyle(lineWidth: 2.5, lineCap: .round))
        case .rect, .ellipse, .diamond:
            let r = CGRect(x: min(a.x, z.x), y: min(a.y, z.y), width: abs(z.x - a.x), height: abs(z.y - a.y))
            var p: Path
            switch tool {
            case .ellipse: p = Path(ellipseIn: r)
            case .diamond:
                p = Path(); p.move(to: CGPoint(x: r.midX, y: r.minY)); p.addLine(to: CGPoint(x: r.maxX, y: r.midY))
                p.addLine(to: CGPoint(x: r.midX, y: r.maxY)); p.addLine(to: CGPoint(x: r.minX, y: r.midY)); p.closeSubpath()
            default: p = Path(r)
            }
            ctx.stroke(p, with: .color(Color.accent), style: dash)
        case .erase: ctx.fill(Path(ellipseIn: CGRect(x: z.x - 14, y: z.y - 14, width: 28, height: 28)), with: .color(Color.accent.opacity(0.3)))
        case .text: break
        }
    }
}
