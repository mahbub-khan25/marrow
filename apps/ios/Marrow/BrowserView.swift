import SwiftUI

/// A folder's notes and folders ("" for the top of the vault).
struct BrowserView: View {
    let folder: String
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var items: [FileItem]?
    @State private var starredItems: [FileItem] = []
    @State private var templates: [Template] = []
    @State private var asking: String?
    @State private var name = ""
    @State private var renamingItem: FileItem?
    @State private var movingItem: FileItem?
    @State private var deletingItem: FileItem?
    @State private var lockingItem: FileItem?
    @State private var pickTemplate = false

    private var title: String { folder.isEmpty ? (store.vault?.name ?? String(localized: "Notes")) : nameOf(folder) }

    private var sorted: [FileItem] {
        let list = items ?? []
        if store.prefs.sort == "modified" {
            return list.sorted { a, b in
                if (a.kind == .folder) != (b.kind == .folder) { return a.kind == .folder }
                return a.modifiedMs > b.modifiedMs
            }
        }
        return list
    }

    var body: some View {
        let starredPaths = Set(starredItems.map(\.path))
        List {
            if folder.isEmpty {
                Section {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 10) {
                            chip("Calendar", "calendar") { router.push(.calendar) }
                            chip("Tags", "number") { router.tab = .search; router.searchText = "#" }
                            chip("Graph", "point.3.connected.trianglepath.dotted") { router.push(.graph()) }
                            chip("Time machine", "clock.arrow.circlepath") { router.push(.timeMachine) }
                        }
                        .padding(.horizontal, 20)
                    }
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(Color.clear)
                }
                if !starredItems.isEmpty {
                    Section {
                        ScrollView(.horizontal, showsIndicators: false) {
                            HStack(spacing: 10) {
                                ForEach(starredItems, id: \.path) { s in
                                    Button { router.open(s) } label: {
                                        VStack(alignment: .leading, spacing: 6) {
                                            Text(verbatim: s.name).font(.subheadline.weight(.semibold)).foregroundStyle(Color.ink).lineLimit(2)
                                            Text(verbatim: s.preview).font(.caption).foregroundStyle(Color.inkSoft).lineLimit(3)
                                            Spacer(minLength: 0)
                                        }
                                        .padding(14)
                                        .frame(width: 168, height: 112, alignment: .topLeading)
                                        .background(Color.card, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                                    }
                                    .buttonStyle(.plain)
                                }
                            }
                            .padding(.horizontal, 20)
                        }
                        .listRowInsets(EdgeInsets())
                        .listRowBackground(Color.clear)
                    } header: { SectionTitle(text: "Starred") }
                }
            }
            Section {
                if items?.isEmpty == true {
                    EmptyState(symbol: "folder", title: "Nothing here yet", text: "Tap New to write a note, or make a folder to keep notes together.")
                        .listRowBackground(Color.clear)
                }
                ForEach(sorted, id: \.path) { item in
                    Button { router.open(item) } label: { FileRow(item: item, starred: starredPaths.contains(item.path)) }
                        .buttonStyle(.plain)
                        .listRowBackground(Color.paper)
                        .contextMenu { menu(item, starred: starredPaths.contains(item.path)) }
                        .swipeActions(edge: .trailing) {
                            Button(role: .destructive) { deletingItem = item } label: { Label("Delete", systemImage: "trash") }
                            Button { movingItem = item } label: { Label("Move to…", systemImage: "folder") }.tint(.accent)
                        }
                }
            } header: {
                if folder.isEmpty { SectionTitle(text: "All notes") }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(Color.paper)
        .navigationTitle(Text(verbatim: title))
        .toolbar {
            ToolbarItemGroup(placement: .topBarTrailing) {
                Menu {
                    Picker("Sort", selection: Binding(get: { store.prefs.sort }, set: { store.prefs.sort = $0 })) {
                        Text("By name").tag("name")
                        Text("Last changed first").tag("modified")
                    }
                    if folder.isEmpty {
                        Divider()
                        Button { router.push(.settings) } label: { Label("Settings", systemImage: "gearshape") }
                    }
                } label: { Image(systemName: "arrow.up.arrow.down.circle") }
                .accessibilityLabel(Text("Sort"))
                Menu {
                    Button { name = ""; asking = "note" } label: { Label("New note", systemImage: "doc.badge.plus") }
                    if !templates.isEmpty { Button { pickTemplate = true } label: { Label("From a template", systemImage: "doc.on.doc") } }
                    Button { name = ""; asking = "canvas" } label: { Label("New canvas", systemImage: "rectangle.3.group") }
                    Button { name = ""; asking = "folder" } label: { Label("New folder", systemImage: "folder.badge.plus") }
                } label: { Image(systemName: "square.and.pencil") }
                .accessibilityLabel(Text("New"))
            }
        }
        .task(id: store.tick) { await load() }
        .refreshable { store.marrow.syncNow(force: false); await load() }
        .alert(asking == "folder" ? Text("New folder") : asking == "canvas" ? Text("New canvas") : Text("New note"), isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } })) {
            TextField("Untitled", text: $name)
            Button("Cancel", role: .cancel) { asking = nil }
            Button("Create") { create() }
        }
        .alert("Rename", isPresented: Binding(get: { renamingItem != nil }, set: { if !$0 { renamingItem = nil } })) {
            TextField("Name", text: $name)
            Button("Cancel", role: .cancel) { renamingItem = nil }
            Button("Rename") {
                if let it = renamingItem { act { try $0.rename(path: it.path, newName: name) } }
                renamingItem = nil
            }
        }
        .confirmationDialog(Text("Delete “\(deletingItem?.name ?? "")”?"), isPresented: Binding(get: { deletingItem != nil }, set: { if !$0 { deletingItem = nil } }), titleVisibility: .visible) {
            Button("Delete", role: .destructive) {
                if let it = deletingItem { act { try $0.trash(path: it.path) } }
                deletingItem = nil
            }
        } message: {
            Text(deletingItem?.kind == .folder ? "The folder and everything in it go to the vault's trash folder (.marrow/trash)." : "It goes to the vault's trash folder (.marrow/trash), so you can still get it back.")
        }
        .sheet(item: Binding(get: { movingItem.map(Wrapped.init) }, set: { movingItem = $0?.item })) { w in
            MoveSheet(path: w.item.path) { to in act { try $0.movePath(path: w.item.path, folder: to) } }
        }
        .sheet(item: Binding(get: { lockingItem.map(Wrapped.init) }, set: { lockingItem = $0?.item })) { w in
            LockSheet(folder: w.item.kind == .folder) { pw, hint in
                Task {
                    do {
                        if w.item.kind == .folder {
                            let n = try await store.run { try $0.lockFolder(path: w.item.path, password: pw, hint: hint) }
                            store.say(String(localized: "Locked \(Int(n)) notes"))
                        } else {
                            let warn = try await store.run { try $0.lockNote(path: w.item.path, password: pw, hint: hint) }
                            store.say(warn ?? String(localized: "Locked. Marrow forgets the password when you leave the app for a while."))
                        }
                    } catch { store.say(error) }
                }
            }
        }
        .sheet(isPresented: $pickTemplate) {
            NavigationStack {
                List(templates, id: \.path) { t in
                    Button { pickTemplate = false; fromTemplate(t) } label: { Label(t.name, systemImage: "doc.on.doc") }
                }
                .navigationTitle(Text("From a template"))
                .navigationBarTitleDisplayMode(.inline)
            }
            .presentationDetents([.medium])
        }
    }

    @ViewBuilder
    private func menu(_ item: FileItem, starred: Bool) -> some View {
        if item.kind != .folder {
            Button { act { try $0.setStarred(path: item.path, starred: !starred) } } label: { Label(starred ? "Remove star" : "Star", systemImage: starred ? "star.slash" : "star") }
        }
        Button { name = item.name; renamingItem = item } label: { Label("Rename", systemImage: "pencil.line") }
        Button { movingItem = item } label: { Label("Move to…", systemImage: "folder") }
        if item.kind == .folder || (item.kind == .note && !item.locked) {
            Button { lockingItem = item } label: { Label("Lock with a password", systemImage: "lock") }
        }
        Divider()
        Button(role: .destructive) { deletingItem = item } label: { Label("Delete", systemImage: "trash") }
    }

    private func chip(_ title: LocalizedStringKey, _ symbol: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: symbol)
                .font(.subheadline.weight(.medium))
                .padding(.horizontal, 14).padding(.vertical, 8)
                .background(Color.cardStrong, in: Capsule())
                .foregroundStyle(Color.ink)
        }
        .buttonStyle(.plain)
    }

    private func load() async {
        let f = folder
        items = try? await store.run { try $0.listFolder(folder: f) }
        if f.isEmpty { starredItems = (try? await store.run { try $0.starred() }) ?? [] }
        templates = (try? await store.run { try $0.templates() }) ?? []
    }

    /// Runs a change, showing what went wrong if it fails.
    private func act(_ body: @escaping (Marrow) throws -> Void) {
        Task {
            do { try await store.run(body) } catch { store.say(error) }
        }
    }

    private func create() {
        let kind = asking
        let n = name.trimmingCharacters(in: .whitespaces)
        let f = folder
        asking = nil
        Task {
            do {
                switch kind {
                case "folder":
                    _ = try await store.run { try $0.createFolder(parent: f, name: n.isEmpty ? String(localized: "New folder") : n) }
                case "canvas":
                    let p = try await store.run { try $0.createCanvas(folder: f, name: n.isEmpty ? String(localized: "Untitled") : n) }
                    router.push(.canvas(p))
                default:
                    let p = try await store.run { try $0.createNote(folder: f, name: n.isEmpty ? nil : n, text: "") }
                    router.note(p, edit: true)
                }
            } catch { store.say(error) }
        }
    }

    private func fromTemplate(_ t: Template) {
        let f = folder
        Task {
            do {
                let title = String(localized: "Untitled")
                let p = try await store.run { m -> String in
                    let text = try m.templateText(path: t.path, title: title, date: todayString(), time: nowTime())
                    return try m.createNote(folder: f, name: title, text: text)
                }
                router.note(p, edit: true)
            } catch { store.say(error) }
        }
    }
}

