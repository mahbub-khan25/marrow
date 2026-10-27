package app.marrow.ui.settings

import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.AutoAwesome
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import app.marrow.R
import app.marrow.core.AiPreset
import app.marrow.core.AiProvider
import app.marrow.data.words
import app.marrow.ui.common.BackBar
import app.marrow.ui.common.Empty
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.SectionTitle
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AiSettingsScreen() {
    val engine = LocalEngine.current
    val scope = rememberCoroutineScope()
    var cfg by remember { mutableStateOf(engine.marrow.aiConfig()) }
    var picking by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf<AiProvider?>(null) }
    val presets = remember { engine.marrow.aiPresets() }

    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { BackBar(stringResource(R.string.ai)) }) { pad ->
        Box(Modifier.padding(pad).fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            LazyColumn(Modifier.widthIn(max = 640.dp).fillMaxWidth()) {
                item { Explain(stringResource(R.string.ai_explain)) }
                if (cfg.providers.isEmpty()) {
                    item { Empty(Icons.Outlined.AutoAwesome, stringResource(R.string.no_ai_yet), stringResource(R.string.no_ai_yet_text)) }
                } else {
                    item { SectionTitle(stringResource(R.string.your_ais)) }
                }
                items(cfg.providers, key = { it.id }) { p ->
                    ListItem(
                        headlineContent = { Text(p.name) },
                        supportingContent = { Text(listOf(p.model.ifEmpty { stringResource(R.string.no_model) }, if (!p.hasKey && presets.firstOrNull { it.id == p.preset }?.needsKey == true) stringResource(R.string.no_key) else null).filterNotNull().joinToString(" · ")) },
                        leadingContent = {
                            RadioButton(cfg.defaultId == p.id, onClick = { scope.launch { cfg = engine.run { aiSetDefault(p.id) } } })
                        },
                        trailingContent = {
                            IconButton(onClick = { scope.launch { cfg = engine.run { aiRemoveProvider(p.id) } } }) { Icon(Icons.Outlined.Delete, stringResource(R.string.remove)) }
                        },
                        modifier = Modifier.clickable { editing = p },
                    )
                }
                item {
                    FilledTonalButton(onClick = { picking = true }, modifier = Modifier.padding(20.dp)) {
                        Icon(Icons.Outlined.Add, null)
                        Text("  " + stringResource(R.string.add_an_ai))
                    }
                }
            }
        }
    }
    if (picking) {
        AlertDialog(
            onDismissRequest = { picking = false },
            title = { Text(stringResource(R.string.add_an_ai)) },
            text = {
                LazyColumn(Modifier.heightIn(max = 460.dp)) {
                    presets.groupBy { it.group }.forEach { (group, list) ->
                        item { Text(group, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(top = 12.dp, bottom = 4.dp)) }
                        items(list, key = { it.id }) { pr ->
                            Text(pr.name, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.fillMaxWidth().clickable {
                                picking = false
                                editing = AiProvider("", pr.id, if (pr.id == "custom") "" else pr.name, pr.protocol, pr.baseUrl, "", false)
                            }.padding(vertical = 10.dp))
                        }
                    }
                }
            },
            confirmButton = { TextButton(onClick = { picking = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
    editing?.let { p ->
        ProviderDialog(p, presets.firstOrNull { it.id == p.preset }, onDismiss = { editing = null }) { saved, key ->
            scope.launch {
                runCatching { engine.run { aiSaveProvider(saved, key) } }
                    .onSuccess { cfg = it; editing = null }
                    .onFailure { engine.say(it.words()) }
            }
        }
    }
}

@Composable
private fun ProviderDialog(start: AiProvider, preset: AiPreset?, onDismiss: () -> Unit, onSave: (AiProvider, String?) -> Unit) {
    val engine = LocalEngine.current
    val ctx = LocalContext.current
    val scope = rememberCoroutineScope()
    var name by remember { mutableStateOf(start.name) }
    var base by remember { mutableStateOf(start.baseUrl) }
    var key by remember { mutableStateOf("") }
    var model by remember { mutableStateOf(start.model) }
    var models by remember { mutableStateOf<List<String>>(emptyList()) }
    var modelMenu by remember { mutableStateOf(false) }
    var busy by remember { mutableStateOf(false) }
    var result by remember { mutableStateOf<String?>(null) }
    fun current() = start.copy(name = name.trim(), baseUrl = base.trim(), model = model.trim())

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(preset?.name ?: start.name) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                preset?.hint?.takeIf { it.isNotEmpty() }?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                OutlinedTextField(name, { name = it }, label = { Text(stringResource(R.string.name)) }, singleLine = true, modifier = Modifier.fillMaxWidth())
                OutlinedTextField(base, { base = it }, label = { Text(stringResource(R.string.address)) }, placeholder = { Text("https://…/v1") }, singleLine = true, keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri), modifier = Modifier.fillMaxWidth())
                if (preset?.needsKey != false) {
                    OutlinedTextField(
                        key,
                        { key = it },
                        label = { Text(stringResource(R.string.api_key)) },
                        placeholder = { if (start.hasKey) Text(stringResource(R.string.key_saved)) },
                        singleLine = true,
                        visualTransformation = PasswordVisualTransformation(),
                        modifier = Modifier.fillMaxWidth(),
                    )
                    if (!preset?.keyUrl.isNullOrEmpty()) {
                        TextButton(onClick = { runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(preset!!.keyUrl))) } }) { Text(stringResource(R.string.get_a_key)) }
                    }
                }
                Box {
                    OutlinedTextField(model, { model = it }, label = { Text(stringResource(R.string.model)) }, singleLine = true, modifier = Modifier.fillMaxWidth(), trailingIcon = {
                        TextButton(enabled = !busy && base.isNotBlank(), onClick = {
                            busy = true
                            scope.launch {
                                runCatching { engine.run { aiModels(current(), key.ifBlank { null }) } }
                                    .onSuccess { models = it; modelMenu = it.isNotEmpty() }
                                    .onFailure { result = it.words() }
                                busy = false
                            }
                        }) { Text(stringResource(R.string.list)) }
                    })
                    DropdownMenu(modelMenu, { modelMenu = false }, modifier = Modifier.heightIn(max = 320.dp)) {
                        models.forEach { m -> DropdownMenuItem(text = { Text(m) }, onClick = { model = m; modelMenu = false }) }
                    }
                }
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(enabled = !busy && model.isNotBlank(), onClick = {
                        busy = true
                        result = null
                        scope.launch {
                            result = runCatching { ctx.getString(R.string.ai_says, engine.run { aiTest(current(), key.ifBlank { null }) }) }.getOrElse { it.words() }
                            busy = false
                        }
                    }) { Text(stringResource(R.string.test)) }
                    if (busy) CircularProgressIndicator(Modifier.size(22.dp))
                }
                result?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
            }
        },
        confirmButton = { Button(onClick = { onSave(current(), key.ifBlank { null }) }, enabled = name.isNotBlank() && base.isNotBlank()) { Text(stringResource(R.string.save)) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}
