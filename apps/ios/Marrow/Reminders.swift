import SwiftUI
import UserNotifications

/// Task reminders (`- [ ] Call Sam ⏰ 2026-11-03 09:00`). The notes are the
/// source of truth: after the vault opens and whenever notes change (here or
/// by sync), the reminders still ahead are handed to iOS as local
/// notifications, so they arrive even when Marrow is closed. iOS keeps at
/// most 64 waiting, so the soonest 60 are scheduled.
enum Reminders {
    static let category = "marrow.reminder"
    static let doneAction = "marrow.reminder.done"
    static let snoozeAction = "marrow.reminder.snooze"
    static let prefix = "reminder."
    static let snoozeMinutes = 10
    private static let most = 60

    static let format: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd HH:mm"
        return f
    }()

    static func date(_ at: String) -> Date? { format.date(from: at) }
    static func stamp(_ d: Date) -> String { format.string(from: d) }

    /// "Today, 09:00" / "Tomorrow, 18:00" / "Tue 3 Nov, 09:00".
    static func label(_ at: String) -> String {
        guard let d = date(at) else { return at }
        let time = d.formatted(date: .omitted, time: .shortened)
        let cal = Calendar.current
        if cal.isDateInToday(d) { return String(localized: "Today") + ", " + time }
        if cal.isDateInTomorrow(d) { return String(localized: "Tomorrow") + ", " + time }
        let sameYear = cal.component(.year, from: d) == cal.component(.year, from: Date())
        let day = sameYear ? d.formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated)) : d.formatted(date: .abbreviated, time: .omitted)
        return day + ", " + time
    }

    /// The Done and Snooze buttons on a reminder.
    static func registerCategory() {
        let done = UNNotificationAction(identifier: doneAction, title: String(localized: "Done"), options: [])
        let snooze = UNNotificationAction(identifier: snoozeAction, title: String(localized: "Snooze 10 min"), options: [])
        let c = UNNotificationCategory(identifier: category, actions: [done, snooze], intentIdentifiers: [], options: [])
        UNUserNotificationCenter.current().setNotificationCategories([c])
    }

    /// Asks once whether Marrow may show notifications.
    static func askPermission() {
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { _, _ in }
    }

    /// Reads the reminders from the open vault and schedules them, replacing
    /// the ones scheduled before.
    static func refresh(_ marrow: Marrow) async {
        guard let list = try? marrow.reminders() else { return }
        let center = UNUserNotificationCenter.current()
        let now = Date()
        let wanted = list.filter { (date($0.at) ?? .distantPast) > now }.prefix(most)
        let keep = Set(wanted.map { prefix + $0.id })
        let pending = await center.pendingNotificationRequests().map(\.identifier).filter { $0.hasPrefix(prefix) }
        let gone = pending.filter { !keep.contains($0) }
        if !gone.isEmpty { center.removePendingNotificationRequests(withIdentifiers: gone) }
        let have = Set(pending)
        for r in wanted where !have.contains(prefix + r.id) {
            guard let when = date(r.at) else { continue }
            let content = UNMutableNotificationContent()
            content.title = r.text
            content.body = r.title
            content.sound = .default
            content.categoryIdentifier = category
            content.userInfo = ["path": r.path, "line": Int(r.line), "text": r.text]
            let parts = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute], from: when)
            let trigger = UNCalendarNotificationTrigger(dateMatching: parts, repeats: false)
            try? await center.add(UNNotificationRequest(identifier: prefix + r.id, content: content, trigger: trigger))
        }
    }
}

/// A reminder that was tapped: the app opens its note.
enum ReminderTap {
    static let notice = Notification.Name("marrow.reminderTap")
    static var pending: (path: String, line: Int)? {
        didSet { if pending != nil { NotificationCenter.default.post(name: notice, object: nil) } }
    }
}

/// Shows reminders while Marrow is open, and handles Done, Snooze and taps.
final class ReminderCenter: NSObject, UNUserNotificationCenterDelegate {
    static let shared = ReminderCenter()
    /// Set by the app once its store exists.
    var marrow: Marrow?

    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .list, .sound]
    }

    func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let info = response.notification.request.content.userInfo
        guard let path = info["path"] as? String, let line = info["line"] as? Int, let text = info["text"] as? String else { return }
        switch response.actionIdentifier {
        case Reminders.doneAction, Reminders.snoozeAction:
            guard let m = marrow else { return }
            if m.vaultInfo() == nil, let last = m.lastVault() { _ = try? m.openVault(path: last) }
            if response.actionIdentifier == Reminders.doneAction {
                try? m.completeTask(path: path, line: UInt32(line), text: text)
            } else {
                let next = Reminders.stamp(Date().addingTimeInterval(Double(Reminders.snoozeMinutes * 60)))
                _ = try? m.setReminder(path: path, line: UInt32(line), text: text, at: next)
            }
            await Reminders.refresh(m)
        default:
            await MainActor.run { ReminderTap.pending = (path, line) }
        }
    }
}

/// Pick when to be reminded: soon, this evening, tomorrow morning, or any
/// date and time; or remove the reminder.
struct RemindSheet: View {
    let current: String?
    let onPick: (String?) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var custom = Date()
    @State private var picking = false

    var body: some View {
        let now = Date()
        let cal = Calendar.current
        let inAnHour = now.addingTimeInterval(3600)
        let evening = cal.date(bySettingHour: 18, minute: 0, second: 0, of: now)!
        let tomorrow = cal.date(bySettingHour: 9, minute: 0, second: 0, of: cal.date(byAdding: .day, value: 1, to: now)!)!
        NavigationStack {
            List {
                if let current {
                    Text(verbatim: Reminders.label(current)).foregroundStyle(Color.accent)
                }
                Section {
                    option("clock", "In an hour", inAnHour)
                    if evening > now.addingTimeInterval(15 * 60) { option("sunset", "This evening", evening) }
                    option("sun.max", "Tomorrow morning", tomorrow)
                    if picking {
                        DatePicker(selection: $custom, in: now..., displayedComponents: [.date, .hourAndMinute]) {
                            Text("Pick a date and time…")
                        }
                        Button("Remind me") { pick(Reminders.stamp(custom)) }.bold()
                    } else {
                        Button { picking = true } label: { Label("Pick a date and time…", systemImage: "calendar") }
                    }
                }
                if current != nil {
                    Section {
                        Button(role: .destructive) { pick(nil) } label: { Label("Remove the reminder", systemImage: "bell.slash") }
                    }
                }
            }
            .navigationTitle("Remind me")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
        }
        .presentationDetents([.medium, .large])
        .onAppear {
            custom = current.flatMap(Reminders.date) ?? tomorrow
            Reminders.askPermission()
        }
    }

    private func option(_ symbol: String, _ title: LocalizedStringKey, _ at: Date) -> some View {
        Button { pick(Reminders.stamp(at)) } label: {
            HStack {
                Label(title, systemImage: symbol)
                Spacer()
                Text(verbatim: Reminders.label(Reminders.stamp(at))).font(.subheadline).foregroundStyle(Color.faint)
            }
        }
        .foregroundStyle(Color.ink)
    }

    private func pick(_ at: String?) {
        // Whole minutes: the note keeps `HH:MM`.
        onPick(at)
        dismiss()
    }
}

/// On a task that has a reminder: the bell and when.
struct ReminderChip: View {
    let at: String
    var body: some View {
        Label { Text(verbatim: Reminders.label(at)) } icon: { Image(systemName: "alarm") }
            .font(.caption)
            .foregroundStyle(Color.accent)
            .labelStyle(.titleAndIcon)
    }
}
