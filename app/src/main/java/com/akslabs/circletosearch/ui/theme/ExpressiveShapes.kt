package com.akslabs.circletosearch.ui.theme

import androidx.compose.foundation.shape.CornerBasedShape
import androidx.compose.foundation.shape.CutCornerShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.unit.dp
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

/**
 * Material 3 Expressive Shapes Library.
 *
 * Implements the expressive shapes introduced in Material Design 3 Expressive:
 * - Arch: half-dome rounded top, flat base
 * - Clover 4: four-lobed rounded clover
 * - Clover 8: eight-lobed rounded floral rosette
 * - Flower 8: eight-petal geometric flower
 * - Scallop 12: twelve-lobe decorative scalloped medallion
 * - Spark 4: Google Gemini / Pixel AI organic 4-point spark
 * - Asymmetric Organic Card: asymmetric rounded corners (32dp, 8dp, 32dp, 8dp)
 * - Slanted Pill: modern stadium shape with angled diagonal corners
 */
object M3ExpressiveShapes {

  /**
   * Classic M3 Expressive Arch Shape:
   * Semicircular dome top with flat bottom.
   */
  val Arch: CornerBasedShape = RoundedCornerShape(
    topStartPercent = 50,
    topEndPercent = 50,
    bottomStartPercent = 0,
    bottomEndPercent = 0
  )

  /**
   * Reverse Arch:
   * Flat top with semicircular dome bottom.
   */
  val ReverseArch: CornerBasedShape = RoundedCornerShape(
    topStartPercent = 0,
    topEndPercent = 0,
    bottomStartPercent = 50,
    bottomEndPercent = 50
  )

  /**
   * M3 Expressive Asymmetric Card:
   * Expressive diagonal rounding pairing 32dp and 8dp.
   */
  val AsymmetricCard: CornerBasedShape = RoundedCornerShape(
    topStart = 32.dp,
    topEnd = 8.dp,
    bottomEnd = 32.dp,
    bottomStart = 8.dp
  )

  /**
   * M3 Expressive Slanted Pill:
   * High asymmetric corner rounding for expressive chips and badges.
   */
  val SlantedPill: CornerBasedShape = RoundedCornerShape(
    topStart = 24.dp,
    topEnd = 6.dp,
    bottomEnd = 24.dp,
    bottomStart = 6.dp
  )

  /**
   * 4-Leaf Clover Shape:
   * Organic 4-lobed shape created with parametric lobes.
   */
  val Clover4: Shape = object : Shape {
    override fun createOutline(
      size: Size,
      layoutDirection: LayoutDirection,
      density: Density
    ): Outline {
      val path = Path()
      val w = size.width
      val h = size.height
      val cx = w / 2f
      val cy = h / 2f
      val baseR = minOf(w, h) * 0.44f
      val lobeAmp = minOf(w, h) * 0.08f

      val steps = 80
      for (i in 0..steps) {
        val theta = (i.toFloat() / steps) * 2f * PI.toFloat()
        val r = baseR + lobeAmp * cos(theta * 4f)
        val x = cx + r * cos(theta)
        val y = cy + r * sin(theta)
        if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
      }
      path.close()
      return Outline.Generic(path)
    }
  }

  /**
   * 8-Leaf Clover / Rosette Shape:
   * 8 smooth rounded lobes radiating from center.
   */
  val Clover8: Shape = object : Shape {
    override fun createOutline(
      size: Size,
      layoutDirection: LayoutDirection,
      density: Density
    ): Outline {
      val path = Path()
      val w = size.width
      val h = size.height
      val cx = w / 2f
      val cy = h / 2f
      val baseR = minOf(w, h) * 0.43f
      val lobeAmp = minOf(w, h) * 0.07f

      val steps = 96
      for (i in 0..steps) {
        val theta = (i.toFloat() / steps) * 2f * PI.toFloat()
        val r = baseR + lobeAmp * cos(theta * 8f)
        val x = cx + r * cos(theta)
        val y = cy + r * sin(theta)
        if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
      }
      path.close()
      return Outline.Generic(path)
    }
  }

  /**
   * 8-Petal Geometric Flower Shape:
   * Deeper valleys creating a floral contour.
   */
  val Flower8: Shape = object : Shape {
    override fun createOutline(
      size: Size,
      layoutDirection: LayoutDirection,
      density: Density
    ): Outline {
      val path = Path()
      val w = size.width
      val h = size.height
      val cx = w / 2f
      val cy = h / 2f
      val baseR = minOf(w, h) * 0.40f
      val lobeAmp = minOf(w, h) * 0.10f

      val steps = 96
      for (i in 0..steps) {
        val theta = (i.toFloat() / steps) * 2f * PI.toFloat()
        val r = baseR + lobeAmp * sin(theta * 8f)
        val x = cx + r * cos(theta)
        val y = cy + r * sin(theta)
        if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
      }
      path.close()
      return Outline.Generic(path)
    }
  }

