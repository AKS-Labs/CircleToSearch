package dev.davidv.translator.ui.screens

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

/** Marshmallow-scallop outline: [lobes] rounded petals between an outer (peak) and
 *  inner (valley) radius, connected with tangent cubic Béziers — smooth, never pointed. */
class ScallopShape(
    private val lobes: Int = 10,
    private val depthRatio: Float = 0.065f,
) : Shape {
    override fun createOutline(
        size: androidx.compose.ui.geometry.Size,
        layoutDirection: LayoutDirection,
        density: Density,
    ): Outline {
        val path = Path()
        val cx = size.width / 2f
        val cy = size.height / 2f
        val rBase = minOf(cx, cy) * 0.94f
        val rOuter = rBase
        val rInner = rBase * (1f - depthRatio)

        val totalPoints = lobes * 2
        val angleStep = ((2.0 * PI) / totalPoints).toFloat()

        for (i in 0 until totalPoints) {
            val a0 = i * angleStep
            val a1 = (i + 1) * angleStep

            val isPeak0 = i % 2 == 0
            val r0 = if (isPeak0) rOuter else rInner
            val r1 = if (isPeak0) rInner else rOuter

            val x0 = cx + r0 * cos(a0)
            val y0 = cy + r0 * sin(a0)
            val x1 = cx + r1 * cos(a1)
            val y1 = cy + r1 * sin(a1)

            if (i == 0) {
                path.moveTo(x0, y0)
            }

            val handleLen = ((r0 + r1) / 2f) * angleStep * 0.55f
            val cp1x = x0 - handleLen * sin(a0)
            val cp1y = y0 + handleLen * cos(a0)
            val cp2x = x1 + handleLen * sin(a1)
            val cp2y = y1 - handleLen * cos(a1)

            path.cubicTo(cp1x, cp1y, cp2x, cp2y, x1, y1)
        }
        path.close()
        return Outline.Generic(path)
    }
}

/** Dual-layer scalloped shutter: a translucent outer scallop with a solid white border,
 *  over a solid white inner scallop — over the dark preview the translucent ring reads
 *  as the grey rim of the badge. Dims and disables while [isCapturing]. */
@Composable
fun ScallopedCaptureButton(
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    isCapturing: Boolean = false,
) {
    val interactionSource = remember { MutableInteractionSource() }
    val isPressed by interactionSource.collectIsPressedAsState()
    val scale by animateFloatAsState(
        targetValue = if (isPressed) 0.92f else 1.0f,
        label = "shutter",
    )
    val shutterShape = remember { ScallopShape(lobes = 10, depthRatio = 0.065f) }

    Box(
        modifier = modifier
            .size(94.dp)
            .scale(scale)
            .clip(shutterShape)
            .background(Color.White.copy(alpha = 0.4f))
            .border(3.dp, Color.White, shutterShape)
            .padding(5.dp)
            .clickable(
                interactionSource = interactionSource,
                indication = null,
                enabled = !isCapturing,
                onClick = onClick,
            ),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            modifier = Modifier
                .size(76.dp)
                .clip(shutterShape)
                .background(Color.White.copy(alpha = if (isCapturing) 0.5f else 1f)),
        )
    }
}
