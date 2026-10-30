package app.marrow.ui.today

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.speech.RecognizerIntent
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.PickVisualMediaRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Article
import androidx.compose.material.icons.outlined.CheckBox
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Mic
import androidx.compose.material.icons.outlined.PhotoLibrary
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.data.words
import app.marrow.ui.Incoming
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.nowTime
import app.marrow.ui.common.today
import app.marrow.ui.note.ATTACHMENTS
import app.marrow.ui.theme.LocalReading
import coil3.compose.AsyncImage
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Jot something down: it goes into today's note, under Log (or Tasks). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CaptureSheet(shared: Incoming.Shared?, onDismiss: () -> Unit, onSaved: (String) -> Unit) {
    val engine = LocalEngine.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var text by remember { mutableStateOf(shared?.text.orEmpty()) }
    var asTask by remember { mutableStateOf(false) }
    // A shared link: save the page itself (in Clippings), not just the link.
    val link = remember { shared?.text?.let { app.marrow.core.linkIn(it) } }
    var savePage by remember { mutableStateOf(link != null) }
    var saving by remember { mutableStateOf(false) }
    val images = remember { mutableStateListOf<Uri>().apply { addAll(shared?.images.orEmpty()) } }
    val focus = remember { FocusRequester() }
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.PickMultipleVisualMedia(6)) { images.addAll(it) }
    val dictate = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        val said = r.data?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)?.firstOrNull()
        if (r.resultCode == Activity.RESULT_OK && !said.isNullOrBlank()) text = (text.trimEnd() + " " + said).trim()
    }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }

    fun save() {
        saving = true
        scope.launch {
            try {
                if (link != null && savePage) {
                    val path = engine.run { clipPage(link, null, null, null, today()) }
                    onSaved(ctx.getString(R.string.page_saved, path.substringAfterLast('/').removeSuffix(".md")))
                    return@launch
                }
                val embeds = withContext(Dispatchers.IO) {
                    images.mapIndexed { i, uri ->
                        val bytes = ctx.contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: return@mapIndexed null
                        val ext = (ctx.contentResolver.getType(uri) ?: "image/jpeg").substringAfter('/').replace("jpeg", "jpg").substringBefore('+')
                        val name = "Photo ${today()} ${nowTime().replace(":", "")}${if (images.size > 1) "-${i + 1}" else ""}.$ext"
                        "![[" + engine.marrow.saveAttachment(ATTACHMENTS, name, bytes).substringAfterLast('/') + "]]"
                    }.filterNotNull()
                }
                val words = text.ifBlank { if (embeds.isNotEmpty()) ctx.getString(R.string.photo) else "" }
                if (words.isBlank()) {
                    onDismiss()
                    return@launch
                }
                engine.run { capture(today(), nowTime(), words, asTask, embeds) }
                onSaved(ctx.getString(if (asTask) R.string.added_to_tasks else R.string.added_to_today))
            } catch (e: Exception) {
                saving = false
                engine.say(e.words())
            }
        }
    }

    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp).padding(bottom = 16.dp).imePadding(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(stringResource(R.string.quick_note), style = MaterialTheme.typography.titleLarge)
            TextField(
                text,
                { text = it },
                placeholder = { Text(stringResource(if (asTask) R.string.what_needs_doing else R.string.whats_on_your_mind)) },
                textStyle = LocalReading.current.body,
                colors = TextFieldDefaults.colors(focusedContainerColor = Color.Transparent, unfocusedContainerColor = Color.Transparent, focusedIndicatorColor = Color.Transparent, unfocusedIndicatorColor = Color.Transparent),
                modifier = Modifier.fillMaxWidth().heightIn(min = 120.dp).focusRequester(focus),
            )
            if (images.isNotEmpty()) {
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    images.forEach { uri ->
                        androidx.compose.foundation.layout.Box {
                            AsyncImage(uri, null, contentScale = ContentScale.Crop, modifier = Modifier.size(84.dp).clip(RoundedCornerShape(12.dp)))
                            IconButton(onClick = { images.remove(uri) }, modifier = Modifier.align(Alignment.TopEnd).size(28.dp)) {
                                Icon(Icons.Outlined.Close, stringResource(R.string.remove), tint = Color.White)
                            }
                        }
                    }
                }
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                if (link != null) {
                    FilterChip(savePage, { savePage = !savePage }, label = { Text(stringResource(R.string.save_the_page)) }, leadingIcon = { Icon(Icons.Outlined.Article, null, Modifier.size(18.dp)) })
                }
                if (link == null || !savePage) {
                    FilterChip(asTask, { asTask = !asTask }, label = { Text(stringResource(R.string.as_a_task)) }, leadingIcon = { Icon(Icons.Outlined.CheckBox, null, Modifier.size(18.dp)) })
                }
                IconButton(onClick = { pick.launch(PickVisualMediaRequest(ActivityResultContracts.PickVisualMedia.ImageOnly)) }) { Icon(Icons.Outlined.PhotoLibrary, stringResource(R.string.add_photo)) }
                IconButton(onClick = {
                    val i = Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                    runCatching { dictate.launch(i) }.onFailure { engine.say(ctx.getString(R.string.no_dictation)) }
                }) { Icon(Icons.Outlined.Mic, stringResource(R.string.dictate)) }
                Spacer(Modifier.weight(1f))
                Button(onClick = ::save, enabled = !saving && (text.isNotBlank() || images.isNotEmpty())) {
                    Text(stringResource(if (saving && link != null && savePage) R.string.saving_the_page else R.string.save))
                }
            }
        }
    }
}
