package app.marrow.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Cloud
import androidx.compose.material.icons.outlined.CloudDone
import androidx.compose.material.icons.outlined.CloudOff
import androidx.compose.material.icons.outlined.CloudSync
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.marrow.R
import app.marrow.core.SyncPlace
import app.marrow.core.SyncProblem
import app.marrow.data.words
import app.marrow.ui.common.BackBar
import app.marrow.ui.common.ConfirmDialog
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.ago
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SyncScreen() {
    val engine = LocalEngine.current
    val scope = rememberCoroutineScope()
    val sync by engine.sync.collectAsStateWithLifecycle()
    var stopping by remember { mutableStateOf(false) }
    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { BackBar(stringResource(R.string.sync)) }) { pad ->
        Box(Modifier.padding(pad).fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            Column(Modifier.widthIn(max = 640.dp).fillMaxWidth().verticalScroll(rememberScrollState()).padding(bottom = 40.dp)) {
                val s = sync
                if (s == null) {
                    Explain(stringResource(R.string.sync_explain))
                    CloudChoices(onDone = { engine.refreshSync() })
                    Spacer(Modifier.height(12.dp))
                    OwnServer(onDone = { engine.refreshSync() }, open = engine.marrow.cloudServices().none { it.available })
                } else {
                    Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.padding(16.dp).fillMaxWidth()) {
                        Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                                val bad = s.problem != null && s.problem != SyncProblem.OFFLINE
                                Icon(
                                    when {
                                        s.syncing -> Icons.Outlined.CloudSync
                                        bad -> Icons.Outlined.CloudOff
                                        s.lastSyncMs != null -> Icons.Outlined.CloudDone
                                        else -> Icons.Outlined.Cloud
                                    },
                                    null,
                                    Modifier.size(32.dp),
                                    tint = if (bad) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary,
                                )
                                Column {
                                    Text(s.label, style = MaterialTheme.typography.titleLarge)
                                    Text(s.location, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                }
                            }
                            if (s.encrypted) {
                                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                    Icon(Icons.Outlined.Lock, null, Modifier.size(16.dp), tint = MaterialTheme.colorScheme.primary)
                                    Text(stringResource(R.string.end_to_end_encrypted), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                                }
                            }
                            Text(
                                when {
                                    s.syncing -> stringResource(R.string.syncing)
                                    s.lastSyncMs != null -> stringResource(R.string.last_synced, ago(s.lastSyncMs!!))
                                    else -> stringResource(R.string.not_synced_yet)
                                },
                                style = MaterialTheme.typography.bodyMedium,
                            )
                            s.message?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = if (s.problem == SyncProblem.OFFLINE) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.error) }
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
                                FilledTonalButton(onClick = { engine.marrow.syncNow(false) }, enabled = !s.syncing) { Text(stringResource(R.string.sync_now)) }
                                if (s.problem == SyncProblem.NEEDS_CONFIRMING) {
                                    Button(onClick = { engine.marrow.syncNow(true) }) { Text(stringResource(R.string.sync_anyway)) }
                                }
                            }
                        }
                    }
                    Explain(stringResource(R.string.sync_how))
                    OutlinedButton(onClick = { stopping = true }, modifier = Modifier.padding(horizontal = 20.dp)) { Text(stringResource(R.string.turn_off_sync)) }
                }
            }
        }
    }
    if (stopping) ConfirmDialog(stringResource(R.string.turn_off_sync_q), stringResource(R.string.turn_off_sync_text), stringResource(R.string.turn_off), onDismiss = { stopping = false }) {
        stopping = false
        scope.launch {
            runCatching { engine.run { turnOffSync() } }.onFailure { engine.say(it.words()) }
            engine.refreshSync()
        }
    }
}

/** Sign in to a WebDAV server (Nextcloud, ownCloud, Synology…) and start syncing. */
@Composable
fun WebDavForm(onDone: () -> Unit) {
    val engine = LocalEngine.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var url by remember { mutableStateOf("") }
    var user by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var encrypt by remember { mutableStateOf(true) }
    var pass by remember { mutableStateOf("") }
    var pass2 by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var place by remember { mutableStateOf<SyncPlace?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val passOk = !encrypt || (pass.length >= 8 && (pass == pass2 || place?.encrypted == true))

    FormColumn {
        OutlinedTextField(url, { url = it; place = null }, label = { Text(stringResource(R.string.server_address)) }, placeholder = { Text("https://cloud.example.com/remote.php/dav/files/me/Marrow") }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri), modifier = Modifier.fillMaxWidth(), supportingText = { Text(stringResource(R.string.server_address_hint)) })
        OutlinedTextField(user, { user = it; place = null }, label = { Text(stringResource(R.string.user_name)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(password, { password = it; place = null }, label = { Text(stringResource(R.string.password)) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password), modifier = Modifier.fillMaxWidth(), supportingText = { Text(stringResource(R.string.app_password_hint)) })
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(stringResource(R.string.encrypt_end_to_end), style = MaterialTheme.typography.titleMedium)
                Text(stringResource(R.string.encrypt_explain), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Switch(encrypt || place?.encrypted == true, { encrypt = it }, enabled = place?.encrypted != true)
        }
        if (encrypt || place?.encrypted == true) {
            OutlinedTextField(pass, { pass = it }, label = { Text(stringResource(R.string.sync_passphrase)) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth(), supportingText = { Text(stringResource(if (place?.encrypted == true) R.string.passphrase_existing else R.string.passphrase_new)) })
            if (place?.encrypted != true) {
                OutlinedTextField(pass2, { pass2 = it }, label = { Text(stringResource(R.string.passphrase_again)) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), isError = pass2.isNotEmpty() && pass2 != pass, modifier = Modifier.fillMaxWidth())
            }
        }
        place?.let { p ->
            Text(
                if (p.files == 0u) stringResource(R.string.place_empty) else stringResource(R.string.place_has_files, p.files.toInt()),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.primary,
            )
        }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            OutlinedButton(enabled = !busy && url.isNotBlank(), onClick = {
                busy = true
                error = null
                scope.launch {
                    runCatching { engine.run { checkWebdav(url.trim(), user.trim(), password) } }
                        .onSuccess { place = it; if (it.encrypted) encrypt = true }
                        .onFailure { error = it.words() }
                    busy = false
                }
            }) { Text(stringResource(R.string.check)) }
            Button(enabled = !busy && url.isNotBlank() && passOk, onClick = {
                busy = true
                error = null
                scope.launch {
                    runCatching { engine.run { syncWithWebdav(url.trim(), user.trim(), password, if (encrypt || place?.encrypted == true) pass else null) } }
                        .onSuccess {
                            engine.say(ctx.getString(R.string.sync_started))
                            onDone()
                        }
                        .onFailure { error = it.words() }
                    busy = false
                }
            }) { Text(stringResource(R.string.start_syncing)) }
            if (busy) CircularProgressIndicator(Modifier.size(24.dp))
        }
    }
}
