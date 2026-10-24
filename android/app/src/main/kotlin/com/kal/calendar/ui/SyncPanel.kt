package com.kal.calendar.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch

/**
 * Sync-panel state, mirroring the Dioxus `SyncUiState`.
 *
 * [ShowPhrase] is the one-time view after starting or joining a chain; it is
 * rendered as a modal at the shell level (like the editor), so the sidebar
 * behind it shows the paired fingerprint.
 */
sealed interface SyncUi {
    data object NotPaired : SyncUi

    /** Just created/joined; the recovery phrase is shown once. */
    data class ShowPhrase(val phrase: String, val fingerprint: String) : SyncUi

    data class Paired(val fingerprint: String) : SyncUi

    data class Error(val message: String) : SyncUi
}

/**
 * The sidebar's Sync section: the chain profile header, then the pairing
 * controls (start / join), the paired panel (fingerprint, reveal, sync now,
 * leave) or the inline failure.
 *
 * Copy follows the Dioxus panel exactly; a failed round is transient and stays
 * inline ("Sync failed: …"), while start/join failures show the error state with
 * a Back button.
 */
@Composable
fun SyncPanel(state: ShellState) {
    val colors = LocalKalColors.current
    val scope = rememberCoroutineScope()

    // The chain's profile header (avatar + name). It used to open the Settings
    // modal; the editable profile now sits in the sidebar right below, so the
    // header is a plain summary.
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(KalDims.controlRadius))
            .padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        AvatarPreview(state.profile, size = 40.dp)
        Text(
            text = state.profile.displayName,
            color = colors.fg,
            fontSize = KalDims.bodySize,
            maxLines = 1,
        )
    }
    Spacer(Modifier.height(6.dp))

    when (val ui = state.syncUi) {
        SyncUi.NotPaired -> {
            KalButton(
                label = "Start sync chain",
                modifier = Modifier.fillMaxWidth(),
                primary = true,
                onClick = state::startSyncChain,
            )
            Spacer(Modifier.height(6.dp))
            KalTextField(
                value = state.joinPhrase,
                onValueChange = state::updateJoinPhrase,
                modifier = Modifier.fillMaxWidth().heightIn(min = 56.dp),
                placeholder = "Paste 24-word recovery phrase\u2026",
                singleLine = false,
            )
            Spacer(Modifier.height(6.dp))
            KalButton(
                label = "Join chain\u2026",
                modifier = Modifier.fillMaxWidth(),
                onClick = state::joinSyncChain,
            )
        }

        is SyncUi.Error -> {
            Text(
                text = ui.message,
                color = KalDanger,
                fontSize = KalDims.smallSize,
            )
            Spacer(Modifier.height(6.dp))
            KalButton(
                label = "Back",
                modifier = Modifier.fillMaxWidth(),
                onClick = state::dismissSyncError,
            )
        }

        is SyncUi.Paired, is SyncUi.ShowPhrase -> {
            val fingerprint = when (ui) {
                is SyncUi.Paired -> ui.fingerprint
                is SyncUi.ShowPhrase -> ui.fingerprint
                else -> ""
            }
            Text(
                text = "Device fingerprint",
                color = colors.fgMuted,
                fontSize = KalDims.smallSize,
            )
            Text(
                text = fingerprint,
                color = colors.fg,
                fontSize = KalDims.smallSize,
                fontWeight = FontWeight.Medium,
            )
            Spacer(Modifier.height(6.dp))
            KalButton(
                label = "Show recovery phrase",
                modifier = Modifier.fillMaxWidth(),
                onClick = state::showSyncPhrase,
            )
            Spacer(Modifier.height(6.dp))
            KalButton(
                label = if (state.syncing) "Syncing\u2026" else "Sync now",
                modifier = Modifier.fillMaxWidth(),
                enabled = !state.syncing,
                onClick = { scope.launch { state.syncNow() } },
            )
            state.lastSync?.let { line ->
                Text(
                    text = line,
                    color = colors.fgMuted,
                    fontSize = KalDims.smallSize,
                    modifier = Modifier.padding(top = 4.dp),
                )
            }
            Spacer(Modifier.height(10.dp))
            HorizontalDivider(color = colors.border)
            Spacer(Modifier.height(10.dp))
            LeaveSyncControls(onLeave = state::leaveSyncChain)
        }
    }
}

/**
 * The 24-word recovery phrase modal: a numbered 3-column grid, the "they grant
 * full access" warning and a single acknowledge button.
 */
@Composable
fun SyncPhraseDialog(phrase: SyncUi.ShowPhrase, onDone: () -> Unit) {
    val colors = LocalKalColors.current
    ModalCard(onDismiss = onDone) {
        Text(
            text = "Your sync chain",
            color = colors.fg,
            fontSize = 16.sp,
            fontWeight = FontWeight.SemiBold,
        )
        Text(
            text = "Write these 24 words down and enter them on your other devices. " +
                "They grant full access.",
            color = colors.fgMuted,
            fontSize = KalDims.smallSize,
        )
        val words = phrase.phrase.split(Regex("\\s+")).filter { it.isNotEmpty() }
        words.withIndex().chunked(3).forEach { row ->
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                row.forEach { (index, word) ->
                    Text(
                        text = "${index + 1}. $word",
                        color = colors.fg,
                        fontSize = KalDims.smallSize,
                        modifier = Modifier.weight(1f),
                    )
                }
                // Keep the last row's columns aligned with the others.
                repeat(3 - row.size) { Spacer(Modifier.weight(1f)) }
            }
        }
        KalButton(
            label = "I wrote it down",
            modifier = Modifier.fillMaxWidth(),
            primary = true,
            onClick = onDone,
        )
    }
}

/** Two-step "Leave sync chain" control with inline confirmation. */
@Composable
private fun LeaveSyncControls(onLeave: () -> Unit) {
    var confirm by remember { mutableStateOf(false) }
    if (confirm) {
        Text(
            text = "Leave the sync chain? This device will stop exchanging snapshots " +
                "with other paired devices.",
            color = LocalKalColors.current.fgMuted,
            fontSize = KalDims.smallSize,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            KalButton(label = "Leave", primary = true, onClick = onLeave)
            KalButton(label = "Cancel", onClick = { confirm = false })
        }
    } else {
        KalButton(
            label = "Leave sync chain\u2026",
            modifier = Modifier.fillMaxWidth(),
            onClick = { confirm = true },
        )
    }
}
