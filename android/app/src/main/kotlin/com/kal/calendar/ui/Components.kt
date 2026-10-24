package com.kal.calendar.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.kal.calendar.bridge.DisplayEntry
import com.kal.calendar.bridge.ItemKind
import java.time.LocalDate

/** Parse a `#RRGGBB` token from a payload, falling back to [fallback]. */
fun colorOf(hex: String, fallback: Color): Color {
    if (hex.length != 7 || !hex.startsWith("#")) return fallback
    val value = hex.substring(1).toLongOrNull(16) ?: return fallback
    return Color(0xFF000000L or value)
}

/** The effective colour of an entry (dots, day blocks). */
@Composable
fun effectiveColor(entry: DisplayEntry): Color =
    colorOf(entry.color, LocalKalColors.current.accent)

/** The chip colour: the item's own override, else the accent (calendars are not consulted). */
@Composable
fun chipColor(entry: DisplayEntry): Color =
    colorOf(entry.overrideColor ?: "", LocalKalColors.current.accent)

/** `.dot` / `.item-dot`: an 8px circle in the entry's effective colour. */
@Composable
fun KalDot(color: Color, modifier: Modifier = Modifier) {
    Box(
        modifier
            .size(KalDims.dotSize)
            .clip(CircleShape)
            .background(color),
    )
}

/** `.event-chip`: a coloured pill with 11px white text, single-line, ellipsised. */
@Composable
fun EventChip(
    label: String,
    background: Color,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
) {
    Box(
        modifier
            .clip(RoundedCornerShape(KalDims.chipRadius))
            .background(background)
            .then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier)
            .padding(horizontal = 6.dp, vertical = 2.dp),
    ) {
        Text(
            text = label,
            color = Color.White,
            fontSize = KalDims.tinySize,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

/**
 * One `.agenda-list li` / week item row: dot, optional date column, task
 * checkbox, title (struck through when completed) and the when column.
 */
@Composable
fun ItemRow(
    entry: DisplayEntry,
    date: LocalDate?,
    time24h: Boolean,
    onToggleTask: () -> Unit,
    onClick: () -> Unit,
) {
    val colors = LocalKalColors.current
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 7.dp),
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        KalDot(effectiveColor(entry))

        if (date != null) {
            Text(
                text = Formats.dayLabel(date),
                color = colors.accent,
                fontSize = KalDims.smallSize,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
                modifier = Modifier.widthIn(min = KalDims.agendaDateMinWidth),
            )
        }

        if (entry.kind == ItemKind.TASK) {
            // The checkbox consumes its own tap, so ticking a task never opens
            // the editor — the original stopped propagation the same way.
            Checkbox(
                checked = entry.completed,
                onCheckedChange = { onToggleTask() },
                colors = CheckboxDefaults.colors(checkedColor = colors.accent, uncheckedColor = colors.fgMuted),
            )
        }

        Text(
            text = Formats.rowTitle(entry),
            color = if (entry.completed) colors.fgMuted else colors.fg,
            fontSize = KalDims.bodySize,
            textDecoration = if (entry.completed) TextDecoration.LineThrough else null,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f, fill = false),
        )

        Text(
            text = Formats.whenLabel(entry, time24h),
            color = colors.fgMuted,
            fontSize = KalDims.smallSize,
            maxLines = 1,
            modifier = Modifier.widthIn(min = KalDims.whenMinWidth),
        )
    }
}

/**
 * `button` from the stylesheet: 13px text, 8px radius, 1px border, 6×12
 * padding. [primary] is the `.primary` variant (accent fill, white text).
 */
@Composable
fun KalButton(
    label: String,
    modifier: Modifier = Modifier,
    primary: Boolean = false,
    enabled: Boolean = true,
    dense: Boolean = false,
    onClick: () -> Unit,
) {
    val colors = LocalKalColors.current
    val shape = RoundedCornerShape(KalDims.controlRadius)
    val filled = primary && enabled
    Box(
        modifier
            .clip(shape)
            .background(if (filled) colors.accent else colors.bg)
            .border(1.dp, if (filled) colors.accent else colors.border, shape)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(
                horizontal = if (dense) 7.dp else 12.dp,
                vertical = if (dense) 4.dp else 6.dp,
            ),
    ) {
        Text(
            text = label,
            color = when {
                !enabled -> colors.fgMuted
                primary -> colors.onAccent
                else -> colors.fg
            },
            fontSize = if (dense) 12.sp else KalDims.bodySize,
            fontWeight = if (filled) FontWeight.Medium else null,
            maxLines = 1,
        )
    }
}

