package com.kal.calendar.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.kal.calendar.bridge.PositionedEntry
import com.kal.calendar.bridge.Visibility

/**
 * `ui::DayView`: the all-day strip, a 24-hour gutter, and absolutely positioned
 * timed blocks.
 *
 * Position, height and lane geometry all come from the native layout
 * (`topFrac`/`heightFrac`/`lane`/`lanes`), so the Kotlin side never re-implements
 * the overlap algorithm.
 */
@Composable
fun DayView(state: ShellState) {
    val colors = LocalKalColors.current
    val payload = state.dayPayload?.let { Visibility.filterDay(it, state.hidden) }

    Column(Modifier.fillMaxSize()) {
        val allDay = payload?.allDay.orEmpty()
        if (allDay.isNotEmpty()) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState())
                    .padding(horizontal = 12.dp, vertical = 4.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = Formats.ALL_DAY_STRIP,
                    color = colors.fgMuted,
                    fontSize = KalDims.smallSize,
                )
                allDay.forEach { entry ->
                    EventChip(
                        label = Formats.chipLabel(entry, state.preferences.time24h),
                        background = chipColor(entry),
                        onClick = { state.editItem(entry.id, entry.start) },
                    )
                }
            }
        }

        val scroll = rememberScrollState()
        val density = LocalDensity.current
        // Jump to ~08:00 once when the view is entered (the original set
        // `scrollTop = 8 * 60` px, i.e. 480 CSS pixels, once per mount).
        LaunchedEffect(Unit) {
            scroll.scrollTo(with(density) { 480.dp.roundToPx() })
        }

        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(scroll),
        ) {
            Row(Modifier.height(KalDims.dayCanvasHeight)) {
                HourGutter(time24h = state.preferences.time24h)

                Box(
                    Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .background(colors.bg),
                ) {
                    HourLines()
                    payload?.timed?.forEach { block ->
                        TimedEventBlock(block = block, state = state)
                    }
                }
            }
        }
    }
}

/** The 64px hour gutter with one label per hour. */
@Composable
private fun HourGutter(time24h: Boolean) {
    val colors = LocalKalColors.current
    Column(Modifier.width(KalDims.dayGutterWidth)) {
        repeat(24) { hour ->
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(KalDims.hourRow),
                contentAlignment = Alignment.CenterEnd,
            ) {
                Text(
                    text = Formats.hourLabel(hour, time24h),
                    color = colors.fgMuted,
                    fontSize = KalDims.tinySize,
                    maxLines = 1,
                    modifier = Modifier
                        .offset(y = KalDims.hourLabelShift)
                        .padding(end = 8.dp),
                )
            }
        }
    }
}

/** 24 hour rules; the first has no line (`border-top: none`). */
@Composable
private fun HourLines() {
    val colors = LocalKalColors.current
    val stroke = with(LocalDensity.current) { 1.dp.toPx() }
    Column(Modifier.fillMaxSize()) {
        repeat(24) { hour ->
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(KalDims.hourRow)
                    .drawBehind {
                        if (hour > 0) {
                            drawLine(
                                color = colors.border,
                                start = Offset(0f, 0f),
                                end = Offset(size.width, 0f),
                                strokeWidth = stroke,
                            )
                        }
                    },
            )
        }
    }
}

/** One absolutely positioned timed block. */
@Composable
private fun TimedEventBlock(block: PositionedEntry, state: ShellState) {
    val lanes = block.lanes.coerceAtLeast(1)
    val heightFraction = block.heightFrac.coerceAtLeast(0.02)

    BoxWithConstraints(
        Modifier
            .fillMaxSize(),
    ) {
        val laneWidth = maxWidth / lanes
        val top = KalDims.dayCanvasHeight * block.topFrac.toFloat()
        val height = KalDims.dayCanvasHeight * heightFraction.toFloat()

        Box(
            modifier = Modifier
                .offset(x = laneWidth * block.lane, y = top)
                .width((laneWidth - KalDims.blockWidthGutter).coerceAtLeast(0.dp))
                .height(height)
                .padding(horizontal = 2.dp, vertical = 1.dp),
        ) {
            Column(
                Modifier
                    .fillMaxSize()
                    .clip(RoundedCornerShape(KalDims.blockRadius))
                    .background(effectiveColor(block.item))
                    .clickable { state.editItem(block.item.id, block.item.start) }
                    .padding(horizontal = 5.dp, vertical = 2.dp),
            ) {
                Text(
                    text = Formats.blockTime(block.item.start),
                    color = Color.White.copy(alpha = 0.9f),
                    fontSize = KalDims.blockTimeSize,
                    maxLines = 1,
                )
                Text(
                    text = block.item.title,
                    color = Color.White,
                    fontSize = KalDims.blockSize,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}
