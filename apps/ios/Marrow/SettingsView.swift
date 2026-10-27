import AVFoundation
import Speech
import SwiftUI
import UniformTypeIdentifiers

/// Folders picked in the Files app (iCloud Drive, a USB drive…) stay
/// reachable across launches through security-scoped bookmarks.
enum Bookmarks {
    private static let key = "folderBookmarks"

    static func keep(_ url: URL) {
        guard let data = try? url.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil) else { return }
        var all = UserDefaults.standard.dictionary(forKey: key) as? [String: Data] ?? [:]
        all[url.path] = data
        UserDefaults.standard.set(all, forKey: key)
    }

    /// Opens access to every kept folder again (call once at launch).
    static func restore() {
        let all = UserDefaults.standard.dictionary(forKey: key) as? [String: Data] ?? [:]
        for data in all.values {
            var stale = false
            if let url = try? URL(resolvingBookmarkData: data, options: [], relativeTo: nil, bookmarkDataIsStale: &stale) {
                _ = url.startAccessingSecurityScopedResource()
                if stale { keep(url) }
            }
        }
    }
}

struct SettingsView: View {
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router

    var body: some View {
        @Bindable var prefs = store.prefs
        List {
            Section {
                row("Vault", store.vault?.name ?? "", "archivebox") { router.push(.vaultSettings) }
                row("Sync", store.sync.map { $0.label + ($0.encrypted ? " · " + String(localized: "encrypted") : "") } ?? String(localized: "Off: notes stay on this phone"), "icloud") { router.push(.sync) }
                row("Import notes", String(localized: "From Notion, Evernote, Word and more"), "square.and.arrow.down") { router.push(.importNotes) }
                row("Conflicting copies", String(localized: "Merge copies sync apps made"), "arrow.triangle.merge") { router.push(.conflicts) }
            } header: { SectionTitle(text: "Vault") }
            Section {
                row("Appearance", String(localized: "Theme, note text, text size"), "paintpalette") { router.push(.appearance) }
                Button {
                    if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
                } label: {
                    Label { VStack(alignment: .leading) { Text("Language").foregroundStyle(Color.ink); Text(Locale.current.localizedString(forLanguageCode: Locale.current.language.languageCode?.identifier ?? "en") ?? "").font(.caption).foregroundStyle(Color.inkSoft) } } icon: { Image(systemName: "globe") }
                }
                let ai = store.marrow.aiConfig()
                row("AI", ai.providers.isEmpty ? String(localized: "Off") : ai.providers.map(\.name).joined(separator: ", "), "sparkles") { router.push(.ai) }
            } header: { SectionTitle(text: "App") }
            Section {
                Picker(selection: $prefs.lockAfter) {
                    Text("When I leave the app").tag(0)
                    Text("After \(1) minutes away").tag(1)
                    Text("After \(5) minutes away").tag(5)
                    Text("After \(15) minutes away").tag(15)
                    Text("After an hour away").tag(60)
                } label: { Label("Forget passwords", systemImage: "timer") }
                Button {
                    store.marrow.lockAll()
                    store.say(String(localized: "Every locked note is locked again."))
                } label: { Label("Lock all now", systemImage: "lock") }
            } header: { SectionTitle(text: "Locked notes") }
            Section {
                Label { VStack(alignment: .leading) { Text(verbatim: "Marrow \(store.marrow.version())"); Text("Plain Markdown notes that stay yours. Free and open source (MIT).").font(.caption).foregroundStyle(Color.inkSoft) } } icon: { Image(systemName: "info.circle") }
                Link(destination: URL(string: "https://github.com/mahbub-khan25/marrow")!) { Label("Website and source code", systemImage: "safari") }
            } header: { SectionTitle(text: "About") }
        }
        .navigationTitle(Text("Settings"))
    }

    private func row(_ title: LocalizedStringKey, _ detail: String, _ symbol: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).foregroundStyle(Color.ink)
                    if !detail.isEmpty { Text(verbatim: detail).font(.caption).foregroundStyle(Color.inkSoft).lineLimit(2) }
                }
            } icon: { Image(systemName: symbol) }
        }
    }
}

