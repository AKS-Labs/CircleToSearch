Here is the complete, ready-to-paste Jetpack Compose code for the dual-layer marshmallow scalloped capture button used in CameraScreen.kt along with ScallopShape from AndroidifyShapes.kt.
1. ScallopShape.kt (Custom Scalloped Shape)
   Kotlin
   import androidx.compose.ui.geometry.Size
   import androidx.compose.ui.graphics.Outline
   import androidx.compose.ui.graphics.Path
   import androidx.compose.ui.graphics.Shape
   import androidx.compose.ui.unit.Density
   import androidx.compose.ui.unit.LayoutDirection
   import kotlin.math.PI
   import kotlin.math.cos
   import kotlin.math.sin

class ScallopShape(
private val lobes: Int = 10,
private val depthRatio: Float = 0.065f
) : Shape {
override fun createOutline(
size: Size,
layoutDirection: LayoutDirection,
density: Density
): Outline {
val path = Path()
val cx = size.width / 2f
val cy = size.height / 2f
val rBase = minOf(cx, cy) * 0.94f
val rOuter = rBase
val rInner = rBase * (1f - depthRatio)

        val totalPoints = lobes * 2
        val angleStep = (2f * PI / totalPoints).toFloat()

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

            val handleLen = (r0 + r1) / 2f * angleStep * 0.55f
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
2. ScallopedCaptureButton.kt (Capture Button Composable)
   Kotlin
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
   import androidx.compose.ui.unit.dp

@Composable
fun ScallopedCaptureButton(
onClick: () -> Unit,
modifier: Modifier = Modifier
) {
val interactionSource = remember { MutableInteractionSource() }
val isPressed by interactionSource.collectIsPressedAsState()
val scale by animateFloatAsState(
targetValue = if (isPressed) 0.92f else 1.0f,
label = "shutter"
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
                onClick = onClick
            ),
        contentAlignment = Alignment.Center
    ) {
        Box(
            modifier = Modifier
                .size(76.dp)
                .clip(shutterShape)
                .background(Color.White)
        )
    }
}
Usage
Kotlin
ScallopedCaptureButton(
onClick = { /* trigger camera capture */ }
)
Note on styling: The outer ring (94.dp, 0.4f alpha white fill with a 3.dp solid white border) and inner button (76.dp solid white) are designed to sit over a camera preview or darker background. If you place it on a bright/white surface, swap Color.White for a contrasting color or dark border.
App drawer icon note: If you also still want the launcher icon zoom adjusted in this project (ic_launcher_foreground.xml currently uses 108dp instead of the 66dp adaptive safe zone), let me know and I can update it.
