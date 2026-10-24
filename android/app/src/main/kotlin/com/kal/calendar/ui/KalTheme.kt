package com.kal.calendar.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/**
 * The design tokens of `app/assets/main.css`, ported 1:1 — the eight colour
 * custom properties, the geometry and the type sizes the shell and the views
 * use. Nothing else is invented: a value that is not in the stylesheet is not
 * here.
 */
data class KalColors(
    val bg: Color,
    val bgAlt: Color,
    val fg: Color,
    val fgMuted: Color,
    val accent: Color,
    val accentSoft: Color,
    val border: Color,
    val today: Color,
    val onAccent: Color,
)

val KalLightColors = KalColors(
    bg = Color(0xFFFFFFFF),
    bgAlt = Color(0xFFF5F6F8),
    fg = Color(0xFF1B1F24),
    fgMuted = Color(0xFF6A737D),
    accent = Color(0xFF3366CC),
    accentSoft = Color(0xFFE8EFFC),
    border = Color(0xFFD8DEE4),
    today = Color(0xFFEEF4FF),
    onAccent = Color.White,
)

val KalDarkColors = KalColors(
    bg = Color(0xFF16191D),
    bgAlt = Color(0xFF1F2329),
    fg = Color(0xFFE6E8EB),
    fgMuted = Color(0xFF9AA4AE),
    accent = Color(0xFF6C99F0),
    accentSoft = Color(0xFF24304A),
    border = Color(0xFF33393F),
    today = Color(0xFF22304A),
    onAccent = Color.White,
)

val LocalKalColors = staticCompositionLocalOf { KalDarkColors }

/** `#c0392b` — the stylesheet's inline error colour (modal error lines, sidebar notices). */
val KalDanger = Color(0xFFC0392B)

/** Geometry and type scale from the stylesheet. */
object KalDims {
    /** `48px` top bar and day hour rows. */
    val topBarHeight = 48.dp
    val hourRow = 48.dp

    /** `24 × 48px` day canvas. */
    val dayCanvasHeight = 1152.dp
    val dayGutterWidth = 64.dp
    val hourLabelShift = (-7).dp

    /** Month grid. */
    val monthRowMinHeight = 64.dp
    val monthCellPadding = 3.dp

    /** Mobile drawer (`.app.mobile .sidebar`). */
    val sidebarWidth = 280.dp

    /** `.day-event` width gutter: `calc(100% / lanes - 4px)`. */
    val blockWidthGutter = 4.dp

    val dotSize = 8.dp
    val whenMinWidth = 110.dp
    val agendaDateMinWidth = 90.dp

    val chipRadius = 5.dp
    val blockRadius = 5.dp
    val rowRadius = 6.dp
    val controlRadius = 8.dp
    val cardRadius = 12.dp

    val bodySize = 13.sp
    val smallSize = 12.sp
    val tinySize = 11.sp
    val blockTimeSize = 10.sp
    val sectionHeadingSize = 11.sp
    val monthHeaderSize = 11.sp
    val monthDaySize = 12.sp
    val blockSize = 11.sp
}

@Composable
fun KalTheme(dark: Boolean, content: @Composable () -> Unit) {
    val colors = if (dark) KalDarkColors else KalLightColors
    val scheme = if (dark) {
        darkColorScheme(
            primary = colors.accent,
            onPrimary = colors.onAccent,
            background = colors.bg,
            onBackground = colors.fg,
            surface = colors.bgAlt,
            onSurface = colors.fg,
            surfaceVariant = colors.accentSoft,
            onSurfaceVariant = colors.fgMuted,
            outline = colors.border,
        )
    } else {
        lightColorScheme(
            primary = colors.accent,
            onPrimary = colors.onAccent,
            background = colors.bg,
            onBackground = colors.fg,
            surface = colors.bgAlt,
            onSurface = colors.fg,
            surfaceVariant = colors.accentSoft,
            onSurfaceVariant = colors.fgMuted,
            outline = colors.border,
        )
    }
    CompositionLocalProvider(LocalKalColors provides colors) {
        MaterialTheme(colorScheme = scheme, content = content)
    }
}
