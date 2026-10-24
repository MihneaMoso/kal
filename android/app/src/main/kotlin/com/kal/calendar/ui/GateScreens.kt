package com.kal.calendar.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp

/**
 * Blocking gate shown when the loaded native library speaks a different bridge
 * version than this build was compiled against. Per the rewrite guide the app
 * must never run against an unknown ABI, so there is no way past this screen.
 */
@Composable
fun BridgeMismatchScreen(expected: Int, actual: Int) {
    GateScaffold(
        title = "Incompatible build",
        body = "This app expects native bridge version $expected but the installed " +
            "library reports $actual.\n\nReinstall the app so the Kotlin code and " +
            "libkal_jni.so come from the same build.",
    )
}

/** Shown when the database cannot be opened, or the library cannot be loaded. */
@Composable
fun StartupErrorScreen(message: String) {
    GateScaffold(title = "Could not start", body = message)
}

@Composable
private fun GateScaffold(title: String, body: String) {
    val clipboard = LocalClipboardManager.current
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(32.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp, Alignment.CenterVertically),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(text = title, style = MaterialTheme.typography.headlineSmall, textAlign = TextAlign.Center)
        Text(text = body, style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center)
        Button(
            onClick = { clipboard.setText(AnnotatedString("$title\n\n$body")) },
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text("Copy details")
        }
    }
}
