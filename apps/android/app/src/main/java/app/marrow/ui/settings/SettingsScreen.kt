package app.marrow.ui.settings

import android.content.Intent
import android.net.Uri
import androidx.appcompat.app.AppCompatDelegate
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.selection.selectable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.CallMerge
import androidx.compose.material.icons.outlined.AutoAwesome
import androidx.compose.material.icons.outlined.Cloud
import androidx.compose.material.icons.outlined.FileDownload
import androidx.compose.material.icons.outlined.Info
import androidx.compose.material.icons.outlined.Language
import androidx.compose.material.icons.outlined.Lock
import androidx.compose.material.icons.outlined.Palette
import androidx.compose.material.icons.outlined.Timer
import androidx.compose.material.icons.outlined.Web
import androidx.compose.material.icons.outlined.Inventory2
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.core.os.LocaleListCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.marrow.R
import app.marrow.ui.AiRoute
import app.marrow.ui.AppearanceRoute
import app.marrow.ui.ConflictsRoute
import app.marrow.ui.ImportRoute
import app.marrow.ui.SyncRoute
import app.marrow.ui.VaultRoute
import app.marrow.ui.common.BackBar
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.SectionTitle

@Composable
fun SettingRow(icon: ImageVector?, title: String, subtitle: String? = null, onClick: (() -> Unit)? = null, trailing: (@Composable () -> Unit)? = null) {
    ListItem(
        headlineContent = { Text(title) },
        supportingContent = subtitle?.let { { Text(it) } },
        leadingContent = icon?.let { { Icon(it, null, tint = MaterialTheme.colorScheme.primary) } },
        trailingContent = trailing,
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.background),
        modifier = if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier,
    )
}

/** Picks one of a few choices. */
@Composable
fun ChoiceDialog(title: String, options: List<Pair<String, String>>, selected: String, onDismiss: () -> Unit, onPick: (String) -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column {
                options.forEach { (key, label) ->
                    androidx.compose.foundation.layout.Row(
                        Modifier.fillMaxWidth().selectable(key == selected, role = Role.RadioButton) { onPick(key) }.padding(vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RadioButton(key == selected, null)
                        Text(label, Modifier.padding(start = 14.dp), style = MaterialTheme.typography.bodyLarge)
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen() {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    val ctx = LocalContext.current
    val vault by engine.vault.collectAsStateWithLifecycle()
    val sync by engine.sync.collectAsStateWithLifecycle()
    val look by engine.prefs.look.collectAsStateWithLifecycle()
    var language by remember { mutableStateOf(false) }
    var lockAfter by remember { mutableStateOf(false) }
    val lang = AppCompatDelegate.getApplicationLocales().toLanguageTags()
    val ai = remember { engine.marrow.aiConfig() }

    Scaffold(containerColor = MaterialTheme.colorScheme.background, topBar = { BackBar(stringResource(R.string.settings)) }) { pad ->
        Box(Modifier.padding(pad).fillMaxSize(), contentAlignment = Alignment.TopCenter) {
            LazyColumn(Modifier.widthIn(max = 720.dp).fillMaxWidth(), contentPadding = PaddingValues(bottom = 40.dp)) {
                item { SectionTitle(stringResource(R.string.vault)) }
                item { SettingRow(Icons.Outlined.Inventory2, vault?.name ?: "", vault?.root, onClick = { nav.go(VaultRoute) }) }
                item {
                    SettingRow(
                        Icons.Outlined.Cloud,
                        stringResource(R.string.sync),
                        sync?.let { s -> s.label + (if (s.encrypted) " · " + ctx.getString(R.string.encrypted) else "") } ?: stringResource(R.string.sync_off),
                        onClick = { nav.go(SyncRoute) },
                    )
                }
                item { SettingRow(Icons.Outlined.FileDownload, stringResource(R.string.import_notes), stringResource(R.string.import_subtitle), onClick = { nav.go(ImportRoute) }) }
                item { SettingRow(Icons.AutoMirrored.Outlined.CallMerge, stringResource(R.string.conflicts), stringResource(R.string.conflicts_subtitle), onClick = { nav.go(ConflictsRoute) }) }

                item { SectionTitle(stringResource(R.string.app)) }
                item { SettingRow(Icons.Outlined.Palette, stringResource(R.string.appearance), stringResource(R.string.appearance_subtitle), onClick = { nav.go(AppearanceRoute) }) }
                item {
                    SettingRow(
                        Icons.Outlined.Language,
                        stringResource(R.string.language),
                        when {
                            lang.startsWith("bn") -> "বাংলা"
                            lang.startsWith("en") -> "English"
                            else -> stringResource(R.string.same_as_phone)
                        },
                        onClick = { language = true },
                    )
                }
                item {
                    SettingRow(
                        Icons.Outlined.AutoAwesome,
                        stringResource(R.string.ai),
                        if (ai.providers.isEmpty()) stringResource(R.string.ai_off) else ai.providers.joinToString { it.name },
                        onClick = { nav.go(AiRoute) },
                    )
                }

                item { SectionTitle(stringResource(R.string.locked_notes)) }
                item {
                    SettingRow(Icons.Outlined.Timer, stringResource(R.string.forget_passwords), lockLabel(look.lockAfter), onClick = { lockAfter = true })
                }
                item {
                    SettingRow(Icons.Outlined.Lock, stringResource(R.string.lock_all_now), stringResource(R.string.lock_all_now_subtitle), onClick = {
                        engine.marrow.lockAll()
                        engine.say(ctx.getString(R.string.all_locked))
                    })
                }

                item { SectionTitle(stringResource(R.string.about)) }
                item { SettingRow(Icons.Outlined.Info, "Marrow ${engine.marrow.version()}", stringResource(R.string.about_text)) }
                item {
                    SettingRow(Icons.Outlined.Web, stringResource(R.string.website), "github.com/mahbub-khan25/marrow", onClick = {
                        runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/mahbub-khan25/marrow"))) }
                    })
                }
            }
        }
    }

    if (language) {
        ChoiceDialog(
            stringResource(R.string.language),
            listOf("" to stringResource(R.string.same_as_phone), "en" to "English", "bn" to "বাংলা"),
            when {
                lang.startsWith("bn") -> "bn"
                lang.startsWith("en") -> "en"
                else -> ""
            },
            onDismiss = { language = false },
        ) {
            language = false
            AppCompatDelegate.setApplicationLocales(if (it.isEmpty()) LocaleListCompat.getEmptyLocaleList() else LocaleListCompat.forLanguageTags(it))
        }
    }
    if (lockAfter) {
        ChoiceDialog(
            stringResource(R.string.forget_passwords),
            listOf(0, 1, 5, 15, 60).map { it.toString() to lockLabel(it) },
            look.lockAfter.toString(),
            onDismiss = { lockAfter = false },
        ) { v ->
            lockAfter = false
            engine.prefs.update { it.copy(lockAfter = v.toInt()) }
        }
    }
}

@Composable
private fun lockLabel(minutes: Int): String = when (minutes) {
    0 -> stringResource(R.string.lock_when_leaving)
    60 -> stringResource(R.string.lock_after_hour)
    else -> stringResource(R.string.lock_after_minutes, minutes)
}

@Composable
fun Explain(text: String) {
    Text(text, Modifier.padding(horizontal = 20.dp, vertical = 8.dp), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

@Composable
fun FormColumn(content: @Composable () -> Unit) {
    Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) { content() }
}
