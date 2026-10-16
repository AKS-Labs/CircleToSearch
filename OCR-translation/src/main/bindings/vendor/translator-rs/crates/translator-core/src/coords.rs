//! Phantom-typed coordinate spaces for the live-camera pipeline.
//!
//! The pipeline transforms points through multiple distinct coordinate
//! systems (sensor → oriented → tracker → anchor → rectified anchor →
//! screen). Without types these are all `(f32, f32)` and homographies
//! are all `[f32; 9]`, so a wrong-direction multiply or a misrouted
//! point compiles silently. The types here exist to make those bugs
//! into compile errors.
//!
//! Wraps the raw helpers in `homography.rs` — actual matrix math is
//! delegated, not duplicated.
//!
//! Anchor-id wrinkle: there can be multiple live anchors (an active
//! one plus cached chains), so `AnchorSpace` is a single phantom type
//! and the per-instance discriminator lives at value level in
//! `AnchorPoint::anchor_id`. The compile-time type catches
//! "anchor vs screen" confusion; runtime asserts catch
//! "anchor A vs anchor B" confusion.

use std::marker::PhantomData;

use crate::homography::{invert, mat3_mul, project};

/// Marker for raw camera sensor pixels (native sensor orientation).
#[derive(Copy, Clone, Debug, Default)]
pub struct SensorSpace;

/// Marker for sensor pixels after rotation to device-display orientation.
#[derive(Copy, Clone, Debug, Default)]
pub struct OrientedSpace;

/// Marker for the cropped display-orient frame the tracker actually sees.
#[derive(Copy, Clone, Debug, Default)]
pub struct TrackerSpace;

/// Marker for an anchor's canonical-frame coordinates. Multiple anchors
/// share this phantom type; the specific anchor is carried at value
/// level via [`AnchorPoint::anchor_id`].
#[derive(Copy, Clone, Debug, Default)]
pub struct AnchorSpace;

/// Marker for an anchor after rectification (fronto-parallel canonical).
#[derive(Copy, Clone, Debug, Default)]
pub struct RectifiedAnchorSpace;

/// Marker for final composited SurfaceView pixels.
#[derive(Copy, Clone, Debug, Default)]
pub struct ScreenSpace;

/// Anchor instance id. Mirrors the engine's `AnchorId` so the types can
/// be used without a circular dep into `planar_engine`.
pub type AnchorId = u64;

/// 2D point tagged with the coordinate space it lives in.
#[derive(Debug)]
pub struct Point2<S> {
    pub x: f32,
    pub y: f32,
    _s: PhantomData<S>,
}

impl<S> Point2<S> {
    pub const fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            _s: PhantomData,
        }
    }

    pub const fn xy(&self) -> (f32, f32) {
        (self.x, self.y)
    }
}

// Manual Copy/Clone so callers don't need `S: Copy + Clone` — the
// marker types are always ZSTs.
impl<S> Clone for Point2<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S> Copy for Point2<S> {}

impl<S> From<(f32, f32)> for Point2<S> {
    fn from((x, y): (f32, f32)) -> Self {
        Self::new(x, y)
    }
}

/// Axis-aligned bounding box tagged with the coordinate space it lives in.
#[derive(Debug)]
pub struct Aabb<S> {
    pub min: Point2<S>,
    pub max: Point2<S>,
}

impl<S> Aabb<S> {
    pub fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            min: Point2::new(min_x, min_y),
            max: Point2::new(max_x, max_y),
        }
    }

    pub fn width(&self) -> f32 {
        (self.max.x - self.min.x).max(0.0)
    }

    pub fn height(&self) -> f32 {
        (self.max.y - self.min.y).max(0.0)
    }

    pub fn area(&self) -> f32 {
        self.width() * self.height()
    }

    pub fn is_empty(&self) -> bool {
        self.width() <= 0.0 || self.height() <= 0.0
    }

    pub fn intersect(&self, other: &Self) -> Self {
        let min_x = self.min.x.max(other.min.x);
        let min_y = self.min.y.max(other.min.y);
        let max_x = self.max.x.min(other.max.x);
        let max_y = self.max.y.min(other.max.y);
        Self::new(min_x, min_y, max_x, max_y)
    }

    pub fn contains(&self, p: Point2<S>) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    /// Build an AABB from four corner points. Useful when projecting a
    /// rect through a homography: project each corner, then bound.
    pub fn from_corners(corners: [Point2<S>; 4]) -> Self {
        let mut min_x = corners[0].x;
        let mut min_y = corners[0].y;
        let mut max_x = corners[0].x;
        let mut max_y = corners[0].y;
        for c in corners.iter().skip(1) {
            if c.x < min_x {
                min_x = c.x;
            }
            if c.y < min_y {
                min_y = c.y;
            }
            if c.x > max_x {
                max_x = c.x;
            }
            if c.y > max_y {
                max_y = c.y;
            }
        }
        Self::new(min_x, min_y, max_x, max_y)
    }
}

