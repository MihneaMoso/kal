package com.kal.calendar.ui

import android.graphics.BitmapFactory
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.kal.calendar.BuildConfig
import com.kal.calendar.KalUpdater
import com.kal.calendar.bridge.KalBridge
import com.kal.calendar.data.UserProfile
import com.kal.calendar.data.installBlockMessage
import com.kal.calendar.data.readAvatar
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

// The three settings sections, rendered at the bottom of the sidebar: the
// device-local preferences, the sync-chain profile (name + avatar) and the
// Software & updates controls.
//
// These are the controls the old top-bar popover and the settings modal held;
// they keep the same behaviour (profile edits are staged and committed on
// Save). The profile is plain settings data, so no native profile code is
// involved; the avatar's magic-byte/size validation is Kotlin
// (UserProfile.withAvatar, Decision C). The update check is the native release
// check — called off the UI thread — and installing is KalUpdater's intent.

/** Time format, first day of week and the theme toggle. */
@Composable
fun PreferencesSection(state: ShellState) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        SelectMenu(
            title = "Time format",
            current = if (state.preferences.time24h) "24-hour" else "12-hour",
            options = listOf("24-hour" to true, "12-hour" to false),
            onSelect = state::setTime24h,
            modifier = Modifier.fillMaxWidth(),
        )
        SelectMenu(
            title = "First day of week",
            current = if (state.preferences.firstDayMonday) "Week starts Monday" else "Week starts Sunday",
            options = listOf("Week starts Monday" to true, "Week starts Sunday" to false),
            onSelect = state::setFirstDayMonday,
            modifier = Modifier.fillMaxWidth(),
        )
        KalButton(
            label = if (state.preferences.isDark) "Dark mode" else "Light mode",
            modifier = Modifier.fillMaxWidth(),
            onClick = state::toggleTheme,
        )
    }
}

/** The sync-chain profile: avatar, user name, upload/remove and Save/Cancel. */
@Composable
fun ProfileSection(state: ShellState) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val profile = state.profileEdit

    // `accept="image/*"` in the Dioxus file input; SAF's GetContent is its
    // Android equivalent.
    val pickAvatar =
        rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
            if (uri != null) {
                scope.launch {
                    val picked = withContext(Dispatchers.IO) { readAvatar(context, uri) }
                    if (picked == null) {
                        // Upstream's copy for a picker read failure, which it
                        // only logged; this build shows it in the section.
                        state.reportProfileError("Could not read image")
                    } else {
                        state.setProfileAvatar(picked.first, picked.second)
                    }
                }
            }
        }

    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            modifier = Modifier.fillMaxWidth(),
        ) {
            AvatarPreview(profile, size = 44.dp)
            Column(
                verticalArrangement = Arrangement.spacedBy(6.dp),
                modifier = Modifier.weight(1f),
            ) {
                KalTextField(
                    value = profile.username,
                    onValueChange = state::updateUsername,
                    modifier = Modifier.fillMaxWidth(),
                    placeholder = "Your name",
                )
                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    KalButton(
                        label = "Upload picture",
                        modifier = Modifier.fillMaxWidth(),
                    ) { pickAvatar.launch("image/*") }
                    if (profile.hasAvatar) {
                        KalButton(
                            label = "Remove picture",
                            modifier = Modifier.fillMaxWidth(),
                            onClick = state::clearProfileAvatar,
                        )
                    }
                }
            }
        }

        state.profileError?.let {
            Text(text = it, color = KalDanger, fontSize = KalDims.smallSize)
        }

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            KalButton(label = "Cancel", onClick = state::resetProfileEdit)
            KalButton(label = "Save", primary = true, onClick = state::saveProfile)
        }
    }
}

/** Current version, check-at-startup and the update check / install action. */
@Composable
fun SoftwareSection(state: ShellState) {
    val colors = LocalKalColors.current
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(
            text = "Kal v${BuildConfig.VERSION_NAME}",
            color = colors.fg,
            fontSize = KalDims.bodySize,
        )
        KalButton(
            label = "Check for updates",
            modifier = Modifier.fillMaxWidth(),
            enabled = !state.checkingUpdate,
            onClick = {
                val path = KalBridge.updateApkPath(context.filesDir.absolutePath)
                scope.launch {
                    state.checkForUpdates(path) { KalUpdater.stagedInstallBlock(context, path) }
                }
            },
        )
        Row(verticalAlignment = Alignment.CenterVertically) {
            Checkbox(
                checked = state.preferences.autoCheckUpdates,
                onCheckedChange = state::setAutoCheckUpdates,
                colors = CheckboxDefaults.colors(
                    checkedColor = colors.accent,
                    uncheckedColor = colors.fgMuted,
                ),
            )
            Text(text = "Check at startup", color = colors.fg, fontSize = KalDims.smallSize)
        }
        state.updateLine?.let {
            Text(text = it, color = colors.fgMuted, fontSize = KalDims.smallSize)
        }
        if (state.updateReady) {
            KalButton(
                label = "Apply update now",
                modifier = Modifier.fillMaxWidth(),
                primary = true,
                onClick = {
                    // The check only offers a staged APK this build can install,
                    // but the installer intent is fired from here: verify again
                    // so the button can never hand Android an APK it will refuse.
                    val path = KalBridge.updateApkPath(context.filesDir.absolutePath)
                    val block = KalUpdater.stagedInstallBlock(context, path)
                    if (block != null) {
                        state.finishInstall(
                            opened = false,
                            reason = "the installer would refuse it \u2014 ${installBlockMessage(block)}",
                        )
                    } else {
                        val outcome = runCatching { KalUpdater.installApk(context, path) }
                        state.finishInstall(
                            opened = outcome.isSuccess,
                            reason = outcome.exceptionOrNull()?.message,
                        )
                    }
                },
            )
        }
    }
}

/** A `<select>`: the current option on a bordered button, opening a menu. */
@Composable
private fun SelectMenu(
    title: String,
    current: String,
    options: List<Pair<String, Boolean>>,
    onSelect: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    var open by remember { mutableStateOf(false) }

    Box(modifier) {
        KalButton(
            label = current,
            modifier = Modifier.fillMaxWidth(),
            onClick = { open = true },
        )
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            options.forEach { (label, value) ->
                DropdownMenuItem(
                    text = { Text(label) },
                    onClick = {
                        open = false
                        onSelect(value)
                    },
                )
            }
        }
    }
}

/** `.profile-avatar`: a circle with the picture, or the initials. */
@Composable
fun AvatarPreview(profile: UserProfile, size: Dp = 48.dp) {
    val colors = LocalKalColors.current
    val bitmap = remember(profile.avatarB64) {
        profile.avatarBytes()?.let { bytes ->
            runCatching { BitmapFactory.decodeByteArray(bytes, 0, bytes.size) }.getOrNull()
        }
    }

    if (bitmap != null) {
        Image(
            bitmap = bitmap.asImageBitmap(),
            contentDescription = "Profile picture",
            contentScale = ContentScale.Crop,
            modifier = Modifier.size(size).clip(CircleShape),
        )
    } else {
        Box(
            modifier = Modifier.size(size).clip(CircleShape).background(colors.accentSoft),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                text = profile.initials,
                color = colors.accent,
                fontSize = (size.value * 0.33f).sp,
                fontWeight = FontWeight.SemiBold,
            )
        }
    }
}
