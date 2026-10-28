import AuthenticationServices
import SwiftUI

// Sync with iCloud (iPhone, iPad and Mac), or Google Drive, OneDrive and
// Dropbox (every device): sign in, pick the vault's folder, start.

extension CloudKind {
    var symbol: String {
        switch self {
        case .googleDrive: "externaldrive.badge.icloud"
        case .oneDrive: "cloud"
        case .dropbox: "shippingbox"
        }
    }
}

/// Marrow's own folder in iCloud Drive (shown there as "Marrow").
enum ICloud {
    /// The Documents folder in Marrow's iCloud container, or nil when iCloud
    /// Drive is off. Slow the first time: call it off the main thread.
    static func documents() -> URL? {
        guard let c = FileManager.default.url(forUbiquityContainerIdentifier: nil) else { return nil }
        let docs = c.appendingPathComponent("Documents", isDirectory: true)
        try? FileManager.default.createDirectory(at: docs, withIntermediateDirectories: true)
        return docs
    }

    /// The vault folders already there.
    static func folders(in docs: URL) -> [String] {
        let items = (try? FileManager.default.contentsOfDirectory(at: docs, includingPropertiesForKeys: [.isDirectoryKey], options: [.skipsHiddenFiles])) ?? []
        return items.filter { (try? $0.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true }.map(\.lastPathComponent).sorted()
    }

    /// Asks iCloud to download every file it has only a stand-in for, so
    /// sync can read them.
    static func downloadAll(in folder: URL) {
        let fm = FileManager.default
        guard let e = fm.enumerator(at: folder, includingPropertiesForKeys: nil) else { return }
        for case let url as URL in e where url.lastPathComponent.hasPrefix(".") && url.pathExtension == "icloud" {
            var name = url.deletingPathExtension().lastPathComponent
            name.removeFirst()
            try? fm.startDownloadingUbiquitousItem(at: url.deletingLastPathComponent().appendingPathComponent(name))
        }
    }

    static func isICloudFolder(_ path: String) -> Bool {
        path.contains("/Mobile Documents/") || path.contains("/Library/Mobile Documents")
    }
}

/// What the person picks the folder from, after signing in.
struct CloudChoice: Identifiable {
    let id = UUID()
    /// nil for iCloud.
    let service: CloudService?
    let account: String
    let folders: [String]
    let suggested: String
    /// iCloud only: its Documents folder.
    var icloud: URL?
}

/// The rows at the top of the sync screen.
struct CloudSection: View {
    @Environment(Store.self) private var store
    @Environment(\.webAuthenticationSession) private var webAuth
    @State private var busy: String?
    @State private var error: String?
    @State private var choice: CloudChoice?
    private var services: [CloudService] { store.marrow.cloudServices().filter(\.available) }

    var body: some View {
        Section {
            row(symbol: "icloud", title: String(localized: "iCloud"), text: String(localized: "Your iPhone, iPad and Mac, with your iCloud storage"), id: "icloud") { iCloud() }
            ForEach(services, id: \.name) { s in
                row(symbol: s.kind.symbol, title: String(localized: "Sign in to \(s.name)"), text: String(localized: "Notes go in \(s.place)"), id: s.name) { signIn(s) }
            }
            if let error { Text(verbatim: error).foregroundStyle(Color.danger).font(.callout) }
        }
        .sheet(item: $choice) { c in CloudSetupSheet(choice: c) }
    }

    private func row(symbol: String, title: String, text: String, id: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 14) {
                Image(systemName: symbol).font(.title2).foregroundStyle(Color.accent).frame(width: 32)
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: title).foregroundStyle(Color.ink)
                    Text(verbatim: text).font(.caption).foregroundStyle(Color.inkSoft)
                }
                Spacer()
                if busy == id { ProgressView() } else { Image(systemName: "chevron.right").font(.footnote.weight(.semibold)).foregroundStyle(Color.faint) }
            }
        }
        .disabled(busy != nil)
    }

    private func suggested() -> String {
        store.vault?.name ?? "Notes"
    }

    private func iCloud() {
        busy = "icloud"
        error = nil
        let name = suggested()
        Task {
            let docs = await Task.detached { ICloud.documents() }.value
            busy = nil
            guard let docs else {
                error = String(localized: "Sign in to iCloud in the Settings app and turn on iCloud Drive, then come back.")
                return
            }
            choice = CloudChoice(service: nil, account: "iCloud", folders: ICloud.folders(in: docs), suggested: name, icloud: docs)
        }
    }

    private func signIn(_ s: CloudService) {
        busy = s.name
        error = nil
        Task {
            defer { busy = nil }
            do {
                let start = try store.marrow.cloudSignInStart(kind: s.kind)
                guard let url = URL(string: start.url) else { return }
                let back = try await webAuth.authenticate(using: url, callbackURLScheme: start.callbackScheme, preferredBrowserSession: .ephemeral)
                let acc = try await store.run { try $0.cloudSignInFinish(callbackUrl: back.absoluteString) }
                choice = CloudChoice(service: s, account: acc.account, folders: acc.folders, suggested: acc.suggested)
            } catch let e as ASWebAuthenticationSessionError where e.code == .canceledLogin {
                // Closed by the person: nothing to say.
            } catch {
                self.error = words(error)
            }
        }
    }
}

/// After signing in: which folder, encryption, and start.
struct CloudSetupSheet: View {
    let choice: CloudChoice
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @State private var folder = ""
    @State private var isNew = false
    @State private var newName = ""
    @State private var place: SyncPlace?
    @State private var encrypt = false
    @State private var pass = ""
    @State private var pass2 = ""
    @State private var busy = false
    @State private var error: String?