struct SyncView: View {
    @Environment(Store.self) private var store
    @State private var url = ""
    @State private var user = ""
    @State private var password = ""
    @State private var encrypt = true
    @State private var pass = ""
    @State private var pass2 = ""
    @State private var place: SyncPlace?
    @State private var busy = false
    @State private var error: String?
    @State private var stopping = false
    @State private var pickingFolder = false

    var body: some View {
        Form {
            if let s = store.sync {
                Section {
                    HStack(spacing: 12) {
                        Image(systemName: s.syncing ? "arrow.triangle.2.circlepath.icloud" : (s.problem != nil && s.problem != .offline) ? "exclamationmark.icloud" : "checkmark.icloud")
                            .font(.largeTitle).foregroundStyle((s.problem != nil && s.problem != .offline) ? Color.danger : Color.accent)
                        VStack(alignment: .leading) {
                            Text(verbatim: s.label).font(.headline)
                            Text(verbatim: s.location).font(.caption).foregroundStyle(Color.inkSoft).lineLimit(2)
                        }
                    }
                    if s.encrypted { Label("End-to-end encrypted", systemImage: "lock.fill").foregroundStyle(Color.accent) }
                    Text(s.syncing ? String(localized: "Syncing…") : s.lastSyncMs.map { String(localized: "Last synced \(ago($0))") } ?? String(localized: "Not synced yet"))
                    if let m = s.message { Text(verbatim: m).foregroundStyle(s.problem == .offline ? Color.inkSoft : Color.danger).font(.callout) }
                    Button("Sync now") { store.marrow.syncNow(force: false) }.disabled(s.syncing)
                    if s.problem == .needsConfirming { Button("Sync anyway") { store.marrow.syncNow(force: true) } }
                } footer: {
                    Text("Marrow syncs soon after each change, every 15 seconds while it's open, and when you come back to it. Notes changed in two places at once are merged.")
                }
                Section { Button("Turn off sync", role: .destructive) { stopping = true } }
            } else {
                Section {
                    Text("Keep this vault the same on your phone, tablet and computer. Marrow syncs with a WebDAV server you choose: Nextcloud, ownCloud, Synology, pCloud, Koofr and many more. Turn on encryption and the server only ever sees sealed files.")
                        .font(.callout).foregroundStyle(Color.inkSoft)
                }
                Section {
                    TextField("Server address", text: $url).keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
                    TextField("User name", text: $user).textInputAutocapitalization(.never).autocorrectionDisabled()
                    SecureField("Password", text: $password)
                } footer: {
                    Text("The folder to sync with. In Nextcloud: Files → Settings → WebDAV, then add a folder name.")
                }
                Section {
                    Toggle(isOn: Binding(get: { encrypt || place?.encrypted == true }, set: { encrypt = $0 })) {
                        VStack(alignment: .leading) {
                            Text("Encrypt end to end")
                            Text("Notes are sealed on this phone with a passphrase; the server can't read them.").font(.caption).foregroundStyle(Color.inkSoft)
                        }
                    }
                    .disabled(place?.encrypted == true)
                    if encrypt || place?.encrypted == true {
                        SecureField("Sync passphrase", text: $pass)
                        if place?.encrypted != true { SecureField("Passphrase again", text: $pass2) }
                    }
                } footer: {
                    if encrypt || place?.encrypted == true {
                        Text(place?.encrypted == true ? "This folder is already encrypted: enter the passphrase you chose for it." : "At least 8 characters. Use the same one on every device. Without it the notes there can't be read.")
                    }
                }
                Section {
                    if let p = place {
                        Text(p.files == 0 ? String(localized: "Signed in. The folder is empty: this vault will be copied there.") : String(localized: "Signed in. The folder has \(Int(p.files)) files: they'll be merged with this vault.")).foregroundStyle(Color.accent)
                    }
                    if let error { Text(verbatim: error).foregroundStyle(Color.danger) }
                    HStack {
                        Button("Check") { check() }.disabled(busy || url.isEmpty)
                        Spacer()
                        if busy { ProgressView() }
                        Button("Start syncing") { start() }.buttonStyle(.borderedProminent).disabled(busy || url.isEmpty || !passOk)
                    }
                }
                Section {
                    Button { pickingFolder = true } label: { Label("Sync with a folder in Files (iCloud Drive…)", systemImage: "folder") }
                } footer: {
                    Text("Marrow on a Mac can use the same folder.")
                }
            }
        }
        .navigationTitle(Text("Sync"))
        .confirmationDialog(Text("Turn off sync?"), isPresented: $stopping, titleVisibility: .visible) {
            Button("Turn off", role: .destructive) {
                Task {
                    do { try await store.run { try $0.turnOffSync() } } catch { store.say(error) }
                    store.sync = nil
                }
            }
        } message: { Text("Nothing is deleted, here or on the server. Changes simply stop moving between them.") }
        .fileImporter(isPresented: $pickingFolder, allowedContentTypes: [.folder]) { result in
            guard case .success(let folder) = result else { return }
            _ = folder.startAccessingSecurityScopedResource()
            Bookmarks.keep(folder)
            let p = encrypt && pass.count >= 8 ? pass : nil
            Task {
                do {
                    try await store.run { try $0.syncWithFolder(path: folder.path, passphrase: p) }
                    store.sync = store.marrow.syncStatus()
                } catch { self.error = words(error) }
            }
        }
    }

