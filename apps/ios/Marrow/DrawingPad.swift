import SwiftUI

// Drawings inside notes: a small canvas (a `.canvas` file) shown where its
// `![[Drawing ….canvas|360]]` line is. Tap it to draw right there with a
// finger or Apple Pencil: pen, highlighter, lines, arrows, shapes (filled,
// light or see-through) and words, in colour. The
// cards it makes are the same as on the desktop and in canvases (the
// engine's strokeNode / shapeNode), so a drawing looks the same everywhere.

/// JSON Canvas colours "1"–"6".
private let drawPalette: [(String, Color)] = [("1", Color(light: 0xE03E3E, dark: 0xF07070)), ("2", Color(light: 0xE8780C, dark: 0xF5A04B)), ("3", Color(light: 0xD4A106, dark: 0xE8C040)), ("4", Color(light: 0x2F9E44, dark: 0x5CC071)), ("5", Color(light: 0x0B8CB5, dark: 0x4FB6DC)), ("6", Color(light: 0x8E44AD, dark: 0xB57BD0))]

private func drawColor(_ c: String) -> Color? {
    if let p = drawPalette.first(where: { $0.0 == c }) { return p.1 }
    if c.hasPrefix("#"), c.count == 7, let v = UInt32(c.dropFirst(), radix: 16) { return Color(uiColor: UIColor(hex: v)) }
    return nil
}

enum DrawTool: CaseIterable {
    case select, pen, shape, line, arrow, double, highlight, text, erase
    /// What the engine's strokeNode calls it.
    var stroke: String {
        switch self {
        case .pen: return "pen"
        case .line: return "line"
        case .arrow: return "arrow"
        case .double: return "double"
        case .highlight: return "highlighter"
        default: return ""
        }
    }
}

/// The highlighter's colour when none is picked.
let highlightColor = Color(light: 0xF2C94C, dark: 0xE8C040)

/// The engine's shapes (the same list as the desktop's): outlines in a 100 × 100 box.
let drawShapes: [ShapeGeometry] = shapeKinds()
func shapeOf(_ kind: String) -> ShapeGeometry? { drawShapes.first { $0.kind == kind } ?? drawShapes.first }

/// SVG path data (only M, L, C and Z) from the 100 × 100 box, placed in `r`.
func placePath(_ d: String, in r: CGRect) -> Path {
    var tokens: [String] = []
    var cur = ""
    for ch in d {
        if "MLCZ".contains(ch) {
            if !cur.isEmpty { tokens.append(cur); cur = "" }
            tokens.append(String(ch))
        } else if ch == "," || ch == " " {
            if !cur.isEmpty { tokens.append(cur); cur = "" }
        } else if ch == "-", !cur.isEmpty {
            tokens.append(cur)
            cur = "-"
        } else {
            cur.append(ch)
        }
    }
    if !cur.isEmpty { tokens.append(cur) }
    var p = Path()
    var i = 0
    var cmd: Character = "M"
    func pt() -> CGPoint? {
        guard i + 1 < tokens.count, let x = Double(tokens[i]), let y = Double(tokens[i + 1]) else { return nil }
        i += 2
        return CGPoint(x: r.minX + CGFloat(x) / 100 * r.width, y: r.minY + CGFloat(y) / 100 * r.height)
    }
    while i < tokens.count {
        if let c = tokens[i].first, c.isLetter {
            cmd = c
            i += 1
            if cmd == "Z" { p.closeSubpath() }
            continue
        }
        switch cmd {
        case "M":
            guard let a = pt() else { return p }
            p.move(to: a)
            cmd = "L"
        case "L":
            guard let a = pt() else { return p }
            p.addLine(to: a)
        case "C":
            guard let a = pt(), let b = pt(), let z = pt() else { return p }
            p.addCurve(to: z, control1: a, control2: b)
        default:
            i += 1
        }
    }
    return p
}

