package com.kal.calendar.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.kal.calendar.bridge.Visibility

/**
 * `ui::AgendaView`: the next 30 days from today, chronological, capped at 100
 * rows by the native query, with the date column shown on every row.
 */
@Composable
fun AgendaView(state: ShellState) {
    val colors = LocalKalColors.current
    val rows = Visibility.filterAgenda(state.agendaRows, state.hidden)

    Column(
        Modifier
            .fillMaxSize()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 12.dp, vertical = 8.dp),
    ) {
        if (rows.isEmpty()) {
            Text(
                text = Formats.AGAIN_EMPTY,
                color = colors.fgMuted,
                fontSize = KalDims.bodySize,
                modifier = Modifier.padding(24.dp),
            )
        } else {
            rows.forEach { row ->
                ItemRow(
                    entry = row.item,
                    date = row.date,
                    time24h = state.preferences.time24h,
                    onToggleTask = { state.toggleTask(row.item) },
                    onClick = { state.editItem(row.item.id, row.item.start) },
                )
            }
        }
    }
}
