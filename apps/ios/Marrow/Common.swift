import SwiftUI

/// The places in the app, pushed onto a tab's navigation stack.
enum Route: Hashable {
    case note(String, line: Int = -1, edit: Bool = false)
    case folder(String)
    case tag(String)
    case canvas(String)
    case file(String)
    case calendar
    case graph(around: String = "")
    case history(String)
    case timeMachine
    case settings
    case sync
    case ai
    case appearance
    case vaultSettings
    case importNotes
    case conflicts
}

enum AppTab: Hashable {
    case today, notes, ask, tasks, search
}

/// Each tab's navigation stack, and moving between screens.
@Observable
final class Router {
    var tab: AppTab = .today
    var paths: [AppTab: [Route]] = [.today: [], .notes: [], .ask: [], .tasks: [], .search: []]
    /// A search to start (from a tag chip, say).
    var searchText = ""
    var capture = false
    var captureText = ""

    func push(_ r: Route) {
        paths[tab, default: []].append(r)
    }

    func binding(_ t: AppTab) -> Binding<[Route]> {
        Binding(get: { self.paths[t] ?? [] }, set: { self.paths[t] = $0 })
    }

    func note(_ path: String, line: Int = -1, edit: Bool = false) { push(.note(path, line: line, edit: edit)) }

    /// Opens a note, canvas or other file by its path.
    func open(path: String, line: Int = -1) {
        let lower = path.lowercased()
        if lower.hasSuffix(".md") { push(.note(path, line: line)) }
        else if lower.hasSuffix(".canvas") { push(.canvas(path)) }
        else { push(.file(path)) }
    }

    func open(_ item: FileItem) {
        switch item.kind {
        case .folder: push(.folder(item.path))
        case .note: push(.note(item.path))
        case .canvas: push(.canvas(item.path))
        default: push(.file(item.path))
        }
    }
}

// MARK: - Dates and names

/// Today in the person's time zone, as YYYY-MM-DD.
func todayString(_ date: Date = Date()) -> String {
    let f = DateFormatter()
    f.calendar = Calendar(identifier: .gregorian)
    f.locale = Locale(identifier: "en_US_POSIX")
    f.dateFormat = "yyyy-MM-dd"
    return f.string(from: date)
}

func nowTime() -> String {
    let f = DateFormatter()
    f.locale = Locale(identifier: "en_US_POSIX")
    f.dateFormat = "HH:mm"
    return f.string(from: Date())
}

func parseDay(_ s: String) -> Date? {
    let f = DateFormatter()
    f.calendar = Calendar(identifier: .gregorian)
    f.locale = Locale(identifier: "en_US_POSIX")
    f.dateFormat = "yyyy-MM-dd"
    return f.date(from: s)
}

/// "Monday, 26 October" (with the year when it isn't this year).
func longDate(_ day: String) -> String {
    guard let d = parseDay(day) else { return day }
    let thisYear = Calendar.current.component(.year, from: Date()) == Calendar.current.component(.year, from: d)
    return d.formatted(thisYear ? .dateTime.weekday(.wide).day().month(.wide) : .dateTime.weekday(.wide).day().month(.wide).year())
}

func mediumDate(_ day: String) -> String {
    guard let d = parseDay(day) else { return day }
    return d.formatted(date: .abbreviated, time: .omitted)
}

/// "Just now", "5 min. ago", "yesterday".
func ago(_ ms: Int64) -> String {
    guard ms > 0 else { return "" }
    let d = Date(timeIntervalSince1970: TimeInterval(ms) / 1000)
    if Date().timeIntervalSince(d) < 60 { return String(localized: "Just now") }
    return d.formatted(.relative(presentation: .named, unitsStyle: .abbreviated))
}

func stamp(_ ms: Int64) -> String {
    Date(timeIntervalSince1970: TimeInterval(ms) / 1000).formatted(date: .abbreviated, time: .shortened)
}

/// "Note" for "Projects/Note.md"; "Map" for "Map.canvas".
func nameOf(_ path: String) -> String {
    let n = path.split(separator: "/").last.map(String.init) ?? path
    if n.lowercased().hasSuffix(".md") { return String(n.dropLast(3)) }
    if n.lowercased().hasSuffix(".canvas") { return String(n.dropLast(7)) }
    return n
}

/// "Projects/Work" for "Projects/Work/Note.md".
func parentOf(_ path: String) -> String {
    guard let i = path.lastIndex(of: "/") else { return "" }
    return String(path[..<i])
}

func symbol(_ kind: FileKind, locked: Bool = false) -> String {
    if locked { return "lock" }
    switch kind {
    case .folder: return "folder"
    case .note: return "doc.text"
    case .canvas: return "rectangle.3.group"
    case .image: return "photo"
    case .pdf: return "doc.richtext"
    case .audio: return "waveform"
    case .other: return "doc"
    }
}

func isDay(_ s: String) -> Bool { parseDay(s) != nil && s.count == 10 }

// MARK: - Views used everywhere

/// A row for a note, folder or file: icon, name, preview and when it changed.
struct FileRow: View {
    let item: FileItem
    var starred = false
    var subtitle: String? = nil

