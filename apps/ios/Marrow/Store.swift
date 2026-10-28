import Foundation
import Observation
import Security
import SwiftUI

/// The app's one link to Marrow's engine (the Rust core, through UniFFI).
/// Engine calls run off the main thread through `run`; what the engine
/// reports (saves, sync, other apps) bumps `tick`, so screens reload.
@Observable
final class Store {
    let marrow: Marrow
    let prefs = Prefs()
    /// The open vault, or nil before one is chosen.
    var vault: VaultInfo?
    var sync: SyncStatus?
    /// Goes up whenever files change; screens that show them reload.
    var tick = 0
    /// What changed last.
    var changed: [String] = []
    /// Goes up when a password is entered or forgotten.
    var locksTick = 0
    /// A short message to show at the bottom of the screen.
    var toast: String?
    var starting = true

    init(dataDir: URL? = nil, secrets: SecretStore = KeychainSecrets()) {
        let dir = dataDir ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("Marrow", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        marrow = Marrow(dataDir: dir.path, secrets: secrets)
        marrow.setListener(listener: Listener(store: self))
    }

    /// Runs an engine call off the main thread.
    func run<T>(_ body: @escaping (Marrow) throws -> T) async throws -> T {
        let m = marrow
        return try await Task.detached(priority: .userInitiated) { try body(m) }.value
    }

    /// Opens the vault used last time, if it's still there.
    @MainActor
    func openLast() async {
        defer { starting = false }
        guard let last = marrow.lastVault() else { return }
        _ = try? await open(last)
    }

    @MainActor
    @discardableResult
    func open(_ path: String) async throws -> VaultInfo {
        let info = try await run { try $0.openVault(path: path) }
        vault = info
        sync = marrow.syncStatus()
        tick += 1
        return info
    }

    @MainActor
    func close() {
        marrow.closeVault()
        vault = nil
        sync = nil
    }

    @MainActor
    func say(_ message: String) {
        toast = message
        let shown = message
        Task { @MainActor in
            try? await Task.sleep(nanoseconds: 3_000_000_000)
            if self.toast == shown { self.toast = nil }
        }
    }

    @MainActor
    func say(_ error: Error) {
        say(words(error))
    }

    /// Where a new vault goes: the app's Documents, which the Files app shows.
    static var documents: URL {
        FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
    }
}

/// Hears from the engine (on its threads) and passes it to the store.
private final class Listener: MarrowListener, @unchecked Sendable {
    weak var store: Store?
    init(store: Store) { self.store = store }

    func vaultChanged(paths: [String]) {
        DispatchQueue.main.async { [weak store] in
            store?.changed = paths
            store?.tick += 1
        }
    }

    func syncChanged(status: SyncStatus) {
        DispatchQueue.main.async { [weak store] in store?.sync = status }
        // Syncing with iCloud: have it download what it only keeps a stand-in for.
        if status.kind == .folder, !status.syncing, ICloud.isICloudFolder(status.location) {
            ICloud.downloadAll(in: URL(fileURLWithPath: status.location))
        }
    }

    func locksChanged() {
        DispatchQueue.main.async { [weak store] in store?.locksTick += 1 }
    }
}

/// Words for an error, to show the person.
func words(_ error: Error) -> String {
    if let e = error as? MarrowError {
        switch e {
        case .Failed(let reason): return reason
        case .Locked: return String(localized: "This note is locked.")
        case .LockedFolder(let folder): return String(localized: "The folder “\(folder)” is locked.")
        case .ChangedElsewhere: return String(localized: "This note was changed somewhere else.")
        case .NoVault: return "No vault is open."
        }
    }
    return error.localizedDescription
}

/// Sync passwords, the sync passphrase and AI keys, in the iOS Keychain
/// (this device only, readable after the first unlock).
final class KeychainSecrets: SecretStore, @unchecked Sendable {
    private let service = "app.marrow.secrets"

    func get(id: String) -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: id,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var out: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess, let data = out as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    func set(id: String, value: String?) {
        let base: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: id,
        ]
        SecItemDelete(base as CFDictionary)
        guard let value, let data = value.data(using: .utf8) else { return }
        var add = base
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        SecItemAdd(add as CFDictionary, nil)
    }
}

/// Secrets kept in memory only (screenshot runs).
final class MemorySecrets: SecretStore, @unchecked Sendable {
    private var map: [String: String] = [:]
    private let lock = NSLock()
    func get(id: String) -> String? { lock.lock(); defer { lock.unlock() }; return map[id] }
    func set(id: String, value: String?) { lock.lock(); defer { lock.unlock() }; map[id] = value }
}

/// How the app looks and behaves on this device (not part of the vault).
@Observable
final class Prefs {
    private let d = UserDefaults.standard

    /// "system", "light" or "dark".
    var theme: String { didSet { d.set(theme, forKey: "theme") } }
    /// "book" (Literata), "sans" or "classic" (EB Garamond).
    var readFont: String { didSet { d.set(readFont, forKey: "readFont") } }
    /// 0 small, 1 medium, 2 large, 3 larger.
    var textSize: Int { didSet { d.set(textSize, forKey: "textSize") } }
    /// Notes open for "read"ing or straight into "edit"ing.
    var openIn: String { didSet { d.set(openIn, forKey: "openIn") } }
    /// Forget passwords after this many minutes away (0 = when leaving).
    var lockAfter: Int { didSet { d.set(lockAfter, forKey: "lockAfter") } }
    /// "name" or "modified".
    var sort: String { didSet { d.set(sort, forKey: "sort") } }
    var recentSearches: [String] { didSet { d.set(recentSearches, forKey: "recentSearches") } }
    var recentQuestions: [String] { didSet { d.set(recentQuestions, forKey: "recentQuestions") } }
    /// What to show once the vault opens ("sync" after "Get my notes from a server").
    var afterOpen: String? { didSet { d.set(afterOpen, forKey: "afterOpen") } }

    init() {
        theme = d.string(forKey: "theme") ?? "system"
        readFont = d.string(forKey: "readFont") ?? "book"
        textSize = d.object(forKey: "textSize") as? Int ?? 1
        openIn = d.string(forKey: "openIn") ?? "read"
        lockAfter = d.object(forKey: "lockAfter") as? Int ?? 5
        sort = d.string(forKey: "sort") ?? "name"
        recentSearches = d.stringArray(forKey: "recentSearches") ?? []
        recentQuestions = d.stringArray(forKey: "recentQuestions") ?? []
        afterOpen = d.string(forKey: "afterOpen")
    }

    var colorScheme: ColorScheme? {
        switch theme {
        case "light": return .light
        case "dark": return .dark
        default: return nil
        }
    }

    var scale: CGFloat {
        switch textSize {
        case 0: return 0.9
        case 2: return 1.12
        case 3: return 1.25
        default: return 1
        }
    }
}
