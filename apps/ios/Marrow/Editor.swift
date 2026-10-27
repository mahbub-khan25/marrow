import SwiftUI
import UIKit

/// Editing a note's Markdown, coloured as it's typed (the engine finds what to
/// colour), with a bar above the keyboard for formatting, links and photos.
struct MarkdownEditor: UIViewRepresentable {
    @Binding var text: String
    /// Where to put the cursor when the editor opens (a line number), or -1.
    var startLine: Int = -1
    let reading: Reading
    /// Notes whose names match the `[[` being typed.
    var suggest: (String) -> [FileHit] = { _ in [] }
    var onPhoto: () -> Void = {}
    var onDictate: (() -> Void)? = nil
    /// Set to insert text at the cursor (a photo's embed, dictated words).
    @Binding var insert: String?

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> UITextView {
        let v = UITextView()
        v.delegate = context.coordinator
        v.backgroundColor = .clear
        v.textContainerInset = UIEdgeInsets(top: 8, left: 0, bottom: 240, right: 0)
        v.textContainer.lineFragmentPadding = 0
        v.alwaysBounceVertical = true
        v.keyboardDismissMode = .interactive
        v.autocapitalizationType = .sentences
        v.smartDashesType = .no
        v.smartQuotesType = .no
        v.text = text
        context.coordinator.view = v
        context.coordinator.colour()
        let bar = EditorBarHost(coordinator: context.coordinator)
        v.inputAccessoryView = bar
        context.coordinator.bar = bar
        // Start where asked, or at the top (not the end, which a long note
        // would scroll to).
        v.selectedRange = NSRange(location: startLine >= 0 ? lineOffset(text, startLine) : 0, length: 0)
        DispatchQueue.main.async {
            v.becomeFirstResponder()
            if startLine >= 0 { v.scrollRangeToVisible(v.selectedRange) } else { v.setContentOffset(.zero, animated: false) }
        }
        return v
    }

