//! Reading a record's groups by code: the values a DXF record carries, as
//! the writer takes them.

use cad_core::Point2;

/// A record's groups, looked up by code.
#[derive(Clone, Copy, Debug)]
pub struct Fields<'a> {
    groups: &'a [(i32, String)],
}

impl<'a> Fields<'a> {
    pub fn new(groups: &'a [(i32, String)]) -> Self {
        Self { groups }
    }

    pub fn groups(&self) -> &'a [(i32, String)] {
        self.groups
    }

    /// The first value under `code`, as written.
    pub fn text(&self, code: i32) -> Option<&'a str> {
        self.groups
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, v)| v.as_str())
    }

    pub fn int(&self, code: i32) -> Option<i64> {
        self.text(code).and_then(|v| v.trim().parse().ok())
    }

    pub fn real(&self, code: i32) -> Option<f64> {
        self.text(code).and_then(|v| v.trim().parse().ok())
    }

    /// A handle, written in upper-case hex.
    pub fn handle(&self, code: i32) -> Option<u64> {
        self.text(code)
            .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
    }

    /// A point under `code` and `code + 10`.
    pub fn point(&self, code: i32) -> Option<Point2> {
        Some(Point2::new(self.real(code)?, self.real(code + 10)?))
    }

    /// The Z under `code + 20`, zero when absent.
    pub fn z(&self, code: i32) -> f64 {
        self.real(code + 20).unwrap_or(0.0)
    }

    /// Every value under `code`, in order.
    pub fn reals(&self, code: i32) -> Vec<f64> {
        self.groups
            .iter()
            .filter(|(c, _)| *c == code)
            .filter_map(|(_, v)| v.trim().parse().ok())
            .collect()
    }

    pub fn texts(&self, code: i32) -> Vec<&'a str> {
        self.groups
            .iter()
            .filter(|(c, _)| *c == code)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    pub fn handles(&self, code: i32) -> Vec<u64> {
        self.groups
            .iter()
            .filter(|(c, _)| *c == code)
            .filter_map(|(_, v)| u64::from_str_radix(v.trim(), 16).ok())
            .collect()
    }

    /// Every point that starts under `code`: each `code` begins one, its
    /// `code + 10` and `code + 20` follow.
    pub fn points(&self, code: i32) -> Vec<(Point2, f64)> {
        let mut out: Vec<(Point2, f64)> = Vec::new();
        for (c, v) in self.groups {
            let Ok(value) = v.trim().parse::<f64>() else {
                continue;
            };
            if *c == code {
                out.push((Point2::new(value, 0.0), 0.0));
            } else if *c == code + 10 {
                if let Some(last) = out.last_mut() {
                    last.0.y = value;
                }
            } else if *c == code + 20
                && let Some(last) = out.last_mut()
            {
                last.1 = value;
            }
        }
        out
    }

    /// The groups after the subclass marker `100 <name>`, to its end or
    /// the next marker; all of them when there is no such marker.
    pub fn subclass(&self, name: &str) -> Fields<'a> {
        let start = self
            .groups
            .iter()
            .position(|(c, v)| *c == 100 && v.trim() == name);
        match start {
            Some(at) => {
                let rest = &self.groups[at + 1..];
                let end = rest
                    .iter()
                    .position(|(c, _)| *c == 100)
                    .unwrap_or(rest.len());
                Fields {
                    groups: &rest[..end],
                }
            }
            None => *self,
        }
    }

    /// The groups from `from` (inclusive) to `to` (exclusive).
    pub fn slice(&self, from: usize, to: usize) -> Fields<'a> {
        Fields {
            groups: &self.groups[from.min(self.groups.len())..to.min(self.groups.len())],
        }
    }

    pub fn position(&self, code: i32, value: &str) -> Option<usize> {
        self.groups
            .iter()
            .position(|(c, v)| *c == code && v.trim() == value)
    }

    pub fn len(&self) -> usize {
        self.groups.len()
    }
}

/// The lineweight index of §20.4.1 for a DXF 370 value: 29 by layer, 30
/// by block, 31 the default, else the nearest row of the table.
pub fn lineweight_index(code: i64) -> u8 {
    const TABLE: [i64; 24] = [
        0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158,
        200, 211,
    ];
    match code {
        -1 => 29,
        -2 => 30,
        -3 => 31,
        value => TABLE
            .iter()
            .position(|&w| w >= value)
            .unwrap_or(TABLE.len() - 1) as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_follow_their_x() {
        let groups = vec![
            (10, "1".to_owned()),
            (20, "2".to_owned()),
            (30, "3".to_owned()),
            (10, "4".to_owned()),
            (20, "5".to_owned()),
        ];
        let f = Fields::new(&groups);
        assert_eq!(
            f.points(10),
            vec![(Point2::new(1.0, 2.0), 3.0), (Point2::new(4.0, 5.0), 0.0)]
        );
        assert_eq!(f.point(10), Some(Point2::new(1.0, 2.0)));
        assert_eq!(f.z(10), 3.0);
    }

    #[test]
    fn a_subclass_is_its_groups_alone() {
        let groups = vec![
            (5, "A".to_owned()),
            (100, "AcDbPlotSettings".to_owned()),
            (1, "setup".to_owned()),
            (100, "AcDbLayout".to_owned()),
            (1, "Layout1".to_owned()),
        ];
        let f = Fields::new(&groups);
        assert_eq!(f.subclass("AcDbLayout").text(1), Some("Layout1"));
        assert_eq!(f.subclass("AcDbPlotSettings").text(1), Some("setup"));
        assert_eq!(f.subclass("AcDbNothing").text(1), Some("setup"));
        assert_eq!(f.handle(5), Some(10));
    }

    #[test]
    fn lineweights_map_to_the_index_table() {
        assert_eq!(lineweight_index(-1), 29);
        assert_eq!(lineweight_index(-2), 30);
        assert_eq!(lineweight_index(-3), 31);
        assert_eq!(lineweight_index(0), 0);
        assert_eq!(lineweight_index(50), 11);
        assert_eq!(lineweight_index(211), 23);
        assert_eq!(lineweight_index(52), 12);
        assert_eq!(lineweight_index(999), 23);
    }
}
