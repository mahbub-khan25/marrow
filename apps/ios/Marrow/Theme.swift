import SwiftUI
import UIKit

extension UIColor {
    convenience init(hex: UInt32, alpha: CGFloat = 1) {
        self.init(red: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255, blue: CGFloat(hex & 0xFF) / 255, alpha: alpha)
    }
}

extension Color {
    /// A colour for light mode and one for dark mode.
    init(light: UInt32, dark: UInt32, alpha: CGFloat = 1) {
        self.init(uiColor: UIColor { t in
            t.userInterfaceStyle == .dark ? UIColor(hex: dark, alpha: alpha) : UIColor(hex: light, alpha: alpha)
        })
    }

    // Marrow's colours: warm paper, ink, and one calm blue (the desktop app's).
    static let paper = Color(light: 0xFBFAF7, dark: 0x1C1B19)
    static let chrome = Color(light: 0xF2F0EB, dark: 0x211F1D)
    static let card = Color(light: 0xF5F3EE, dark: 0x262421)
    static let cardStrong = Color(light: 0xEBE8E2, dark: 0x2E2C28)
    static let ink = Color(light: 0x1D1C1A, dark: 0xECE8E2)
    static let inkSoft = Color(light: 0x5C5850, dark: 0xB8B2A8)
    static let faint = Color(light: 0x8C877E, dark: 0x8E887E)
    static let line = Color(light: 0xDEDAD2, dark: 0x3D3A35)
    static let accent = Color(light: 0x0A7EA4, dark: 0x4FB6DC)
    static let accentSoft = Color(light: 0xD3ECF6, dark: 0x0D4D66)
    static let onAccentSoft = Color(light: 0x00354A, dark: 0xC8ECFA)
    static let tagText = Color(light: 0x0A6F91, dark: 0x6CC4E6)
    static let tagBg = Color(light: 0x0A7EA4, dark: 0x4FB6DC, alpha: 0.10)
    static let mark = Color(light: 0xFFD43B, dark: 0xE8B400, alpha: 0.42)
    static let missing = Color(light: 0x9B6B12, dark: 0xE0A84A)
    static let codeBg = Color(light: 0xF2F0EB, dark: 0x262421)
    static let quoteBar = Color(light: 0xD5D0C6, dark: 0x4A4640)
    static let danger = Color(light: 0xB3261E, dark: 0xF2B8B5)

    /// Callout colours by kind (the same families as the desktop app).
    static func callout(_ kind: String) -> Color {
        switch kind {
        case "tip", "hint", "important", "success", "check", "done": return Color(light: 0x2F9E44, dark: 0x5CC071)
        case "warning", "caution", "attention": return Color(light: 0xE8780C, dark: 0xF5A04B)
        case "danger", "error", "bug", "failure", "fail", "missing": return Color(light: 0xE03E3E, dark: 0xF07070)
        case "question", "help", "faq": return Color(light: 0xD4A106, dark: 0xE8C040)
        case "example": return Color(light: 0x8E44AD, dark: 0xB57BD0)
        case "quote", "cite": return Color(light: 0x8C877E, dark: 0x9E988E)
        case "abstract", "summary", "tldr", "todo", "info": return Color(light: 0x0B8CB5, dark: 0x4FB6DC)
        default: return .accent
        }
    }
}

/// Text styles for reading a note, from the person's choices.
struct Reading {
    let family: String?
    let scale: CGFloat

    init(prefs: Prefs) {
        switch prefs.readFont {
        case "sans": family = nil
        case "classic": family = "EB Garamond"
        default: family = "Literata"
        }
        scale = prefs.scale * (prefs.readFont == "classic" ? 1.1 : 1)
    }

    func font(_ size: CGFloat, _ weight: Font.Weight = .regular, relativeTo style: Font.TextStyle = .body) -> Font {
        let s = size * scale
        if let family {
            return Font.custom(family, size: s, relativeTo: style).weight(weight)
        }
        return Font.system(size: s, weight: weight)
    }

    var body: Font { font(17) }
    var bodyLine: CGFloat { 9 * scale }
    var title: Font { font(30, .semibold, relativeTo: .largeTitle) }
    var h1: Font { font(26, .semibold, relativeTo: .title) }
    var h2: Font { font(22, .semibold, relativeTo: .title2) }
    var h3: Font { font(19, .semibold, relativeTo: .title3) }
    var h4: Font { font(17, .bold, relativeTo: .headline) }
    var code: Font { Font.system(size: 14.5 * scale, design: .monospaced) }
    var uiBody: UIFont {
        let size = 17 * scale
        if let family, let f = UIFont(name: family, size: size) {
            return UIFontMetrics(forTextStyle: .body).scaledFont(for: f)
        }
        return UIFontMetrics(forTextStyle: .body).scaledFont(for: .systemFont(ofSize: size))
    }
}

private struct ReadingKey: EnvironmentKey {
    static let defaultValue = Reading(prefs: Prefs())
}

extension EnvironmentValues {
    var reading: Reading {
        get { self[ReadingKey.self] }
        set { self[ReadingKey.self] = newValue }
    }
}

/// The big serif titles Marrow uses for screens.
extension Font {
    static func display(_ size: CGFloat) -> Font { .custom("Literata", size: size, relativeTo: .largeTitle).weight(.semibold) }
}
