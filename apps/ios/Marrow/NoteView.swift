import PhotosUI
import SwiftUI

struct NoteView: View {
    let path: String
    var line: Int = -1
    var startEditing = false

    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @Environment(\.reading) private var reading
    @Environment(\.dismiss) private var dismiss

    @State private var current = ""
    @State private var loaded = false
    @State private var lock: LockState?
    @State private var error: String?
    /// The text as last read from or written to disk.
    @State private var saved = ""
    @State private var text = ""
    @State private var blocks: [Block] = []
    @State private var editing = false
    @State private var starred = false
    @State private var showInfo = false
    @State private var showAi = false
    @State private var renaming = false
    @State private var newName = ""
    @State private var moving = false
    @State private var deleting = false
    @State private var unlocking = false
    @State private var unlockError: String?
    @State private var locking = false
    @State private var insert: String?
    @State private var photo: PhotosPickerItem?
    @State private var pickingPhoto = false
    @State private var dictating = false
    @State private var saveTask: Task<Void, Never>?

    var body: some View {
        content
            .background(Color.paper)
            .navigationTitle(editing ? Text(verbatim: nameOf(current)) : Text(verbatim: ""))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbar }
            .task(id: "\(current)-\(store.locksTick)") { await reload(first: !loaded) }
            .onChange(of: store.tick) { _, _ in
                // Changed by sync or another app, while not being edited here.
                if store.changed.contains(current) && text == saved { Task { await reload(first: false) } }
            }
            .onChange(of: text) { _, _ in scheduleSave() }
            .onAppear { if current.isEmpty { current = path } }
            .onDisappear { Task { await save() } }
            .sheet(isPresented: $showInfo) { NoteInfoSheet(path: current) }
            .sheet(isPresented: $showAi) {
                NoteAiSheet(path: current, text: text, onReplace: { new in replaceAll(new) }, onAppend: { add in
                    replaceAll(text.trimmingCharacters(in: .whitespacesAndNewlines) + "\n\n" + add.trimmingCharacters(in: .whitespacesAndNewlines) + "\n")
                })
            }
            .sheet(isPresented: $moving) { MoveSheet(path: current) { folder in move(to: folder) } }
            .sheet(isPresented: $unlocking) {
                PasswordSheet(title: String(localized: "Unlock “\(nameOf(current))”"), hint: lock?.hint, error: unlockError) { pw in
                    Task {
                        do {
                            try await store.run { [p = current] m in try m.unlock(path: p, password: pw) }
                            unlocking = false
                            unlockError = nil
                        } catch { unlockError = String(localized: "That password isn't right.") }
                    }
                }
            }
            .sheet(isPresented: $locking) {
                LockSheet { pw, hint in
                    Task {
                        await save()
                        do {
                            let warn = try await store.run { [p = current] m in try m.lockNote(path: p, password: pw, hint: hint) }
                            store.say(warn ?? String(localized: "Locked. Marrow forgets the password when you leave the app for a while."))
                        } catch { store.say(error) }
                    }
                }
            }
            .sheet(isPresented: $dictating) { DictationSheet { said in insert = said } }
            .photosPicker(isPresented: $pickingPhoto, selection: $photo, matching: .images)
            .onChange(of: photo) { _, item in addPhoto(item) }
            .alert("Rename", isPresented: $renaming) {
                TextField("Name", text: $newName)
                Button("Cancel", role: .cancel) {}
                Button("Rename") { rename() }
            }
            .confirmationDialog(Text("Delete “\(nameOf(current))”?"), isPresented: $deleting, titleVisibility: .visible) {
                Button("Delete", role: .destructive) {
                    Task {
                        do { try await store.run { [p = current] m in try m.trash(path: p) }; dismiss() } catch { store.say(error) }
                    }
                }
            } message: {
                Text("It goes to the vault's trash folder (.marrow/trash), so you can still get it back.")
            }
    }

    @ViewBuilder
    private var content: some View {
        if !loaded {
            Color.paper
        } else if let error {
            Text(verbatim: error).foregroundStyle(Color.danger).padding(24)
        } else if let lock {
            EmptyState(symbol: "lock", title: LocalizedStringKey(nameOf(current)), text: lock.folder != nil ? "This note is in a locked folder. Enter the folder's password to read it." : "This note is locked. Enter its password to read it.") {
                Button("Unlock") { unlocking = true }.buttonStyle(.borderedProminent).padding(.top, 6)
            }
            .padding(.top, 60)
        } else if editing {
            MarkdownEditor(
                text: $text,
                startLine: line,
                reading: reading,
                suggest: { q in (try? store.marrow.findFiles(query: q, source: current, limit: 8).filter(\.isNote)) ?? [] },
                onPhoto: { pickingPhoto = true },
                onDictate: { dictating = true },
                insert: $insert
            )
            .padding(.horizontal, 20)
            .readingWidth()
        } else {
            reader
        }
    }

    private var reader: some View {
        let title = nameOf(current)
        // The first heading repeats the title: don't show it twice.
        var shown = blocks
        if let i = shown.firstIndex(where: { $0.kind != .properties }), shown[i].kind == .heading, shown[i].level == 1,
           shown[i].spans.map(\.text).joined().trimmingCharacters(in: .whitespaces).lowercased() == title.lowercased() {
            shown.remove(at: i)
        }
        return ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text(verbatim: title).font(reading.title).foregroundStyle(Color.ink).padding(.top, 4).padding(.bottom, 16)
                if shown.isEmpty {
                    Button { editing = true } label: {
                        Text("This note is empty. Tap here to write.").font(reading.body).foregroundStyle(Color.faint)
                    }
                    .buttonStyle(.plain)
                }
                Blocks(blocks: shown, actions: actions)
                LinkedFrom(path: current)
                Color.clear.frame(height: 120)
            }
            .padding(.horizontal, 22)
            .readingWidth()
        }
        .textSelection(.enabled)
    }

    private var actions: BlockActions {
        BlockActions(
            onLink: { link in
                switch link.kind {
                case .url:
                    if let u = URL(string: link.target) { UIApplication.shared.open(u) }
                case .tag:
                    router.push(.tag(link.target))
                case .note:
                    let parts = link.target.components(separatedBy: "#L")
                    router.open(path: parts[0], line: parts.count > 1 ? Int(parts[1]) ?? -1 : -1)
                case .missing:
                    Task {
                        do {
                            let target = try await store.run { [p = current] m in try m.followLink(link: link.target, source: p) }
                            router.note(target, edit: true)
                        } catch { store.say(error) }
                    }
                }
            },
            onToggle: { at in
                Task {
                    await save()
                    do {
                        let next = try await store.run { [p = current] m in try m.toggleTask(path: p, line: UInt32(at)) }
                        saved = next
                        text = next
                        await render(next)
                    } catch { store.say(error) }
                }
            },
            onOpen: { target in
                if target.hasPrefix("http"), let u = URL(string: target) { UIApplication.shared.open(u) } else { router.open(path: target) }
            },
            fileOf: { p in try? store.marrow.filePath(path: p) }
        )
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .topBarTrailing) {
            if loaded && lock == nil {
                if editing {
                    Button("Done") {
                        editing = false
                        Task { await save(); await render(text) }
                    }
                    .fontWeight(.semibold)
                } else {
                    Button { editing = true } label: { Image(systemName: "pencil") }.accessibilityLabel(Text("Edit"))
                }
                Button { showInfo = true } label: { Image(systemName: "info.circle") }.accessibilityLabel(Text("About this note"))
            }
            Menu {
                if store.marrow.aiReady() && lock == nil {
                    Button { showAi = true } label: { Label("Ask AI about this note", systemImage: "sparkles") }
                }
                Button {
                    Task {
                        let (p, on) = (current, !starred)
                        try? await store.run { m in try m.setStarred(path: p, starred: on) }
                        starred.toggle()
                    }
                } label: { Label(starred ? "Remove star" : "Star", systemImage: starred ? "star.slash" : "star") }
                ShareLink(item: text, subject: Text(verbatim: nameOf(current))) { Label("Share", systemImage: "square.and.arrow.up") }
                Button { router.push(.history(current)) } label: { Label("Version history", systemImage: "clock.arrow.circlepath") }
                Button { router.push(.graph(around: current)) } label: { Label("Linked notes graph", systemImage: "point.3.connected.trianglepath.dotted") }
                Divider()
                Button { newName = nameOf(current); renaming = true } label: { Label("Rename", systemImage: "pencil.line") }
                Button { moving = true } label: { Label("Move to…", systemImage: "folder") }
                Button {
                    Task {
                        do {
                            let copy = try await store.run { [p = current] m in try m.duplicate(path: p) }
                            router.note(copy)
                        } catch { store.say(error) }
                    }
                } label: { Label("Duplicate", systemImage: "plus.square.on.square") }
                if lock == nil {
                    Button { locking = true } label: { Label("Lock with a password", systemImage: "lock") }
                } else if lock?.unlocked == true {
                    Button {
                        Task { do { _ = try await store.run { [p = current] m in try m.removeLock(path: p) } } catch { store.say(error) } }
                    } label: { Label("Remove the lock", systemImage: "lock.open") }
                }
                Divider()
                Button(role: .destructive) { deleting = true } label: { Label("Delete", systemImage: "trash") }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
            .accessibilityLabel(Text("More"))
        }
    }

    // MARK: - Loading and saving

    private func render(_ t: String) async {
        let p = current
        blocks = (try? await store.run { try $0.renderNote(path: p, text: t) }) ?? []
    }

    private func reload(first: Bool) async {
        if current.isEmpty { current = path }
        let p = current
        do {
            let t = try await store.run { try $0.readNote(path: p) }
            lock = nil
            if t != saved || first {
                saved = t
                if text != t { text = t }
                await render(t)
            }
            if first {
                editing = startEditing || store.prefs.openIn == "edit" || t.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                starred = (try? await store.run { try $0.vaultSettings().starred.contains(p) }) ?? false
                try? await store.run { try $0.markSeen(path: p, nowMs: Int64(Date().timeIntervalSince1970 * 1000)) }
            }
            error = nil
        } catch MarrowError.Locked {
            lock = try? await store.run { try $0.lockState(path: p) }
        } catch {
            self.error = words(error)
        }
        loaded = true
    }

    private func scheduleSave() {
        saveTask?.cancel()
        saveTask = Task {
            try? await Task.sleep(nanoseconds: 700_000_000)
            if !Task.isCancelled { await save() }
        }
    }

    /// Saves what's typed; merges if the note changed elsewhere meanwhile.
    private func save() async {
        let t = text, base = saved, p = current
        guard loaded, lock == nil, t != base else { return }
        do {
            try await store.run { try $0.writeNote(path: p, text: t, expected: base) }
            saved = t
        } catch MarrowError.ChangedElsewhere {
            do {
                let merged = try await store.run { m -> Merged in
                    let disk = try m.readNote(path: p)
                    let merged = m.mergeText(base: base, mine: t, theirs: disk)
                    try m.writeNote(path: p, text: merged.text, expected: disk)
                    return merged
                }
                saved = merged.text
                text = merged.text
                if merged.conflicts > 0 { store.say(String(localized: "This note changed elsewhere too. Both versions were kept; look for the marked part.")) }
            } catch { store.say(error) }
        } catch {
            store.say(error)
        }
    }

    private func replaceAll(_ new: String) {
        text = new
        Task { await save(); await render(new) }
    }

    private func rename() {
        let name = newName
        Task {
            await save()
            do {
                let r = try await store.run { [p = current] m in try m.rename(path: p, newName: name) }
                current = r.newPath
                if r.linksUpdated > 0 { store.say(String(localized: "\(Int(r.linksUpdated)) links were updated")) }
            } catch { store.say(error) }
        }
    }

    private func move(to folder: String) {
        Task {
            await save()
            do {
                let r = try await store.run { [p = current] m in try m.movePath(path: p, folder: folder) }
                current = r.newPath
            } catch { store.say(error) }
        }
    }

    private func addPhoto(_ item: PhotosPickerItem?) {
        guard let item else { return }
        Task {
            defer { photo = nil }
            guard let data = try? await item.loadTransferable(type: Data.self) else { return }
            var ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
            if ext == "jpeg" { ext = "jpg" }
            let name = "Photo \(todayString()) \(nowTime().replacingOccurrences(of: ":", with: "")).\(ext)"
            do {
                let saved = try await store.run { try $0.saveAttachment(folder: "Attachments", name: name, bytes: data) }
                insert = "![[\(nameOf(saved))]]\n"
            } catch { store.say(error) }
        }
    }
}