/// A shape, filled lightly (see-through), solid, or not at all ("none": what's
/// under it shows), with its outline in `line` (or its colour).
func drawShape(_ c: inout GraphicsContext, _ g: ShapeGeometry?, fill: String, color col: Color?, in r: CGRect, width: CGFloat, line: Color? = nil) {
    let shape = g.map { placePath($0.outline, in: r) } ?? Path(r)
    switch fill {
    case "none": break
    case "solid":
        if let col { c.fill(shape, with: .color(col)) } else {
            c.fill(shape, with: .color(Color.paper))
            c.fill(shape, with: .color(Color.ink.opacity(0.78)))
        }
    default: c.fill(shape, with: .color(col.map { $0.opacity(0.16) } ?? Color.paper.opacity(0.82)))
    }
    let stroke = line ?? col ?? Color.ink.opacity(0.7)
    c.stroke(shape, with: .color(stroke), style: StrokeStyle(lineWidth: width, lineJoin: .round))
    if let extra = g?.extra, !extra.isEmpty { c.stroke(placePath(extra, in: r), with: .color(stroke), lineWidth: width) }
}

/// A shape's name, in the interface language.
func shapeName(_ g: ShapeGeometry) -> LocalizedStringKey {
    switch g.kind {
    case "rect": return "Rectangle"
    case "rounded": return "Rounded rectangle"
    case "ellipse": return "Ellipse"
    case "diamond": return "Diamond"
    case "triangle": return "Triangle"
    case "hexagon": return "Hexagon"
    case "parallelogram": return "Parallelogram"
    case "star": return "Star"
    case "cylinder": return "Cylinder"
    case "bubble": return "Speech bubble"
    case "cloud": return "Cloud"
    default: return LocalizedStringKey(g.name)
    }
}

/// Lays its views out in lines, starting a new line when the next one doesn't fit.
struct WrapLayout: Layout {
    var spacing: CGFloat = 4
    var lineSpacing: CGFloat = 4

    private func lines(_ width: CGFloat, _ subviews: Subviews) -> [(items: [(Int, CGSize)], height: CGFloat)] {
        var out: [(items: [(Int, CGSize)], height: CGFloat)] = []
        var cur: [(Int, CGSize)] = []
        var x: CGFloat = 0, h: CGFloat = 0
        for (i, v) in subviews.enumerated() {
            let size = v.sizeThatFits(.unspecified)
            if !cur.isEmpty, x + spacing + size.width > width {
                out.append((cur, h))
                cur = []
                x = 0
                h = 0
            }
            x += (cur.isEmpty ? 0 : spacing) + size.width
            h = max(h, size.height)
            cur.append((i, size))
        }
        if !cur.isEmpty { out.append((cur, h)) }
        return out
    }

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        let ls = lines(width, subviews)
        let used = ls.map { l in l.items.map(\.1.width).reduce(0, +) + spacing * CGFloat(max(0, l.items.count - 1)) }.max() ?? 0
        let height = ls.map(\.height).reduce(0, +) + lineSpacing * CGFloat(max(0, ls.count - 1))
        return CGSize(width: proposal.width ?? used, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for l in lines(bounds.width, subviews) {
            var x = bounds.minX
            for (i, size) in l.items {
                subviews[i].place(at: CGPoint(x: x, y: y + (l.height - size.height) / 2), proposal: ProposedViewSize(size))
                x += size.width + spacing
            }
            y += l.height + lineSpacing
        }
    }
}

