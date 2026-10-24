package com.kal.calendar.ui

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import com.kal.calendar.KalShare
import com.kal.calendar.KalUpdater
import com.kal.calendar.bridge.CalendarModel
import com.kal.calendar.bridge.ItemKind
import com.kal.calendar.bridge.KalBridge
import com.kal.calendar.data.ICS_PICKER_MIME
import com.kal.calendar.data.findActivity
import com.kal.calendar.data.readIcs
import com.kal.calendar.notify.ReminderAlarms
import kotlin.math.abs
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The application shell: top bar, the mobile-overlay sidebar drawer, the nav
 * strip and the current view.
 *
 * The Dioxus Android build was always "mobile" (`cfg!(target_os = "android")`),
 * so the sidebar is always the 280px overlay drawer with a scrim — there is no
 * in-flow, resizable desktop sidebar to port. The top bar's own breakpoint is
 * width-based in the stylesheet, so it is reproduced with the same 900px rule.
 */
@Composable
fun KalApp(state: ShellState) {
    val colors = LocalKalColors.current

    // Reminder reconcile: ShellState.reload() refreshes the firings after every
    // mutation, and the OS is re-armed whenever the app reaches the foreground
    // (§5.3's "reschedule on every foreground / after mutations").
    val context = LocalContext.current
    val alarms = remember { ReminderAlarms(context.applicationContext) }
    LaunchedEffect(state.reminderFirings) { alarms.reconcile(state.reminderFirings) }
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) {
        alarms.reconcile(state.reminderFirings)
    }

    // Startup update check, once per mount when the preference is on (the
    // Dioxus shell did the same and put the result in its Software & updates
    // section, now at the bottom of the sidebar).
    LaunchedEffect(Unit) {
        if (state.preferences.autoCheckUpdates) {
            val path = KalBridge.updateApkPath(context.filesDir.absolutePath)
            state.checkForUpdates(path) { KalUpdater.stagedInstallBlock(context, path) }
        }
    }

    // Sync-chain background driver: while paired, run a round immediately and
    // then every 10 s — the same cadence as the Dioxus app's background thread.
    // The round blocks on peer discovery, so ShellState runs it on IO. Leaving
    // the chain flips syncPaired and cancels this effect.
    LaunchedEffect(state.syncPaired) {
        if (!state.syncPaired) return@LaunchedEffect
        while (isActive) {
            state.backgroundSync()
            delay(SYNC_INTERVAL_MS)
        }
    }

    BoxWithConstraints(
        modifier = Modifier
            .fillMaxSize()
            .background(colors.bg)
            .statusBarsPadding(),
    ) {
        val compact = maxWidth < 900.dp

        Column(Modifier.fillMaxSize()) {
            TopBar(state = state, compact = compact)

            Box(Modifier.weight(1f)) {
                SwipeableContent(state = state, modifier = Modifier.fillMaxSize())

                // A phone-width shell opens with the drawer closed, as the
                // mobile build did.
                SidebarDrawer(state = state)
            }
        }

        state.error?.let { message ->
            ErrorDialog(message = message, onDismiss = state::dismissError)
        }
        EditorModal(state)
        (state.syncUi as? SyncUi.ShowPhrase)?.let { phrase ->
            SyncPhraseDialog(phrase = phrase, onDone = state::acknowledgeSyncPhrase)
        }
    }
}

/** The view body for the current view mode. */
@Composable
private fun CurrentView(state: ShellState) {
    when (state.view) {
        ViewMode.MONTH -> MonthView(state)
        ViewMode.WEEK -> WeekView(state)
        ViewMode.DAY -> DayView(state)
        ViewMode.AGENDA -> AgendaView(state)
    }
}

/**
 * `header.topbar`: 48px, `--bg-alt`, 1px bottom border. The former second bar
 * (‹ Today › + period label + view buttons) is merged in here: the period label
 * sits between the title and the view tabs, and ‹ › are replaced by swipes.
 */
@Composable
private fun TopBar(state: ShellState, compact: Boolean) {
    val colors = LocalKalColors.current
    Column {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .height(KalDims.topBarHeight)
                .background(colors.bgAlt)
                .padding(horizontal = if (compact) 10.dp else 14.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(if (compact) 6.dp else 10.dp),
        ) {
            KalIconButton(
                glyph = "\u2630",
                label = "Toggle sidebar",
                onClick = state::toggleSidebar,
            )

            Text(
                text = "Kal",
                color = colors.fg,
                fontSize = 17.sp,
                fontWeight = FontWeight.SemiBold,
                maxLines = 1,
            )

            Text(
                text = CalendarNav.title(state.view, state.cursor, compact = compact),
                color = colors.fg,
                fontSize = if (compact) 14.sp else 16.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                // Takes the slack: the tabs stay pinned right and the label
                // truncates rather than pushing them off a narrow screen.
                modifier = Modifier.weight(1f),
            )

            ViewTabs(state, compact)
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(colors.border))
    }
}

