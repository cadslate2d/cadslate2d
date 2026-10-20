//! DXF groups as the DXF reader takes them from this crate (ADR-0036 §2):
//! each decoded field under the group code the specification lists
//! beside it, its value as text in the shortest form that reads back
//! exactly.

use cad_core::Point2;

/// The groups of one record, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Groups(pub Vec<(i32, String)>);

impl Groups {
    pub fn int(&mut self, code: i32, value: impl Into<i64>) {
        self.0.push((code, value.into().to_string()));
    }

    pub fn real(&mut self, code: i32, value: f64) {
        self.0.push((code, real_text(value)));
    }

    /// An angle the file holds in radians, under a code DXF gives degrees.
    pub fn degrees(&mut self, code: i32, radians: f64) {
        self.real(code, radians.to_degrees());
    }

    pub fn text(&mut self, code: i32, value: &str) {
        self.0.push((code, value.to_owned()));
    }

    /// A handle, as DXF writes one: upper-case hex.
    pub fn handle(&mut self, code: i32, handle: u64) {
        self.0.push((code, format!("{handle:X}")));
    }

    /// A point under `x` and `x + 10`.
    pub fn point(&mut self, x: i32, point: Point2) {
        self.real(x, point.x);
        self.real(x + 10, point.y);
    }

    /// A point with its z under `x`, `x + 10`, and `x + 20`.
    pub fn point3(&mut self, x: i32, point: Point2, z: f64) {
        self.point(x, point);
        self.real(x + 20, z);
    }

    pub fn push(&mut self, code: i32, value: String) {
        self.0.push((code, value));
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A real as text: Rust's shortest representation reads back to the same
/// double, which is the DXF writer's rule too.
pub fn real_text(value: f64) -> String {
    if value.is_finite() {
        format!("{value}")
    } else {
        "0".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reals_read_back_exactly_and_angles_become_degrees() {
        for value in [0.0, 1.0, -2.5, 1e-7, 123456.789, 1e300, f64::MIN_POSITIVE] {
            assert_eq!(real_text(value).parse::<f64>().unwrap(), value);
        }
        assert_eq!(real_text(f64::NAN), "0");
        let mut g = Groups::default();
        g.degrees(50, std::f64::consts::PI);
        g.handle(5, 0x1ab);
        g.point3(10, Point2::new(1.0, 2.0), 3.0);
        g.int(70, 5);
        assert_eq!(
            g.0,
            [
                (50, "180".to_owned()),
                (5, "1AB".to_owned()),
                (10, "1".to_owned()),
                (20, "2".to_owned()),
                (30, "3".to_owned()),
                (70, "5".to_owned()),
            ]
        );
    }
}
