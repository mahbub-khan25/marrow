package app.marrow.ui.theme

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import app.marrow.R
import app.marrow.data.Look

// Marrow's colours: warm paper, ink, and one calm blue (the desktop app's).
private val Light = lightColorScheme(
    primary = Color(0xFF0A7EA4),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFD3ECF6),
    onPrimaryContainer = Color(0xFF00354A),
    secondary = Color(0xFF5F5A52),
    onSecondary = Color.White,
    secondaryContainer = Color(0xFFE9E5DD),
    onSecondaryContainer = Color(0xFF1D1C1A),
    tertiary = Color(0xFF7A5C00),
    onTertiary = Color.White,
    tertiaryContainer = Color(0xFFFFE28F),
    onTertiaryContainer = Color(0xFF261A00),
    error = Color(0xFFB3261E),
    errorContainer = Color(0xFFF9DEDC),
    onErrorContainer = Color(0xFF410E0B),
    background = Color(0xFFFBFAF7),
    onBackground = Color(0xFF1D1C1A),
    surface = Color(0xFFFBFAF7),
    onSurface = Color(0xFF1D1C1A),
    surfaceVariant = Color(0xFFEBE8E2),
    onSurfaceVariant = Color(0xFF5C5850),
    surfaceTint = Color(0xFF0A7EA4),
    outline = Color(0xFFB4AFA6),
    outlineVariant = Color(0xFFDEDAD2),
    surfaceBright = Color(0xFFFFFFFF),
    surfaceDim = Color(0xFFE4E1DA),
    surfaceContainerLowest = Color(0xFFFFFFFF),
    surfaceContainerLow = Color(0xFFF7F5F1),
    surfaceContainer = Color(0xFFF2F0EB),
    surfaceContainerHigh = Color(0xFFEBE8E2),
    surfaceContainerHighest = Color(0xFFE4E1DA),
    inverseSurface = Color(0xFF32302C),
    inverseOnSurface = Color(0xFFF4F1EB),
    inversePrimary = Color(0xFF4FB6DC),
    scrim = Color.Black,
)

private val Dark = darkColorScheme(
    primary = Color(0xFF4FB6DC),
    onPrimary = Color(0xFF00364A),
    primaryContainer = Color(0xFF0D4D66),
    onPrimaryContainer = Color(0xFFC8ECFA),
    secondary = Color(0xFFCBC5BB),
    onSecondary = Color(0xFF33302B),
    secondaryContainer = Color(0xFF3A3732),
    onSecondaryContainer = Color(0xFFECE8E2),
    tertiary = Color(0xFFF0C445),
    onTertiary = Color(0xFF3F2E00),
    tertiaryContainer = Color(0xFF5B4400),
    onTertiaryContainer = Color(0xFFFFE28F),
    error = Color(0xFFF2B8B5),
    errorContainer = Color(0xFF8C1D18),
    onErrorContainer = Color(0xFFF9DEDC),
    background = Color(0xFF1C1B19),
    onBackground = Color(0xFFECE8E2),
    surface = Color(0xFF1C1B19),
    onSurface = Color(0xFFECE8E2),
    surfaceVariant = Color(0xFF34312D),
    onSurfaceVariant = Color(0xFFB8B2A8),
    surfaceTint = Color(0xFF4FB6DC),
    outline = Color(0xFF7A756D),
    outlineVariant = Color(0xFF3D3A35),
    surfaceBright = Color(0xFF3A3834),
    surfaceDim = Color(0xFF141311),
    surfaceContainerLowest = Color(0xFF141311),
    surfaceContainerLow = Color(0xFF191816),
    surfaceContainer = Color(0xFF211F1D),
    surfaceContainerHigh = Color(0xFF292724),
    surfaceContainerHighest = Color(0xFF33312D),
    inverseSurface = Color(0xFFECE8E2),
    inverseOnSurface = Color(0xFF32302C),
    inversePrimary = Color(0xFF0A7EA4),
    scrim = Color.Black,
)

@OptIn(androidx.compose.ui.text.ExperimentalTextApi::class)
private fun literata(weight: FontWeight, style: FontStyle = FontStyle.Normal) = Font(
    if (style == FontStyle.Italic) R.font.literata_italic else R.font.literata,
    weight,
    style,
    variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight)),
)

@OptIn(androidx.compose.ui.text.ExperimentalTextApi::class)
private fun garamond(weight: FontWeight, style: FontStyle = FontStyle.Normal) = Font(
    if (style == FontStyle.Italic) R.font.eb_garamond_italic else R.font.eb_garamond,
    weight,
    style,
    variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight)),
)

/** Literata: a book face made for reading on screens. */
val Literata = FontFamily(
    literata(FontWeight.Normal), literata(FontWeight.Medium), literata(FontWeight.SemiBold), literata(FontWeight.Bold),
    literata(FontWeight.Normal, FontStyle.Italic), literata(FontWeight.SemiBold, FontStyle.Italic), literata(FontWeight.Bold, FontStyle.Italic),
)

val Garamond = FontFamily(
    garamond(FontWeight.Normal), garamond(FontWeight.Medium), garamond(FontWeight.SemiBold), garamond(FontWeight.Bold),
    garamond(FontWeight.Normal, FontStyle.Italic), garamond(FontWeight.Bold, FontStyle.Italic),
)

