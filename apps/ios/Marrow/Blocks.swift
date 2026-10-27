import SwiftUI
import UIKit

/// What tapping things in a rendered note does.
struct BlockActions {
    /// A link to a note, a missing note, a tag or a web page.
    var onLink: (SpanLink) -> Void = { _ in }
    /// The task on this line was ticked or unticked.
    var onToggle: (Int) -> Void = { _ in }
    /// Opens an image or embedded file (vault path, or web address).
    var onOpen: (String) -> Void = { _ in }
    /// Full path of a vault file, to show an image.
    var fileOf: (String) -> String? = { _ in nil }
}

/// Links inside text are carried as marrow:// addresses, then handed to `onLink`.
private func linkURL(_ l: SpanLink) -> URL? {
    var c = URLComponents()
    c.scheme = "marrow"
    c.host = switch l.kind {
    case .url: "url"
    case .note: "note"
    case .missing: "missing"
    case .tag: "tag"
    }
    c.queryItems = [URLQueryItem(name: "t", value: l.target)]
    return c.url
}

private func linkFrom(_ url: URL) -> SpanLink? {
    guard url.scheme == "marrow", let c = URLComponents(url: url, resolvingAgainstBaseURL: false),
          let t = c.queryItems?.first(where: { $0.name == "t" })?.value else { return nil }
    let kind: LinkKind = switch c.host {
    case "url": .url
    case "note": .note
    case "tag": .tag
    default: .missing
    }
    return SpanLink(kind: kind, target: t)
}

/// Styled runs of text, with tappable links and tags.
func attributed(_ spans: [Span], font: Font, reading: Reading) -> AttributedString {
    var out = AttributedString()
    for s in spans {
        var a = AttributedString(s.text)
        var f = s.code ? reading.code : font
        if s.bold { f = f.bold() }
        if s.italic { f = f.italic() }
        a.font = f
        if s.strike { a.strikethroughStyle = .single }
        if s.code { a.backgroundColor = .codeBg }
        if s.highlight { a.backgroundColor = .mark }
        if let l = s.link {
            a.link = linkURL(l)
            switch l.kind {
            case .tag:
                a.foregroundColor = .tagText
                a.backgroundColor = .tagBg
            case .missing:
                a.foregroundColor = .missing
                a.underlineStyle = .single
            case .url:
                a.foregroundColor = .accent
                a.underlineStyle = .single
            case .note:
                a.foregroundColor = .accent
            }
        }
        out.append(a)
    }
    return out
}

struct Rich: View {
    let spans: [Span]
    var font: Font? = nil
    var color: Color = .ink
    let actions: BlockActions
    @Environment(\.reading) private var reading

    var body: some View {
        Text(attributed(spans, font: font ?? reading.body, reading: reading))
            .foregroundStyle(color)
            .lineSpacing(reading.bodyLine * 0.6)
            .fixedSize(horizontal: false, vertical: true)
            .environment(\.openURL, OpenURLAction { url in
                if let l = linkFrom(url) {
                    actions.onLink(l)
                    return .handled
                }
                return .systemAction
            })
    }
}

