package com.kal.calendar.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.kal.calendar.bridge.Visibility

/**
 * `ui::WeekView`: seven day rows from the Monday of the cursor's week, each
 * with its heading and one item row per occurrence (or `—` when empty).
 *
 * The week starts on Monday regardless of the first-day preference — the
 * original did the same, and it is preserved deliberately.
 */
@Composable
fun WeekView(state: ShellState) {
    val colors = LocalKalColors.current
    val days = Visibility.filterWeek(state.weekDays, state.hidden)

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 12.dp, vertical = 8.dp),
    ) {
        days.forEach { day ->
            Column(Modifier.fillMaxWidth().padding(vertical = 7.dp)) {
                Text(
                    text = Formats.dayLabel(day.date),
                    color = colors.fg,
                    fontSize = KalDims.bodySize,
                    modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
                )

                if (day.items.isEmpty()) {
                    Text(
                        text = Formats.EMPTY_DAY,
                        color = colors.fgMuted,
                        fontSize = KalDims.smallSize,
                        modifier = Modifier.padding(horizontal = 10.dp),
                    )
                } else {
                    day.items.forEach { entry ->
                        ItemRow(
                            entry = entry,
                            date = null,
                            time24h = state.preferences.time24h,
                            onToggleTask = { state.toggleTask(entry) },
                            onClick = { state.editItem(entry.id, entry.start) },
                        )
                    }
                }
            }
        }
    }
}