/// "Linked from": notes that link here, at the end of a note.
struct LinkedFrom: View {
    let path: String
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var links: [Backlink] = []

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !links.isEmpty {
                Divider().overlay(Color.line).padding(.top, 36)
                Text(links.count == 1 ? String(localized: "Linked from 1 note") : String(localized: "Linked from \(links.count) notes"))
                    .font(.footnote.weight(.semibold)).foregroundStyle(Color.inkSoft).padding(.top, 8)
                ForEach(links, id: \.source) { b in
                    Button { router.note(b.source, line: Int(b.lines.first?.line ?? 0)) } label: {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(verbatim: b.title).font(.subheadline.weight(.semibold)).foregroundStyle(Color.accent)
                            ForEach(b.lines.prefix(2), id: \.line) { l in
                                Text(verbatim: l.text).font(.caption).foregroundStyle(Color.inkSoft).lineLimit(2)
                            }
                        }
                        .padding(14)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Color.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .task(id: "\(path)-\(store.tick)") {
            links = (try? await store.run { [p = path] m in try m.noteDetails(path: p).backlinks }) ?? []
        }
    }
}

/// About a note: tags, outline, what links here and what it links to.
struct NoteInfoSheet: View {
    let path: String
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @Environment(\.dismiss) private var dismiss
    @State private var d: NoteDetails?

