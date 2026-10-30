import PhotosUI
import SwiftUI

private struct TodayData {
    var todayNote: (path: String, text: String)?
    var due: [TaskItem] = []
    var week: Set<String> = []
    var resurfaced: Resurfaced?
    var recent: [FileItem] = []
}

struct TodayView: View {
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var data = TodayData()
    @State private var offset: UInt32 = 0

    private var day: String { todayString() }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                header
                captureCard
                todayCard
                weekStrip
                if !data.due.isEmpty {
                    sectionHead("Due today and overdue") {
                        Button("See all") { router.tab = .tasks }.font(.subheadline)
                    }
                    VStack(spacing: 0) {
                        ForEach(data.due.prefix(6), id: \.self) { t in TaskRow(task: t, today: day) }
                    }
                }
                resurfaced
                if !data.recent.isEmpty {
                    sectionHead("Recent") { EmptyView() }
                    VStack(spacing: 6) {
                        ForEach(data.recent, id: \.path) { f in
                            Button { router.open(f) } label: { FileRow(item: f) }.buttonStyle(.plain)
                        }
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 32)
            .readingWidth()
        }
        .background(Color.paper)
        .toolbar(.hidden, for: .navigationBar)
        .task(id: "\(store.tick)-\(offset)") { await load() }
        .refreshable { store.marrow.syncNow(force: false); await load() }
    }

    private func load() async {
        let d = day
        let off = offset
        let ym = String(d.prefix(7))
        let weekAgo = todayString(Calendar.current.date(byAdding: .day, value: -6, to: Date()) ?? Date())
        let result = try? await store.run { m -> TodayData in
            var t = TodayData()
            if let p = try m.dailyNote(day: d) { t.todayNote = (p, (try? m.readNote(path: p)) ?? "") }
            t.due = try m.tasks(includeDone: false).filter { ($0.due ?? "9999") <= d }.sorted { ($0.due ?? "") < ($1.due ?? "") }
            for month in Set([ym, String(weekAgo.prefix(7))]) {
                for n in try m.daysWithNotes(month: month) { t.week.insert(String(format: "%@-%02d", month, n)) }
            }
            t.resurfaced = try? m.resurface(today: d, nowMs: Int64(Date().timeIntervalSince1970 * 1000), offset: off)
            t.recent = try m.recentNotes(limit: 6)
            return t
        }
        if let result { data = result }
    }

    private func openDay(_ d: String) {
        Task {
            do {
                let p = try await store.run { try $0.openDay(day: d) }
                router.note(p, edit: true)
            } catch { store.say(error) }
        }
    }

    private var header: some View {
        HStack(alignment: .top) {
            VStack(alignment: .leading, spacing: 2) {
                Text(Date().formatted(.dateTime.weekday(.wide)).uppercased())
                    .font(.subheadline.weight(.semibold)).foregroundStyle(Color.accent)
                Text(Date().formatted(.dateTime.day().month(.wide)))
                    .font(.display(34)).foregroundStyle(Color.ink)
            }
            Spacer()
            if let s = store.sync {
                Button { router.push(.sync) } label: {
                    Image(systemName: s.syncing ? "arrow.triangle.2.circlepath.icloud" : (s.problem != nil && s.problem != .offline) ? "exclamationmark.icloud" : "checkmark.icloud")
                        .font(.title3)
                        .foregroundStyle((s.problem != nil && s.problem != .offline) ? Color.danger : Color.inkSoft)
                }
                .accessibilityLabel(Text("Sync"))
            }
            Button { router.push(.settings) } label: { Image(systemName: "gearshape").font(.title3).foregroundStyle(Color.inkSoft) }
                .accessibilityLabel(Text("Settings"))
                .padding(.leading, 12)
        }
        .padding(.top, 12)
    }

    private var captureCard: some View {
        Button { router.captureText = ""; router.capture = true } label: {
            HStack(spacing: 12) {
                Image(systemName: "square.and.pencil").foregroundStyle(Color.accent)
                Text("What's on your mind?").foregroundStyle(Color.inkSoft)
                Spacer()
                Image(systemName: "checklist").foregroundStyle(Color.inkSoft)
                Image(systemName: "camera").foregroundStyle(Color.inkSoft)
                Image(systemName: "mic").foregroundStyle(Color.inkSoft)
            }
            .font(.body)
            .padding(.horizontal, 18).padding(.vertical, 16)
            .background(Color.cardStrong, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        }
        .buttonStyle(.plain)
    }

    private var todayCard: some View {
        Button { openDay(day) } label: {
            HStack(spacing: 14) {
                Image(systemName: "sun.max.fill")
                    .font(.title3).foregroundStyle(.white)
                    .frame(width: 44, height: 44)
                    .background(Color.accent, in: Circle())
                VStack(alignment: .leading, spacing: 2) {
                    Text(data.todayNote == nil ? "Start today's note" : "Today's note").font(.headline).foregroundStyle(Color.onAccentSoft)
                    let body = data.todayNote.map { preview(text: $0.text, max: 140) } ?? ""
                    Text(body.isEmpty ? String(localized: "A place for the day's thoughts and to-dos.") : body)
                        .font(.subheadline).foregroundStyle(Color.onAccentSoft.opacity(0.8)).lineLimit(2)
                }
                Spacer(minLength: 0)
                Image(systemName: "chevron.right").foregroundStyle(Color.onAccentSoft)
            }
            .padding(18)
            .background(Color.accentSoft, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        }
        .buttonStyle(.plain)
    }

    private var weekStrip: some View {
        let cal = Calendar.current
        let days = (0..<7).reversed().compactMap { cal.date(byAdding: .day, value: -$0, to: Date()) }
        return HStack(spacing: 0) {
            ForEach(days, id: \.self) { d in
                let key = todayString(d)
                let isToday = key == day
                Button { openDay(key) } label: {
                    VStack(spacing: 5) {
                        Text(d.formatted(.dateTime.weekday(.narrow))).font(.caption2).foregroundStyle(Color.faint)
                        Text(d.formatted(.dateTime.day()))
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(isToday ? Color.white : Color.ink)
                            .frame(width: 34, height: 34)
                            .background(isToday ? Color.accent : Color.clear, in: Circle())
                        Circle().fill(data.week.contains(key) ? Color.accent : Color.clear).frame(width: 5, height: 5)
                    }
                    .frame(maxWidth: .infinity)
                }
                .buttonStyle(.plain)
            }
            Button { router.push(.calendar) } label: {
                Image(systemName: "calendar").font(.title3).foregroundStyle(Color.inkSoft).frame(width: 44)
            }
            .accessibilityLabel(Text("Calendar"))
        }
        .padding(.vertical, 10).padding(.horizontal, 6)
        .background(Color.card, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
    }

    @ViewBuilder
    private var resurfaced: some View {
        if let r = data.resurfaced, r.forgotten != nil || !r.onThisDay.isEmpty {
            sectionHead("Worth another look") { EmptyView() }
            if let f = r.forgotten {
                VStack(alignment: .leading, spacing: 8) {
                    Label("From your archive", systemImage: "archivebox").font(.subheadline.weight(.semibold)).foregroundStyle(Color.accent)
                    Button { router.note(f.path) } label: { Text(verbatim: f.title).font(.title3.weight(.semibold)).foregroundStyle(Color.ink) }.buttonStyle(.plain)
                    Text("Last opened \(ago(f.lastMs))").font(.caption).foregroundStyle(Color.faint)
                    if !f.preview.isEmpty { Text(verbatim: f.preview).font(.subheadline).foregroundStyle(Color.inkSoft).lineLimit(4) }
                    HStack(spacing: 14) {
                        Button("Open") { router.note(f.path) }.buttonStyle(.bordered)
                        if r.forgottenCount > 1 { Button("Another") { offset += 1 } }
                        Button("Not this one") {
                            Task { try? await store.run { try $0.skipResurfacing(path: f.path) }; offset += 1 }
                        }
                    }
                    .font(.subheadline)
                }
                .padding(18)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Color.card, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
            }
            if !r.onThisDay.isEmpty {
                VStack(alignment: .leading, spacing: 10) {
                    Label("On this day", systemImage: "clock.arrow.circlepath").font(.subheadline.weight(.semibold)).foregroundStyle(Color.accent)
                    ForEach(r.onThisDay, id: \.path) { o in
                        Button { router.note(o.path) } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(o.yearsAgo == 1 ? String(localized: "A year ago") : String(localized: "\(Int(o.yearsAgo)) years ago"))
                                    .font(.subheadline.weight(.medium)).foregroundStyle(Color.accent)
                                Text(verbatim: o.preview.isEmpty ? o.day : o.preview).font(.subheadline).foregroundStyle(Color.ink).lineLimit(2)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(18)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Color.card, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
            }
        }
    }

    private func sectionHead<T: View>(_ title: LocalizedStringKey, @ViewBuilder trailing: () -> T) -> some View {
        HStack {
            SectionTitle(text: title)
            trailing()
        }
        .padding(.top, 10)
    }
}

/// A to-do with its tick box, the note it's in and when it's due.
struct TaskRow: View {
    let task: TaskItem
    let today: String
    var showNote = true
    @Environment(Store.self) private var store
    @Environment(Router.self) private var router
    @State private var ticked = false
    @State private var reminding = false

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Button {
                ticked.toggle()
                Task {
                    do { _ = try await store.run { try $0.toggleTask(path: task.path, line: task.line) } }
                    catch { ticked.toggle(); store.say(error) }
                }
            } label: {
                Image(systemName: (task.done != ticked) ? "checkmark.circle.fill" : "circle")
                    .font(.title3)
                    .foregroundStyle((task.done != ticked) ? Color.accent : Color.faint)
            }
            .buttonStyle(.plain)
            VStack(alignment: .leading, spacing: 2) {
                let spans = (renderMarkdown(text: task.text).first?.spans ?? []).map { s -> Span in
                    // Links in a task aren't looked up here; show them all as links.
                    var s = s
                    if s.link?.kind == .missing { s.link = SpanLink(kind: .note, target: s.link!.target) }
                    return s
                }
                Rich(spans: spans, font: .body, actions: BlockActions(onLink: { _ in router.note(task.path, line: Int(task.line)) }))
                if !meta.isEmpty {
                    Text(verbatim: meta).font(.caption).foregroundStyle(overdue ? Color.danger : Color.faint)
                }
                if let at = task.remind, !task.done { ReminderChip(at: at) }
            }
            Spacer(minLength: 0)
            if !task.done {
                Button { reminding = true } label: {
                    Image(systemName: task.remind == nil ? "alarm" : "alarm.fill")
                        .font(.subheadline)
                        .foregroundStyle(task.remind == nil ? Color.faint : Color.accent)
                        .frame(width: 32, height: 28)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Remind me"))
            }
        }
        .padding(.vertical, 8)
        .contentShape(Rectangle())
        .onTapGesture { router.note(task.path, line: Int(task.line)) }
        .sheet(isPresented: $reminding) {
            RemindSheet(current: task.remind) { at in
                Task {
                    do {
                        _ = try await store.run { try $0.setReminder(path: task.path, line: task.line, text: task.text, at: at) }
                        store.say(at.map { String(localized: "Reminder set: \(Reminders.label($0))") } ?? String(localized: "Reminder removed"))
                    } catch { store.say(error) }
                }
            }
        }
    }

    private var overdue: Bool { (task.due ?? "9999") < today }

    private var meta: String {
        var parts: [String] = []
        if let d = task.due, d != task.title { parts.append(d == today ? String(localized: "Today") : mediumDate(d)) }
        if showNote {
            if task.title == today { parts.append(String(localized: "Today's note")) }
            else if isDay(task.title) { parts.append(longDate(task.title)) }
            else { parts.append(task.title) }
        }
        return parts.joined(separator: " · ")
    }
}

/// Jot something down: it goes into today's note, under Log (or Tasks).
struct CaptureSheet: View {
    var initial = ""
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    @Environment(\.reading) private var reading
    @State private var text = ""
    @State private var asTask = false
    @State private var picks: [PhotosPickerItem] = []
    @State private var images: [(Data, String)] = []
    @State private var saving = false
    @State private var dictating = false
    @FocusState private var focused: Bool

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 12) {
                TextField(asTask ? String(localized: "What needs doing?") : String(localized: "What's on your mind?"), text: $text, axis: .vertical)
                    .font(reading.body)
                    .lineLimit(4...12)
                    .focused($focused)
                if !images.isEmpty {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack {
                            ForEach(Array(images.enumerated()), id: \.offset) { i, img in
                                if let ui = UIImage(data: img.0) {
                                    Image(uiImage: ui).resizable().scaledToFill().frame(width: 84, height: 84)
                                        .clipShape(RoundedRectangle(cornerRadius: 12))
                                        .overlay(alignment: .topTrailing) {
                                            Button { images.remove(at: i) } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(.white, .black.opacity(0.5)) }
                                                .padding(4)
                                        }
                                }
                            }
                        }
                    }
                }
                HStack(spacing: 18) {
                    Toggle(isOn: $asTask) { Label("As a task", systemImage: "checklist") }
                        .toggleStyle(.button)
                    PhotosPicker(selection: $picks, maxSelectionCount: 6, matching: .images) {
                        Image(systemName: "photo").font(.title3)
                    }
                    .accessibilityLabel(Text("Add a photo"))
                    Button { dictating = true } label: { Image(systemName: "mic").font(.title3) }
                        .accessibilityLabel(Text("Dictate"))
                    Spacer()
                }
                Spacer()
            }
            .padding(20)
            .navigationTitle(Text("Quick note"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { save() }.disabled(saving || (text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && images.isEmpty))
                }
            }
            .onAppear {
                if text.isEmpty { text = initial }
                focused = true
            }
            .onChange(of: picks) { _, items in
                Task {
                    for item in items {
                        if let data = try? await item.loadTransferable(type: Data.self) {
                            let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
                            images.append((data, ext == "jpeg" ? "jpg" : ext))
                        }
                    }
                    picks = []
                }
            }
            .sheet(isPresented: $dictating) {
                DictationSheet { said in text = (text.isEmpty ? "" : text + " ") + said }
            }
        }
        .presentationDetents([.medium, .large])
    }

    private func save() {
        saving = true
        let words = text
        let photos = images
        let task = asTask
        Task {
            do {
                try await store.run { m in
                    var embeds: [String] = []
                    for (i, img) in photos.enumerated() {
                        let name = "Photo \(todayString()) \(nowTime().replacingOccurrences(of: ":", with: ""))\(photos.count > 1 ? "-\(i + 1)" : "").\(img.1)"
                        let saved = try m.saveAttachment(folder: "Attachments", name: name, bytes: img.0)
                        embeds.append("![[\(nameOf(saved))]]")
                    }
                    let body = words.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? String(localized: "Photo") : words
                    _ = try m.capture(today: todayString(), time: nowTime(), text: body, asTask: task, extra: embeds)
                }
                store.say(task ? String(localized: "Added to today's tasks.") : String(localized: "Added to today's note."))
                dismiss()
            } catch {
                saving = false
                store.say(error)
            }
        }
    }
}
