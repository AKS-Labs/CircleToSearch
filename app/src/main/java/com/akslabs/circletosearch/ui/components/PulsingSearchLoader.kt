/*
 *
 *  * Copyright (C) 2025 AKS-Labs (original author)
 *  *
 *  * This program is free software: you can redistribute it and/or modify
 *  * it under the terms of the GNU General Public License as published by
 *  * the Free Software Foundation, either version 3 of the License, or
 *  * (at your option) any later version.
 *  *
 *  * This program is distributed in the hope that it will be useful,
 *  * but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  * GNU General Public License for more details.
 *  *
 *  * You should have received a copy of the GNU General Public License
 *  * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 */

package com.akslabs.circletosearch.ui.components

import androidx.compose.animation.core.*
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp

@Composable
fun PulsingSearchLoader(
    modifier: Modifier = Modifier,
    size: androidx.compose.ui.unit.Dp = 64.dp
) {
    val infiniteTransition = rememberInfiniteTransition(label = "PulseTransition")

    // 1. Rotation for the outer ring
    val rotation by infiniteTransition.animateFloat(
        initialValue = 0f,
        targetValue = 360f,
        animationSpec = infiniteRepeatable(
            animation = tween(1800, easing = LinearEasing),
            repeatMode = RepeatMode.Restart
        ),
        label = "Rotation"
    )

    // 2. Scale for the pulsing core
    val scale by infiniteTransition.animateFloat(
        initialValue = 0.8f,
        targetValue = 1.2f,
        animationSpec = infiniteRepeatable(
            animation = tween(800, easing = FastOutSlowInEasing),
            repeatMode = RepeatMode.Reverse
        ),
        label = "Scale"
    )

    // Google Colors
    val colors = listOf(
        Color(0xFF4285F4), // Blue
        Color(0xFFEA4335), // Red
        Color(0xFFFBBC05), // Yellow
        Color(0xFF34A853)  // Green
    )

    Box(
        modifier = modifier.size(size),
        contentAlignment = Alignment.Center
    ) {
        Canvas(modifier = Modifier.matchParentSize()) {
            val strokeWidth = size.toPx() * 0.1f
            val radius = (size.toPx() - strokeWidth) / 2

            // Draw rotating arc segments
            // We draw 4 arcs, one for each color
            val arcLength = 70f // leaving some gaps
            val gap = 20f
            
            colors.forEachIndexed { index, color ->
                val startAngle = rotation + (index * 90f)
                
                drawArc(
                    color = color,
                    startAngle = startAngle,
                    sweepAngle = arcLength,
                    useCenter = false,
                    topLeft = Offset(strokeWidth / 2, strokeWidth / 2),
                    size = androidx.compose.ui.geometry.Size(size.toPx() - strokeWidth, size.toPx() - strokeWidth),
                    style = Stroke(width = strokeWidth, cap = StrokeCap.Round)
                )
            }
            
            // Draw pulsing core dots in the center
            val center = Offset(size.toPx() / 2, size.toPx() / 2)
            val dotRadius = size.toPx() * 0.08f * scale
            val dotOffset = size.toPx() * 0.2f 
            
            // Draw 4 dots in a square formation, rotating opposite direction or static
            // Let's make them static but pulsing in size
            
            drawCircle(colors[0], radius = dotRadius, center = center + Offset(-dotOffset, -dotOffset))
            drawCircle(colors[1], radius = dotRadius, center = center + Offset(dotOffset, -dotOffset))
            drawCircle(colors[2], radius = dotRadius, center = center + Offset(dotOffset, dotOffset))
            drawCircle(colors[3], radius = dotRadius, center = center + Offset(-dotOffset, dotOffset))
        }
    }
}