    var body: some View {
        NavigationStack {
            List {
                if let d {
                    Section {
                        Text("\(Int(d.words)) words · changed \(ago(d.modifiedMs))").foregroundStyle(Color.inkSoft)
                        if !d.tags.isEmpty {
                            ScrollView(.horizontal, showsIndicators: false) {
                                HStack {
                                    ForEach(d.tags, id: \.self) { t in
                                        Button { dismiss(); router.push(.tag(t)) } label: {
                                            Text(verbatim: t).font(.subheadline).padding(.horizontal, 10).padding(.vertical, 5)
                                                .background(Color.tagBg, in: Capsule()).foregroundStyle(Color.tagText)
                                        }
                                        .buttonStyle(.plain)
                                    }
                                }
                            }
                        }
                    }
                    if !d.outline.isEmpty {
                        Section("Outline") {
                            ForEach(d.outline, id: \.line) { h in
                                Button { dismiss(); router.note(path, line: Int(h.line)) } label: {
                                    Text(verbatim: h.text).padding(.leading, CGFloat(Int(h.level) - 1) * 14)
                                        .font(h.level <= 2 ? .body.weight(.semibold) : .body)
                                }
                            }
                        }
                    }
                    Section {
                        if d.backlinks.isEmpty { Text("No other note links here yet.").foregroundStyle(Color.faint) }
                        ForEach(d.backlinks, id: \.source) { b in
                            Button { dismiss(); router.note(b.source, line: Int(b.lines.first?.line ?? 0)) } label: {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(verbatim: b.title).font(.headline)
                                    ForEach(b.lines.prefix(2), id: \.line) { l in Text(verbatim: l.text).font(.caption).foregroundStyle(Color.inkSoft).lineLimit(2) }
                                }
                            }
                        }
                    } header: { Text("Linked from (\(d.backlinks.count))") }
                    if !d.mentions.isEmpty {
                        Section("Mentioned without a link") {
                            ForEach(d.mentions, id: \.self) { m in
                                HStack {
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(verbatim: m.title).font(.headline)
                                        Text(verbatim: m.text).font(.caption).foregroundStyle(Color.inkSoft).lineLimit(2)
                                    }
                                    Spacer()
                                    Button("Link it") {
                                        Task {
                                            do { try await store.run { [p = path] mm in try mm.linkMention(path: p, source: m.source, line: m.line) } } catch { store.say(error) }
                                            await load()
                                        }
                                    }
                                    .buttonStyle(.bordered)
                                }
                            }
                        }
                    }
                    if !d.outgoing.isEmpty || !d.unresolved.isEmpty {
                        Section("Links to") {
                            ForEach(d.outgoing, id: \.self) { o in
                                Button { dismiss(); router.open(path: o) } label: { Label(nameOf(o), systemImage: "arrow.up.right") }
                            }
                            ForEach(d.unresolved, id: \.self) { u in Label(u, systemImage: "questionmark.circle").foregroundStyle(Color.faint) }
                        }
                    }
                } else {
                    ProgressView()
                }
            }
            .navigationTitle(Text(verbatim: d?.title ?? nameOf(path)))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
            .task { await load() }
        }
        .presentationDetents([.medium, .large])
    }

    private func load() async {
        d = try? await store.run { [p = path] m in try m.noteDetails(path: p) }
    }
}