impl<S> Clone for Aabb<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S> Copy for Aabb<S> {}

/// Homography mapping points from coordinate space `From` to space `To`.
///
/// Row-major `[f32; 9]` storage; same convention as
/// [`crate::homography`] (`(qx, qy, 1) ~ H * (px, py, 1)`).
#[derive(Debug)]
pub struct Homography<From, To> {
    pub m: [f32; 9],
    _from: PhantomData<From>,
    _to: PhantomData<To>,
}

impl<F, T> Clone for Homography<F, T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<F, T> Copy for Homography<F, T> {}

impl<F, T> Homography<F, T> {
    pub const fn from_raw(m: [f32; 9]) -> Self {
        Self {
            m,
            _from: PhantomData,
            _to: PhantomData,
        }
    }

    pub const fn into_raw(self) -> [f32; 9] {
        self.m
    }

    pub fn identity() -> Self {
        Self::from_raw([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0])
    }

    /// Project a point through the homography. Returns `None` for
    /// degenerate projections (`w ≈ 0`).
    pub fn apply(&self, p: Point2<F>) -> Option<Point2<T>> {
        project(&self.m, p.x, p.y).map(|(qx, qy)| Point2::new(qx, qy))
    }

    /// Project the four corners of `bbox` and return the bounding box
    /// in the destination space. Returns `None` if any corner
    /// projection is degenerate.
    pub fn apply_aabb(&self, bbox: Aabb<F>) -> Option<Aabb<T>> {
        let corners = [
            (bbox.min.x, bbox.min.y),
            (bbox.max.x, bbox.min.y),
            (bbox.max.x, bbox.max.y),
            (bbox.min.x, bbox.max.y),
        ];
        let mut projected = [Point2::<T>::new(0.0, 0.0); 4];
        for (i, (x, y)) in corners.iter().enumerate() {
            let (px, py) = project(&self.m, *x, *y)?;
            projected[i] = Point2::new(px, py);
        }
        Some(Aabb::from_corners(projected))
    }

    /// Compose `self: F→T` with `next: T→U` to produce `F→U`.
    pub fn then<U>(self, next: Homography<T, U>) -> Homography<F, U> {
        // Composition order: `next` is applied after `self`, so the
        // result matrix is `next.m * self.m`.
        Homography::from_raw(mat3_mul(&next.m, &self.m))
    }

    /// Invert the homography. Returns `None` if the matrix is singular.
    pub fn inverse(self) -> Option<Homography<T, F>> {
        invert(&self.m).map(Homography::from_raw)
    }

    /// 2D affine scale factor `sqrt(|det|)` of the upper-left 2×2
    /// sub-matrix. Approximates the homography's local linear-scale
    /// magnitude at the projective centre — used as a cheap "how much
    /// has scale changed since acquire" indicator for the
    /// anchor-handoff trigger.
    pub fn approx_scale(&self) -> f32 {
        let det = self.m[0] * self.m[4] - self.m[1] * self.m[3];
        det.abs().sqrt()
    }
}

/// A point in some anchor's canonical frame, tagged with which anchor
/// owns the frame. Use this when the same `Point2<AnchorSpace>` value
/// could otherwise be confused between anchors.
#[derive(Copy, Clone, Debug)]
pub struct AnchorPoint {
    pub p: Point2<AnchorSpace>,
    pub anchor_id: AnchorId,
}

impl AnchorPoint {
    pub fn new(x: f32, y: f32, anchor_id: AnchorId) -> Self {
        Self {
            p: Point2::new(x, y),
            anchor_id,
        }
    }
}