/// A shape's outline as a small icon.
struct ShapeGlyph: View {
    let kind: String
    var body: some View {
        Canvas { ctx, size in
            guard let g = shapeOf(kind) else { return }
            let r = CGRect(origin: .zero, size: size).insetBy(dx: 1.5, dy: 1.5)
            ctx.stroke(placePath(g.outline, in: r), with: .foreground, style: StrokeStyle(lineWidth: 1.6, lineJoin: .round))
            if !g.extra.isEmpty { ctx.stroke(placePath(g.extra, in: r), with: .foreground, lineWidth: 1.6) }
        }
        .frame(width: 18, height: 18)
    }
}

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
    var onDelete: (() -> Void)?
    @State private var active = false

    var body: some View {
        DrawingPad(path: path, active: $active, onOpenFull: onOpenFull, onDelete: onDelete, boardHeight: CGFloat(height))
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
    var onDelete: (() -> Void)?
    /// In a note the drawing keeps this height (the tools sit above it); nil fills the space.
    var boardHeight: CGFloat?
    @State private var askDelete = false

    @Environment(Store.self) private var store
    @State private var board: Board?
    @State private var missing = false
    @State private var past: [Board] = []
    @State private var tool: DrawTool = .pen
    @State private var color = ""
    @State private var penSize: Double = 4
    @State private var shapeKind = "rect"
    @State private var fill = ""
    /// The card picked with the select tool (dragged to move, its corner to resize).
    @State private var selected: String?
    @State private var moving: Moving?

    private struct Moving {
        var from: Board
        var start: CGPoint
        var target: CanvasNode?
        var resizing: Bool
        var moved = false
    }

    private var selection: CanvasNode? { tool == .select ? board?.nodes.first { $0.id == selected } : nil }
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
                    if let n = selection {
                        let pad = 4 / view.k
                        let r = CGRect(x: n.x, y: n.y, width: n.width, height: n.height).insetBy(dx: -pad, dy: -pad)
                        c.stroke(Path(r), with: .color(Color.accent), style: StrokeStyle(lineWidth: 1.5 / view.k, dash: [8 / view.k, 6 / view.k]))
                        let h = CGRect(x: n.x + n.width - 8 / view.k, y: n.y + n.height - 8 / view.k, width: 16 / view.k, height: 16 / view.k)
                        c.fill(Path(ellipseIn: h), with: .color(Color.paper))
                        c.stroke(Path(ellipseIn: h), with: .color(Color.accent), lineWidth: 2 / view.k)
                    }
                    if draft.count > 1 { drawDraft(ctx, draft) }
                }
                if missing {
                    Text("This drawing was moved or deleted.").font(.subheadline).foregroundStyle(Color.faint)
                } else if !active, board?.nodes.isEmpty == true {
                    Text("Tap to draw").font(.subheadline).foregroundStyle(Color.faint)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: boardHeight == nil ? .infinity : nil)
            .frame(height: boardHeight)
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
        .onChange(of: tool) { _, t in if t != .select { selected = nil } }
        .confirmationDialog(Text("Delete this drawing?"), isPresented: $askDelete, titleVisibility: .visible) {
            Button("Delete drawing", role: .destructive) {
                saveTask?.cancel()
                active = false
                onDelete?()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("It's taken out of the note, and its file goes to Marrow's trash, where you can restore it.")
        }
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
                if tool == .select { dragSelected(v); return }
                if draft.isEmpty { draft = [v.startLocation] }
                if let last = draft.last, hypot(v.location.x - last.x, v.location.y - last.y) > 1.5 { draft.append(v.location) }
            }
            .onEnded { v in
                if tool == .select { endDrag(); return }
                var pts = draft.isEmpty ? [v.startLocation] : draft
                pts.append(v.location)
                draft = []
                finish(pts)
            }
    }

    /// Select tool: pick what's under the finger (or the picked card's corner), then drag it.
    private func dragSelected(_ v: DragGesture.Value) {
        if moving == nil {
            guard let b = board else { return }
            let p = view.world(v.startLocation)
            let tol = 12 / view.k
            let cur = b.nodes.first { $0.id == selected }
            var resizing = false
            if let c = cur { resizing = hypot(p.x - CGFloat(c.x + c.width), p.y - CGFloat(c.y + c.height)) < tol * 1.8 }
            let target = resizing ? cur : hitNode(b, p, tol)
            selected = target?.id
            moving = Moving(from: b, start: p, target: target, resizing: resizing)
        }
        guard var m = moving, let t = m.target else { return }
        let w = view.world(v.location)
        let dx = Double(w.x - m.start.x), dy = Double(w.y - m.start.y)
        if !m.moved, hypot(dx, dy) * Double(view.k) < 4 { return }
        m.moved = true
        moving = m
        var b = m.from
        if let i = b.nodes.firstIndex(where: { $0.id == t.id }) {
            if m.resizing {
                b.nodes[i].width = max(16, (t.width + dx).rounded())
                b.nodes[i].height = max(16, (t.height + dy).rounded())
            } else {
                b.nodes[i].x = (t.x + dx).rounded()
                b.nodes[i].y = (t.y + dy).rounded()
            }
        }
        board = b
    }

    private func endDrag() {
        if let m = moving, m.moved, let now = board {
            past.append(m.from)
            if past.count > 100 { past.removeFirst() }
            save(now)
        }
        moving = nil
    }

    /// The topmost card at `p`. Strokes count only near their line, and
    /// see-through shapes only near their outline, so what's under them can
    /// still be picked.
    private func hitNode(_ b: Board, _ p: CGPoint, _ tol: CGFloat) -> CanvasNode? {
        b.nodes.last { n in
            let r = CGRect(x: n.x, y: n.y, width: n.width, height: n.height)
            guard r.insetBy(dx: -tol, dy: -tol).contains(p) else { return false }
            if let ink = n.ink {
                return inkPath(ink, in: r).strokedPath(StrokeStyle(lineWidth: CGFloat(ink.size) + tol * 2, lineCap: .round, lineJoin: .round)).contains(p)
            }
            if !n.shape.isEmpty, n.fill == "none" {
                let outline = shapeOf(n.shape).map { placePath($0.outline, in: r) } ?? Path(r)
                if outline.strokedPath(StrokeStyle(lineWidth: tol * 2)).contains(p) { return true }
                return !n.text.isEmpty && abs(p.x - r.midX) < r.width / 4 && abs(p.y - r.midY) < r.height / 4
            }
            return true
        }
    }

    /// After a shape, line, arrow or words: pick it, ready to move (pen and highlighter keep drawing).
    private func pick(_ n: CanvasNode) {
        selected = n.id
        tool = .select
    }

    private func update(_ id: String, _ f: (inout CanvasNode) -> Void) {
        change { b in if let i = b.nodes.firstIndex(where: { $0.id == id }) { f(&b.nodes[i]) } }
    }

    private func finish(_ pts: [CGPoint]) {
        let w = pts.map { view.world($0) }
        guard let a = w.first, let z = w.last, let b = board else { return }
        switch tool {
        case .select: break
        case .pen, .line, .arrow, .double, .highlight:
            if let n = strokeNode(points: w.flatMap { [Double($0.x), Double($0.y)] }, size: penSize, kind: tool.stroke, color: color, zoom: Double(view.k)) {
                change { $0.nodes.append(n) }
                if tool != .pen, tool != .highlight { pick(n) }
            }
        case .shape:
            let n = shapeNode(shape: shapeKind, x0: Double(a.x), y0: Double(a.y), x1: Double(z.x), y1: Double(z.y), color: color, fill: fill)
            change { $0.nodes.append(n) }
            pick(n)
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
            pick(n)
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
            // Every tool shows: on a narrow screen they wrap onto more lines.
            WrapLayout(spacing: 2, lineSpacing: 2) {
                toolButton(.select, "hand.point.up.left", "Select and move")
                toolButton(.pen, "scribble.variable", "Pen")
                toolButton(.highlight, "highlighter", "Highlighter")
                // One button for every shape: a tap lists them.
                Menu {
                    ForEach(drawShapes, id: \.kind) { g in
                        Button {
                            shapeKind = g.kind
                            tool = .shape
                        } label: { Text(shapeName(g)) }
                    }
                } label: {
                    ShapeGlyph(kind: shapeKind)
                        .foregroundStyle(tool == .shape ? Color.paper : Color.ink)
                        .frame(width: 38, height: 34)
                        .background(tool == .shape ? Color.accent : Color.clear, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                }
                .accessibilityLabel(Text("Shapes"))
                toolButton(.line, "line.diagonal", "Line")
                toolButton(.arrow, "arrow.right", "Arrow")
                toolButton(.double, "arrow.left.and.right", "Double arrow")
                toolButton(.text, "textformat", "Words")
                toolButton(.erase, "eraser", "Eraser")
                Button {
                    if let prev = past.popLast() { board = prev; save(prev) }
                } label: { Image(systemName: "arrow.uturn.backward").frame(width: 38, height: 34) }
                .disabled(past.isEmpty)
                .accessibilityLabel(Text("Undo"))
                if onDelete != nil {
                    Button { askDelete = true } label: {
                        Image(systemName: "trash").font(.system(size: 17)).foregroundStyle(Color.red).frame(width: 38, height: 34)
                    }
                    .accessibilityLabel(Text("Delete drawing"))
                }
                Button { active = false } label: {
                    Label("Done", systemImage: "checkmark").font(.subheadline.weight(.semibold)).frame(height: 34).padding(.horizontal, 8)
                }
            }
            .padding(.horizontal, 4)
            WrapLayout(spacing: 8, lineSpacing: 6) {
                swatch(nil, "")
                ForEach(drawPalette, id: \.0) { p in swatch(p.1, p.0) }
                if let n = selection {
                    gap
                    if n.ink == nil {
                        actionChip("Edit words") {
                            words = n.text
                            asking = .label(n.id)
                        }
                    }
                    actionChip("Delete") {
                        change { b in b.nodes.removeAll { $0.id == n.id }; b.edges.removeAll { $0.fromNode == n.id || $0.toNode == n.id } }
                        selected = nil
                    }
                }
                if tool == .shape || selection?.shape.isEmpty == false {
                    gap
                    fillChip("none", "No fill")
                    fillChip("", "Light")
                    fillChip("solid", "Solid")
                }
                if tool == .pen {
                    gap
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
        .padding(.vertical, 4)
        .background(Color.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .buttonStyle(.plain)
    }

    /// A thin line between groups in the second row.
    private var gap: some View { Rectangle().fill(Color.line).frame(width: 1, height: 20) }

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

    private func actionChip(_ label: LocalizedStringKey, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(label).font(.subheadline)
                .padding(.horizontal, 10).frame(height: 28)
                .overlay(Capsule().stroke(Color.line, lineWidth: 1))
        }
    }

    private func fillChip(_ f: String, _ label: LocalizedStringKey) -> some View {
        let on = (selection?.fill ?? fill) == f
        return Button {
            fill = f
            if let n = selection { update(n.id) { $0.fill = f } }
        } label: {
            Text(label).font(.subheadline)
                .padding(.horizontal, 10).frame(height: 28)
                .background(on ? Color.line : Color.clear, in: Capsule())
                .overlay(Capsule().stroke(Color.line, lineWidth: 1))
        }
    }

    private func swatch(_ c: Color?, _ id: String) -> some View {
        let on = (selection?.color ?? color) == id
        return Button {
            color = id
            if let n = selection { update(n.id) { $0.color = id } }
        } label: {
            ZStack {
                Circle().fill(c ?? Color.paper).padding(4)
                if c == nil { Path { p in p.move(to: CGPoint(x: 22, y: 6)); p.addLine(to: CGPoint(x: 6, y: 22)) }.stroke(Color.red, lineWidth: 2) }
                Circle().stroke(on ? Color.ink : Color.line, lineWidth: on ? 2 : 1)
            }
            .frame(width: 28, height: 28)
        }
    }

    /// What's being drawn right now, on screen.
    private func drawDraft(_ ctx: GraphicsContext, _ pts: [CGPoint]) {
        guard let a = pts.first, let z = pts.last else { return }
        let ink = drawColor(color) ?? Color.ink
        let dash = StrokeStyle(lineWidth: 1.5, dash: [6, 5])
        switch tool {
        case .pen: ctx.stroke(smooth(pts), with: .color(ink), style: StrokeStyle(lineWidth: max(1.5, penSize * view.k), lineCap: .round, lineJoin: .round))
        case .highlight:
            ctx.stroke(smooth(pts), with: .color((drawColor(color) ?? highlightColor).opacity(0.4)), style: StrokeStyle(lineWidth: max(14, penSize) * view.k, lineCap: .round, lineJoin: .round))
        case .line:
            var p = Path(); p.move(to: a); p.addLine(to: z)
            ctx.stroke(p, with: .color(ink), style: StrokeStyle(lineWidth: max(1.5, penSize * view.k), lineCap: .round))
        case .arrow, .double:
            var p = Path(); p.move(to: a); p.addLine(to: z); p.addPath(head(a, z, 2.5))
            if tool == .double { p.addPath(head(z, a, 2.5)) }
            ctx.stroke(p, with: .color(ink), style: StrokeStyle(lineWidth: 2.5, lineCap: .round))
        case .shape:
            let r = CGRect(x: min(a.x, z.x), y: min(a.y, z.y), width: abs(z.x - a.x), height: abs(z.y - a.y))
            let p = shapeOf(shapeKind).map { placePath($0.outline, in: r) } ?? Path(r)
            ctx.stroke(p, with: .color(Color.accent), style: dash)
        case .erase: ctx.fill(Path(ellipseIn: CGRect(x: z.x - 14, y: z.y - 14, width: 28, height: 28)), with: .color(Color.accent.opacity(0.3)))
        case .text, .select: break
        }
    }
}

// MARK: rendering (in world units: 1 = 1 point at 100%); Home draws boards with it too

func drawBoard(_ c: inout GraphicsContext, _ b: Board) {
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
            let inkColor = ink.highlight ? (col ?? highlightColor).opacity(0.4) : col ?? Color.ink.opacity(0.85)
            c.stroke(inkPath(ink, in: r), with: .color(inkColor), style: StrokeStyle(lineWidth: CGFloat(ink.size), lineCap: .round, lineJoin: .round))
        } else if !n.shape.isEmpty {
            let g = shapeOf(n.shape)
            drawShape(&c, g, fill: n.fill, color: col, in: r, width: 2)
            let lb = (g?.label.count == 4 ? g?.label : nil) ?? [4, 4, 96, 96]
            let box = CGRect(x: r.minX + CGFloat(lb[0]) / 100 * r.width, y: r.minY + CGFloat(lb[1]) / 100 * r.height, width: CGFloat(lb[2] - lb[0]) / 100 * r.width, height: CGFloat(lb[3] - lb[1]) / 100 * r.height)
            label(&c, n.text, in: box.insetBy(dx: 4, dy: 2), size: 17, center: true, color: n.fill == "solid" ? Color.paper : Color.ink)
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

fileprivate func label(_ c: inout GraphicsContext, _ text: String, in r: CGRect, size: CGFloat, center: Bool, color: Color = Color.ink) {
    guard !text.trimmingCharacters(in: .whitespaces).isEmpty, r.width > 1, r.height > 1 else { return }
    let t = c.resolve(Text(verbatim: text).font(.system(size: size)).foregroundColor(color))
    let s = t.measure(in: r.size)
    let box = center ? CGRect(x: r.midX - s.width / 2, y: r.midY - s.height / 2, width: s.width, height: min(s.height, r.height)) : CGRect(origin: r.origin, size: CGSize(width: r.width, height: min(s.height, r.height)))
    c.draw(t, in: box)
}

fileprivate func inkPath(_ ink: Ink, in r: CGRect) -> Path {
    let sx = r.width / CGFloat(ink.w), sy = r.height / CGFloat(ink.h)
    var pts: [CGPoint] = []
    var i = 0
    while i + 1 < ink.points.count {
        pts.append(CGPoint(x: r.minX + CGFloat(ink.points[i]) * sx, y: r.minY + CGFloat(ink.points[i + 1]) * sy))
        i += 2
    }
    var p = Path()
    guard pts.count >= 2 else { return p }
    if ink.arrow || ink.startArrow {
        p.move(to: pts[0])
        p.addLine(to: pts[pts.count - 1])
        if ink.arrow { p.addPath(head(pts[0], pts[pts.count - 1], CGFloat(ink.size))) }
        if ink.startArrow { p.addPath(head(pts[pts.count - 1], pts[0], CGFloat(ink.size))) }
        return p
    }
    return smooth(pts)
}

fileprivate func smooth(_ pts: [CGPoint]) -> Path {
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

fileprivate func head(_ from: CGPoint, _ to: CGPoint, _ width: CGFloat) -> Path {
    let a = atan2(to.y - from.y, to.x - from.x)
    let len = 10 + width * 2
    var p = Path()
    for side in [-0.45, 0.45] {
        p.move(to: to)
        p.addLine(to: CGPoint(x: to.x - len * cos(a + side), y: to.y - len * sin(a + side)))
    }
    return p
}