    var body: some View {
        HStack(alignment: item.preview.isEmpty ? .center : .top, spacing: 14) {
            Image(systemName: symbol(item.kind, locked: item.locked))
                .font(.system(size: 17, weight: .regular))
                .foregroundStyle(item.kind == .folder ? Color.onAccentSoft : Color.inkSoft)
                .frame(width: 36, height: 36)
                .background(item.kind == .folder ? Color.accentSoft : Color.cardStrong, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(verbatim: item.name).font(.headline).foregroundStyle(Color.ink).lineLimit(1)
                    if starred { Image(systemName: "star.fill").font(.caption).foregroundStyle(.yellow) }
                }
                if !item.preview.isEmpty {
                    Text(verbatim: item.preview).font(.subheadline).foregroundStyle(Color.inkSoft).lineLimit(2)
                }
                let meta = subtitle ?? (item.kind == .folder ? itemsLabel(Int(item.items)) : item.locked ? String(localized: "Locked") : ago(item.modifiedMs))
                if !meta.isEmpty {
                    Text(verbatim: meta).font(.caption).foregroundStyle(Color.faint)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 4)
        .contentShape(Rectangle())
    }
}

func itemsLabel(_ n: Int) -> String {
    n == 1 ? String(localized: "1 item") : String(localized: "\(n) items")
}

/// A friendly message where a list is empty.
struct EmptyState<Action: View>: View {
    let symbol: String
    let title: LocalizedStringKey
    let text: LocalizedStringKey
    @ViewBuilder var action: () -> Action

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.system(size: 28))
                .foregroundStyle(Color.inkSoft)
                .frame(width: 64, height: 64)
                .background(Color.cardStrong, in: Circle())
            Text(title).font(.headline).multilineTextAlignment(.center)
            Text(text).font(.subheadline).foregroundStyle(Color.inkSoft).multilineTextAlignment(.center).frame(maxWidth: 380)
            action()
        }
        .padding(.horizontal, 32)
        .padding(.vertical, 40)
        .frame(maxWidth: .infinity)
    }
}

extension EmptyState where Action == EmptyView {
    init(symbol: String, title: LocalizedStringKey, text: LocalizedStringKey) {
        self.init(symbol: symbol, title: title, text: text) { EmptyView() }
    }
}

/// A small heading over a group of rows.
struct SectionTitle: View {
    let text: LocalizedStringKey
    var body: some View {
        Text(text)
            .font(.footnote.weight(.semibold))
            .foregroundStyle(Color.inkSoft)
            .textCase(nil)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Shows the store's short messages at the bottom of the screen.
struct ToastOverlay: ViewModifier {
    @Environment(Store.self) private var store
    func body(content: Content) -> some View {
        content.overlay(alignment: .bottom) {
            if let t = store.toast {
                Text(verbatim: t)
                    .font(.subheadline)
                    .foregroundStyle(Color.paper)
                    .padding(.horizontal, 18)
                    .padding(.vertical, 12)
                    .background(Color.ink.opacity(0.92), in: Capsule())
                    .padding(.bottom, 90)
                    .padding(.horizontal, 24)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                    .onTapGesture { store.toast = nil }
            }
        }
        .animation(.spring(duration: 0.3), value: store.toast)
    }
}

/// Asks for a password (to unlock a note or folder).
struct PasswordSheet: View {
    let title: String
    let hint: String?
    var error: String?
    let onDone: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var password = ""
    @FocusState private var focused: Bool

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    SecureField("Password", text: $password).focused($focused).onSubmit { if !password.isEmpty { onDone(password) } }
                } footer: {
                    if let error { Text(verbatim: error).foregroundStyle(Color.danger) }
                    else if let hint, !hint.isEmpty { Text("Hint: \(hint)") }
                }
            }
            .navigationTitle(Text(verbatim: title))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("Unlock") { onDone(password) }.disabled(password.isEmpty) }
            }
            .onAppear { focused = true }
        }
        .presentationDetents([.medium])
    }
}

/// Sets a new password (and hint) to lock a note or folder.
struct LockSheet: View {
    var folder = false
    let onDone: (String, String?) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var password = ""
    @State private var again = ""
    @State private var hint = ""

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    SecureField("Password", text: $password)
                    SecureField("Password again", text: $again)
                    TextField("Hint (optional)", text: $hint)
                } footer: {
                    Text("The note is encrypted on disk and in sync. Without the password it can't be read, not even by you, so keep it somewhere safe.")
                }
            }
            .navigationTitle(folder ? Text("Lock this folder") : Text("Lock this note"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Lock") {
                        onDone(password, hint.isEmpty ? nil : hint)
                        dismiss()
                    }
                    .disabled(password.count < 6 || password != again)
                }
            }
        }
        .presentationDetents([.medium, .large])
    }
}

/// A column of text no wider than is comfortable to read.
struct ReadingWidth: ViewModifier {
    func body(content: Content) -> some View {
        content.frame(maxWidth: 720).frame(maxWidth: .infinity)
    }
}

extension View {
    func readingWidth() -> some View { modifier(ReadingWidth()) }
    func toasts() -> some View { modifier(ToastOverlay()) }
}