/** The four view-mode buttons, tight enough to share the top bar on a phone. */
@Composable
private fun ViewTabs(state: ShellState, compact: Boolean) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        ViewMode.entries.forEach { mode ->
            KalButton(
                label = mode.label,
                primary = state.view == mode,
                dense = compact,
                onClick = { state.selectView(mode) },
            )
        }
    }
}

/**
 * The view body, shifted horizontally while the user swipes: a left swipe goes
 * to the next period, a right swipe to the previous one. On release the
 * outgoing view slides fully off, the cursor steps, and the incoming view
 * slides in from the other edge.
 *
 * A drag that does not pass the distance/velocity threshold (or a vertical
 * scroll, which the child views claim) snaps back untouched.
 */
@Composable
private fun SwipeableContent(state: ShellState, modifier: Modifier = Modifier) {
    val scope = rememberCoroutineScope()
    var widthPx by remember { mutableFloatStateOf(0f) }
    var offsetX by remember { mutableFloatStateOf(0f) }
    var animating by remember { mutableStateOf(false) }

    val dragState = rememberDraggableState { delta ->
        if (!animating && widthPx > 0f) {
            offsetX = (offsetX + delta).coerceIn(-widthPx, widthPx)
        }
    }

    Box(
        modifier
            // Clip first so the shifted view cannot bleed outside the body;
            // the visual shift must not move the gesture's hit area either.
            .clipToBounds()
            .graphicsLayer { translationX = offsetX }
            .onSizeChanged { widthPx = it.width.toFloat() }
            .draggable(
                state = dragState,
                orientation = Orientation.Horizontal,
                enabled = state.view != ViewMode.AGENDA,
                onDragStopped = { velocity ->
                    scope.launch {
                        val direction = swipeDirection(offsetX, velocity, widthPx)
                        if (direction == 0) {
                            animate(
                                initialValue = offsetX,
                                targetValue = 0f,
                                animationSpec = tween(180, easing = FastOutSlowInEasing),
                            ) { value, _ -> offsetX = value }
                        } else {
                            animating = true
                            val out = if (direction > 0) -widthPx else widthPx
                            animate(
                                initialValue = offsetX,
                                targetValue = out,
                                animationSpec = tween(150, easing = FastOutSlowInEasing),
                            ) { value, _ -> offsetX = value }

                            // Same frame: the new period is laid out off-screen
                            // on the entry edge, then slides home.
                            state.step(forward = direction > 0)
                            offsetX = -out
                            animate(
                                initialValue = offsetX,
                                targetValue = 0f,
                                animationSpec = tween(210, easing = FastOutSlowInEasing),
                            ) { value, _ -> offsetX = value }
                            animating = false
                        }
                    }
                },
            ),
    ) {
        CurrentView(state)
    }
}

/**
 * +1 = next period, -1 = previous, 0 = not a swipe. Uses the release offset
 * plus a little velocity projection, so a quick flick works even when the
 * finger only travelled a short distance.
 */
private fun swipeDirection(offsetPx: Float, velocity: Float, widthPx: Float): Int {
    if (widthPx <= 0f) return 0
    val pulled = abs(offsetPx) >= widthPx * 0.18f
    val flung = abs(velocity) >= FLING_VELOCITY
    if (!pulled && !flung) return 0
    val projected = offsetPx + velocity * 0.12f
    return if (projected < 0f) 1 else -1
}

/** Pixels per second that count as a fling rather than a slow drag. */
private const val FLING_VELOCITY = 900f


/** `.drawer-scrim` plus the sliding `.sidebar` overlay. */
@Composable
private fun SidebarDrawer(state: ShellState) {
    val colors = LocalKalColors.current

    // Outer: the scrim fades; inner: the panel slides in from the left, as the
    // stylesheet does (`transform .22s` on the panel, nothing on the scrim).
    AnimatedVisibility(visible = state.sidebarOpen, enter = fadeIn(), exit = fadeOut()) {
        Box(Modifier.fillMaxSize()) {
            // The scrim covers everything below the top bar (CSS: `top:48px`).
            Box(
                Modifier
                    .fillMaxSize()
                    .background(Color.Black.copy(alpha = 0.42f))
                    .clickable(onClick = state::closeSidebar),
            )
            AnimatedVisibility(
                visible = state.sidebarOpen,
                enter = slideInHorizontally(initialOffsetX = { -it }),
                exit = slideOutHorizontally(targetOffsetX = { -it }),
            ) {
                Sidebar(
                    state = state,
                    modifier = Modifier
                        .width(KalDims.sidebarWidth)
                        .fillMaxHeight()
                        .background(colors.bgAlt)
                        // Swallow taps so they do not reach the scrim and close
                        // the drawer when the user taps the panel itself.
                        .clickable(
                            interactionSource = remember { MutableInteractionSource() },
                            indication = null,
                            onClick = {},
                        ),
                )
            }
        }
    }
}