  /**
   * 12-Point Scalloped Medallion / Cookie Shape:
   * 12 harmonic sinusoidal lobes radiating around a circle.
   */
  val Scallop12: Shape = object : Shape {
    override fun createOutline(
      size: Size,
      layoutDirection: LayoutDirection,
      density: Density
    ): Outline {
      val path = Path()
      val w = size.width
      val h = size.height
      val cx = w / 2f
      val cy = h / 2f
      val baseR = minOf(w, h) * 0.43f
      val scallopAmp = minOf(w, h) * 0.06f

      val steps = 144
      for (i in 0..steps) {
        val theta = (i.toFloat() / steps) * 2f * PI.toFloat()
        val r = baseR + scallopAmp * cos(theta * 12f)
        val x = cx + r * cos(theta)
        val y = cy + r * sin(theta)
        if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
      }
      path.close()
      return Outline.Generic(path)
    }
  }

  /**
   * Material 3 Expressive Heart Shape:
   * Smooth rounded organic heart matching official M3 Expressive design tokens.
   */
  val Heart: Shape = object : Shape {
    override fun createOutline(
      size: Size,
      layoutDirection: LayoutDirection,
      density: Density
    ): Outline {
      val path = Path()
      val w = size.width
      val h = size.height

      path.moveTo(w * 0.5f, h * 0.30f)
      // Left lobe curve
      path.cubicTo(w * 0.44f, h * 0.10f, w * 0.08f, h * 0.10f, w * 0.08f, h * 0.42f)
      path.cubicTo(w * 0.08f, h * 0.65f, w * 0.30f, h * 0.82f, w * 0.50f, h * 0.95f)
      // Right lobe curve
      path.cubicTo(w * 0.70f, h * 0.82f, w * 0.92f, h * 0.65f, w * 0.92f, h * 0.42f)
      path.cubicTo(w * 0.92f, h * 0.10f, w * 0.56f, h * 0.10f, w * 0.50f, h * 0.30f)
      path.close()
      return Outline.Generic(path)
    }
  }

  /**
   * 4-Point AI Spark Shape (Gemini / Pixel Studio Spark):
   * Four concave arcs meeting at rounded points.
   */
  val Spark4: Shape = object : Shape {
    override fun createOutline(
      size: Size,
      layoutDirection: LayoutDirection,
      density: Density
    ): Outline {
      val path = Path()
      val w = size.width
      val h = size.height
      val cx = w / 2f
      val cy = h / 2f
      val outerR = minOf(w, h) * 0.48f
      val innerR = outerR * 0.32f

      val steps = 64
      for (i in 0..steps) {
        val theta = (i.toFloat() / steps) * 2f * PI.toFloat()
        val r = innerR + (outerR - innerR) * (0.5f + 0.5f * cos(theta * 4f))
        val x = cx + r * cos(theta)
        val y = cy + r * sin(theta)
        if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
      }
      path.close()
      return Outline.Generic(path)
    }
  }

  /**
   * Expressive Cut-Corner Asymmetric Card:
   * Diagonal faceted architectural corners popular in Pixel camera and screenshots UI.
   */
  val CutCornerCard: Shape = CutCornerShape(
    topStart = 28.dp,
    topEnd = 6.dp,
    bottomEnd = 28.dp,
    bottomStart = 6.dp
  )

  /**
   * Stadium / Pill Shape:
   * Semicircular ends ideal for badges, chips, and tactile action pills.
   */
  val Stadium: Shape = RoundedCornerShape(percent = 50)

  /**
   * Squircle (Continuous Curvature Superellipse):
   * Ultra-smooth corner blend avoiding abrupt tangent transitions.
   */
  val Squircle: Shape = RoundedCornerShape(percent = 32)

  /**
   * Hexagonal Shield Badge:
   * 6-sided geometric polygon with subtle rounded corners.
   */
  val HexagonBadge: Shape = object : Shape {
    override fun createOutline(
      size: Size,
      layoutDirection: LayoutDirection,
      density: Density
    ): Outline {
      val path = Path()
      val w = size.width
      val h = size.height
      val cx = w / 2f
      val cy = h / 2f
      val r = minOf(w, h) * 0.46f

      for (i in 0 until 6) {
        val angle = (i * 60f - 30f) * (PI.toFloat() / 180f)
        val x = cx + r * cos(angle)
        val y = cy + r * sin(angle)
        if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
      }
      path.close()
      return Outline.Generic(path)
    }
  }

