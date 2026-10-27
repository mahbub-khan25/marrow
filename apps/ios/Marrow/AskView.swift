import SwiftUI

/// The conversation, kept while the app runs so switching tabs keeps it.
@Observable
final class Conversation {
    static let shared = Conversation()
    struct Turn: Identifiable {
        let id = UUID()
        let question: String
        let answer: Answer
    }
    var turns: [Turn] = []
}

struct AskView: View {
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var input = ""
    @State private var ready = false
    private let convo = Conversation.shared

    private var busy: Bool { convo.turns.last.map { !$0.answer.done } ?? false }

    var body: some View {
        Group {
            if !ready {
                ScrollView {
                    EmptyState(symbol: "sparkles", title: "Ask your notes", text: "Connect an AI you choose (OpenAI, Claude, Gemini, or one running on your own computer) and ask questions answered from your own notes.") {
                        Button("Set up an AI") { router.push(.ai) }.buttonStyle(.borderedProminent).padding(.top, 8)
                    }
                    .padding(.top, 40)
                }
            } else {
                chat
            }
        }
        .background(Color.paper)
        .navigationTitle(Text("Ask your notes"))
        .toolbar {
            if !convo.turns.isEmpty && !busy {
                ToolbarItem(placement: .topBarTrailing) {
                    Button { convo.turns.removeAll() } label: { Image(systemName: "square.and.pencil") }.accessibilityLabel(Text("New conversation"))
                }
            }
        }
        .onAppear { ready = store.marrow.aiReady() }
    }