/// Streams an AI answer into SwiftUI state.
@Observable
final class Answer: AiListener, @unchecked Sendable {
    var text = ""
    var sources: [Source] = []
    var done = false
    var error: String?
    var task: AiTask?

    func sources(sources: [Source]) { DispatchQueue.main.async { self.sources = sources } }
    func answer(text: String) { DispatchQueue.main.async { self.text = text } }
    func done(text: String) { DispatchQueue.main.async { self.text = text; self.done = true } }
    func failed(message: String) { DispatchQueue.main.async { self.error = message; self.done = true } }
}

/// AI actions on a note: summarize, find action items, translate…
struct NoteAiSheet: View {
    let path: String
    let text: String
    let onReplace: (String) -> Void
    let onAppend: (String) -> Void
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var chosen: NoteAction?
    @State private var answer: Answer?
    @State private var ask = ""

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    if let a = answer {
                        if !a.done { ProgressView().frame(maxWidth: .infinity) }
                        if let e = a.error { Text(verbatim: e).foregroundStyle(Color.danger) }
                        else { Blocks(blocks: renderMarkdown(text: a.text), actions: BlockActions()) }
                        if a.done && a.error == nil {
                            HStack {
                                if chosen?.replaces == true { Button("Replace the note") { onReplace(a.text); dismiss() }.buttonStyle(.borderedProminent) }
                                Button("Add to the note") { onAppend(a.text); dismiss() }.buttonStyle(.bordered)
                                Button("Copy") { UIPasteboard.general.string = a.text; store.say(String(localized: "Copied")) }
                            }
                        }
                    } else {
                        FlowLayout(spacing: 8) {
                            ForEach(store.marrow.noteActions(), id: \.id) { act in
                                Button(act.label) { run(act, "") }.buttonStyle(.bordered)
                            }
                        }
                        HStack {
                            TextField("Or tell the AI what to do…", text: $ask, axis: .vertical).textFieldStyle(.roundedBorder)
                            Button { run(nil, ask) } label: { Image(systemName: "arrow.up.circle.fill").font(.title2) }.disabled(ask.isEmpty)
                        }
                    }
                }
                .padding(20)
            }
            .navigationTitle(Text(verbatim: chosen?.label ?? String(localized: "Ask AI about this note")))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { answer?.task?.cancel(); dismiss() } } }
        }
        .presentationDetents([.medium, .large])
    }

    private func run(_ act: NoteAction?, _ instruction: String) {
        chosen = act
        let a = Answer()
        answer = a
        do {
            a.task = try store.marrow.runNoteAction(action: act?.id ?? "", instruction: instruction, path: path, text: text, providerId: nil, listener: a)
        } catch {
            a.error = words(error)
            a.done = true
        }
    }
}

