import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// "Add to Marrow" in the share sheet: a web page (saved whole, in
/// Clippings), words or pictures (added to today's note). They wait in the
/// app group until Marrow next opens.
@objc(ShareViewController)
final class ShareViewController: UIViewController {
    private let model = ShareModel()

    override func viewDidLoad() {
        super.viewDidLoad()
        model.finish = { [weak self] in self?.extensionContext?.completeRequest(returningItems: nil) }
        model.cancel = { [weak self] in self?.extensionContext?.cancelRequest(withError: CocoaError(.userCancelled)) }
        let host = UIHostingController(rootView: ShareView(model: model))
        addChild(host)
        host.view.frame = view.bounds
        host.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(host.view)
        host.didMove(toParent: self)
        let items = (extensionContext?.inputItems as? [NSExtensionItem]) ?? []
        Task { await model.load(items) }
    }
}

@MainActor
final class ShareModel: ObservableObject {
    @Published var loading = true
    @Published var url: String?
    @Published var title: String?
    @Published var text = ""
    @Published var savePage = true
    @Published var pictures: [(Data, String)] = []
    @Published var error: String?
    var html: String?
    var selection: String?
    var finish: () -> Void = {}
    var cancel: () -> Void = {}

    func load(_ items: [NSExtensionItem]) async {
        defer { loading = false }
        for item in items {
            for p in item.attachments ?? [] {
                if p.hasItemConformingToTypeIdentifier(UTType.propertyList.identifier),
                   let dict = try? await p.loadItem(forTypeIdentifier: UTType.propertyList.identifier) as? NSDictionary,
                   let page = dict[NSExtensionJavaScriptPreprocessingResultsKey] as? [String: Any] {
                    // From Safari: the page itself.
                    url = page["url"] as? String ?? url
                    title = page["title"] as? String
                    html = page["html"] as? String
                    selection = (page["selection"] as? String).flatMap { $0.isEmpty ? nil : $0 }
                } else if p.hasItemConformingToTypeIdentifier(UTType.url.identifier),
                          let u = try? await p.loadItem(forTypeIdentifier: UTType.url.identifier) as? URL {
                    if u.scheme == "http" || u.scheme == "https" { url = url ?? u.absoluteString }
                } else if p.hasItemConformingToTypeIdentifier(UTType.image.identifier) {
                    if let pic = await picture(p) { pictures.append(pic) }
                } else if p.hasItemConformingToTypeIdentifier(UTType.plainText.identifier),
                          let s = try? await p.loadItem(forTypeIdentifier: UTType.plainText.identifier) as? String {
                    text = text.isEmpty ? s : text + "\n" + s
                }
            }
        }
        // Words that are just the link: the link is shown already.
        if let url, text.trimmingCharacters(in: .whitespacesAndNewlines) == url { text = "" }
        savePage = url != nil
    }

    private func picture(_ p: NSItemProvider) async -> (Data, String)? {
        let type = p.registeredTypeIdentifiers.compactMap { UTType($0) }.first { $0.conforms(to: .image) } ?? .jpeg
        let ext = type.preferredFilenameExtension.map { $0 == "jpeg" ? "jpg" : $0 } ?? "jpg"
        return await withCheckedContinuation { done in
            _ = p.loadDataRepresentation(forTypeIdentifier: type.identifier) { data, _ in
                done.resume(returning: data.map { ($0, ext) })
            }
        }
    }

    var canSave: Bool { url != nil || !pictures.isEmpty || !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    func save() {
        var words = text.trimmingCharacters(in: .whitespacesAndNewlines)
        // A link kept as a link goes into today's note with the words.
        if let url, !savePage { words = words.isEmpty ? url : words + " " + url }
        let item = SharedItem(url: savePage ? url : nil, html: savePage ? html : nil, selection: savePage ? selection : nil,
                              text: words, savePage: savePage && url != nil, images: [])
        do {
            try ShareInbox.add(item, pictures: pictures)
            finish()
        } catch {
            self.error = String(localized: "Marrow can't take this right now. Open Marrow once, then try again.")
        }
    }
}

struct ShareView: View {
    @ObservedObject var model: ShareModel

    var body: some View {
        NavigationStack {
            Form {
                if model.loading {
                    ProgressView()
                } else {
                    if let url = model.url {
                        Section {
                            VStack(alignment: .leading, spacing: 2) {
                                if let t = model.title, !t.isEmpty { Text(verbatim: t).font(.headline).lineLimit(2) }
                                Text(verbatim: url).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                            }
                            Toggle("Save the page", isOn: $model.savePage)
                        } footer: {
                            Text(model.savePage ? "The page is saved as a note in Clippings." : "The link is added to today's note.")
                        }
                    }
                    if !model.pictures.isEmpty {
                        Section {
                            ScrollView(.horizontal, showsIndicators: false) {
                                HStack {
                                    ForEach(model.pictures.indices, id: \.self) { i in
                                        if let img = UIImage(data: model.pictures[i].0) {
                                            Image(uiImage: img).resizable().scaledToFill().frame(width: 72, height: 72).clipShape(RoundedRectangle(cornerRadius: 10))
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if !(model.url != nil && model.savePage) {
                        Section {
                            TextField("What's on your mind?", text: $model.text, axis: .vertical).lineLimit(3...8)
                        } footer: {
                            Text("Added to today's note when you next open Marrow.")
                        }
                    }
                    if let e = model.error {
                        Text(verbatim: e).foregroundStyle(.red)
                    }
                }
            }
            .navigationTitle("Add to Marrow")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { model.cancel() } }
                ToolbarItem(placement: .confirmationAction) { Button("Save") { model.save() }.bold().disabled(model.loading || !model.canSave) }
            }
        }
    }
}