    private var chat: some View {
        VStack(spacing: 0) {
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        if convo.turns.isEmpty {
                            Text("Marrow finds the parts of your notes that match and sends only those to your AI. Each point in the answer links to the note it comes from.")
                                .font(.callout).foregroundStyle(Color.inkSoft)
                            let examples = store.prefs.recentQuestions.isEmpty
                                ? [String(localized: "What did I decide about the trip?"), String(localized: "Summarize my notes from this week"), String(localized: "What's still open in my projects?")]
                                : Array(store.prefs.recentQuestions.prefix(4))
                            ForEach(examples, id: \.self) { q in
                                Button { ask(q) } label: {
                                    Text(verbatim: q).frame(maxWidth: .infinity, alignment: .leading).padding(16)
                                        .background(Color.card, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                                        .foregroundStyle(Color.ink)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                        ForEach(convo.turns) { t in
                            VStack(alignment: .leading, spacing: 10) {
                                HStack {
                                    Spacer(minLength: 60)
                                    Text(verbatim: t.question)
                                        .padding(.horizontal, 16).padding(.vertical, 12)
                                        .background(Color.accentSoft, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
                                        .foregroundStyle(Color.onAccentSoft)
                                }
                                AnswerView(answer: t.answer)
                            }
                            .id(t.id)
                        }
                    }
                    .padding(16)
                    .readingWidth()
                }
                .onChange(of: convo.turns.last?.answer.text) { _, _ in
                    if let id = convo.turns.last?.id { withAnimation { proxy.scrollTo(id, anchor: .bottom) } }
                }
            }
            HStack(alignment: .bottom, spacing: 8) {
                TextField("Ask your notes anything…", text: $input, axis: .vertical)
                    .lineLimit(1...5)
                    .padding(.horizontal, 16).padding(.vertical, 12)
                    .background(Color.cardStrong, in: RoundedRectangle(cornerRadius: 24, style: .continuous))
                    .onSubmit { ask(input) }
                if busy {
                    Button { convo.turns.last?.answer.task?.cancel(); convo.turns.last?.answer.done = true } label: {
                        Image(systemName: "stop.circle.fill").font(.system(size: 36))
                    }
                    .accessibilityLabel(Text("Stop"))
                } else {
                    Button { ask(input) } label: { Image(systemName: "arrow.up.circle.fill").font(.system(size: 36)) }
                        .disabled(input.trimmingCharacters(in: .whitespaces).isEmpty)
                        .accessibilityLabel(Text("Send"))
                }
            }
            .padding(.horizontal, 12).padding(.vertical, 8)
            .readingWidth()
        }
    }

    private func ask(_ q: String) {
        let question = q.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !question.isEmpty, !busy else { return }
        input = ""
        store.prefs.recentQuestions = Array(([question] + store.prefs.recentQuestions.filter { $0 != question }).prefix(6))
        let history = convo.turns.suffix(3).flatMap { [AiMessage(role: "user", content: $0.question), AiMessage(role: "assistant", content: $0.answer.text)] }
        let a = Answer()
        convo.turns.append(.init(question: question, answer: a))
        do {
            a.task = try store.marrow.askNotes(question: question, history: history, providerId: nil, listener: a)
        } catch {
            a.error = words(error)
            a.done = true
        }
    }
}

private struct AnswerView: View {
    let answer: Answer
    @Environment(Router.self) private var router

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if !answer.done && answer.text.isEmpty {
                HStack(spacing: 8) { ProgressView(); Text("Thinking…").foregroundStyle(Color.inkSoft) }
            }
            if let e = answer.error {
                Text(verbatim: e).foregroundStyle(Color.danger)
            } else if !answer.text.isEmpty {
                Blocks(blocks: renderAnswer(text: answer.text, sources: answer.sources), actions: BlockActions(onLink: { l in
                    guard l.kind == .note else { return }
                    let parts = l.target.components(separatedBy: "#L")
                    router.note(parts[0], line: parts.count > 1 ? Int(parts[1]) ?? -1 : -1)
                }))
            }
            if answer.done && !answer.sources.isEmpty {
                Text("From your notes").font(.footnote.weight(.semibold)).foregroundStyle(Color.inkSoft)
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(Array(answer.sources.enumerated()), id: \.offset) { i, s in
                            Button { router.note(s.path, line: Int(s.line)) } label: {
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(verbatim: "\(i + 1)  \(s.title)").font(.subheadline.weight(.semibold)).foregroundStyle(Color.accent).lineLimit(1)
                                    if !s.heading.isEmpty { Text(verbatim: s.heading).font(.caption).foregroundStyle(Color.faint).lineLimit(1) }
                                    Text(verbatim: s.text).font(.caption).foregroundStyle(Color.inkSoft).lineLimit(3)
                                }
                                .padding(12)
                                .frame(width: 220, alignment: .leading)
                                .background(Color.card, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
            }
        }
    }
}

/// Daily notes by month: tap a day to open (or start) its note.
struct CalendarView: View {
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var month = Calendar.current.date(from: Calendar.current.dateComponents([.year, .month], from: Date())) ?? Date()
    @State private var days: Set<Int> = []
    @State private var recent: [DayNote] = []
    @State private var previews: [String: String] = [:]

    var body: some View {
        let cal = Calendar.current
        let count = cal.range(of: .day, in: .month, for: month)?.count ?? 30
        let lead = (cal.component(.weekday, from: month) - cal.firstWeekday + 7) % 7
        let today = todayString()
        List {
            Section {
                VStack(spacing: 8) {
                    HStack {
                        Button { shift(-1) } label: { Image(systemName: "chevron.left") }.accessibilityLabel(Text("Previous month"))
                        Spacer()
                        Text(month.formatted(.dateTime.month(.wide).year())).font(.title3.weight(.semibold))
                        Spacer()
                        Button { shift(1) } label: { Image(systemName: "chevron.right") }.accessibilityLabel(Text("Next month"))
                    }
                    .buttonStyle(.borderless)
                    let symbols = cal.veryShortStandaloneWeekdaySymbols
                    HStack {
                        ForEach(0..<7, id: \.self) { i in
                            Text(verbatim: symbols[(i + cal.firstWeekday - 1) % 7]).font(.caption).foregroundStyle(Color.faint).frame(maxWidth: .infinity)
                        }
                    }
                    LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 4), count: 7), spacing: 6) {
                        ForEach(0..<(lead + count), id: \.self) { i in
                            if i < lead {
                                Color.clear.frame(height: 40)
                            } else {
                                let n = i - lead + 1
                                let date = cal.date(byAdding: .day, value: n - 1, to: month) ?? month
                                let key = todayString(date)
                                let has = days.contains(n)
                                Button { open(key) } label: {
                                    Text(verbatim: "\(n)")
                                        .font(.body.weight(has || key == today ? .semibold : .regular))
                                        .foregroundStyle(key == today ? Color.white : has ? Color.onAccentSoft : key > today ? Color.faint : Color.ink)
                                        .frame(width: 40, height: 40)
                                        .background(key == today ? Color.accent : has ? Color.accentSoft : Color.clear, in: Circle())
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }
                }
                .padding(.vertical, 8)
            }
            if !recent.isEmpty {
                Section {
                    ForEach(recent, id: \.path) { d in
                        Button { router.note(d.path) } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(verbatim: longDate(d.day)).font(.headline).foregroundStyle(Color.ink)
                                if let p = previews[d.path], !p.isEmpty { Text(verbatim: p).font(.subheadline).foregroundStyle(Color.inkSoft).lineLimit(2) }
                            }
                        }
                    }
                } header: { SectionTitle(text: "Daily notes") }
            }
        }
        .navigationTitle(Text("Calendar"))
        .task(id: "\(month)-\(store.tick)") { await load() }
    }

