package com.kal.calendar.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.kal.calendar.bridge.CalendarModel
import com.kal.calendar.bridge.EditScope
import com.kal.calendar.bridge.ItemKind
import java.time.Instant
import java.time.LocalTime
import java.time.ZoneOffset

/** The delete-confirm colour the stylesheet uses for a destructive affordance. */

/**
 * `ui::EditorModal`: the create/edit dialog.
 *
 * Rendered as an in-tree overlay — the original was an absolutely positioned
 * `div` with a scrim, not a separate window — so the card centres in the content
 * area and measures its 360dp/90vw/88vh limits against it. Everything it writes
 * goes through `kal_save_item_json`, which owns the save semantics.
 */
@Composable
fun EditorModal(state: ShellState) {
    val fields = state.editor ?: return
    EditorDialog(
        fields = fields,
        calendars = state.calendars.filter { !it.deleted },
        time24h = state.preferences.time24h,
        error = state.editorError,
        onChange = state::updateEditor,
        onSave = state::saveEditor,
        onDelete = state::deleteEditorItem,
        onDismiss = state::closeEditor,
    )
}

@OptIn(ExperimentalLayoutApi::class, ExperimentalMaterial3Api::class)
@Composable
private fun EditorDialog(
    fields: EditorFields,
    calendars: List<CalendarModel>,
    time24h: Boolean,
    error: String?,
    onChange: (EditorFields) -> Unit,
    onSave: (EditScope) -> Unit,
    onDelete: () -> Unit,
    onDismiss: () -> Unit,
) {
    val colors = LocalKalColors.current
    var showDate by remember { mutableStateOf(false) }
    var showStartTime by remember { mutableStateOf(false) }
    var showEndTime by remember { mutableStateOf(false) }

    // The hardware/gesture back closes the dialog, as a modal should; the
    // original only had the backdrop click, which is also still wired below.
    ModalCard(onDismiss = onDismiss) {
        Text(
            text = fields.heading,
            color = colors.fg,
            fontSize = 16.sp,
            fontWeight = FontWeight.SemiBold,
        )

        Field("Title") {
            KalTextField(
                value = fields.title,
                onValueChange = { onChange(fields.copy(title = it)) },
                autofocus = true,
            )
        }

        if (fields.kind == ItemKind.BIRTHDAY) {
            Field("Person") {
                KalTextField(
                    value = fields.person,
                    onValueChange = { onChange(fields.copy(person = it)) },
                    placeholder = "Who?",
                )
            }
        }

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Field("Date", Modifier.weight(1f)) {
                PickerButton(
                    label = Formats.editedDate(fields.date),
                    onClick = { showDate = true },
                )
            }

            // The checkbox lines up with the date input rather than with its
            // label, as the original's `margin-top:18px` did.
            Row(
                modifier = Modifier.weight(1f).padding(top = 18.dp),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Checkbox(
                    checked = fields.allDay,
                    onCheckedChange = { onChange(fields.copy(allDay = it)) },
                    colors = CheckboxDefaults.colors(
                        checkedColor = colors.accent,
                        uncheckedColor = colors.fgMuted,
                    ),
                )
                Text("All day", color = colors.fg, fontSize = KalDims.bodySize)
            }
        }

        if (!fields.allDay) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Field("Start", Modifier.weight(1f)) {
                    PickerButton(
                        label = fields.startTime?.let { Formats.time(it, time24h) }.orEmpty(),
                        onClick = { showStartTime = true },
                    )
                }
                Field("End", Modifier.weight(1f)) {
                    PickerButton(
                        label = fields.endTime?.let { Formats.time(it, time24h) }.orEmpty(),
                        onClick = { showEndTime = true },
                    )
                }
            }
        }

        Field("Remind me") {
            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                REMINDER_PRESETS.forEach { minutes ->
                    val active = fields.reminders.contains(minutes)
                    SmallButton(
                        label = reminderPresetLabel(minutes),
                        primary = active,
                        onClick = {
                            val next = if (active) {
                                fields.reminders - minutes
                            } else {
                                fields.reminders + minutes
                            }
                            onChange(fields.copy(reminders = next))
                        },
                    )
                }
            }
        }

        Field("Custom reminder (minutes before)") {
            KalTextField(
                value = fields.customReminder,
                onValueChange = { onChange(fields.copy(customReminder = it)) },
                placeholder = "e.g. 45",
            )
        }

        Field("Location (optional)") {
            KalTextField(
                value = fields.location,
                onValueChange = { onChange(fields.copy(location = it)) },
            )
        }

        Field("Calendar") {
            DropdownButton(
                // The id may be null when the database has no live calendar
                // yet; the native save resolves the default in that case.
                label = calendars.firstOrNull { it.id == fields.calendarId }?.name
                    ?: calendars.firstOrNull()?.name
                    ?: "No calendars",
                options = calendars.map { it.name to { onChange(fields.copy(calendarId = it.id)) } },
            )
        }

        Field("Color") {
            FlowRow(
                verticalArrangement = Arrangement.spacedBy(6.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                SmallButton(
                    label = "Calendar",
                    primary = fields.color == null,
                    onClick = { onChange(fields.copy(color = null)) },
                )
                COLOR_PRESETS.forEach { hex ->
                    val selected = fields.color == hex
                    Box(
                        Modifier
                            .size(22.dp)
                            .clip(CircleShape)
                            .background(colorOf(hex, colors.accent))
                            .border(
                                width = if (selected) 2.dp else 1.dp,
                                color = if (selected) colors.fg else colors.border,
                                shape = CircleShape,
                            )
                            .clickable { onChange(fields.copy(color = hex)) },
                    )
                }
            }
        }

        Field("Repeat") {
            DropdownButton(
                label = fields.repeat.label,
                options = RepeatPreset.entries.map { it.label to { onChange(fields.copy(repeat = it)) } },
            )
        }

        error?.let {
            Text(text = it, color = KalDanger, fontSize = KalDims.smallSize)
        }

        Row(
            modifier = Modifier.fillMaxWidth().padding(top = 10.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (fields.id != null) {
                DeleteControls(onConfirm = onDelete)
            } else {
                Spacer(Modifier)
            }

            FlowRow(
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                KalButton(label = "Cancel", onClick = onDismiss)
                if (fields.offersScopes) {
                    KalButton(label = "All events", primary = true) { onSave(EditScope.ALL) }
                    // A detached occurrence is the row itself, so there is no
                    // later part of the series to split from it.
                    if (!fields.exception) {
                        KalButton(label = "\u2026and following") { onSave(EditScope.THIS_AND_FOLLOWING) }
                    }
                    KalButton(label = "This event", primary = true) { onSave(EditScope.THIS_ONLY) }
                } else {
                    KalButton(label = "Save", primary = true) { onSave(EditScope.ALL) }
                }
            }
        }
    }

    if (showDate) {
        val picker = rememberDatePickerState(
            initialSelectedDateMillis = fields.date.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli(),
        )
        DatePickerDialog(
            onDismissRequest = { showDate = false },
            confirmButton = {
                TextButton(
                    onClick = {
                        picker.selectedDateMillis?.let { millis ->
                            val picked = Instant.ofEpochMilli(millis).atZone(ZoneOffset.UTC).toLocalDate()
                            onChange(fields.copy(date = picked))
                        }
                        showDate = false
                    },
                ) { Text("OK") }
            },
            dismissButton = {
                TextButton(onClick = { showDate = false }) { Text("Cancel") }
            },
        ) {
            DatePicker(state = picker)
        }
    }

    if (showStartTime || showEndTime) {
        val editingStart = showStartTime
        val current = if (editingStart) fields.startTime else fields.endTime
        val picker = rememberTimePickerState(
            initialHour = current?.hour ?: (if (editingStart) 9 else 10),
            initialMinute = current?.minute ?: 0,
            is24Hour = time24h,
        )
        AlertDialog(
            onDismissRequest = { showStartTime = false; showEndTime = false },
            confirmButton = {
                TextButton(
                    onClick = {
                        val picked = LocalTime.of(picker.hour, picker.minute)
                        onChange(
                            if (editingStart) {
                                fields.copy(startTime = picked)
                            } else {
                                fields.copy(endTime = picked)
                            },
                        )
                        showStartTime = false
                        showEndTime = false
                    },
                ) { Text("OK") }
            },
            dismissButton = {
                Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                    // An end time can be removed again (tasks and end-less items
                    // have none); a start time always exists.
                    if (!editingStart) {
                        TextButton(
                            onClick = {
                                onChange(fields.copy(endTime = null))
                                showStartTime = false
                                showEndTime = false
                            },
                        ) { Text("Clear") }
                    }
                    TextButton(onClick = { showStartTime = false; showEndTime = false }) { Text("Cancel") }
                }
            },
            text = { TimePicker(state = picker) },
        )
    }
}