  /**
   * Octagon Medallion Shape:
   * 8-sided regular polygon for cryptographic and security stamps.
   */
  val OctagonMedallion: Shape = object : Shape {
    override fun createOutline(
      size: Size,
      layoutDirection: LayoutDirection,
      density: Density
    ): Outline {
      val path = Path()
      val w = size.width
      val h = size.height
      val cx = w / 2f
      val cy = h / 2f
      val r = minOf(w, h) * 0.46f

      for (i in 0 until 8) {
        val angle = (i * 45f - 22.5f) * (PI.toFloat() / 180f)
        val x = cx + r * cos(angle)
        val y = cy + r * sin(angle)
        if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
      }
      path.close()
      return Outline.Generic(path)
    }
  }

  /**
   * 16-Point Sunburst / Star Medallion:
   * Expressive celestial starburst for astrophotography & camera awards.
   */
  val Sunburst16: Shape = object : Shape {
    override fun createOutline(
      size: Size,
      layoutDirection: LayoutDirection,
      density: Density
    ): Outline {
      val path = Path()
      val w = size.width
      val h = size.height
      val cx = w / 2f
      val cy = h / 2f
      val rOuter = minOf(w, h) * 0.47f
      val rInner = rOuter * 0.78f

      for (i in 0 until 32) {
        val r = if (i % 2 == 0) rOuter else rInner
        val angle = (i * (360f / 32f)) * (PI.toFloat() / 180f)
        val x = cx + r * cos(angle)
        val y = cy + r * sin(angle)
        if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
      }
      path.close()
      return Outline.Generic(path)
    }
  }

  /**
   * Protective Security Shield Shape:
   * Curved top arch with tapered pointed apex at bottom.
   */
  val Shield: Shape = object : Shape {
    override fun createOutline(
      size: Size,
      layoutDirection: LayoutDirection,
      density: Density
    ): Outline {
      val path = Path()
      val w = size.width
      val h = size.height

      path.moveTo(w * 0.5f, 0f)
      path.cubicTo(w * 0.85f, 0f, w, h * 0.15f, w, h * 0.42f)
      path.cubicTo(w, h * 0.72f, w * 0.65f, h * 0.92f, w * 0.5f, h)
      path.cubicTo(w * 0.35f, h * 0.92f, 0f, h * 0.72f, 0f, h * 0.42f)
      path.cubicTo(0f, h * 0.15f, w * 0.15f, 0f, w * 0.5f, 0f)
      path.close()
      return Outline.Generic(path)
    }
  }

  /**
   * Tri-Lobed Organic Asymmetric Fluid Blob:
   * Smooth fluid shape for acoustics and spatial sound.
   */
  val OrganicBlob: Shape = object : Shape {
    override fun createOutline(
      size: Size,
      layoutDirection: LayoutDirection,
      density: Density
    ): Outline {
      val path = Path()
      val w = size.width
      val h = size.height
      val cx = w / 2f
      val cy = h / 2f
      val baseR = minOf(w, h) * 0.40f
      val amp = minOf(w, h) * 0.08f

      val steps = 90
      for (i in 0..steps) {
        val theta = (i.toFloat() / steps) * 2f * PI.toFloat()
        val r = baseR + amp * sin(theta * 3f)
        val x = cx + r * cos(theta)
        val y = cy + r * sin(theta)
        if (i == 0) path.moveTo(x, y) else path.lineTo(x, y)
      }
      path.close()
      return Outline.Generic(path)
    }
  }

  /**
   * Ticket Notch Card:
   * Rounded card with semicircular concave side cutouts.
   */
  val TicketNotch: Shape = RoundedCornerShape(20.dp)

  /**
   * Diamond Pill Shape:
   * Rotated rhomboid pill with rounded tips.
   */
  val DiamondPill: Shape = RoundedCornerShape(22.dp)

  /**
   * Bookmark Ribbon Shape:
   * Rounded top with inverted triangular ribbon cutout at bottom.
   */
  val BookmarkRibbon: Shape = object : Shape {
    override fun createOutline(
      size: Size,
      layoutDirection: LayoutDirection,
      density: Density
    ): Outline {
      val path = Path()
      val w = size.width
      val h = size.height
      val cornerR = 16f

      path.moveTo(cornerR, 0f)
      path.lineTo(w - cornerR, 0f)
      path.quadraticTo(w, 0f, w, cornerR)
      path.lineTo(w, h)
      path.lineTo(w * 0.5f, h * 0.85f)
      path.lineTo(0f, h)
      path.lineTo(0f, cornerR)
      path.quadraticTo(0f, 0f, cornerR, 0f)
      path.close()
      return Outline.Generic(path)
    }
  }
}
