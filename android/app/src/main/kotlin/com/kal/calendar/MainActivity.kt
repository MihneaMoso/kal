package com.kal.calendar

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Surface
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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

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
                    KalTheme(dark = state.preferences.isDark) {
                        KalApp(state)
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
