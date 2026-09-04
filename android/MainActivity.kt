package dev.dioxus.main

import android.content.Intent
import com.kal.calendar.KalFilePicker

typealias BuildConfig = com.kal.calendar.BuildConfig

class MainActivity : WryActivity() {
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        if (!KalFilePicker.onActivityResult(requestCode, resultCode, data)) {
            super.onActivityResult(requestCode, resultCode, data)
        }
    }
}
