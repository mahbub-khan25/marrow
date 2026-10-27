package app.marrow.ui.files

import android.content.Intent
import android.webkit.MimeTypeMap
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.OpenInNew
import androidx.compose.material.icons.outlined.Share
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.core.content.FileProvider
import app.marrow.R
import app.marrow.ui.common.BackBar
import app.marrow.ui.common.Empty
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.iconFor
import app.marrow.ui.common.nameOf
import coil3.compose.AsyncImage
import java.io.File

/** An image (pinch to zoom), or any other file with a way to open it in another app. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FileScreen(path: String) {
    val engine = LocalEngine.current
    val ctx = LocalContext.current
    val file = remember(path) { runCatching { File(engine.marrow.filePath(path)) }.getOrNull() }
    val kind = remember(path) { runCatching { engine.marrow.fileItem(path).kind }.getOrNull() }
    var scale by remember { mutableFloatStateOf(1f) }
    var pan by remember { mutableStateOf(Offset.Zero) }

    fun intent(action: String): Intent? {
        val f = file ?: return null
        val uri = runCatching { FileProvider.getUriForFile(ctx, ctx.packageName + ".files", f) }.getOrNull() ?: return null
        val type = MimeTypeMap.getSingleton().getMimeTypeFromExtension(f.extension.lowercase()) ?: "*/*"
        return if (action == Intent.ACTION_SEND) {
            Intent(action).setType(type).putExtra(Intent.EXTRA_STREAM, uri).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        } else {
            Intent(action).setDataAndType(uri, type).addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
    }

    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = {
        BackBar(nameOf(path)) {
            IconButton(onClick = { intent(Intent.ACTION_SEND)?.let { ctx.startActivity(Intent.createChooser(it, null)) } }) { Icon(Icons.Outlined.Share, stringResource(R.string.share)) }
        }
    }) { pad ->
        if (kind == app.marrow.core.FileKind.IMAGE && file != null) {
            Box(
                Modifier.padding(pad).fillMaxSize().pointerInput(Unit) {
                    detectTransformGestures { _, p, zoom, _ ->
                        scale = (scale * zoom).coerceIn(1f, 6f)
                        pan = if (scale == 1f) Offset.Zero else pan + p
                    }
                },
                contentAlignment = Alignment.Center,
            ) {
                AsyncImage(file, nameOf(path), contentScale = ContentScale.Fit, modifier = Modifier.fillMaxSize().graphicsLayer {
                    scaleX = scale
                    scaleY = scale
                    translationX = pan.x
                    translationY = pan.y
                })
            }
        } else {
            Box(Modifier.padding(pad).fillMaxSize(), contentAlignment = Alignment.Center) {
                Empty(iconFor(kind ?: app.marrow.core.FileKind.OTHER), nameOf(path), stringResource(R.string.open_in_other_app_text)) {
                    Button(onClick = {
                        intent(Intent.ACTION_VIEW)?.let { runCatching { ctx.startActivity(it) }.onFailure { engine.say(ctx.getString(R.string.no_app_opens)) } }
                    }) {
                        Icon(Icons.AutoMirrored.Outlined.OpenInNew, null)
                        Text("  " + stringResource(R.string.open_in_other_app))
                    }
                }
            }
        }
    }
}