    private var name: String { (isNew ? newName : folder).trimmingCharacters(in: .whitespaces) }
    private var joining: Bool { place?.encrypted == true }
    private var canEncrypt: Bool { place == nil || joining || place?.files == 0 }
    private var passOk: Bool { !(encrypt || joining) || (pass.count >= 8 && (joining || pass == pass2)) }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Label {
                        VStack(alignment: .leading) {
                            Text(verbatim: choice.service?.name ?? "iCloud").font(.headline)
                            if choice.service != nil { Text("Signed in as \(choice.account)").font(.caption).foregroundStyle(Color.inkSoft) }
                        }
                    } icon: { Image(systemName: choice.service?.kind.symbol ?? "icloud").foregroundStyle(Color.accent) }
                } footer: {
                    if let s = choice.service {
                        Text("Your notes go in \(s.place), as plain Markdown files you can open there too. Marrow can't see anything else in your \(s.name).")
                    } else {
                        Text("Your notes go in iCloud Drive › Marrow › \(name). On a Mac, open that folder in Marrow, or sync with it.")
                    }
                }
                Section("Vault folder") {
                    Picker(selection: Binding(get: { isNew ? "" : folder }, set: { v in
                        isNew = v.isEmpty
                        if !v.isEmpty { folder = v }
                    })) {
                        ForEach(choice.folders, id: \.self) { Text(verbatim: $0).tag($0) }
                        Text("A new folder for this vault").tag("")
                    } label: { EmptyView() }
                        .pickerStyle(.inline)
                        .labelsHidden()
                    if isNew { TextField("Folder name", text: $newName).autocorrectionDisabled() }
                }
                if canEncrypt {
                    Section {
                        Toggle(isOn: Binding(get: { encrypt || joining }, set: { encrypt = $0 })) {
                            VStack(alignment: .leading) {
                                Text("Encrypt end to end")
                                Text("Notes are sealed on this phone with a passphrase; the server can't read them.").font(.caption).foregroundStyle(Color.inkSoft)
                            }
                        }
                        .disabled(joining)
                        if encrypt || joining {
                            SecureField("Sync passphrase", text: $pass)
                            if !joining { SecureField("Passphrase again", text: $pass2) }
                        }
                    } footer: {
                        if encrypt || joining {
                            Text(joining ? "This folder is already encrypted: enter the passphrase you chose for it." : "At least 8 characters. Use the same one on every device. Without it the notes there can't be read.")
                        }
                    }
                }
                Section {
                    if let p = place {
                        Text(p.files == 0 ? String(localized: "A new, empty folder: everything in this vault will be copied there.") : String(localized: "Signed in. The folder has \(Int(p.files)) files: they'll be merged with this vault.")).foregroundStyle(Color.accent)
                    }
                    if let error { Text(verbatim: error).foregroundStyle(Color.danger) }
                }
            }
            .navigationTitle(Text("Sync"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    if busy { ProgressView() } else {
                        Button("Start syncing") { start() }.disabled(name.isEmpty || !passOk)
                    }
                }
            }
            .task(id: name) { await check() }
            .onAppear {
                newName = choice.suggested
                if choice.folders.contains(choice.suggested) { folder = choice.suggested } else { isNew = true }
            }
        }
    }

    private func check() async {
        place = nil
        error = nil
        let n = name
        guard !n.isEmpty else { return }
        try? await Task.sleep(nanoseconds: 400_000_000)
        guard n == name else { return }
        do {
            if let docs = choice.icloud {
                // A folder in iCloud: how many files, and whether Marrow encrypted it.
                let dir = docs.appendingPathComponent(n, isDirectory: true)
                let (files, sealed) = await Task.detached { () -> (Int, Bool) in
                    let fm = FileManager.default
                    let list = (try? fm.subpathsOfDirectory(atPath: dir.path)) ?? []
                    return (list.filter { !$0.split(separator: "/").contains { $0.hasPrefix(".") } }.count, fm.fileExists(atPath: dir.appendingPathComponent(".marrow-encryption.json").path) || fm.fileExists(atPath: dir.appendingPathComponent("..marrow-encryption.json.icloud").path))
                }.value
                place = SyncPlace(files: UInt32(files), encrypted: sealed)
            } else if let s = choice.service {
                let (k, a) = (s.kind, choice.account)
                place = try await store.run { try $0.checkCloud(kind: k, account: a, folder: n) }
            }
            if place?.encrypted == true { encrypt = true } else if (place?.files ?? 0) > 0 { encrypt = false }
        } catch {
            self.error = words(error)
        }
    }

    private func start() {
        busy = true
        error = nil
        let n = name
        let phrase = (encrypt || joining) ? pass : nil
        Task {
            do {
                if let docs = choice.icloud {
                    let path = docs.appendingPathComponent(n, isDirectory: true).path
                    try await store.run { try $0.syncWithFolder(path: path, passphrase: phrase) }
                } else if let s = choice.service {
                    let (k, a) = (s.kind, choice.account)
                    try await store.run { try $0.syncWithCloud(kind: k, account: a, folder: n, passphrase: phrase) }
                }
                store.sync = store.marrow.syncStatus()
                store.say(String(localized: "Syncing."))
                dismiss()
            } catch {
                self.error = words(error)
            }
            busy = false
        }
    }
}
