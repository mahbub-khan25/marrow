package app.marrow.ui.settings

import android.app.Activity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.IntentSenderRequest
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowForward
import androidx.compose.material.icons.outlined.AccountCircle
import androidx.compose.material.icons.outlined.AddToDrive
import androidx.compose.material.icons.outlined.Cloud
import androidx.compose.material.icons.outlined.Inventory2
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.core.CloudAccount
import app.marrow.core.CloudKind
import app.marrow.core.CloudService
import app.marrow.core.SyncPlace
import app.marrow.data.CloudLinks
import app.marrow.data.GoogleTokens
import app.marrow.data.words
import app.marrow.ui.common.LocalEngine
import com.google.android.gms.auth.api.identity.Identity
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

fun CloudKind.icon(): ImageVector = when (this) {
    CloudKind.GOOGLE_DRIVE -> Icons.Outlined.AddToDrive
    CloudKind.ONE_DRIVE -> Icons.Outlined.Cloud
    CloudKind.DROPBOX -> Icons.Outlined.Inventory2
}

/**
 * Google Drive, OneDrive and Dropbox: sign in, pick the vault's folder,
 * start syncing. Shows nothing if this copy of Marrow can't sign in to any.
 */
@Composable
fun CloudChoices(onDone: () -> Unit) {
    val engine = LocalEngine.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    val services = remember { engine.marrow.cloudServices().filter { it.available } }
    var account by remember { mutableStateOf<CloudAccount?>(null) }
    var busy by remember { mutableStateOf<CloudKind?>(null) }
    /** The scheme the browser comes back to, while waiting for it. */
    var waitingFor by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    if (services.isEmpty()) return

    fun googleAccount() {
        busy = CloudKind.GOOGLE_DRIVE
        scope.launch {
            runCatching { engine.run { cloudPlatformAccount(CloudKind.GOOGLE_DRIVE) } }
                .onSuccess { account = it }
                .onFailure { error = it.words() }
            busy = null
        }
    }
    val consent = rememberLauncherForActivityResult(ActivityResultContracts.StartIntentSenderForResult()) { r ->
        if (r.resultCode == Activity.RESULT_OK) googleAccount() else {
            busy = null
            error = ctx.getString(R.string.cloud_cancelled)
        }
    }

    // The browser came back (OneDrive, Dropbox).
    LaunchedEffect(Unit) {
        CloudLinks.incoming.collect { url ->
            val scheme = waitingFor ?: return@collect
            if (!url.startsWith(scheme)) return@collect
            waitingFor = null
            CloudLinks.clear()
            runCatching { engine.run { cloudSignInFinish(url) } }
                .onSuccess { account = it }
                .onFailure { error = it.words() }
            busy = null
        }
    }

    fun signIn(s: CloudService) {
        error = null
        account = null
        busy = s.kind
        if (s.signsInOnDevice) {
            Identity.getAuthorizationClient(ctx).authorize(GoogleTokens.request())
                .addOnSuccessListener { r ->
                    val pending = r.pendingIntent
                    if (r.hasResolution() && pending != null) consent.launch(IntentSenderRequest.Builder(pending.intentSender).build()) else googleAccount()
                }
                .addOnFailureListener {
                    busy = null
                    error = ctx.getString(R.string.cloud_no_google)
                }
            return
        }
        runCatching { engine.marrow.cloudSignInStart(s.kind) }
            .onSuccess {
                CloudLinks.clear()
                waitingFor = it.callbackScheme
                CloudLinks.open(ctx, it.url)
            }
            .onFailure {
                busy = null
                error = it.words()
            }
    }

    val acc = account
    val service = services.firstOrNull { it.kind == acc?.kind }
    if (acc != null && service != null) {
        CloudSetup(service, acc, onDone = onDone, onAnother = { signIn(service) })
        return
    }
    Column(Modifier.padding(horizontal = 16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        for (s in services) {
            Surface(
                shape = RoundedCornerShape(18.dp),
                color = MaterialTheme.colorScheme.surfaceContainerLow,
                modifier = Modifier.fillMaxWidth().clickable(enabled = busy == null) { signIn(s) },
            ) {
                Row(Modifier.padding(horizontal = 18.dp, vertical = 16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    Icon(s.kind.icon(), null, Modifier.size(28.dp), tint = MaterialTheme.colorScheme.primary)
                    Column(Modifier.weight(1f)) {
                        Text(stringResource(R.string.cloud_sign_in_to, s.name), style = MaterialTheme.typography.titleMedium)
                        Text(stringResource(R.string.cloud_goes_in, s.place), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    if (busy == s.kind) CircularProgressIndicator(Modifier.size(22.dp), strokeWidth = 2.dp)
                    else Icon(Icons.AutoMirrored.Outlined.ArrowForward, null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
        if (waitingFor != null) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(stringResource(R.string.cloud_signing_in), Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
                TextButton(onClick = { waitingFor = null; busy = null }) { Text(stringResource(R.string.cancel)) }
            }
        }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
    }
}

/** Not a possible folder name (Marrow takes * out of names). */
private const val NEW = "*new*"

/** After signing in: which folder, encryption, and start. */
@Composable
private fun CloudSetup(service: CloudService, account: CloudAccount, onDone: () -> Unit, onAnother: () -> Unit) {
    val engine = LocalEngine.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var choice by remember { mutableStateOf(if (account.suggested in account.folders) account.suggested else NEW) }
    var newName by remember { mutableStateOf(account.suggested) }
    val folder = if (choice == NEW) newName.trim() else choice
    var place by remember { mutableStateOf<SyncPlace?>(null) }
    var encrypt by remember { mutableStateOf(false) }
    var pass by remember { mutableStateOf("") }
    var pass2 by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val joining = place?.encrypted == true
    // Encrypted sync needs an empty folder (or one already encrypted).
    val canEncrypt = place == null || joining || place?.files == 0u
    val passOk = !(encrypt || joining) || (pass.length >= 8 && (joining || pass == pass2))

    LaunchedEffect(folder) {
        place = null
        error = null
        if (folder.isEmpty()) return@LaunchedEffect
        delay(500)
        runCatching { engine.run { checkCloud(account.kind, account.account, folder) } }
            .onSuccess {
                place = it
                if (it.encrypted) encrypt = true else if (it.files > 0u) encrypt = false
            }
            .onFailure { error = it.words() }
    }

    FormColumn {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Icon(Icons.Outlined.AccountCircle, null, tint = MaterialTheme.colorScheme.primary)
            Column(Modifier.weight(1f)) {
                Text(service.name, style = MaterialTheme.typography.titleMedium)
                Text(stringResource(R.string.cloud_signed_in_as, account.account), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            TextButton(onClick = onAnother) { Text(stringResource(R.string.cloud_use_another)) }
        }
        Text(stringResource(R.string.cloud_private, service.place, service.name), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(stringResource(R.string.cloud_vault_folder), style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold)
        Column {
            for (f in account.folders + NEW) {
                Row(
                    Modifier.fillMaxWidth().selectable(selected = choice == f, role = Role.RadioButton) { choice = f }.padding(vertical = 4.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    RadioButton(selected = choice == f, onClick = null)
                    Text(if (f == NEW) stringResource(R.string.cloud_new_folder) else f, Modifier.padding(start = 12.dp), style = MaterialTheme.typography.bodyLarge)
                }
            }
        }
        if (choice == NEW) {
            OutlinedTextField(newName, { newName = it }, label = { Text(stringResource(R.string.cloud_folder_name)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
        }
        place?.let { p ->
            Text(
                when {
                    p.encrypted -> stringResource(R.string.passphrase_existing)
                    p.files == 0u -> stringResource(R.string.cloud_folder_new_empty)
                    else -> stringResource(R.string.place_has_files, p.files.toInt())
                },
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.primary,
            )
        }
        if (canEncrypt) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(stringResource(R.string.encrypt_end_to_end), style = MaterialTheme.typography.titleMedium)
                    Text(stringResource(R.string.encrypt_explain), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                Switch(encrypt || joining, { encrypt = it }, enabled = !joining)
            }
        }
        if (encrypt || joining) {
            OutlinedTextField(pass, { pass = it }, label = { Text(stringResource(R.string.sync_passphrase)) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), modifier = Modifier.fillMaxWidth(), supportingText = { Text(stringResource(if (joining) R.string.passphrase_existing else R.string.passphrase_new)) })
            if (!joining) {
                OutlinedTextField(pass2, { pass2 = it }, label = { Text(stringResource(R.string.passphrase_again)) }, singleLine = true, visualTransformation = PasswordVisualTransformation(), isError = pass2.isNotEmpty() && pass2 != pass, modifier = Modifier.fillMaxWidth())
            }
        }
        error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium) }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            Button(enabled = !busy && folder.isNotEmpty() && passOk, onClick = {
                busy = true
                error = null
                scope.launch {
                    runCatching { engine.run { syncWithCloud(account.kind, account.account, folder, if (encrypt || joining) pass else null) } }
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

/** The WebDAV form, folded away under its own heading. */
@Composable
fun OwnServer(onDone: () -> Unit, open: Boolean = false) {
    var shown by remember { mutableStateOf(open) }
    Surface(
        shape = RoundedCornerShape(18.dp),
        color = MaterialTheme.colorScheme.surfaceContainerLow,
        modifier = Modifier.padding(horizontal = 16.dp).fillMaxWidth().clickable { shown = !shown },
    ) {
        Row(Modifier.padding(horizontal = 18.dp, vertical = 16.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            Icon(Icons.Outlined.Cloud, null, Modifier.size(28.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
            Column(Modifier.weight(1f)) {
                Text(stringResource(R.string.sync_own_server), style = MaterialTheme.typography.titleMedium)
                Text(stringResource(R.string.sync_own_server_text), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
    if (shown) {
        Column(Modifier.padding(top = 12.dp)) { WebDavForm(onDone) }
    }
}