/** `.icon-btn`: 32×30 borderless glyph button (hamburger, gear). */
@Composable
fun KalIconButton(
    glyph: String,
    label: String,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    val colors = LocalKalColors.current
    Box(
        modifier
            .clip(RoundedCornerShape(KalDims.controlRadius))
            .clickable(onClick = onClick)
            .padding(horizontal = 6.dp, vertical = 4.dp)
            .widthIn(min = 32.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text = glyph, color = colors.fg, fontSize = 15.sp, maxLines = 1)
    }
}

/** `.sidebar h2`: small, spaced, upper-case section heading. */@Composable
fun SectionHeading(text: String) {
    Text(
        text = text.uppercase(),
        color = LocalKalColors.current.fgMuted,
        fontSize = KalDims.sectionHeadingSize,
        fontWeight = FontWeight.Medium,
        modifier = Modifier.padding(start = 2.dp, end = 2.dp, top = 18.dp, bottom = 6.dp),
    )
}

/** A native failure, reported instead of swallowed. */
@Composable
fun ErrorDialog(message: String, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Native call failed") },
        text = { Text(message) },
        confirmButton = { TextButton(onClick = onDismiss) { Text("OK") } },
    )
}

/**
 * The `.modal` card every dialog uses: a centred, scrollable panel that
 * swallows taps, over a dimmed backdrop that dismisses it. The hardware/gesture
 * back closes it too — the Dioxus modals only had the backdrop click.
 */
@Composable
fun ModalCard(onDismiss: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    val colors = LocalKalColors.current

    BackHandler(onBack = onDismiss)

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black.copy(alpha = 0.45f))
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null,
                onClick = onDismiss,
            ),
        contentAlignment = Alignment.Center,
    ) {
        val maxCardWidth = maxWidth * 0.9f
        Column(
            modifier = Modifier
                .widthIn(min = minOf(360.dp, maxCardWidth), max = maxCardWidth)
                .heightIn(max = maxHeight * 0.88f)
                .shadow(12.dp, RoundedCornerShape(KalDims.cardRadius))
                .clip(RoundedCornerShape(KalDims.cardRadius))
                .background(colors.bg)
                .border(1.dp, colors.border, RoundedCornerShape(KalDims.cardRadius))
                // Swallow taps so they never reach the scrim behind the card.
                .clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onClick = {},
                )
                .verticalScroll(rememberScrollState())
                .padding(18.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
            content = content,
        )
    }
}

/** `label { caption; control }` from the stylesheet's modal block. */
@Composable
internal fun Field(label: String, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Column(modifier) {
        Text(
            text = label,
            color = LocalKalColors.current.fgMuted,
            fontSize = KalDims.smallSize,
            modifier = Modifier.padding(bottom = 3.dp),
        )
        content()
    }
}

/** A bordered input in the stylesheet's control tokens, with its placeholder. */
@Composable
internal fun KalTextField(
    value: String,
    onValueChange: (String) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    autofocus: Boolean = false,
    singleLine: Boolean = true,
) {
    val colors = LocalKalColors.current
    val shape = RoundedCornerShape(KalDims.controlRadius)
    val focus = remember { FocusRequester() }
    if (autofocus) {
        LaunchedEffect(Unit) { focus.requestFocus() }
    }

    Box(
        modifier
            .clip(shape)
            .background(colors.bg)
            .border(1.dp, colors.border, shape)
            .padding(horizontal = 12.dp, vertical = 7.dp),
    ) {
        if (value.isEmpty() && placeholder != null) {
            Text(
                text = placeholder,
                color = colors.fgMuted,
                fontSize = KalDims.bodySize,
                maxLines = 1,
            )
        }
        BasicTextField(
            value = value,
            onValueChange = onValueChange,
            singleLine = singleLine,
            textStyle = TextStyle(color = colors.fg, fontSize = KalDims.bodySize),
            cursorBrush = SolidColor(colors.accent),
            modifier = Modifier.fillMaxWidth().focusRequester(focus),
        )
    }
}
