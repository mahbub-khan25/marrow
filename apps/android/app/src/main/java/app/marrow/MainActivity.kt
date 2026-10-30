package app.marrow

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.appcompat.app.AppCompatActivity
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.marrow.ui.App
import app.marrow.ui.Incoming
import app.marrow.ui.theme.MarrowTheme
import kotlinx.coroutines.flow.MutableStateFlow

class MainActivity : AppCompatActivity() {
    /** Something shared into Marrow, or a shortcut, waiting for the app to handle it. */
    private val incoming = MutableStateFlow<Incoming?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val engine = (application as MarrowApp).engine
        incoming.value = read(intent)
        setContent {
            val look by engine.prefs.look.collectAsStateWithLifecycle()
            val pending by incoming.collectAsStateWithLifecycle()
            MarrowTheme(look) {
                App(engine, pending, onHandled = { incoming.value = null })
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        read(intent)?.let { incoming.value = it }
    }

    private fun read(intent: Intent?): Incoming? = when (intent?.action) {
        "app.marrow.CAPTURE" -> Incoming.Capture
        "app.marrow.TODAY" -> Incoming.Today
        // A reminder's notification was tapped.
        "app.marrow.OPEN_NOTE" -> intent.getStringExtra("path")?.let { Incoming.Note(it, intent.getIntExtra("line", -1)) }
        Intent.ACTION_SEND -> {
            val text = listOfNotNull(intent.getStringExtra(Intent.EXTRA_SUBJECT), intent.getStringExtra(Intent.EXTRA_TEXT)).joinToString("\n").trim()
            val image = stream(intent)
            Incoming.Shared(text, listOfNotNull(image))
        }
        Intent.ACTION_SEND_MULTIPLE -> {
            @Suppress("DEPRECATION")
            val list: List<Uri> = if (Build.VERSION.SDK_INT >= 33) {
                intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java) ?: emptyList()
            } else {
                intent.getParcelableArrayListExtra<Uri>(Intent.EXTRA_STREAM) ?: emptyList()
            }
            Incoming.Shared(intent.getStringExtra(Intent.EXTRA_TEXT).orEmpty(), list)
        }
        else -> null
    }

    @Suppress("DEPRECATION")
    private fun stream(intent: Intent): Uri? = if (Build.VERSION.SDK_INT >= 33) {
        intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java)
    } else {
        intent.getParcelableExtra(Intent.EXTRA_STREAM)
    }
}
