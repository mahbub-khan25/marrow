package app.marrow.ui.graph

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Hub
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.marrow.R
import app.marrow.core.NodeKind
import app.marrow.ui.common.BackBar
import app.marrow.ui.common.Empty
import app.marrow.ui.common.LocalNav
import app.marrow.ui.common.load
import app.marrow.ui.common.nameOf
import kotlin.math.sqrt

/** How notes link to each other. `around`: only this note and its neighbours. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun GraphScreen(around: String) {
    val nav = LocalNav.current
    var tags by remember { mutableStateOf(false) }
    var missing by remember { mutableStateOf(false) }
    var depth by remember { mutableStateOf(1) }
    val graph = load(around, tags, missing, depth) { run { graph(tags, missing, false, around.ifEmpty { null }, depth.toUInt()) } }
    var scale by remember { mutableFloatStateOf(1f) }
    var pan by remember { mutableStateOf(Offset.Zero) }
    val colors = MaterialTheme.colorScheme
    val density = LocalDensity.current
    val labelPx = with(density) { 12.dp.toPx() }

    Scaffold(containerColor = colors.background, topBar = { BackBar(if (around.isEmpty()) stringResource(R.string.graph) else nameOf(around)) }) { pad ->
        Column(Modifier.padding(pad).fillMaxSize()) {
            Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(tags, { tags = !tags }, label = { Text(stringResource(R.string.tags)) })
                FilterChip(missing, { missing = !missing }, label = { Text(stringResource(R.string.missing_notes)) })
                if (around.isNotEmpty()) {
                    FilterChip(depth == 2, { depth = if (depth == 1) 2 else 1 }, label = { Text(stringResource(R.string.two_steps)) })
                }
            }
            val g = graph.value
            if (g == null) {
                LinearProgressIndicator(Modifier.padding(16.dp))
                return@Column
            }
            if (g.nodes.isEmpty()) {
                Empty(Icons.Outlined.Hub, stringResource(R.string.graph), stringResource(R.string.graph_empty))
                return@Column
            }
            var view by remember { mutableStateOf(androidx.compose.ui.unit.IntSize.Zero) }
            // Show the whole graph when it first appears.
            androidx.compose.runtime.LaunchedEffect(g, view) {
                if (view.width == 0 || g.nodes.isEmpty()) return@LaunchedEffect
                val minX = g.nodes.minOf { it.x }
                val maxX = g.nodes.maxOf { it.x }
                val minY = g.nodes.minOf { it.y }
                val maxY = g.nodes.maxOf { it.y }
                val pad = with(density) { 48.dp.toPx() }
                val s = minOf((view.width - 2 * pad) / (maxX - minX).coerceAtLeast(1f), (view.height - 2 * pad) / (maxY - minY).coerceAtLeast(1f)).coerceIn(0.15f, 3f)
                scale = s
                pan = Offset(-(minX + maxX) / 2 * s, -(minY + maxY) / 2 * s)
            }
            Box(Modifier.fillMaxSize().onSizeChanged { view = it }) {
                val nodes = g.nodes
                val few = nodes.size <= 80
                Canvas(
                    Modifier.fillMaxSize()
                        .pointerInput(g) {
                            detectTransformGestures { _, p, zoom, _ ->
                                scale = (scale * zoom).coerceIn(0.15f, 6f)
                                pan += p
                            }
                        }
                        .pointerInput(g, scale, pan) {
                            detectTapGestures { tap ->
                                val cx = size.width / 2f + pan.x
                                val cy = size.height / 2f + pan.y
                                val hit = nodes.minByOrNull { n ->
                                    val dx = cx + n.x * scale - tap.x
                                    val dy = cy + n.y * scale - tap.y
                                    dx * dx + dy * dy
                                }
                                if (hit != null) {
                                    val dx = cx + hit.x * scale - tap.x
                                    val dy = cy + hit.y * scale - tap.y
                                    if (sqrt(dx * dx + dy * dy) < 32.dp.toPx() && hit.kind == NodeKind.NOTE) nav.openPath(hit.id)
                                }
                            }
                        },
                ) {
                    val cx = size.width / 2f + pan.x
                    val cy = size.height / 2f + pan.y
                    fun at(i: Int) = Offset(cx + nodes[i].x * scale, cy + nodes[i].y * scale)
                    val edge = colors.outline.copy(alpha = 0.45f)
                    g.edges.forEach { e -> drawLine(edge, at(e.from.toInt()), at(e.to.toInt()), strokeWidth = 1.5.dp.toPx()) }
                    val paint = android.graphics.Paint().apply {
                        isAntiAlias = true
                        textSize = labelPx
                        color = colors.onSurface.toArgb()
                        textAlign = android.graphics.Paint.Align.CENTER
                    }
                    nodes.forEachIndexed { i, n ->
                        val r = (4f + sqrt(n.degree.toFloat()) * 2.2f).coerceAtMost(14f).dp.toPx() * sqrt(scale).coerceIn(0.7f, 1.4f)
                        val c = when {
                            n.id == around -> colors.tertiary
                            n.kind == NodeKind.TAG -> colors.secondary
                            n.kind == NodeKind.MISSING -> colors.outline
                            else -> colors.primary
                        }
                        drawCircle(c, r, at(i))
                        if (few || scale > 1.2f || n.degree >= 6u || n.id == around) {
                            drawIntoCanvas { it.nativeCanvas.drawText(n.name, at(i).x, at(i).y + r + labelPx * 1.1f, paint) }
                        }
                    }
                }
            }
        }
    }
}