/// A note, drawn from the blocks the engine made of its Markdown.
struct Blocks: View {
    let blocks: [Block]
    let actions: BlockActions

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(groups().enumerated()), id: \.offset) { i, g in
                switch g {
                case .one(let b, let prev):
                    gap(prev, b)
                    BlockView(block: b, actions: actions)
                case .quote(let inside, let callout, let prev, let first):
                    gap(prev, first)
                    Quote(blocks: inside, callout: callout, actions: actions)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private enum Group {
        case one(Block, prev: Block?)
        case quote([Block], callout: String, prev: Block?, first: Block)
    }

    /// Quotes and callouts become one box holding everything inside them.
    private func groups() -> [Group] {
        var out: [Group] = []
        var i = 0
        var prev: Block? = nil
        while i < blocks.count {
            let b = blocks[i]
            if b.quote > 0 {
                var inside: [Block] = []
                while i < blocks.count && blocks[i].quote > 0 {
                    var x = blocks[i]
                    x.quote -= 1
                    inside.append(x)
                    i += 1
                }
                out.append(.quote(inside, callout: b.callout, prev: prev, first: b))
                prev = b
                continue
            }
            out.append(.one(b, prev: prev))
            prev = b
            i += 1
        }
        return out
    }

    @ViewBuilder
    private func gap(_ prev: Block?, _ b: Block) -> some View {
        if let prev {
            let list: Set<BlockKind> = [.bullet, .numbered, .task]
            let h: CGFloat = b.kind == .heading ? (b.level <= 2 ? 24 : 16)
                : (list.contains(b.kind) && list.contains(prev.kind)) ? 4
                : prev.kind == .heading ? 8 : 12
            Color.clear.frame(height: h)
        }
    }
}

private struct Quote: View {
    let blocks: [Block]
    let callout: String
    let actions: BlockActions

    var body: some View {
        let accent = callout.isEmpty ? Color.quoteBar : Color.callout(callout)
        HStack(alignment: .top, spacing: 0) {
            Rectangle().fill(accent).frame(width: callout.isEmpty ? 3 : 4)
            Blocks(blocks: blocks, actions: actions)
                .padding(.leading, 14)
                .padding(.trailing, 12)
                .padding(.vertical, callout.isEmpty ? 2 : 12)
        }
        .background(callout.isEmpty ? Color.clear : accent.opacity(0.1))
        .clipShape(RoundedRectangle(cornerRadius: callout.isEmpty ? 2 : 10, style: .continuous))
    }
}

private func calloutSymbol(_ kind: String) -> String {
    switch kind {
    case "tip", "hint", "important": return "lightbulb"
    case "warning", "caution", "attention": return "exclamationmark.triangle"
    case "danger", "error", "bug", "failure", "fail", "missing": return "xmark.octagon"
    case "question", "help", "faq": return "questionmark.circle"
    case "success", "check", "done": return "checkmark.circle"
    case "quote", "cite": return "quote.opening"
    default: return "info.circle"
    }
}

private struct BlockView: View {
    let block: Block
    let actions: BlockActions
    @Environment(\.reading) private var reading

    var body: some View {
        let b = block
        switch b.kind {
        case .heading:
            Rich(spans: b.spans, font: b.level == 1 ? reading.h1 : b.level == 2 ? reading.h2 : b.level == 3 ? reading.h3 : reading.h4, actions: actions)
                .accessibilityAddTraits(.isHeader)
        case .paragraph:
            Rich(spans: b.spans, actions: actions).padding(.leading, CGFloat(b.level) * 22)
        case .bullet, .numbered, .task:
            ListItem(block: b, actions: actions)
        case .code:
            VStack(alignment: .leading, spacing: 4) {
                if !b.language.isEmpty { Text(verbatim: b.language).font(.caption2).foregroundStyle(Color.faint) }
                ScrollView(.horizontal, showsIndicators: false) {
                    Text(verbatim: b.text).font(reading.code).foregroundStyle(Color.ink).textSelection(.enabled)
                }
            }
            .padding(.horizontal, 14).padding(.vertical, 12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.codeBg, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        case .math:
            Text(verbatim: b.text).font(reading.code.italic()).frame(maxWidth: .infinity).padding(.vertical, 6)
        case .table:
            TableView(block: b, actions: actions)
        case .rule:
            Divider().overlay(Color.line).padding(.vertical, 10)
        case .image:
            Picture(block: b, actions: actions)
        case .embed:
            Button { actions.onOpen(b.target) } label: {
                Label { Text(verbatim: b.text.isEmpty ? b.target : b.text).font(.subheadline.weight(.semibold)) } icon: { Image(systemName: "doc.text") }
                    .padding(14).frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).stroke(Color.line))
            }
            .buttonStyle(.plain)
        case .properties:
            VStack(alignment: .leading, spacing: 6) {
                ForEach(Array(b.props.enumerated()), id: \.offset) { _, p in
                    HStack(alignment: .firstTextBaseline, spacing: 12) {
                        Text(verbatim: p.key).font(.subheadline).foregroundStyle(Color.inkSoft).frame(width: 100, alignment: .leading)
                        Text(verbatim: p.value).font(.subheadline).foregroundStyle(Color.ink)
                    }
                }
            }
            .padding(.horizontal, 14).padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        case .calloutTitle:
            HStack(spacing: 8) {
                Image(systemName: calloutSymbol(b.callout)).foregroundStyle(Color.callout(b.callout))
                Rich(spans: b.spans, font: reading.font(17, .semibold), color: Color.callout(b.callout), actions: actions)
            }
        }
    }
}

