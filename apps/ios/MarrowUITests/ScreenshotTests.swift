import UIKit
import XCTest

/// Pictures of every main screen, on a Simulator, with the demo vault
/// (apps/demo-vault). They're saved in SHOTS_DIR for a person to look at, and
/// kept in the test results; nothing is compared.
final class ScreenshotTests: XCTestCase {
    /// file name, screen (see `Demo` in the app), theme
    private let screens: [(String, String, String?)] = [
        ("01-today", "today", nil),
        ("02-notes", "notes", nil),
        ("03-folder", "folder:Projects", nil),
        ("04-note", "note:Projects/Garden plan.md", nil),
        ("05-editor", "edit:Projects/Garden plan.md", nil),
        ("06-note-dark", "note:Reading/Notes on memory.md", "dark"),
        ("07-search", "search:garden", nil),
        ("08-tasks", "tasks", nil),
        ("09-ask", "ask", nil),
        ("10-calendar", "calendar", nil),
        ("11-graph", "graph", nil),
        ("12-canvas", "canvas:Ideas.canvas", nil),
        ("13-time-machine", "timemachine", nil),
        ("14-settings", "settings", nil),
        ("15-sync", "sync", nil),
        ("16-welcome", "welcome", nil),
        ("17-today-dark", "today", "dark"),
        ("18-capture", "capture", nil),
        ("19-note-bangla", "note:Reading/বাংলা কবিতা.md", nil),
    ]

    override func setUp() {
        continueAfterFailure = true
    }

    func testScreens() throws {
        let env = ProcessInfo.processInfo.environment
        guard let vault = env["DEMO_VAULT"] else { throw XCTSkip("Set DEMO_VAULT to the demo vault folder.") }
        let out = env["SHOTS_DIR"].map { URL(fileURLWithPath: $0) }
        if let out { try? FileManager.default.createDirectory(at: out, withIntermediateDirectories: true) }
        let prefix = UIDevice.current.userInterfaceIdiom == .pad ? "ipad-" : "iphone-"
        for (name, screen, theme) in screens {
            let app = XCUIApplication()
            app.launchArguments = ["-demoVault", vault, "-screen", screen] + (theme.map { ["-theme", $0] } ?? [])
            app.launch()
            // Let the engine load the vault and the screen settle.
            sleep(4)
            let shot = XCUIScreen.main.screenshot()
            let a = XCTAttachment(screenshot: shot)
            a.name = prefix + name
            a.lifetime = .keepAlways
            add(a)
            if let out {
                try? shot.pngRepresentation.write(to: out.appendingPathComponent("\(prefix)\(name).png"))
            }
            app.terminate()
        }
    }
}