    private var passOk: Bool {
        !(encrypt || place?.encrypted == true) || (pass.count >= 8 && (pass == pass2 || place?.encrypted == true))
    }

    private func check() {
        busy = true
        error = nil
        let (u, n, p) = (url.trimmingCharacters(in: .whitespaces), user.trimmingCharacters(in: .whitespaces), password)
        Task {
            do {
                place = try await store.run { try $0.checkWebdav(url: u, user: n, password: p) }
                if place?.encrypted == true { encrypt = true }
            } catch { self.error = words(error) }
            busy = false
        }
    }

    private func start() {
        busy = true
        error = nil
        let (u, n, p) = (url.trimmingCharacters(in: .whitespaces), user.trimmingCharacters(in: .whitespaces), password)
        let phrase = (encrypt || place?.encrypted == true) ? pass : nil
        Task {
            do {
                try await store.run { try $0.syncWithWebdav(url: u, user: n, password: p, passphrase: phrase) }
                store.sync = store.marrow.syncStatus()
                store.say(String(localized: "Syncing."))
            } catch { self.error = words(error) }
            busy = false
        }
    }
}

struct AiSettingsView: View {
    @Environment(Store.self) private var store
    @State private var config: AiConfig?
    @State private var picking = false
    @State private var editing: AiProvider?

    var body: some View {
        let presets = store.marrow.aiPresets()
        List {
            Section {
                Text("Marrow works with the AI you choose. Your key stays on this phone, in its secure storage. Nothing is sent until you ask something.").font(.callout).foregroundStyle(Color.inkSoft)
            }
            if let c = config {
                if c.providers.isEmpty {
                    EmptyState(symbol: "sparkles", title: "No AI set up", text: "Add one to ask your notes questions, summarize, translate and more.")
                } else {
                    Section {
                        ForEach(c.providers, id: \.id) { p in
                            HStack {
                                Button {
                                    Task { config = try? await store.run { try $0.aiSetDefault(id: p.id) } }
                                } label: {
                                    Image(systemName: c.defaultId == p.id ? "largecircle.fill.circle" : "circle").foregroundStyle(Color.accent)
                                }
                                .buttonStyle(.plain)
                                Button { editing = p } label: {
                                    VStack(alignment: .leading) {
                                        Text(verbatim: p.name).foregroundStyle(Color.ink)
                                        Text(verbatim: p.model.isEmpty ? String(localized: "No model chosen") : p.model).font(.caption).foregroundStyle(Color.inkSoft)
                                    }
                                }
                            }
                            .swipeActions { Button(role: .destructive) { Task { config = try? await store.run { try $0.aiRemoveProvider(id: p.id) } } } label: { Label("Remove", systemImage: "trash") } }
                        }
                    } header: { SectionTitle(text: "Your AIs (the marked one is used)") }
                }
            }
            Section { Button { picking = true } label: { Label("Add an AI", systemImage: "plus") } }
        }
        .navigationTitle(Text("AI"))
        .task { config = store.marrow.aiConfig() }
        .sheet(isPresented: $picking) {
            NavigationStack {
                List {
                    ForEach(Array(Dictionary(grouping: presets, by: \.group).sorted { a, b in (presets.firstIndex { $0.group == a.key } ?? 0) < (presets.firstIndex { $0.group == b.key } ?? 0) }), id: \.key) { group in
                        Section(group.key) {
                            ForEach(group.value, id: \.id) { pr in
                                Button(pr.name) {
                                    picking = false
                                    editing = AiProvider(id: "", preset: pr.id, name: pr.id == "custom" ? "" : pr.name, `protocol`: pr.`protocol`, baseUrl: pr.baseUrl, model: "", hasKey: false)
                                }
                            }
                        }
                    }
                }
                .navigationTitle(Text("Add an AI"))
                .navigationBarTitleDisplayMode(.inline)
                .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { picking = false } } }
            }
        }
        .sheet(item: Binding(get: { editing.map(ProviderBox.init) }, set: { editing = $0?.provider })) { box in
            ProviderForm(start: box.provider, preset: presets.first { $0.id == box.provider.preset }) { saved, key in
                Task {
                    do {
                        config = try await store.run { try $0.aiSaveProvider(provider: saved, key: key) }
                        editing = nil
                    } catch { store.say(error) }
                }
            }
        }
    }
}

