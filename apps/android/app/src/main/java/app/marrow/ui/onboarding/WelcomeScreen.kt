package app.marrow.ui.onboarding

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.DocumentsContract
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowForward
import androidx.compose.material.icons.outlined.Cloud
import androidx.compose.material.icons.outlined.EditNote
import androidx.compose.material.icons.outlined.FolderOpen
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
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
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.data.words
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.theme.LocalReading
import kotlinx.coroutines.launch
import java.io.File

/** The first screen: start a vault here, use a folder on the phone, or sync one down. */
@Composable
fun WelcomeScreen() {
    val engine = LocalEngine.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var error by remember { mutableStateOf<String?>(null) }
    val base = remember { ctx.getExternalFilesDir(null) ?: ctx.filesDir }

    fun start(sync: Boolean) = scope.launch {
        try {
            val existing = File(base, "Marrow")
            val path = if (existing.isDirectory && (existing.list()?.isNotEmpty() == true)) existing.absolutePath
            else engine.run { createVault(base.absolutePath, "Marrow", !sync) }
            if (sync) engine.prefs.afterOpen = "sync"
            engine.open(path)
        } catch (e: Exception) {
            error = e.words()
        }
    }

    val pickFolder = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        val path = treePath(uri)
        if (path == null) {
            error = ctx.getString(R.string.folder_not_on_phone)
            return@rememberLauncherForActivityResult
        }
        scope.launch {
            runCatching { engine.open(path) }.onFailure { error = it.words() }
        }
    }
    val allFiles = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        if (Build.VERSION.SDK_INT >= 30 && Environment.isExternalStorageManager()) pickFolder.launch(null)
    }

    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(
            Modifier.widthIn(max = 520.dp).fillMaxWidth().verticalScroll(rememberScrollState()).statusBarsPadding().navigationBarsPadding().padding(24.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Image(painterResource(R.mipmap.ic_launcher_foreground), null, Modifier.size(132.dp).clip(RoundedCornerShape(32.dp)))
            Text(stringResource(R.string.app_name), style = MaterialTheme.typography.displaySmall)
            Spacer(Modifier.height(8.dp))
            Text(
                stringResource(R.string.welcome_tagline),
                style = LocalReading.current.body,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.Center,
            )
            Spacer(Modifier.height(32.dp))
            Option(Icons.Outlined.EditNote, stringResource(R.string.start_writing_title), stringResource(R.string.start_writing_text), primary = true) { start(false) }
            Option(Icons.Outlined.Cloud, stringResource(R.string.sync_from_server_title), stringResource(R.string.sync_from_server_text)) { start(true) }
            Option(Icons.Outlined.FolderOpen, stringResource(R.string.use_phone_folder_title), stringResource(R.string.use_phone_folder_text)) {
                if (Build.VERSION.SDK_INT >= 30 && !Environment.isExternalStorageManager()) {
                    allFiles.launch(Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION, Uri.parse("package:" + ctx.packageName)))
                } else {
                    pickFolder.launch(null)
                }
            }
            error?.let { Text(it, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(top = 12.dp), textAlign = TextAlign.Center) }
            Spacer(Modifier.height(16.dp))
            Text(stringResource(R.string.welcome_private), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.outline, textAlign = TextAlign.Center)
        }
    }
}

@Composable
private fun Option(icon: ImageVector, title: String, text: String, primary: Boolean = false, onClick: () -> Unit) {
    Surface(
        onClick = onClick,
        shape = RoundedCornerShape(20.dp),
        color = if (primary) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surfaceContainerHigh,
        modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp),
    ) {
        Row(Modifier.padding(18.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            val fg = if (primary) MaterialTheme.colorScheme.onPrimary else MaterialTheme.colorScheme.onSurface
            Icon(icon, null, tint = fg)
            Column(Modifier.weight(1f)) {
                Text(title, style = MaterialTheme.typography.titleMedium, color = fg)
                Text(text, style = MaterialTheme.typography.bodySmall, color = fg.copy(alpha = 0.8f))
            }
            Icon(Icons.AutoMirrored.Outlined.ArrowForward, null, tint = fg)
        }
    }
}

/** A folder picked in the system picker, as a path the engine can use (phone storage only). */
private fun treePath(uri: Uri): String? {
    val id = runCatching { DocumentsContract.getTreeDocumentId(uri) }.getOrNull() ?: return null
    val (volume, rel) = id.split(':', limit = 2).let { it[0] to it.getOrElse(1) { "" } }
    val root = if (volume == "primary") Environment.getExternalStorageDirectory().absolutePath else "/storage/$volume"
    val dir = File(root, rel)
    return if (dir.isDirectory) dir.absolutePath else null
}
