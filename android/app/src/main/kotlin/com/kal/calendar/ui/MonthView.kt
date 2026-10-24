package com.kal.calendar.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.kal.calendar.bridge.ItemKind
import com.kal.calendar.bridge.MonthCell
import com.kal.calendar.bridge.Visibility

/**
 * `ui::MonthView`: a fixed six-row grid (the week of the cursor's month),
 * weekday headers, day numbers and event chips.
 *
 * Cells clip their content (`overflow: hidden`), so a day with more chips than
 * fit simply shows fewer of them, exactly as before.
 */
@Composable
fun MonthView(state: ShellState) {
    val colors = LocalKalColors.current
    val cells = Visibility.filterMonth(state.monthCells, state.hidden)
    val headers = Formats.weekdayHeaders(state.preferences.firstDayMonday)

    // `.month-grid { padding: 0 8px 8px }`: the grid is inset from the window
    // edges on both sides and at the bottom (the nav strip is not).
    Column(Modifier.fillMaxSize().padding(start = 8.dp, end = 8.dp, bottom = 8.dp)) {
        Row(Modifier.fillMaxWidth()) {
            headers.forEach { label ->
                Text(
                    text = label.uppercase(),
                    color = colors.fgMuted,
                    fontSize = KalDims.monthHeaderSize,
                    fontWeight = FontWeight.SemiBold,
                    textAlign = TextAlign.Center,
                    maxLines = 1,
                    modifier = Modifier.weight(1f).padding(4.dp),
                )
            }
        }

        Column(Modifier.fillMaxSize()) {
            cells.forEach { row ->
                // `.month-row { border-top: 1px solid var(--border) }`: every
                // week is separated by a hairline, including the first one
                // under the weekday header.
                Column(Modifier.fillMaxWidth().weight(1f)) {
                    HorizontalDivider(thickness = 1.dp, color = colors.border)
                    Row(Modifier.fillMaxWidth().weight(1f)) {
                        row.forEachIndexed { index, cell ->
                            MonthCellView(
                                cell = cell,
                                state = state,
                                lastInRow = index == row.lastIndex,
                                modifier = Modifier.weight(1f),
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun MonthCellView(
    cell: MonthCell,
    state: ShellState,
    lastInRow: Boolean,
    modifier: Modifier = Modifier,
) {
    val colors = LocalKalColors.current
    val isToday = cell.date == state.today
    val background = if (isToday) colors.today else colors.bg
    val dayColor = when {
        isToday -> colors.fg
        !cell.inMonth -> colors.fgMuted.copy(alpha = 0.55f)
        else -> colors.fg
    }

    Box(
        modifier
            .fillMaxSize()
            .background(background)
            .clickable { state.newItem(ItemKind.EVENT, cell.date) },
    ) {
        Column(
            Modifier.fillMaxSize().clipToBounds().padding(KalDims.monthCellPadding),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            // `.day-num { align-self: flex-end }`: the number sits at the cell's
            // trailing edge (the chips below it stay full width).
            Text(
                text = cell.date.dayOfMonth.toString(),
                color = dayColor,
                fontSize = KalDims.monthDaySize,
                textAlign = TextAlign.End,
                maxLines = 1,
                modifier = Modifier.fillMaxWidth(),
            )

            cell.items.forEach { entry ->
                EventChip(
                    label = Formats.chipLabel(entry, state.preferences.time24h),
                    background = chipColor(entry),
                    modifier = Modifier.fillMaxWidth(),
                    onClick = { state.editItem(entry.id, entry.start) },
                )
            }
        }

        // `.month-cell { border-right: 1px }`, omitted on the last cell of a row.
        if (!lastInRow) {
            Box(
                Modifier
                    .align(Alignment.CenterEnd)
                    .width(1.dp)
                    .fillMaxHeight()
                    .background(colors.border),
            )
        }
    }
}