private struct ProviderBox: Identifiable {
    let provider: AiProvider
    var id: String { provider.id + provider.preset }
}

private struct ProviderForm: View {
    let start: AiProvider
    let preset: AiPreset?
    let onSave: (AiProvider, String?) -> Void
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var base = ""
    @State private var key = ""
    @State private var model = ""
    @State private var models: [String] = []
    @State private var busy = false
    @State private var result: String?

    private var current: AiProvider {
        var p = start
        p.name = name.trimmingCharacters(in: .whitespaces)
        p.baseUrl = base.trimmingCharacters(in: .whitespaces)
        p.model = model.trimmingCharacters(in: .whitespaces)
        return p
    }

    var body: some View {
        NavigationStack {
            Form {
                if let hint = preset?.hint, !hint.isEmpty { Section { Text(verbatim: hint).font(.callout).foregroundStyle(Color.inkSoft) } }
                Section {
                    TextField("Name", text: $name)
                    TextField("Address", text: $base).keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
                    if preset?.needsKey != false {
                        SecureField(start.hasKey ? String(localized: "Saved (leave empty to keep it)") : String(localized: "API key"), text: $key)
                        if let u = preset?.keyUrl, !u.isEmpty, let url = URL(string: u) { Link("Get a key", destination: url) }
                    }
                }
                Section {
                    TextField("Model", text: $model).textInputAutocapitalization(.never).autocorrectionDisabled()
                    if !models.isEmpty {
                        Picker("Model", selection: $model) { ForEach(models, id: \.self) { Text(verbatim: $0).tag($0) } }
                    }
                    Button("List") { list() }.disabled(busy || base.isEmpty)
                    Button("Test") { test() }.disabled(busy || model.isEmpty)
                    if busy { ProgressView() }
                    if let result { Text(verbatim: result).font(.callout) }
                }
            }
            .navigationTitle(Text(verbatim: preset?.name ?? start.name))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("Save") { onSave(current, key.isEmpty ? nil : key) }.disabled(name.isEmpty || base.isEmpty) }
            }
            .onAppear {
                name = start.name
                base = start.baseUrl
                model = start.model
            }
        }
    }

    private func list() {
        busy = true
        let p = current, k = key.isEmpty ? nil : key
        Task {
            do { models = try await store.run { try $0.aiModels(provider: p, key: k) } } catch { result = words(error) }
            busy = false
        }
    }

    private func test() {
        busy = true
        result = nil
        let p = current, k = key.isEmpty ? nil : key
        Task {
            do {
                let said = try await store.run { try $0.aiTest(provider: p, key: k) }
                result = String(localized: "It answered: \(said)")
            } catch { result = words(error) }
            busy = false
        }
    }
}

struct AppearanceView: View {
    @Environment(Store.self) private var store
    @Environment(\.reading) private var reading