    private func shift(_ by: Int) {
        month = Calendar.current.date(byAdding: .month, value: by, to: month) ?? month
    }

    private func load() async {
        let ym = String(todayString(month).prefix(7))
        days = Set(((try? await store.run { try $0.daysWithNotes(month: ym) }) ?? []).map { Int($0) })
        let list = (try? await store.run { try $0.dailyNotes(limit: 40) }) ?? []
        recent = list
        previews = (try? await store.run { m -> [String: String] in
            var out: [String: String] = [:]
            for d in list.prefix(20) { out[d.path] = preview(text: (try? m.readNote(path: d.path)) ?? "", max: 120) }
            return out
        }) ?? [:]
    }

    private func open(_ day: String) {
        Task {
            do {
                let p = try await store.run { try $0.openDay(day: day) }
                router.note(p)
            } catch { store.say(error) }
        }
    }
}

/// How notes link to each other. `around`: only this note and its neighbours.
struct GraphView: View {
    let around: String
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var graph: Graph?
    @State private var tags = false
    @State private var missing = false
    @State private var twoSteps = false
    @State private var scale: CGFloat = 1
    @State private var lastScale: CGFloat = 1
    @State private var pan: CGSize = .zero
    @State private var lastPan: CGSize = .zero
    @State private var fitted = false