/// Lays children out in rows, wrapping like words.
struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? 320
        var x: CGFloat = 0, y: CGFloat = 0, row: CGFloat = 0
        for s in subviews {
            let size = s.sizeThatFits(.unspecified)
            if x + size.width > width && x > 0 { x = 0; y += row + spacing; row = 0 }
            x += size.width + spacing
            row = max(row, size.height)
        }
        return CGSize(width: width, height: y + row)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, row: CGFloat = 0
        for s in subviews {
            let size = s.sizeThatFits(.unspecified)
            if x + size.width > bounds.maxX && x > bounds.minX { x = bounds.minX; y += row + spacing; row = 0 }
            s.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing
            row = max(row, size.height)
        }
    }
}

/// Picks a folder to move something into.
struct MoveSheet: View {
    let path: String
    let onPick: (String) -> Void
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var folders: [String] = []

    var body: some View {
        NavigationStack {
            List(folders, id: \.self) { f in
                Button { onPick(f); dismiss() } label: {
                    VStack(alignment: .leading) {
                        Label(f.isEmpty ? String(localized: "Top of the vault") : nameOf(f), systemImage: "folder")
                        if f.contains("/") { Text(verbatim: parentOf(f)).font(.caption).foregroundStyle(Color.faint) }
                    }
                }
            }
            .navigationTitle(Text("Move “\(nameOf(path))” to"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .task {
                let all = (try? await store.run { try $0.allFolders() }) ?? []
                folders = all.filter { $0 != path && !$0.hasPrefix(path + "/") && $0 != parentOf(path) }
            }
        }
        .presentationDetents([.medium, .large])
    }
}