/// An anchor-tagged homography. Carries the anchor id so callers can
/// assert they're not mixing two anchors' coordinate frames at
/// composition time.
#[derive(Debug)]
pub struct AnchorHomography<To> {
    pub h: Homography<AnchorSpace, To>,
    pub anchor_id: AnchorId,
}

impl<T> Clone for AnchorHomography<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for AnchorHomography<T> {}

impl<To> AnchorHomography<To> {
    pub fn new(m: [f32; 9], anchor_id: AnchorId) -> Self {
        Self {
            h: Homography::from_raw(m),
            anchor_id,
        }
    }

    /// Apply to an anchor-tagged point. Panics in debug if the
    /// anchor ids don't match — this is an internal-logic bug, not a
    /// recoverable runtime condition.
    pub fn apply(&self, ap: AnchorPoint) -> Option<Point2<To>> {
        debug_assert_eq!(
            ap.anchor_id, self.anchor_id,
            "AnchorHomography applied to point from a different anchor"
        );
        self.h.apply(ap.p)
    }
}

/// Coarse 4-way rotation between two oriented frames. Used for "which
/// way is text reading-up?" at the camera/anchor level: the live tracker
/// already absorbs sub-quadrant rotation through its homography, so the
/// only thing the orientation estimator needs to commit to is one of
/// four 90° buckets.
///
/// Conventions: angles are measured counter-clockwise from the
/// camera-frame +x axis. `R0` means the strip's reading direction is
/// already along +x; `R90` means it points along +y (i.e. it needs to be
/// rotated -90° / CW 90° to read horizontally), etc.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Quadrant {
    R0,
    R90,
    R180,
    R270,
}

impl Default for Quadrant {
    fn default() -> Self {
        Self::R0
    }
}

impl Quadrant {
    pub const ALL: [Quadrant; 4] = [Quadrant::R0, Quadrant::R90, Quadrant::R180, Quadrant::R270];

    pub fn degrees(self) -> i32 {
        match self {
            Quadrant::R0 => 0,
            Quadrant::R90 => 90,
            Quadrant::R180 => 180,
            Quadrant::R270 => 270,
        }
    }

    pub fn radians(self) -> f32 {
        (self.degrees() as f32) * std::f32::consts::PI / 180.0
    }

    pub fn from_degrees_mod360(deg: i32) -> Self {
        let mut d = deg % 360;
        if d < 0 {
            d += 360;
        }
        let bucket = (((d as f32) / 90.0).round() as i32).rem_euclid(4);
        match bucket {
            0 => Quadrant::R0,
            1 => Quadrant::R90,
            2 => Quadrant::R180,
            _ => Quadrant::R270,
        }
    }

    /// Snap a radians value (any range) to the nearest quadrant.
    pub fn from_radians(angle: f32) -> Self {
        let deg = (angle * 180.0 / std::f32::consts::PI).round() as i32;
        Self::from_degrees_mod360(deg)
    }

    /// `self - other` as a quadrant. Encodes "how much further is `self`
    /// rotated CCW from `other`."
    pub fn sub(self, other: Quadrant) -> Quadrant {
        Self::from_degrees_mod360(self.degrees() - other.degrees())
    }