    func updateUIView(_ v: UITextView, context: Context) {
        context.coordinator.parent = self
        if v.text != text {
            let sel = v.selectedRange
            v.text = text
            context.coordinator.colour()
            v.selectedRange = NSRange(location: min(sel.location, (text as NSString).length), length: 0)
        }
        if let s = insert {
            DispatchQueue.main.async {
                context.coordinator.insert(s)
                insert = nil
            }
        }
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: MarkdownEditor
        weak var view: UITextView?
        weak var bar: EditorBarHost?

        init(_ p: MarkdownEditor) { parent = p }

        /// Applies the engine's colours to the whole text.
        func colour() {
            guard let v = view else { return }
            let s = v.text ?? ""
            let ns = s as NSString
            let base = parent.reading.uiBody
            let storage = v.textStorage
            let sel = v.selectedRange
            storage.beginEditing()
            let all = NSRange(location: 0, length: ns.length)
            let para = NSMutableParagraphStyle()
            para.lineSpacing = 6 * parent.reading.scale
            storage.setAttributes([.font: base, .foregroundColor: UIColor(Color.ink), .paragraphStyle: para], range: all)
            if ns.length < 300_000 {
                for m in highlight(text: s) {
                    let start = Int(m.start), end = Int(m.end)
                    guard start < end, end <= ns.length else { continue }
                    let r = NSRange(location: start, length: end - start)
                    switch m.kind {
                    case .heading:
                        let size = base.pointSize * (m.level == 1 ? 1.35 : m.level == 2 ? 1.2 : m.level == 3 ? 1.1 : 1)
                        storage.addAttribute(.font, value: bold(base, size: size), range: r)
                    case .bold:
                        storage.addAttribute(.font, value: bold(base, size: base.pointSize), range: r)
                    case .italic, .quote:
                        storage.addAttribute(.font, value: italic(base), range: r)
                    case .strike:
                        storage.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: r)
                    case .code, .codeBlock:
                        storage.addAttribute(.font, value: UIFont.monospacedSystemFont(ofSize: base.pointSize * 0.88, weight: .regular), range: r)
                        storage.addAttribute(.backgroundColor, value: UIColor(Color.codeBg), range: r)
                    case .link:
                        storage.addAttribute(.foregroundColor, value: UIColor(Color.accent), range: r)
                    case .tag:
                        storage.addAttribute(.foregroundColor, value: UIColor(Color.tagText), range: r)
                        storage.addAttribute(.backgroundColor, value: UIColor(Color.tagBg), range: r)
                    case .listMarker, .taskOpen:
                        storage.addAttribute(.foregroundColor, value: UIColor(Color.accent), range: r)
                    case .taskDone:
                        if m.level == 1 {
                            storage.addAttribute(.foregroundColor, value: UIColor(Color.faint), range: r)
                            storage.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: r)
                        } else {
                            storage.addAttribute(.foregroundColor, value: UIColor(Color.accent), range: r)
                        }
                    case .highlight:
                        storage.addAttribute(.backgroundColor, value: UIColor(Color.mark), range: r)
                    case .frontmatter:
                        storage.addAttribute(.foregroundColor, value: UIColor(Color.faint), range: r)
                        storage.addAttribute(.font, value: UIFont.monospacedSystemFont(ofSize: base.pointSize * 0.85, weight: .regular), range: r)
                    case .syntax:
                        storage.addAttribute(.foregroundColor, value: UIColor(Color.faint), range: r)
                    }
                }
            }
            storage.endEditing()
            v.selectedRange = sel
            v.typingAttributes = [.font: base, .foregroundColor: UIColor(Color.ink), .paragraphStyle: para]
        }

        private func bold(_ f: UIFont, size: CGFloat) -> UIFont {
            if let d = f.fontDescriptor.withSymbolicTraits(.traitBold) { return UIFont(descriptor: d, size: size) }
            return .boldSystemFont(ofSize: size)
        }

        private func italic(_ f: UIFont) -> UIFont {
            if let d = f.fontDescriptor.withSymbolicTraits(.traitItalic) { return UIFont(descriptor: d, size: f.pointSize) }
            return .italicSystemFont(ofSize: f.pointSize)
        }

        func textViewDidChange(_ v: UITextView) {
            colour()
            parent.text = v.text
            updateSuggestions()
        }

        func textViewDidChangeSelection(_ v: UITextView) {
            updateSuggestions()
        }

        /// Keeps a list going after Enter ("- ", "- [ ] ", "2. "), or ends it on an empty item.
        func textView(_ v: UITextView, shouldChangeTextIn range: NSRange, replacementText t: String) -> Bool {
            guard t == "\n", range.length == 0 else { return true }
            let ns = (v.text ?? "") as NSString
            let lineRange = ns.lineRange(for: NSRange(location: range.location, length: 0))
            let line = ns.substring(with: NSRange(location: lineRange.location, length: range.location - lineRange.location))
            guard let m = listPrefix(line) else { return true }
            if m.rest.trimmingCharacters(in: .whitespaces).isEmpty {
                // An empty item: Enter ends the list.
                v.textStorage.replaceCharacters(in: NSRange(location: lineRange.location, length: range.location - lineRange.location), with: "")
                v.selectedRange = NSRange(location: lineRange.location, length: 0)
                textViewDidChange(v)
                return false
            }
            insert("\n" + m.next)
            return false
        }

        func insert(_ s: String) {
            guard let v = view else { return }
            let r = v.selectedRange
            v.textStorage.replaceCharacters(in: r, with: s)
            v.selectedRange = NSRange(location: r.location + (s as NSString).length, length: 0)
            textViewDidChange(v)
            v.scrollRangeToVisible(v.selectedRange)
        }

        /// Wraps the selection in `mark`, or puts the cursor between two of them.
        func wrap(_ mark: String) {
            guard let v = view else { return }
            let r = v.selectedRange
            let ns = (v.text ?? "") as NSString
            let inner = ns.substring(with: r)
            v.textStorage.replaceCharacters(in: r, with: mark + inner + mark)
            v.selectedRange = NSRange(location: r.location + (mark as NSString).length, length: r.length)
            textViewDidChange(v)
        }

        /// Sets or changes the start of the cursor's line ("# ", "- ", "- [ ] ", "> ").
        func prefix(_ p: String) {
            guard let v = view else { return }
            let ns = (v.text ?? "") as NSString
            let sel = v.selectedRange
            let lineRange = ns.lineRange(for: NSRange(location: sel.location, length: 0))
            let line = ns.substring(with: lineRange)
            let existing = line.range(of: #"^(#{1,6} |[-*+] \[[ xX]\] |[-*+] |\d+[.)] |> )"#, options: .regularExpression).map { String(line[$0]) } ?? ""
            let replace: String
            if p == "# " && existing.hasPrefix("#") { replace = existing.count >= 7 ? "" : "#" + existing }
            else if existing == p { replace = "" }
            else { replace = p }
            v.textStorage.replaceCharacters(in: NSRange(location: lineRange.location, length: (existing as NSString).length), with: replace)
            let shift = (replace as NSString).length - (existing as NSString).length
            v.selectedRange = NSRange(location: max(lineRange.location, sel.location + shift), length: sel.length)
            textViewDidChange(v)
        }

        /// The `[[` being typed before the cursor, if any.
        func linkQuery() -> String? {
            guard let v = view else { return nil }
            let ns = (v.text ?? "") as NSString
            let before = ns.substring(to: min(v.selectedRange.location, ns.length))
            guard let open = before.range(of: "[[", options: .backwards) else { return nil }
            let q = String(before[open.upperBound...])
            if q.contains("]]") || q.contains("\n") || q.count > 60 { return nil }
            return q
        }

        func updateSuggestions() {
            guard let bar else { return }
            if let q = linkQuery() { bar.model.suggestions = parent.suggest(q) } else if !bar.model.suggestions.isEmpty { bar.model.suggestions = [] }
        }

        /// Replaces the `[[query` before the cursor with a whole link.
        func complete(_ hit: FileHit) {
            guard let v = view else { return }
            let ns = (v.text ?? "") as NSString
            let cur = v.selectedRange.location
            let open = (ns.substring(to: cur) as NSString).range(of: "[[", options: .backwards)
            guard open.location != NSNotFound else { return }
            let after = ns.substring(from: cur)
            let close = after.hasPrefix("]]") ? 2 : 0
            let link = "[[\(hit.linkText)]]"
            v.textStorage.replaceCharacters(in: NSRange(location: open.location, length: cur - open.location + close), with: link)
            v.selectedRange = NSRange(location: open.location + (link as NSString).length, length: 0)
            textViewDidChange(v)
        }

        func done() { view?.resignFirstResponder() }
    }
}