/// Makes a value usable with `.sheet(item:)`.
struct Wrapped: Identifiable {
    let item: FileItem
    var id: String { item.path }
}

/// Notes with a tag.
struct TagView: View {
    let tag: String
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var notes: [FileItem]?

    var body: some View {
        List {
            if notes?.isEmpty == true {
                EmptyState(symbol: "number", title: LocalizedStringKey(tag), text: "No notes carry this tag any more.").listRowBackground(Color.clear)
            }
            ForEach(notes ?? [], id: \.path) { n in
                Button { router.open(n) } label: { FileRow(item: n) }.buttonStyle(.plain).listRowBackground(Color.paper)
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(Color.paper)
        .navigationTitle(Text(verbatim: tag))
        .task(id: store.tick) { notes = try? await store.run { [tag] m in try m.notesWithTag(tag: tag) } }
    }
}

struct SearchView: View {
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var results: SearchResults?
    @State private var names: [FileHit] = []
    @State private var tags: [TagCount] = []

    var body: some View {
        @Bindable var router = router
        List {
            let q = router.searchText.trimmingCharacters(in: .whitespaces)
            if q.isEmpty || q == "#" {
                if !store.prefs.recentSearches.isEmpty && q.isEmpty {
                    Section {
                        ForEach(store.prefs.recentSearches, id: \.self) { r in
                            Button { router.searchText = r } label: { Label(r, systemImage: "clock") }.foregroundStyle(Color.ink)
                        }
                    } header: { SectionTitle(text: "Recent searches") }
                }
                if !tags.isEmpty {
                    Section {
                        FlowLayout(spacing: 8) {
                            ForEach(tags, id: \.tag) { t in
                                Button { router.push(.tag(t.tag)) } label: {
                                    Text(verbatim: "\(t.tag)  \(t.count)").font(.subheadline)
                                        .padding(.horizontal, 12).padding(.vertical, 6)
                                        .background(Color.tagBg, in: Capsule()).foregroundStyle(Color.tagText)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                        .listRowBackground(Color.clear)
                    } header: { SectionTitle(text: "Tags") }
                }
                if store.prefs.recentSearches.isEmpty && tags.isEmpty {
                    EmptyState(symbol: "magnifyingglass", title: "Search your notes", text: "Finds words in every note. Spelling variants in Bangla are found too.").listRowBackground(Color.clear)
                }
            } else {
                if !names.isEmpty {
                    Section {
                        ForEach(names, id: \.path) { h in
                            Button { remember(); router.open(path: h.path) } label: {
                                Label {
                                    VStack(alignment: .leading) {
                                        Text(verbatim: h.name).foregroundStyle(Color.ink)
                                        if !h.folder.isEmpty { Text(verbatim: h.folder).font(.caption).foregroundStyle(Color.faint) }
                                    }
                                } icon: { Image(systemName: "doc.text").foregroundStyle(Color.accent) }
                            }
                        }
                    } header: { SectionTitle(text: "Notes named like this") }
                }
                if let r = results {
                    Section {
                        ForEach(r.hits, id: \.path) { hit in
                            Button { remember(); router.note(hit.path, line: Int(hit.lines.first?.line ?? 0)) } label: {
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(verbatim: hit.title).font(.headline).foregroundStyle(Color.ink)
                                    ForEach(hit.lines.prefix(3), id: \.line) { l in Text(marked(l)).font(.subheadline).foregroundStyle(Color.inkSoft).lineLimit(2) }
                                }
                            }
                        }
                        if r.hits.isEmpty && names.isEmpty {
                            EmptyState(symbol: "magnifyingglass", title: "Nothing found", text: "Try fewer or different words.").listRowBackground(Color.clear)
                        }
                    } header: { SectionTitle(text: r.total == 1 ? "1 note" : "\(Int(r.total)) notes") }
                }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(Color.paper)
        .navigationTitle(Text("Search"))
        .searchable(text: $router.searchText, placement: .navigationBarDrawer(displayMode: .always), prompt: Text("Search your notes"))
        .onSubmit(of: .search) { remember() }
        .task(id: "\(router.searchText)-\(store.tick)") {
            let q = router.searchText.trimmingCharacters(in: .whitespaces)
            tags = (try? await store.run { try $0.tags() }) ?? []
            guard !q.isEmpty, q != "#" else { results = nil; names = []; return }
            try? await Task.sleep(nanoseconds: 150_000_000)
            names = (try? await store.run { try $0.findFiles(query: q, source: nil, limit: 5) }) ?? []
            results = try? await store.run { try $0.search(query: q, limit: 60) }
        }
    }

    private func remember() {
        let t = router.searchText.trimmingCharacters(in: .whitespaces)
        guard t.count >= 2 else { return }
        store.prefs.recentSearches = Array(([t] + store.prefs.recentSearches.filter { $0 != t }).prefix(8))
    }

    /// A matching line with the matches marked.
    private func marked(_ l: SearchLine) -> AttributedString {
        let ns = l.text as NSString
        let out = NSMutableAttributedString(string: l.text)
        var i = 0
        while i + 1 < l.ranges.count {
            let s = Int(l.ranges[i]), e = Int(l.ranges[i + 1])
            if s < e && e <= ns.length {
                out.addAttribute(.backgroundColor, value: UIColor(Color.mark), range: NSRange(location: s, length: e - s))
            }
            i += 2
        }
        return (try? AttributedString(out, including: \.uiKit)) ?? AttributedString(l.text)
    }
}

/// Every to-do in every note: overdue, today, coming up, and the rest.
struct TasksView: View {
    @Environment(Store.self) private var store
    @State private var tasks: [TaskItem]?
    @State private var showDone = false

    var body: some View {
        let day = todayString()
        let week = todayString(Calendar.current.date(byAdding: .day, value: 7, to: Date()) ?? Date())
        let all = tasks ?? []
        let open = all.filter { !$0.done }
        let groups: [(LocalizedStringKey, [TaskItem])] = [
            ("Overdue", open.filter { ($0.due ?? "9999") < day }.sorted { ($0.due ?? "") < ($1.due ?? "") }),
            ("Today", open.filter { $0.due == day }),
            ("Next 7 days", open.filter { if let d = $0.due { return d > day && d <= week } else { return false } }.sorted { ($0.due ?? "") < ($1.due ?? "") }),
            ("Later", open.filter { ($0.due ?? "") > week }.sorted { ($0.due ?? "") < ($1.due ?? "") }),
            ("No date", open.filter { $0.due == nil }),
            ("Done", all.filter(\.done)),
        ].filter { !$0.1.isEmpty }
        List {
            if tasks?.isEmpty == true {
                EmptyState(symbol: "checklist", title: "No tasks", text: "Write - [ ] in any note to add a task. Add a date like 📅 2026-11-02 to see it here when it's due.").listRowBackground(Color.clear)
            }
            ForEach(Array(groups.enumerated()), id: \.offset) { _, g in
                Section {
                    ForEach(g.1, id: \.self) { t in TaskRow(task: t, today: day).listRowBackground(Color.paper) }
                } header: {
                    HStack { SectionTitle(text: g.0); Text(verbatim: "\(g.1.count)").font(.footnote).foregroundStyle(Color.faint) }
                }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(Color.paper)
        .navigationTitle(Text("Tasks"))
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Toggle(isOn: $showDone) { Text("Show done") }.toggleStyle(.button)
            }
        }
        .task(id: "\(store.tick)-\(showDone)") { tasks = try? await store.run { [showDone] m in try m.tasks(includeDone: showDone) } }
        .refreshable { store.marrow.syncNow(force: false) }
    }
}