    var body: some View {
        VStack(spacing: 0) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack {
                    Toggle(isOn: $tags) { Text("Tags") }
                    Toggle(isOn: $missing) { Text("Notes not written yet") }
                    if !around.isEmpty { Toggle(isOn: $twoSteps) { Text("Two links away") } }
                }
                .toggleStyle(.button)
                .padding(.horizontal, 16).padding(.vertical, 6)
            }
            if let g = graph, g.nodes.isEmpty {
                EmptyState(symbol: "point.3.connected.trianglepath.dotted", title: "Graph", text: "Link notes with [[ ]] and they appear here, joined.")
                Spacer()
            } else if let g = graph {
                GeometryReader { geo in
                    let size = geo.size
                    Canvas { ctx, _ in
                        let c = CGPoint(x: size.width / 2 + pan.width, y: size.height / 2 + pan.height)
                        func at(_ i: Int) -> CGPoint { CGPoint(x: c.x + CGFloat(g.nodes[i].x) * scale, y: c.y + CGFloat(g.nodes[i].y) * scale) }
                        var edges = Path()
                        for e in g.edges {
                            edges.move(to: at(Int(e.from)))
                            edges.addLine(to: at(Int(e.to)))
                        }
                        ctx.stroke(edges, with: .color(Color.faint.opacity(0.5)), lineWidth: 1.5)
                        let few = g.nodes.count <= 80
                        for (i, n) in g.nodes.enumerated() {
                            let r = min(14, 4 + sqrt(CGFloat(n.degree)) * 2.2) * min(1.4, max(0.7, sqrt(scale)))
                            let p = at(i)
                            let color: Color = n.id == around ? .orange : n.kind == .tag ? .inkSoft : n.kind == .missing ? .faint : .accent
                            ctx.fill(Path(ellipseIn: CGRect(x: p.x - r, y: p.y - r, width: r * 2, height: r * 2)), with: .color(color))
                            if few || scale > 1.2 || n.degree >= 6 || n.id == around {
                                ctx.draw(Text(verbatim: n.name).font(.caption2).foregroundColor(Color.ink), at: CGPoint(x: p.x, y: p.y + r + 9))
                            }
                        }
                    }
                    .contentShape(Rectangle())
                    .gesture(
                        SimultaneousGesture(
                            MagnificationGesture()
                                .onChanged { v in scale = min(6, max(0.15, lastScale * v)) }
                                .onEnded { _ in lastScale = scale },
                            DragGesture()
                                .onChanged { v in pan = CGSize(width: lastPan.width + v.translation.width, height: lastPan.height + v.translation.height) }
                                .onEnded { _ in lastPan = pan }
                        )
                    )
                    .onTapGesture { loc in tap(loc, size: size, g: g) }
                    .onAppear { fit(g, size) }
                    .onChange(of: g.nodes.count) { _, _ in fit(g, size) }
                }
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Color.paper)
        .navigationTitle(around.isEmpty ? Text("Graph") : Text(verbatim: nameOf(around)))
        .navigationBarTitleDisplayMode(.inline)
        .task(id: "\(tags)-\(missing)-\(twoSteps)-\(store.tick)") {
            let (t, m, a, d) = (tags, missing, around, twoSteps)
            graph = try? await store.run { try $0.graph(showTags: t, showMissing: m, showAttachments: false, around: a.isEmpty ? nil : a, depth: d ? 2 : 1) }
            fitted = false
        }
    }

    private func fit(_ g: Graph, _ size: CGSize) {
        guard !g.nodes.isEmpty, size.width > 0, !fitted else { return }
        let xs = g.nodes.map { CGFloat($0.x) }, ys = g.nodes.map { CGFloat($0.y) }
        let w = max(1, (xs.max() ?? 0) - (xs.min() ?? 0)), h = max(1, (ys.max() ?? 0) - (ys.min() ?? 0))
        let s = min(3, max(0.15, min((size.width - 96) / w, (size.height - 96) / h)))
        scale = s
        lastScale = s
        pan = CGSize(width: -((xs.min() ?? 0) + (xs.max() ?? 0)) / 2 * s, height: -((ys.min() ?? 0) + (ys.max() ?? 0)) / 2 * s)
        lastPan = pan
        fitted = true
    }

    private func tap(_ loc: CGPoint, size: CGSize, g: Graph) {
        let c = CGPoint(x: size.width / 2 + pan.width, y: size.height / 2 + pan.height)
        let hit = g.nodes.min { a, b in
            hypot(c.x + CGFloat(a.x) * scale - loc.x, c.y + CGFloat(a.y) * scale - loc.y) < hypot(c.x + CGFloat(b.x) * scale - loc.x, c.y + CGFloat(b.y) * scale - loc.y)
        }
        if let hit, hit.kind == .note, hypot(c.x + CGFloat(hit.x) * scale - loc.x, c.y + CGFloat(hit.y) * scale - loc.y) < 32 {
            router.open(path: hit.id)
        }
    }
}

/// An image (pinch to zoom), or any other file with a way to open or share it.
struct FileView: View {
    let path: String
    @Environment(Store.self) private var store
    @State private var image: UIImage?
    @State private var url: URL?
    @State private var scale: CGFloat = 1

    var body: some View {
        Group {
            if let image {
                Image(uiImage: image).resizable().scaledToFit()
                    .scaleEffect(scale)
                    .gesture(MagnificationGesture().onChanged { scale = max(1, min(6, $0)) }.onEnded { _ in if scale < 1.05 { scale = 1 } })
            } else {
                EmptyState(symbol: "doc", title: LocalizedStringKey(nameOf(path)), text: "Marrow keeps this file with your notes. Open it with an app that knows this kind of file.") {
                    if let url { ShareLink(item: url) { Label("Open in another app", systemImage: "square.and.arrow.up") }.buttonStyle(.borderedProminent) }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.paper)
        .navigationTitle(Text(verbatim: nameOf(path)))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { if let url { ToolbarItem(placement: .topBarTrailing) { ShareLink(item: url) } } }
        .task {
            guard let p = try? store.marrow.filePath(path: path) else { return }
            url = URL(fileURLWithPath: p)
            image = await Task.detached { UIImage(contentsOfFile: p) }.value
        }
    }
}