/// "- ", "- [ ] ", "3. " at the start of a list line, and what comes next.
func listPrefix(_ line: String) -> (next: String, rest: String)? {
    guard let r = line.range(of: #"^(\s*)([-*+]|\d+[.)])( \[[ xX]\])? "#, options: .regularExpression) else { return nil }
    let head = String(line[r])
    let rest = String(line[r.upperBound...])
    let indent = String(head.prefix { $0 == " " || $0 == "\t" })
    let body = head.dropFirst(indent.count)
    let marker = String(body.prefix { $0 != " " })
    var next = marker
    if let n = Int(marker.dropLast()), let last = marker.last { next = "\(n + 1)\(last)" }
    let task = head.contains("[") ? " [ ]" : ""
    return (indent + next + task + " ", rest)
}

/// Character offset (UTF-16) where `line` (zero-based) starts.
func lineOffset(_ text: String, _ line: Int) -> Int {
    let ns = text as NSString
    var at = 0
    for _ in 0..<max(0, line) {
        let r = ns.range(of: "\n", options: [], range: NSRange(location: at, length: ns.length - at))
        if r.location == NSNotFound { return ns.length }
        at = r.location + 1
    }
    return at
}

@Observable
final class EditorBarModel {
    var suggestions: [FileHit] = []
}

/// The bar above the keyboard, drawn by SwiftUI inside a UIKit input view.
final class EditorBarHost: UIInputView {
    let model = EditorBarModel()
    private var host: UIHostingController<EditorBar>?

    init(coordinator: MarkdownEditor.Coordinator) {
        super.init(frame: CGRect(x: 0, y: 0, width: 320, height: 96), inputViewStyle: .keyboard)
        allowsSelfSizing = true
        let bar = EditorBar(model: model, coordinator: coordinator)
        let h = UIHostingController(rootView: bar)
        h.view.backgroundColor = .clear
        h.view.translatesAutoresizingMaskIntoConstraints = false
        h.sizingOptions = [.intrinsicContentSize]
        addSubview(h.view)
        NSLayoutConstraint.activate([
            h.view.leadingAnchor.constraint(equalTo: leadingAnchor),
            h.view.trailingAnchor.constraint(equalTo: trailingAnchor),
            h.view.topAnchor.constraint(equalTo: topAnchor),
            h.view.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
        host = h
    }

    required init?(coder: NSCoder) { fatalError() }
}

private struct EditorBar: View {
    let model: EditorBarModel
    let coordinator: MarkdownEditor.Coordinator

    var body: some View {
        VStack(spacing: 0) {
            if !model.suggestions.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(model.suggestions, id: \.path) { hit in
                            Button { coordinator.complete(hit) } label: {
                                Text(verbatim: hit.name).font(.subheadline.weight(.medium)).lineLimit(1)
                                    .padding(.horizontal, 14).padding(.vertical, 7)
                                    .background(Color.accentSoft, in: Capsule())
                                    .foregroundStyle(Color.onAccentSoft)
                            }
                        }
                    }
                    .padding(.horizontal, 10).padding(.vertical, 6)
                }
            }
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 2) {
                    tool("textformat.size", "Heading") { coordinator.prefix("# ") }
                    tool("bold", "Bold") { coordinator.wrap("**") }
                    tool("italic", "Italic") { coordinator.wrap("*") }
                    tool("list.bullet", "List") { coordinator.prefix("- ") }
                    tool("checklist", "Task") { coordinator.prefix("- [ ] ") }
                    tool("link", "Link to a note") { coordinator.insert("[[]]"); moveBack(2) }
                    tool("number", "Tag") { coordinator.insert("#") }
                    tool("text.quote", "Quote") { coordinator.prefix("> ") }
                    tool("chevron.left.forwardslash.chevron.right", "Code") { coordinator.wrap("`") }
                    tool("photo", "Add a photo") { coordinator.parent.onPhoto() }
                    if let d = coordinator.parent.onDictate { tool("mic", "Dictate", d) }
                    tool("keyboard.chevron.compact.down", "Done") { coordinator.done() }
                }
                .padding(.horizontal, 6)
            }
            .frame(height: 46)
        }
        .background(.bar)
    }

    private func moveBack(_ n: Int) {
        guard let v = coordinator.view else { return }
        v.selectedRange = NSRange(location: max(0, v.selectedRange.location - n), length: 0)
        coordinator.updateSuggestions()
    }

    private func tool(_ symbol: String, _ label: LocalizedStringKey, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).font(.system(size: 17)).frame(width: 42, height: 40)
        }
        .foregroundStyle(Color.inkSoft)
        .accessibilityLabel(Text(label))
    }
}
