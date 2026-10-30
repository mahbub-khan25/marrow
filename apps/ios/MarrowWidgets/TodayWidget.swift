import SwiftUI
import UIKit
import WidgetKit

/// Today's tasks on the home screen: what's due today and overdue. Ticking
/// one, or tapping it, opens Marrow, which marks it done or shows its note.
@main
struct MarrowWidgets: WidgetBundle {
    var body: some Widget { TodayWidget() }
}

struct TodayWidget: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "today", provider: Provider()) { entry in
            TodayWidgetView(entry: entry)
                .containerBackground(for: .widget) { paper }
        }
        .configurationDisplayName("Today's tasks")
        .description("What's due today and overdue, with a button to add a task.")
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge])
    }
}

struct Entry: TimelineEntry {
    let date: Date
    let snapshot: TodaySnapshot?
}

struct Provider: TimelineProvider {
    func placeholder(in context: Context) -> Entry { Entry(date: Date(), snapshot: .sample) }

    func getSnapshot(in context: Context, completion: @escaping (Entry) -> Void) {
        completion(Entry(date: Date(), snapshot: context.isPreview ? (TodaySnapshot.read() ?? .sample) : TodaySnapshot.read()))
    }

    /// The app reloads the widget when notes change; besides that it only
    /// needs to move on to the next day at midnight.
    func getTimeline(in context: Context, completion: @escaping (Timeline<Entry>) -> Void) {
        let now = Date()
        let midnight = Calendar.current.startOfDay(for: now.addingTimeInterval(86_400))
        let s = TodaySnapshot.read()
        completion(Timeline(entries: [Entry(date: now, snapshot: s), Entry(date: midnight, snapshot: s)], policy: .after(midnight)))
    }
}

extension TodaySnapshot {
    static var sample: TodaySnapshot {
        let today = dayString(Date())
        return TodaySnapshot(items: [
            Item(path: "", line: 0, text: "Order the seed potatoes", due: today),
            Item(path: "", line: 1, text: "Call Sam about the garden", due: today),
            Item(path: "", line: 2, text: "Water the seedlings", due: today),
        ])
    }
}

private func tone(_ light: UInt32, _ dark: UInt32) -> Color {
    func c(_ v: UInt32) -> UIColor {
        UIColor(red: CGFloat((v >> 16) & 0xFF) / 255, green: CGFloat((v >> 8) & 0xFF) / 255, blue: CGFloat(v & 0xFF) / 255, alpha: 1)
    }
    return Color(UIColor { $0.userInterfaceStyle == .dark ? c(dark) : c(light) })
}

private let paper = tone(0xFBFAF7, 0x1C1B19)
private let accent = tone(0x0A7EA4, 0x4FB6DC)

struct TodayWidgetView: View {
    let entry: Entry
    @Environment(\.widgetFamily) private var family

    var body: some View {
        let day = dayString(entry.date)
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 0) {
                    Text("Today").font(.headline)
                    Text(entry.date, format: .dateTime.weekday(.wide).day().month(.wide))
                        .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                }
                Spacer(minLength: 4)
                if family != .systemSmall {
                    Link(destination: TodaySnapshot.link("capture")) {
                        Image(systemName: "plus")
                            .font(.system(size: 17, weight: .semibold))
                            .foregroundStyle(.white)
                            .frame(width: 34, height: 34)
                            .background(accent, in: Circle())
                    }
                    .accessibilityLabel(Text("Add a task"))
                }
            }
            if let s = entry.snapshot {
                let due = s.due(on: day)
                if due.isEmpty {
                    Spacer(minLength: 0)
                    Text("Nothing due today.").font(.subheadline).foregroundStyle(.secondary)
                    Spacer(minLength: 0)
                } else {
                    let fit = family == .systemLarge ? 7 : (family == .systemMedium ? 3 : 3)
                    let shown = due.count > fit ? Array(due.prefix(fit - 1)) : due
                    ForEach(shown, id: \.self) { t in row(t, day: day) }
                    if shown.count < due.count {
                        Text("+\(due.count - shown.count) more").font(.caption).foregroundStyle(accent)
                            .padding(.leading, family == .systemSmall ? 0 : 30)
                    }
                    Spacer(minLength: 0)
                }
            } else {
                Spacer(minLength: 0)
                Text("Open Marrow to choose your notes.").font(.subheadline).foregroundStyle(.secondary)
                Spacer(minLength: 0)
            }
        }
        .widgetURL(TodaySnapshot.link("today"))
    }

    @ViewBuilder
    private func row(_ t: TodaySnapshot.Item, day: String) -> some View {
        if family == .systemSmall {
            // Small widgets open with one link: show the words only.
            Text(verbatim: "• " + plainTask(t.text)).font(.caption).lineLimit(1)
        } else {
            HStack(alignment: .firstTextBaseline, spacing: 10) {
                Link(destination: TodaySnapshot.link("done", ["path": t.path, "line": String(t.line), "text": t.text])) {
                    Image(systemName: "circle").font(.body).foregroundStyle(.secondary)
                }
                .accessibilityLabel(Text("Done"))
                Link(destination: TodaySnapshot.link("note", ["path": t.path, "line": String(t.line)])) {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(verbatim: plainTask(t.text)).font(.subheadline).lineLimit(1)
                        if let at = t.remind, let when = time(at) {
                            Label { Text(verbatim: when) } icon: { Image(systemName: "alarm") }
                                .font(.caption2).foregroundStyle(accent)
                        } else if t.due < day, let d = date(t.due) {
                            Text(d, format: .dateTime.day().month(.abbreviated)).font(.caption2).foregroundStyle(.red)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
    }

    private func date(_ day: String) -> Date? {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd"
        return f.date(from: day)
    }

    private func time(_ at: String) -> String? {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd HH:mm"
        guard let d = f.date(from: at) else { return nil }
        return Calendar.current.isDate(d, inSameDayAs: entry.date)
            ? d.formatted(date: .omitted, time: .shortened)
            : d.formatted(.dateTime.day().month(.abbreviated).hour().minute())
    }
}