    var body: some View {
        @Bindable var prefs = store.prefs
        Form {
            Picker("Theme", selection: $prefs.theme) {
                Text("Same as the phone").tag("system")
                Text("Light").tag("light")
                Text("Dark").tag("dark")
            }
            Picker("Note text", selection: $prefs.readFont) {
                Text("Book").tag("book")
                Text("Clean").tag("sans")
                Text("Classic").tag("classic")
            }
            Picker("Text size", selection: $prefs.textSize) {
                Text("Small").tag(0)
                Text("Medium").tag(1)
                Text("Large").tag(2)
                Text("Larger").tag(3)
            }
            Picker("Open notes for", selection: $prefs.openIn) {
                Text("Reading").tag("read")
                Text("Editing").tag("edit")
            }
            Section {
                VStack(alignment: .leading, spacing: 8) {
                    Text("A quiet place to think").font(reading.h2)
                    Text("Notes are plain text files you own. Write freely; Marrow keeps everything safe as you type.").font(reading.body)
                    Text(verbatim: "আমার সোনার বাংলা, আমি তোমায় ভালোবাসি।").font(reading.body)
                }
                .padding(.vertical, 6)
            } header: { SectionTitle(text: "Preview") }
        }
        .navigationTitle(Text("Appearance"))
    }
}

struct VaultSettingsView: View {
    @Environment(Store.self) private var store
    @State private var settings: VaultSettings?
    @State private var switching = false

    var body: some View {
        Form {
            Section {
                Label { VStack(alignment: .leading) { Text(verbatim: store.vault?.name ?? ""); Text(verbatim: store.vault?.root ?? "").font(.caption).foregroundStyle(Color.inkSoft) } } icon: { Image(systemName: "archivebox") }
            } footer: {
                Text("A vault is the folder your notes live in, as plain Markdown files. The same vault works in Marrow on a computer, and in Obsidian.")
            }
            if let s = settings {
                Section {
                    TextField("Daily notes folder", text: Binding(get: { s.dailyFolder }, set: { v in update { $0.dailyFolder = v } }))
                    Picker("Daily notes are kept", selection: Binding(get: { s.dailyLayout }, set: { v in update { $0.dailyLayout = v } })) {
                        Text("In year and month folders").tag("year-month")
                        Text("All in one folder").tag("flat")
                    }
                } header: { SectionTitle(text: "Daily notes") }
                Section {
                    TextField("Templates folder", text: Binding(get: { s.templateFolder }, set: { v in update { $0.templateFolder = v } }))
                } header: { SectionTitle(text: "Templates") }
            }
            Section {
                Button("Open another vault") { switching = true }
            } header: { SectionTitle(text: "Other vaults") } footer: {
                Text("This vault stays as it is on this phone; you can come back to it any time.")
            }
        }
        .navigationTitle(Text("Vault"))
        .task { settings = try? await store.run { try $0.vaultSettings() } }
        .confirmationDialog(Text("Open another vault"), isPresented: $switching, titleVisibility: .visible) {
            Button("Continue") { store.close() }
        }
    }

    private func update(_ f: (inout VaultSettings) -> Void) {
        guard var s = settings else { return }
        f(&s)
        settings = s
        let next = s
        Task { do { try await store.run { try $0.setVaultSettings(settings: next) } } catch { store.say(error) } }
    }
}

/// Brings in notes exported from Notion, Evernote, Word, web pages or Markdown files.
struct ImportView: View {
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var kind = "notion"
    @State private var picking = false
    @State private var busy = false
    @State private var report: ImportReport?

    var body: some View {
        List {
            Section { Text("Bring notes in from another app. They go into a new folder, as plain Markdown, with their pictures and attachments.").font(.callout).foregroundStyle(Color.inkSoft) }
            source("notion", "Notion", "In Notion: Settings → Export all workspace content, as Markdown & CSV. Pick the .zip it gives you.")
            source("evernote", "Evernote", "In Evernote: export notebooks as .enex files, then pick them here.")
            source("word", "Word and OneNote", "Pick .docx files (OneNote can save pages as Word documents).")
            source("markdown", "Markdown, text and web pages", "Pick .md, .txt or .html files.")
            if busy { HStack { ProgressView(); Text("Importing…") } }
            if let r = report {
                Section {
                    Text(r.notes == 1 ? String(localized: "1 note imported into “\(r.folder)”") : String(localized: "\(Int(r.notes)) notes imported into “\(r.folder)”")).font(.headline)
                    ForEach(r.warnings.prefix(5), id: \.self) { Text(verbatim: "• \($0)").font(.caption) }
                    Button("Open folder") { router.push(.folder(r.folder)) }
                }
            }
        }
        .navigationTitle(Text("Import notes"))
        .fileImporter(isPresented: $picking, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            guard case .success(let urls) = result, !urls.isEmpty else { return }
            busy = true
            let k = kind
            Task {
                // The engine reads files by path: copy them in first.
                let dir = FileManager.default.temporaryDirectory.appendingPathComponent("import-\(UUID().uuidString)")
                try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
                var paths: [String] = []
                for u in urls {
                    let ok = u.startAccessingSecurityScopedResource()
                    let dest = dir.appendingPathComponent(u.lastPathComponent)
                    try? FileManager.default.copyItem(at: u, to: dest)
                    if ok { u.stopAccessingSecurityScopedResource() }
                    paths.append(dest.path)
                }
                let folder = k == "notion" ? "Notion" : k == "evernote" ? "Evernote" : k == "word" ? "Word" : "Imported"
                do { report = try await store.run { try $0.importFiles(kind: k, paths: paths, folder: folder) } } catch { store.say(error) }
                try? FileManager.default.removeItem(at: dir)
                busy = false
            }
        }
    }

