package app.marrow.ui.search

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material.icons.outlined.SearchOff
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SuggestionChip
import androidx.compose.material3.SuggestionChipDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.core.FileHit
import app.marrow.core.SearchLine
import app.marrow.core.SearchResults
import app.marrow.ui.common.Empty
import app.marrow.ui.common.LocalEngine
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.SectionTitle
import app.marrow.ui.common.load
import app.marrow.ui.theme.LocalNoteColors
import kotlinx.coroutines.delay

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SearchScreen(initial: String) {
    val engine = LocalEngine.current
    val nav = LocalNav.current
    var query by remember { mutableStateOf(initial) }
    var results by remember { mutableStateOf<SearchResults?>(null) }
    var names by remember { mutableStateOf<List<FileHit>>(emptyList()) }
    var recent by remember { mutableStateOf(engine.prefs.recentSearches) }
    val focus = remember { FocusRequester() }
    val tags = load { run { tags() } }.value.orEmpty()

    LaunchedEffect(query) {
        val q = query.trim()
        if (q.isEmpty() || q == "#") {
            results = null
            names = emptyList()
            return@LaunchedEffect
        }
        delay(180)
        names = runCatching { engine.run { findFiles(q, null, 5u) } }.getOrDefault(emptyList())
        results = runCatching { engine.run { search(q, 60u) } }.getOrNull()
    }
    LaunchedEffect(Unit) { if (initial.isEmpty()) runCatching { focus.requestFocus() } }

    fun remember(q: String) {
        val t = q.trim()
        if (t.length < 2) return
        recent = (listOf(t) + recent.filter { it != t }).take(8)
        engine.prefs.recentSearches = recent
    }

    Column(Modifier.fillMaxSize().statusBarsPadding(), horizontalAlignment = Alignment.CenterHorizontally) {
        TextField(
            query,
            { query = it },
            placeholder = { Text(stringResource(R.string.search_your_notes)) },
            leadingIcon = { Icon(Icons.Outlined.Search, null) },
            trailingIcon = {
                if (query.isNotEmpty()) IconButton(onClick = { query = "" }) { Icon(Icons.Outlined.Close, stringResource(R.string.clear)) }
            },
            singleLine = true,
            shape = RoundedCornerShape(28.dp),
            colors = TextFieldDefaults.colors(
                focusedIndicatorColor = Color.Transparent,
                unfocusedIndicatorColor = Color.Transparent,
                focusedContainerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
                unfocusedContainerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
            ),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
            keyboardActions = KeyboardActions(onSearch = { remember(query) }),
            modifier = Modifier.widthIn(max = 720.dp).fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp).focusRequester(focus),
        )
        val r = results
        LazyColumn(Modifier.widthIn(max = 720.dp).fillMaxSize(), contentPadding = PaddingValues(bottom = 40.dp)) {
            if (query.isBlank() || query.trim() == "#") {
                if (recent.isNotEmpty() && query.isBlank()) {
                    item { SectionTitle(stringResource(R.string.recent_searches)) }
                    items(recent) { q ->
                        Row(Modifier.fillMaxWidth().clickable { query = q }.padding(horizontal = 20.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Outlined.History, null, tint = MaterialTheme.colorScheme.outline)
                            Text(q, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.padding(start = 16.dp))
                        }
                    }
                }
                if (tags.isNotEmpty()) {
                    item { SectionTitle(stringResource(R.string.tags)) }
                    item {
                        FlowRow(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            tags.forEach { t ->
                                SuggestionChip(
                                    onClick = { nav.tag(t.tag) },
                                    label = { Text("${t.tag}  ${t.count}") },
                                    colors = SuggestionChipDefaults.suggestionChipColors(labelColor = LocalNoteColors.current.tag),
                                )
                            }
                        }
                    }
                }
                if (recent.isEmpty() && tags.isEmpty()) {
                    item { Empty(Icons.Outlined.Search, stringResource(R.string.search_your_notes), stringResource(R.string.search_hint)) }
                }
            } else {
                if (names.isNotEmpty()) {
                    item { SectionTitle(stringResource(R.string.notes_named)) }
                    items(names, key = { "n:" + it.path }) { h ->
                        Row(Modifier.fillMaxWidth().clickable { remember(query); nav.openPath(h.path) }.padding(horizontal = 20.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                            Icon(Icons.Outlined.Description, null, tint = MaterialTheme.colorScheme.primary)
                            Column(Modifier.padding(start = 16.dp)) {
                                Text(h.name, style = MaterialTheme.typography.titleMedium)
                                if (h.folder.isNotEmpty()) Text(h.folder, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.outline)
                            }
                        }
                    }
                }
                if (r != null) {
                    item {
                        SectionTitle(pluralStringResource(R.plurals.notes_found, r.total.toInt(), r.total.toInt()))
                    }
                    items(r.hits, key = { it.path }) { hit ->
                        Column(Modifier.fillMaxWidth().clickable { remember(query); nav.note(hit.path, hit.lines.firstOrNull()?.line?.toInt() ?: -1) }.padding(horizontal = 20.dp, vertical = 12.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text(hit.title, style = MaterialTheme.typography.titleMedium)
                            hit.lines.take(3).forEach { Line(it) }
                        }
                        HorizontalDivider(Modifier.padding(start = 20.dp), color = MaterialTheme.colorScheme.outlineVariant)
                    }
                    if (r.hits.isEmpty() && names.isEmpty()) {
                        item { Empty(Icons.Outlined.SearchOff, stringResource(R.string.nothing_found), stringResource(R.string.nothing_found_text)) }
                    }
                }
            }
        }
    }
}

/** A matching line with the matches marked. */
@Composable
private fun Line(l: SearchLine) {
    val notes = LocalNoteColors.current
    val text = buildAnnotatedString {
        append(l.text)
        l.ranges.chunked(2).forEach { (s, e) ->
            val (a, b) = s.toInt().coerceIn(0, l.text.length) to e.toInt().coerceIn(0, l.text.length)
            if (b > a) addStyle(SpanStyle(background = notes.highlight, fontWeight = FontWeight.SemiBold), a, b)
        }
    }
    Box { Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis) }
}
