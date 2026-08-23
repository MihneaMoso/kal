// Kal — Android Glance agenda widget.
//
// Reads the same SQLite DB as the app through the kal-ffi cdylib
// (libkal_ffi.so, loaded from jniLibs/<abi>/). The widget refreshes on its own
// schedule and whenever the main app updates data.

package app.kal.widgets

import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.provideContent
import androidx.glance.text.FontWeight
import androidx.glance.text.Text
import androidx.glance.text.TextStyle
import org.json.JSONArray
import java.io.File

// JNI bindings resolved against libkal_ffi.so (crates/kal-ffi cdylib).
// NOTE: the C ABI uses raw pointers; on Android we pass them as Long handles.
private external fun kal_open(path: String): Long
private external fun kal_close(db: LongArray)
private external fun kal_upcoming_json(db: Long, fromEpoch: Long, toEpoch: Long): String?
private external fun kal_free(s: LongArray)

fun loadUpcoming(context: Context, daysAhead: Int = 14): JSONArray {
    // Must match where the main Kal app stores its DB (see app/src/main.rs).
    val dbPath = File(context.filesDir.parentFile?.resolve("databases"), "kal/calendar.db")
    runCatching { System.loadLibrary("kal_ffi") }
        .onFailure { return JSONArray() }
    val handle = longArrayOf(kal_open(dbPath.absolutePath))
    if (handle[0] == 0L) return JSONArray()
    try {
        val now = System.currentTimeMillis() / 1000
        return JSONArray(kal_upcoming_json(handle[0], now, now + daysAhead * 86_400L) ?: "[]")
    } finally {
        kal_close(handle)
    }
}

class KalAgendaWidget : GlanceAppWidget() {

    override suspend fun provideGlance(context: Context, widgetId: Int) {
        val items = runCatching { loadUpcoming(context) }.getOrDefault(JSONArray())
        provideContent {
            AgendaContent(items)
        }
    }

    @Composable
    private fun AgendaContent(items: JSONArray) {
        androidx.glance.appwidget.Column(modifier = androidx.glance.GlanceModifier.padding(8.dp)) {
            Text("Kal", style = TextStyle(fontSize = 12.sp, fontWeight = FontWeight.Bold))
            if (items.length() == 0) {
                Text("Nothing coming up", style = TextStyle(fontSize = 11.sp))
            } else {
                for (i in 0 until minOf(items.length(), 8)) {
                    val item = items.getJSONObject(i)
                    val kind = item.optString("kind")
                    val age = if (item.isNull("age")) null else item.optInt("age")
                    Text(
                        text = buildString {
                            append(item.optString("title"))
                            if (kind == "birthday" && age != null) append(" · $age")
                        },
                        style = TextStyle(fontSize = 11.sp),
                        modifier = androidx.glance.GlanceModifier.padding(top = 2.dp),
                    )
                }
            }
        }
    }
}

class KalAgendaWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget = KalAgendaWidget()
}
