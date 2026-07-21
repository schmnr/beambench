use serde::{Deserialize, Serialize};

/// A 2D point in millimeters.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point2D {
    pub x: f64,
    pub y: f64,
}

impl Point2D {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    /// Euclidean distance to another point.
    pub fn distance_to(&self, other: &Point2D) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Linear interpolation between self and other at parameter t.
    pub fn lerp(&self, other: &Point2D, t: f64) -> Point2D {
        Point2D {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
        }
    }
}

/// An axis-aligned bounding box in millimeters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub min: Point2D,
    pub max: Point2D,
}

impl Bounds {
    pub fn new(min: Point2D, max: Point2D) -> Self {
        Self { min, max }
    }

    pub fn width(&self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn contains(&self, point: &Point2D) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }

    /// Return the smallest bounds that contains both self and other.
    pub fn union(&self, other: &Bounds) -> Bounds {
        Bounds {
            min: Point2D::new(self.min.x.min(other.min.x), self.min.y.min(other.min.y)),
            max: Point2D::new(self.max.x.max(other.max.x), self.max.y.max(other.max.y)),
        }
    }
}

/// A 2D affine transform stored as a 3x2 matrix.
/// Supports translate, rotate, scale, and their combinations.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform2D {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Transform2D {
    pub fn identity() -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    pub fn translate(tx: f64, ty: f64) -> Self {
        Self {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx,
            ty,
        }
    }

    pub fn apply(&self, point: &Point2D) -> Point2D {
        Point2D {
            x: self.a * point.x + self.c * point.y + self.tx,
            y: self.b * point.x + self.d * point.y + self.ty,
        }
    }

    /// Create a rotation transform (angle in radians).
    pub fn rotate(angle_rad: f64) -> Self {
        let (s, c) = angle_rad.sin_cos();
        Self {
            a: c,
            b: s,
            c: -s,
            d: c,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Create a scale transform.
    pub fn scale(sx: f64, sy: f64) -> Self {
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Create a shear transform.
    /// `sx` shears X proportional to Y, `sy` shears Y proportional to X.
    pub fn shear(sx: f64, sy: f64) -> Self {
        Self {
            a: 1.0,
            b: sy,
            c: sx,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// Apply this transform to a point relative to a center.
    /// Translates the point so `center` is the origin, applies the transform,
    /// then translates back.
    pub fn apply_around_center(&self, point: &Point2D, center: &Point2D) -> Point2D {
        let rx = point.x - center.x;
        let ry = point.y - center.y;
        Point2D {
            x: self.a * rx + self.c * ry + self.tx + center.x,
            y: self.b * rx + self.d * ry + self.ty + center.y,
        }
    }

    /// Compose this transform with another: self * other.
    /// The result applies `other` first, then `self`.
    pub fn compose(&self, other: &Transform2D) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            tx: self.a * other.tx + self.c * other.ty + self.tx,
            ty: self.b * other.tx + self.d * other.ty + self.ty,
        }
    }

    /// Compute the determinant of the 2x2 linear part.
    pub fn determinant(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// Compute the inverse transform, if it exists.
    pub fn inverse(&self) -> Option<Self> {
        let det = self.determinant();
        if det.abs() < 1e-12 {
            return None;
        }
        let inv_det = 1.0 / det;
        Some(Self {
            a: self.d * inv_det,
            b: -self.b * inv_det,
            c: -self.c * inv_det,
            d: self.a * inv_det,
            tx: (self.c * self.ty - self.d * self.tx) * inv_det,
            ty: (self.b * self.tx - self.a * self.ty) * inv_det,
        })
    }

    /// Returns true if this is (approximately) the identity transform.
    pub fn is_identity(&self) -> bool {
        (self.a - 1.0).abs() < 1e-10
            && self.b.abs() < 1e-10
            && self.c.abs() < 1e-10
            && (self.d - 1.0).abs() < 1e-10
            && self.tx.abs() < 1e-10
            && self.ty.abs() < 1e-10
    }
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::identity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_dimensions() {
        let b = Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(50.0, 60.0));
        assert_eq!(b.width(), 40.0);
        assert_eq!(b.height(), 40.0);
    }

    #[test]
    fn bounds_contains_point() {
        let b = Bounds::new(Point2D::zero(), Point2D::new(100.0, 100.0));
        assert!(b.contains(&Point2D::new(50.0, 50.0)));
        assert!(!b.contains(&Point2D::new(150.0, 50.0)));
    }

    #[test]
    fn bounds_union() {
        let a = Bounds::new(Point2D::new(0.0, 10.0), Point2D::new(50.0, 60.0));
        let b = Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(80.0, 40.0));
        let u = a.union(&b);
        assert_eq!(u.min.x, 0.0);
        assert_eq!(u.min.y, 0.0);
        assert_eq!(u.max.x, 80.0);
        assert_eq!(u.max.y, 60.0);
    }

    #[test]
    fn identity_transform_preserves_point() {
        let t = Transform2D::identity();
        let p = Point2D::new(42.0, 17.0);
        assert_eq!(t.apply(&p), p);
    }

    #[test]
    fn translate_transform_moves_point() {
        let t = Transform2D::translate(10.0, -5.0);
        let p = Point2D::new(3.0, 7.0);
        let result = t.apply(&p);
        assert_eq!(result.x, 13.0);
        assert_eq!(result.y, 2.0);
    }

    #[test]
    fn geometry_roundtrips_through_json() {
        let p = Point2D::new(1.5, 2.5);
        let json = serde_json::to_string(&p).unwrap();
        let restored: Point2D = serde_json::from_str(&json).unwrap();
        assert_eq!(p, restored);
    }

    #[test]
    fn point_distance_to() {
        let a = Point2D::new(0.0, 0.0);
        let b = Point2D::new(3.0, 4.0);
        assert!((a.distance_to(&b) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn point_lerp() {
        let a = Point2D::new(0.0, 0.0);
        let b = Point2D::new(10.0, 20.0);
        let mid = a.lerp(&b, 0.5);
        assert!((mid.x - 5.0).abs() < 1e-10);
        assert!((mid.y - 10.0).abs() < 1e-10);
    }

    #[test]
    fn rotate_90_degrees() {
        let t = Transform2D::rotate(std::f64::consts::FRAC_PI_2);
        let p = Point2D::new(1.0, 0.0);
        let result = t.apply(&p);
        assert!((result.x - 0.0).abs() < 1e-10);
        assert!((result.y - 1.0).abs() < 1e-10);
    }

    #[test]
    fn scale_transform() {
        let t = Transform2D::scale(2.0, 3.0);
        let p = Point2D::new(5.0, 10.0);
        let result = t.apply(&p);
        assert_eq!(result.x, 10.0);
        assert_eq!(result.y, 30.0);
    }

    #[test]
    fn compose_translate_then_scale() {
        let translate = Transform2D::translate(10.0, 20.0);
        let scale = Transform2D::scale(2.0, 2.0);
        // scale applied first, then translate
        let composed = translate.compose(&scale);
        let p = Point2D::new(5.0, 5.0);
        let result = composed.apply(&p);
        assert!((result.x - 20.0).abs() < 1e-10);
        assert!((result.y - 30.0).abs() < 1e-10);
    }

    #[test]
    fn determinant_identity() {
        assert_eq!(Transform2D::identity().determinant(), 1.0);
    }

    #[test]
    fn determinant_scale() {
        let t = Transform2D::scale(2.0, 3.0);
        assert_eq!(t.determinant(), 6.0);
    }

    #[test]
    fn inverse_of_identity() {
        let inv = Transform2D::identity().inverse().unwrap();
        assert!(inv.is_identity());
    }

    #[test]
    fn inverse_of_translate() {
        let t = Transform2D::translate(10.0, 20.0);
        let inv = t.inverse().unwrap();
        let p = Point2D::new(5.0, 5.0);
        let result = inv.apply(&t.apply(&p));
        assert!((result.x - 5.0).abs() < 1e-10);
        assert!((result.y - 5.0).abs() < 1e-10);
    }

    #[test]
    fn inverse_of_rotation() {
        let t = Transform2D::rotate(std::f64::consts::FRAC_PI_4);
        let inv = t.inverse().unwrap();
        let composed = inv.compose(&t);
        assert!(composed.is_identity());
    }

    #[test]
    fn inverse_of_singular_returns_none() {
        let t = Transform2D::scale(0.0, 0.0);
        assert!(t.inverse().is_none());
    }

    #[test]
    fn is_identity() {
        assert!(Transform2D::identity().is_identity());
        assert!(!Transform2D::translate(1.0, 0.0).is_identity());
    }

    #[test]
    fn shear_matrix_values() {
        let t = Transform2D::shear(0.5, 0.3);
        assert_eq!(t.a, 1.0);
        assert_eq!(t.b, 0.3);
        assert_eq!(t.c, 0.5);
        assert_eq!(t.d, 1.0);
        assert_eq!(t.tx, 0.0);
        assert_eq!(t.ty, 0.0);
    }

    #[test]
    fn shear_applies_correctly() {
        let t = Transform2D::shear(1.0, 0.0);
        let p = Point2D::new(0.0, 1.0);
        let result = t.apply(&p);
        // x = 1*0 + 1*1 = 1, y = 0*0 + 1*1 = 1
        assert!((result.x - 1.0).abs() < 1e-10);
        assert!((result.y - 1.0).abs() < 1e-10);
    }

    #[test]
    fn apply_around_center_identity_no_effect() {
        let t = Transform2D::identity();
        let p = Point2D::new(5.0, 10.0);
        let c = Point2D::new(3.0, 7.0);
        let result = t.apply_around_center(&p, &c);
        assert!((result.x - 5.0).abs() < 1e-10);
        assert!((result.y - 10.0).abs() < 1e-10);
    }

    #[test]
    fn apply_around_center_rotation_90() {
        let t = Transform2D::rotate(std::f64::consts::FRAC_PI_2);
        let p = Point2D::new(1.0, 0.0);
        let c = Point2D::new(0.0, 0.0);
        let result = t.apply_around_center(&p, &c);
        assert!((result.x - 0.0).abs() < 1e-10);
        assert!((result.y - 1.0).abs() < 1e-10);
    }

    #[test]
    fn apply_around_center_with_nonzero_center() {
        let t = Transform2D::rotate(std::f64::consts::FRAC_PI_2);
        let p = Point2D::new(11.0, 10.0);
        let c = Point2D::new(10.0, 10.0);
        let result = t.apply_around_center(&p, &c);
        // relative: (1, 0) -> rotate 90 -> (0, 1) + center -> (10, 11)
        assert!((result.x - 10.0).abs() < 1e-10);
        assert!((result.y - 11.0).abs() < 1e-10);
    }

    #[test]
    fn shear_transform() {
        let t = Transform2D::shear(0.5, 0.0);
        assert_eq!(t.a, 1.0);
        assert_eq!(t.b, 0.0);
        assert_eq!(t.c, 0.5);
        assert_eq!(t.d, 1.0);
        assert_eq!(t.tx, 0.0);
        assert_eq!(t.ty, 0.0);
    }

    #[test]
    fn apply_around_center_identity() {
        let t = Transform2D::identity();
        let pt = Point2D::new(5.0, 3.0);
        let center = Point2D::new(1.0, 1.0);
        let result = t.apply_around_center(&pt, &center);
        assert!((result.x - 5.0).abs() < 1e-9);
        assert!((result.y - 3.0).abs() < 1e-9);
    }

    #[test]
    fn apply_around_center_rotation_90_at_origin() {
        let t = Transform2D::rotate(std::f64::consts::FRAC_PI_2); // 90 degrees
        let pt = Point2D::new(2.0, 0.0);
        let center = Point2D::new(0.0, 0.0);
        let result = t.apply_around_center(&pt, &center);
        assert!(
            (result.x - 0.0).abs() < 1e-9,
            "x should be ~0, got {}",
            result.x
        );
        assert!(
            (result.y - 2.0).abs() < 1e-9,
            "y should be ~2, got {}",
            result.y
        );
    }

    #[test]
    fn apply_around_center_rotation_90_offset_center() {
        let t = Transform2D::rotate(std::f64::consts::FRAC_PI_2);
        let pt = Point2D::new(3.0, 1.0); // 2 units right of center
        let center = Point2D::new(1.0, 1.0);
        let result = t.apply_around_center(&pt, &center);
        assert!(
            (result.x - 1.0).abs() < 1e-9,
            "x should be ~1, got {}",
            result.x
        );
        assert!(
            (result.y - 3.0).abs() < 1e-9,
            "y should be ~3, got {}",
            result.y
        );
    }
}