    private func source(_ k: String, _ title: LocalizedStringKey, _ how: LocalizedStringKey) -> some View {
        Section {
            VStack(alignment: .leading, spacing: 6) {
                Text(title).font(.headline)
                Text(how).font(.callout).foregroundStyle(Color.inkSoft)
                Button("Choose files") { kind = k; picking = true }.buttonStyle(.bordered).disabled(busy)
            }
            .padding(.vertical, 4)
        }
    }
}

/// Copies of notes that sync apps made when both sides changed.
struct ConflictsView: View {
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var list: [Conflict]?

    var body: some View {
        List {
            Section { Text("When a note changes on two devices before they sync, a sync app may keep both as a copy. Merge puts the copy's changes into the note and moves the copy to the trash.").font(.callout).foregroundStyle(Color.inkSoft) }
            if list?.isEmpty == true { EmptyState(symbol: "arrow.triangle.merge", title: "No conflicting copies", text: "Everything is in step.") }
            ForEach(list ?? [], id: \.copy) { c in
                VStack(alignment: .leading, spacing: 6) {
                    Text(verbatim: nameOf(c.original)).font(.headline)
                    Text("Copy: \(nameOf(c.copy))").font(.caption).foregroundStyle(Color.inkSoft)
                    HStack {
                        Button("Merge") {
                            Task {
                                do {
                                    let n = try await store.run { try $0.resolveConflict(copy: c.copy, original: c.original) }
                                    store.say(n == 0 ? String(localized: "Merged.") : String(localized: "Merged. Where both changed the same lines, both were kept."))
                                } catch { store.say(error) }
                            }
                        }
                        .buttonStyle(.borderedProminent)
                        Button("See the copy") { router.note(c.copy) }.buttonStyle(.bordered)
                    }
                }
            }
        }
        .navigationTitle(Text("Conflicting copies"))
        .task(id: store.tick) { list = try? await store.run { try $0.conflicts() } }
    }
}

/// The first screen: start a vault here, open a folder, or sync one down.
struct WelcomeView: View {
    @Environment(Store.self) private var store
    @State private var error: String?
    @State private var picking = false

    var body: some View {
        ScrollView {
            VStack(spacing: 14) {
                Image("Logo")
                    .resizable().frame(width: 120, height: 120)
                    .clipShape(RoundedRectangle(cornerRadius: 28, style: .continuous))
                    .padding(.top, 40)
                Text("Marrow").font(.display(40))
                Text("Notes that stay yours: plain Markdown files, linked like your thoughts.")
                    .font(.custom("Literata", size: 17, relativeTo: .body)).foregroundStyle(Color.inkSoft).multilineTextAlignment(.center)
                    .padding(.bottom, 18)
                option("square.and.pencil", "Start writing", "A new vault on this phone, with a short welcome", primary: true) { start(sync: false) }
                option("icloud", "Get my notes from a server", "Sync with Nextcloud or another WebDAV server") { start(sync: true) }
                option("folder", "Open a folder", "From Files: iCloud Drive, or a folder on this iPhone") { picking = true }
                if let error { Text(verbatim: error).foregroundStyle(Color.danger).multilineTextAlignment(.center) }
                Text("No account. No tracking. Your notes never leave your devices unless you set up sync.")
                    .font(.footnote).foregroundStyle(Color.faint).multilineTextAlignment(.center).padding(.top, 12)
            }
            .padding(24)
            .frame(maxWidth: 520)
            .frame(maxWidth: .infinity)
        }
        .background(Color.paper)
        .fileImporter(isPresented: $picking, allowedContentTypes: [.folder]) { result in
            guard case .success(let url) = result else { return }
            _ = url.startAccessingSecurityScopedResource()
            Bookmarks.keep(url)
            Task { do { try await store.open(url.path) } catch { self.error = words(error) } }
        }
    }