    /// `self + other` as a quadrant.
    pub fn add(self, other: Quadrant) -> Quadrant {
        Self::from_degrees_mod360(self.degrees() + other.degrees())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_round_trips() {
        let id = Homography::<SensorSpace, OrientedSpace>::identity();
        let p = Point2::<SensorSpace>::new(3.0, 7.0);
        let q = id.apply(p).expect("identity is non-degenerate");
        assert_eq!(q.x, 3.0);
        assert_eq!(q.y, 7.0);
    }

    #[test]
    fn inverse_round_trips() {
        // Translation + scale.
        let h = Homography::<TrackerSpace, AnchorSpace>::from_raw([
            2.0, 0.0, 5.0, 0.0, 2.0, -3.0, 0.0, 0.0, 1.0,
        ]);
        let p = Point2::<TrackerSpace>::new(10.0, 20.0);
        let q = h.apply(p).unwrap();
        // Forward: (2*10 + 5, 2*20 - 3) = (25, 37)
        assert!((q.x - 25.0).abs() < 1e-4);
        assert!((q.y - 37.0).abs() < 1e-4);
        let h_inv = h.inverse().unwrap();
        let back = h_inv.apply(q).unwrap();
        assert!((back.x - p.x).abs() < 1e-3);
        assert!((back.y - p.y).abs() < 1e-3);
    }

    #[test]
    fn compose_chains_spaces() {
        let a = Homography::<SensorSpace, OrientedSpace>::from_raw([
            2.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        let b = Homography::<OrientedSpace, TrackerSpace>::from_raw([
            1.0, 0.0, 10.0, 0.0, 1.0, 20.0, 0.0, 0.0, 1.0,
        ]);
        let ab = a.then(b);
        let p = Point2::<SensorSpace>::new(3.0, 5.0);
        let q = ab.apply(p).unwrap();
        // a: (6, 10), b: (16, 30)
        assert!((q.x - 16.0).abs() < 1e-4);
        assert!((q.y - 30.0).abs() < 1e-4);
    }

    #[test]
    fn aabb_intersection_and_area() {
        let a = Aabb::<AnchorSpace>::new(0.0, 0.0, 100.0, 100.0);
        let b = Aabb::<AnchorSpace>::new(50.0, 50.0, 200.0, 200.0);
        let inter = a.intersect(&b);
        assert_eq!(inter.area(), 50.0 * 50.0);
        let c = Aabb::<AnchorSpace>::new(200.0, 200.0, 300.0, 300.0);
        let empty = a.intersect(&c);
        assert!(empty.is_empty());
        assert_eq!(empty.area(), 0.0);
    }

    #[test]
    fn aabb_projects_via_homography() {
        // Identity-with-translation: shift +10, +20.
        let h = Homography::<AnchorSpace, ScreenSpace>::from_raw([
            1.0, 0.0, 10.0, 0.0, 1.0, 20.0, 0.0, 0.0, 1.0,
        ]);
        let src = Aabb::<AnchorSpace>::new(0.0, 0.0, 100.0, 100.0);
        let dst = h.apply_aabb(src).unwrap();
        assert!((dst.min.x - 10.0).abs() < 1e-4);
        assert!((dst.min.y - 20.0).abs() < 1e-4);
        assert!((dst.max.x - 110.0).abs() < 1e-4);
        assert!((dst.max.y - 120.0).abs() < 1e-4);
    }

    #[test]
    fn approx_scale_recovers_uniform() {
        // 1.5× scale + translation.
        let h = Homography::<AnchorSpace, TrackerSpace>::from_raw([
            1.5, 0.0, 7.0, 0.0, 1.5, -3.0, 0.0, 0.0, 1.0,
        ]);
        assert!((h.approx_scale() - 1.5).abs() < 1e-3);
    }

    #[test]
    fn quadrant_from_radians_snaps_to_nearest() {
        assert_eq!(Quadrant::from_radians(0.0), Quadrant::R0);
        assert_eq!(Quadrant::from_radians(0.1), Quadrant::R0);
        assert_eq!(
            Quadrant::from_radians(std::f32::consts::FRAC_PI_2),
            Quadrant::R90
        );
        assert_eq!(
            Quadrant::from_radians(-std::f32::consts::FRAC_PI_2),
            Quadrant::R270
        );
        assert_eq!(Quadrant::from_radians(std::f32::consts::PI), Quadrant::R180);
        assert_eq!(
            Quadrant::from_radians(3.0 * std::f32::consts::PI),
            Quadrant::R180
        );
    }

    #[test]
    fn quadrant_arithmetic_wraps() {
        assert_eq!(Quadrant::R90.sub(Quadrant::R270), Quadrant::R180);
        assert_eq!(Quadrant::R0.sub(Quadrant::R90), Quadrant::R270);
        assert_eq!(Quadrant::R270.add(Quadrant::R180), Quadrant::R90);
    }

    #[test]
    fn anchor_homography_carries_id() {
        let ah = AnchorHomography::<TrackerSpace>::new(
            [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            42,
        );
        let ap = AnchorPoint::new(1.0, 2.0, 42);
        let q = ah.apply(ap).expect("identity applies");
        assert!((q.x - 1.0).abs() < 1e-6);
        assert!((q.y - 2.0).abs() < 1e-6);
    }
}