/** `aside.sidebar`: calendars, sync, import/export, new-item and settings. */
@Composable
private fun Sidebar(state: ShellState, modifier: Modifier = Modifier) {
    val colors = LocalKalColors.current
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    // Upstream's Android picker accepted any file (`*/*`), so this one does too.
    // The document is read off the main thread; the native import itself is not.
    val pickIcs =
        rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            if (uri != null) {
                scope.launch {
                    val text = withContext(Dispatchers.IO) { readIcs(context, uri) }
                    if (text == null) state.sidebarFailed(unreadableFile()) else state.importIcs(text)
                }
            }
        }

    Column(
        modifier = modifier
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 12.dp, vertical = 14.dp),
        horizontalAlignment = Alignment.Start,
    ) {
        SectionHeading("Calendars")
        state.listedCalendars.forEach { calendar ->
            CalendarRow(calendar = calendar, onToggle = { state.toggleCalendarVisibility(calendar) })
        }

        SectionHeading("Sync")
        SyncPanel(state)

        SectionHeading("Import / Export")
        KalButton(
            label = "Import .ics\u2026",
            modifier = Modifier.fillMaxWidth(),
            onClick = { pickIcs.launch(arrayOf(ICS_PICKER_MIME)) },
        )
        Spacer(Modifier.height(6.dp))
        KalButton(
            label = "Export all (.ics)",
            modifier = Modifier.fillMaxWidth(),
            onClick = {
                // Rust writes the file; Kotlin owns the share intent. The path is
                // the one KalFileProvider serves as content://…/ics.
                val path = KalBridge.exportPath(context.filesDir.absolutePath)
                if (state.exportIcs(path)) {
                    context.findActivity()?.let(KalShare::shareIcs)
                }
            },
        )
        state.sidebarError?.let { message ->
            Text(
                text = message,
                color = KalDanger,
                fontSize = KalDims.smallSize,
                modifier = Modifier.padding(top = 6.dp),
            )
        }

        SectionHeading("New item")
        KalButton(
            label = "+ Event",
            modifier = Modifier.fillMaxWidth(),
            primary = true,
            onClick = { state.newItem(ItemKind.EVENT) },
        )
        Spacer(Modifier.height(6.dp))
        KalButton(
            label = "+ Task",
            modifier = Modifier.fillMaxWidth(),
            onClick = { state.newItem(ItemKind.TASK) },
        )
        Spacer(Modifier.height(6.dp))
        KalButton(
            label = "+ Birthday",
            modifier = Modifier.fillMaxWidth(),
            onClick = { state.newItem(ItemKind.BIRTHDAY) },
        )

        SectionHeading("Preferences")
        PreferencesSection(state)

        SectionHeading("Sync-chain profile")
        ProfileSection(state)

        SectionHeading("Software & updates")
        SoftwareSection(state)
    }
}

/** One calendar row: visibility checkbox, colour dot, name, source label. */
@Composable
private fun CalendarRow(calendar: CalendarModel, onToggle: () -> Unit) {
    val colors = LocalKalColors.current
    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Checkbox(
            checked = calendar.visible,
            onCheckedChange = { onToggle() },
            colors = CheckboxDefaults.colors(checkedColor = colors.accent, uncheckedColor = colors.fgMuted),
        )
        KalDot(colorOf(calendar.color, colors.accent))
        Text(
            text = calendar.name,
            color = colors.fg,
            fontSize = KalDims.bodySize,
            maxLines = 1,
            modifier = Modifier.weight(1f, fill = false),
        )
        if (calendar.sourceLabel.isNotEmpty()) {
            Text(
                text = calendar.sourceLabel,
                color = colors.fgMuted,
                fontSize = KalDims.smallSize,
                maxLines = 1,
            )
        }
    }
}

/** How often the paired background driver attempts a round (the Dioxus app used 10 s). */
private const val SYNC_INTERVAL_MS = 10_000L