    private func option(_ symbol: String, _ title: LocalizedStringKey, _ text: LocalizedStringKey, primary: Bool = false, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 16) {
                Image(systemName: symbol).font(.title3)
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.headline)
                    Text(text).font(.caption).opacity(0.8)
                }
                Spacer()
                Image(systemName: "chevron.right")
            }
            .foregroundStyle(primary ? Color.white : Color.ink)
            .padding(18)
            .background(primary ? Color.accent : Color.cardStrong, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
        }
        .buttonStyle(.plain)
    }

    private func start(sync: Bool) {
        Task {
            do {
                let docs = Store.documents
                let existing = docs.appendingPathComponent("Marrow")
                let has = (try? FileManager.default.contentsOfDirectory(atPath: existing.path))?.isEmpty == false
                let path = has ? existing.path : try await store.run { try $0.createVault(parent: docs.path, name: "Marrow", welcome: !sync) }
                if sync { store.prefs.afterOpen = "sync" }
                try await store.open(path)
            } catch { self.error = words(error) }
        }
    }
}

/// Turns speech into text with Apple's recogniser (on the device when it can).
struct DictationSheet: View {
    let onDone: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var model = Dictation()

    var body: some View {
        NavigationStack {
            VStack(spacing: 20) {
                Image(systemName: model.listening ? "waveform" : "mic")
                    .font(.system(size: 44)).foregroundStyle(Color.accent)
                    .symbolEffect(.variableColor.iterative, isActive: model.listening)
                Text(verbatim: model.text.isEmpty ? (model.error ?? "…") : model.text)
                    .font(.title3).multilineTextAlignment(.center).foregroundStyle(model.error == nil ? Color.ink : Color.danger)
                Spacer()
            }
            .padding(24)
            .navigationTitle(Text("Dictate"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { model.stop(); dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        model.stop()
                        if !model.text.isEmpty { onDone(model.text) }
                        dismiss()
                    }
                }
            }
            .task { await model.start() }
            .onDisappear { model.stop() }
        }
        .presentationDetents([.medium])
    }
}

@Observable
final class Dictation {
    var text = ""
    var listening = false
    var error: String?
    private let engine = AVAudioEngine()
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?

    @MainActor
    func start() async {
        let auth = await withCheckedContinuation { c in SFSpeechRecognizer.requestAuthorization { c.resume(returning: $0) } }
        guard auth == .authorized, let recognizer = SFSpeechRecognizer(), recognizer.isAvailable else {
            error = String(localized: "This phone has no voice typing.")
            return
        }
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement, options: .duckOthers)
            try session.setActive(true, options: .notifyOthersOnDeactivation)
            let req = SFSpeechAudioBufferRecognitionRequest()
            req.shouldReportPartialResults = true
            if recognizer.supportsOnDeviceRecognition { req.requiresOnDeviceRecognition = true }
            request = req
            let input = engine.inputNode
            input.installTap(onBus: 0, bufferSize: 1024, format: input.outputFormat(forBus: 0)) { buf, _ in req.append(buf) }
            engine.prepare()
            try engine.start()
            listening = true
            task = recognizer.recognitionTask(with: req) { [weak self] result, err in
                DispatchQueue.main.async {
                    if let result { self?.text = result.bestTranscription.formattedString }
                    if err != nil || result?.isFinal == true { self?.stop() }
                }
            }
        } catch {
            self.error = error.localizedDescription
        }
    }

    func stop() {
        guard listening else { return }
        engine.stop()
        engine.inputNode.removeTap(onBus: 0)
        request?.endAudio()
        task?.cancel()
        listening = false
    }
}