private struct ListItem: View {
    let block: Block
    let actions: BlockActions
    @Environment(\.reading) private var reading

    var body: some View {
        let b = block
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            switch b.kind {
            case .task:
                Button { actions.onToggle(Int(b.line)) } label: {
                    Image(systemName: b.checked ? "checkmark.circle.fill" : "circle")
                        .font(.system(size: 20 * reading.scale))
                        .foregroundStyle(b.checked ? Color.accent : Color.faint)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(b.checked ? Text("Done") : Text("Task"))
            case .numbered:
                Text(verbatim: "\(b.number).").font(reading.body).foregroundStyle(Color.inkSoft).frame(minWidth: 22, alignment: .trailing)
            default:
                Text(verbatim: b.level % 2 == 0 ? "•" : "◦").font(reading.body).foregroundStyle(Color.inkSoft).frame(width: 14)
            }
            Rich(spans: b.spans, color: b.kind == .task && b.checked ? .inkSoft : .ink, actions: actions)
                .strikethrough(b.kind == .task && b.checked, color: .inkSoft)
        }
        .padding(.leading, CGFloat(b.level) * 22)
    }
}

private struct TableView: View {
    let block: Block
    let actions: BlockActions
    @Environment(\.reading) private var reading

    var body: some View {
        let cols = block.rows.map(\.cells.count).max() ?? 0
        ScrollView(.horizontal, showsIndicators: false) {
            Grid(alignment: .topLeading, horizontalSpacing: 0, verticalSpacing: 0) {
                ForEach(Array(block.rows.enumerated()), id: \.offset) { r, row in
                    GridRow {
                        ForEach(0..<cols, id: \.self) { c in
                            Rich(spans: c < row.cells.count ? row.cells[c] : [], font: reading.font(15.5, row.header ? .semibold : .regular), actions: actions)
                                .frame(minWidth: 64, maxWidth: 240, alignment: .leading)
                                .padding(.horizontal, 12).padding(.vertical, 8)
                                .frame(maxHeight: .infinity, alignment: .topLeading)
                                .background(row.header ? Color.chrome : Color.clear)
                                .overlay(alignment: .trailing) { if c < cols - 1 { Rectangle().fill(Color.line).frame(width: 1) } }
                        }
                    }
                    if r < block.rows.count - 1 {
                        Divider().overlay(Color.line).gridCellUnsizedAxes(.horizontal)
                    }
                }
            }
            .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).stroke(Color.line))
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
    }
}

private struct Picture: View {
    let block: Block
    let actions: BlockActions
    @State private var image: UIImage?

    var body: some View {
        Group {
            if block.local {
                if let image {
                    Image(uiImage: image).resizable().scaledToFit()
                } else {
                    RoundedRectangle(cornerRadius: 12).fill(Color.card).frame(height: 160)
                }
            } else if let url = URL(string: block.target), block.target.hasPrefix("http") {
                AsyncImage(url: url) { img in img.resizable().scaledToFit() } placeholder: {
                    RoundedRectangle(cornerRadius: 12).fill(Color.card).frame(height: 160)
                }
            } else {
                Label(block.target, systemImage: "photo").foregroundStyle(Color.inkSoft)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: 520)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .onTapGesture { actions.onOpen(block.target) }
        .accessibilityLabel(Text(verbatim: block.text))
        .task(id: block.target) {
            guard block.local, let path = actions.fileOf(block.target) else { return }
            image = await Task.detached { UIImage(contentsOfFile: path)?.preparingForDisplay() }.value
        }
    }
}