/** Text styles for reading a note. */
@Immutable
data class ReadingType(
    val family: FontFamily,
    val body: TextStyle,
    val h1: TextStyle,
    val h2: TextStyle,
    val h3: TextStyle,
    val h4: TextStyle,
    val title: TextStyle,
    val code: TextStyle,
    /** The editor's text (Markdown source). */
    val editor: TextStyle,
    val scale: Float,
)

fun readingType(look: Look): ReadingType {
    val scale = when (look.textSize) {
        0 -> 0.9f
        2 -> 1.12f
        3 -> 1.25f
        else -> 1f
    }
    val family = when (look.readFont) {
        "sans" -> FontFamily.Default
        "classic" -> Garamond
        else -> Literata
    }
    val bump = if (look.readFont == "classic") 1.1f else 1f
    fun s(size: Float, line: Float, weight: FontWeight = FontWeight.Normal, spacing: Float = 0f) = TextStyle(
        fontFamily = family,
        fontSize = (size * scale * bump).sp,
        lineHeight = (line * scale * bump).sp,
        fontWeight = weight,
        letterSpacing = spacing.em,
    )
    return ReadingType(
        family = family,
        body = s(17f, 27f),
        h1 = s(26f, 33f, FontWeight.SemiBold, -0.01f),
        h2 = s(22f, 29f, FontWeight.SemiBold, -0.005f),
        h3 = s(19f, 26f, FontWeight.SemiBold),
        h4 = s(17f, 25f, FontWeight.Bold),
        title = s(30f, 37f, FontWeight.SemiBold, -0.015f),
        code = TextStyle(fontFamily = FontFamily.Monospace, fontSize = (14.5f * scale).sp, lineHeight = (21f * scale).sp),
        editor = TextStyle(fontFamily = family, fontSize = (17f * scale * bump).sp, lineHeight = (26f * scale * bump).sp),
        scale = scale,
    )
}

val LocalReading = staticCompositionLocalOf { readingType(Look()) }

/** Extra colours the notes use: tags, highlights, callouts. */
@Immutable
data class NoteColors(
    val tag: Color,
    val tagBg: Color,
    val highlight: Color,
    val missingLink: Color,
    val codeBg: Color,
    val quoteBar: Color,
    val faint: Color,
)

val LocalNoteColors = staticCompositionLocalOf {
    NoteColors(Color(0xFF0A7EA4), Color(0x140A7EA4), Color(0x66FFD43B), Color(0xFF9B6B12), Color(0xFFF2F0EB), Color(0xFFD5D0C6), Color(0xFF8C877E))
}

/** Callout colours by kind (the same families as the desktop app). */
fun calloutColor(kind: String, dark: Boolean): Color {
    val c = when (kind) {
        "tip", "hint", "important", "success", "check", "done" -> 0xFF2F9E44
        "warning", "caution", "attention" -> 0xFFE8780C
        "danger", "error", "bug", "failure", "fail", "missing" -> 0xFFE03E3E
        "question", "help", "faq" -> 0xFFD4A106
        "example" -> 0xFF8E44AD
        "quote", "cite" -> 0xFF8C877E
        "abstract", "summary", "tldr", "todo", "info" -> 0xFF0B8CB5
        else -> 0xFF0A7EA4
    }
    val base = Color(c)
    return if (dark) base.copy(red = (base.red + 0.15f).coerceAtMost(1f), green = (base.green + 0.15f).coerceAtMost(1f), blue = (base.blue + 0.15f).coerceAtMost(1f)) else base
}

private val AppType = Typography().let { t ->
    t.copy(
        displaySmall = t.displaySmall.copy(fontFamily = Literata, fontWeight = FontWeight.SemiBold),
        headlineLarge = t.headlineLarge.copy(fontFamily = Literata, fontWeight = FontWeight.SemiBold, letterSpacing = (-0.01).em),
        headlineMedium = t.headlineMedium.copy(fontFamily = Literata, fontWeight = FontWeight.SemiBold),
        headlineSmall = t.headlineSmall.copy(fontFamily = Literata, fontWeight = FontWeight.SemiBold),
    )
}

@Composable
fun MarrowTheme(look: Look = Look(), content: @Composable () -> Unit) {
    val dark = when (look.theme) {
        "light" -> false
        "dark" -> true
        else -> isSystemInDarkTheme()
    }
    val scheme: ColorScheme = if (dark) Dark else Light
    val notes = if (dark) {
        NoteColors(Color(0xFF6CC4E6), Color(0x224FB6DC), Color(0x55E8B400), Color(0xFFE0A84A), Color(0xFF262421), Color(0xFF4A4640), Color(0xFF8E887E))
    } else {
        NoteColors(Color(0xFF0A6F91), Color(0x140A7EA4), Color(0x70FFD43B), Color(0xFF9B6B12), Color(0xFFF2F0EB), Color(0xFFD5D0C6), Color(0xFF8C877E))
    }
    CompositionLocalProvider(LocalReading provides readingType(look), LocalNoteColors provides notes) {
        MaterialTheme(colorScheme = scheme, typography = AppType, content = content)
    }
}

/** True when the dark colours are in use. */
@Composable
fun isDark(): Boolean = MaterialTheme.colorScheme.background.luminance() < 0.5f

private fun Color.luminance(): Float = 0.2126f * red + 0.7152f * green + 0.0722f * blue
