package com.kal.calendar

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import com.kal.calendar.bridge.BridgeStatus
import com.kal.calendar.bridge.KalBridge
import com.kal.calendar.bridge.KalSession
import com.kal.calendar.notify.ReminderAlarms
import com.kal.calendar.notify.askForNotificationPermissionOnce
import com.kal.calendar.ui.BridgeMismatchScreen
import com.kal.calendar.ui.KalApp
import com.kal.calendar.ui.KalTheme
import com.kal.calendar.ui.ShellState
import com.kal.calendar.ui.StartupErrorScreen
import com.kal.calendar.widgets.WidgetIntents
import java.time.LocalDate

/**
 * Single-activity host for the Kotlin interface layer.
 *
 * The activity does the startup work the whole app depends on — the JNI
 * handshake and the database open — and hosts the Compose shell. A bridge
 * version mismatch or a failed open shows a blocking gate instead of the shell,
 * as the guide requires.
 */
class MainActivity : ComponentActivity() {
    private var session: KalSession? = null

    /**
     * A date passed by the month widget. [KalApp] consumes it once and clears
     * it, so recomposition or a later ordinary launch does not reopen the
     * editor. Kept in the activity because the widget can deliver a new date
     * through [onNewIntent] while the shell is already composed.
     */
    private val widgetAddDate = mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // A configuration change recreates the activity with the same launch
        // intent; only the first creation should consume the widget date.
        if (savedInstanceState == null) handleWidgetIntent(intent)

        val status = KalBridge.start(filesDir.absolutePath)
        if (status is BridgeStatus.Ready) {
            session = status.session
        }

        // Create the reminder channel from the first launch, so system settings
        // list it before the first reminder exists. The permission prompt waits
        // for onResume: a dialog cannot be shown from onCreate.
        ReminderAlarms(this).ensureChannel()

        setContent {
            when (status) {
                is BridgeStatus.Ready -> {
                    val state = remember { ShellState(status.session) }
                    val addDate = widgetAddDate.value
                    KalTheme(dark = state.preferences.isDark) {
                        KalApp(
                            state = state,
                            widgetAddDate = addDate,
                            onWidgetAddDateConsumed = { widgetAddDate.value = null },
                        )
                    }
                }

                is BridgeStatus.Incompatible -> KalTheme(dark = true) {
                    Surface(modifier = Modifier.fillMaxSize()) {
                        BridgeMismatchScreen(status.expected, status.actual)
                    }
                }

                is BridgeStatus.Failed -> KalTheme(dark = true) {
                    Surface(modifier = Modifier.fillMaxSize()) {
                        StartupErrorScreen(status.message)
                    }
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleWidgetIntent(intent)
    }

    private fun handleWidgetIntent(intent: Intent?) {
        widgetAddDate.value = when {
            intent == null -> null
            intent.getBooleanExtra(WidgetIntents.EXTRA_ADD_TODAY, false) ->
                LocalDate.now().toString()
            else -> intent.getStringExtra(WidgetIntents.EXTRA_ADD_DATE)
        }
    }

    override fun onResume() {
        super.onResume()
        // Android 13+: ask once per install, then leave it to system settings.
        askForNotificationPermissionOnce(this)
    }

    override fun onDestroy() {
        session?.close()
        session = null
        super.onDestroy()
    }
}