/** A control that opens a platform picker instead of typing a date or time. */
@Composable
private fun PickerButton(label: String, onClick: () -> Unit) {
    val colors = LocalKalColors.current
    val shape = RoundedCornerShape(KalDims.controlRadius)
    Box(
        Modifier
            .fillMaxWidth()
            .clip(shape)
            .background(colors.bg)
            .border(1.dp, colors.border, shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 7.dp),
    ) {
        Text(
            text = label,
            color = colors.fg,
            fontSize = KalDims.bodySize,
            maxLines = 1,
        )
    }
}

/** A `<select>`: the current value on a bordered button that opens a menu. */
@Composable
private fun DropdownButton(label: String, options: List<Pair<String, () -> Unit>>) {
    var open by remember { mutableStateOf(false) }
    Box {
        PickerButton(label = label, onClick = { open = true })
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            options.forEach { (text, action) ->
                DropdownMenuItem(
                    text = { Text(text) },
                    onClick = {
                        open = false
                        action()
                    },
                )
            }
        }
    }
}

/** The 12px padding-2×8 buttons of the reminder and colour rows. */
@Composable
private fun SmallButton(label: String, primary: Boolean = false, onClick: () -> Unit) {
    val colors = LocalKalColors.current
    val shape = RoundedCornerShape(KalDims.controlRadius)
    Box(
        Modifier
            .clip(shape)
            .background(if (primary) colors.accent else colors.bg)
            .border(1.dp, if (primary) colors.accent else colors.border, shape)
            .clickable(onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 2.dp),
    ) {
        Text(
            text = label,
            color = if (primary) colors.onAccent else colors.fg,
            fontSize = KalDims.smallSize,
            maxLines = 1,
        )
    }
}

/** Two-step delete: the first tap asks, the second confirms. */
@Composable
private fun DeleteControls(onConfirm: () -> Unit) {
    var confirming by remember { mutableStateOf(false) }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (confirming) {
            Text(
                text = "Delete this item?",
                color = KalDanger,
                fontSize = KalDims.smallSize,
                maxLines = 1,
            )
            KalButton(label = "Delete", onClick = onConfirm)
            KalButton(label = "Keep", onClick = { confirming = false })
        } else {
            KalButton(label = "Delete", onClick = { confirming = true })
        }
    }
}
