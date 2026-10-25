//! Constrained drawings for the solver's measurements (ADR-0038): plates
//! drawn as four separate lines, held together as a drafter constrains
//! them — corners coincident, sides horizontal and vertical, a width and a
//! height — and chained, each plate's lower left on the previous plate's
//! lower right, the first plate's corner fixed. Eleven constraints a plate
//! (fifteen equations), so one long chain is one component for the solver.

use cad_geometry::constraint::{Equation, LineVar, PointVar, System};

/// A chain of plates, and the points of each, corner by corner.
#[derive(Debug)]
pub struct Plates {
    pub system: System,
    /// Per plate, its four lines.
    pub lines: Vec<[LineVar; 4]>,
    /// Constraints, as a drafter counts them (a coincidence is one).
    pub constraints: usize,
}

/// `count` plates, 120 × 80, chained along x — or each alone, a component
/// of its own, when `chained` is false.
pub fn plates(count: usize, chained: bool) -> Plates {
    let mut system = System::default();
    let mut lines = Vec::new();
    let mut constraints = 0;
    for i in 0..count {
        let x = i as f64 * (if chained { 120.0 } else { 200.0 });
        let corners = [(x, 0.0), (x + 120.0, 0.0), (x + 120.0, 80.0), (x, 80.0)];
        // Four lines, each with its own two end points.
        let plate: [LineVar; 4] = std::array::from_fn(|side| {
            let (ax, ay) = corners[side];
            let (bx, by) = corners[(side + 1) % 4];
            LineVar {
                a: system.point(ax, ay),
                b: system.point(bx, by),
            }
        });
        let coincident = |system: &mut System, p: PointVar, q: PointVar| {
            system.equations.push(Equation::Equal { a: p.x, b: q.x });
            system.equations.push(Equation::Equal { a: p.y, b: q.y });
        };
        for side in 0..4 {
            coincident(&mut system, plate[side].b, plate[(side + 1) % 4].a);
        }
        system.equations.extend([
            Equation::Equal {
                a: plate[0].a.y,
                b: plate[0].b.y,
            },
            Equation::Equal {
                a: plate[2].a.y,
                b: plate[2].b.y,
            },
            Equation::Equal {
                a: plate[1].a.x,
                b: plate[1].b.x,
            },
            Equation::Equal {
                a: plate[3].a.x,
                b: plate[3].b.x,
            },
            Equation::Offset {
                from: plate[0].a.x,
                to: plate[0].b.x,
                value: 120.0,
            },
            Equation::Offset {
                from: plate[1].a.y,
                to: plate[1].b.y,
                value: 80.0,
            },
        ]);
        constraints += 10;
        match lines.last() {
            Some(previous) if chained => {
                let previous: &[LineVar; 4] = previous;
                coincident(&mut system, plate[0].a, previous[0].b);
                constraints += 1;
            }
            _ => {
                system.equations.push(Equation::Fixed {
                    param: plate[0].a.x,
                    value: x,
                });
                system.equations.push(Equation::Fixed {
                    param: plate[0].a.y,
                    value: 0.0,
                });
                constraints += 1;
            }
        }
        lines.push(plate);
    }
    Plates {
        system,
        lines,
        constraints,
    }
}

/// The plates with the first plate's width changed to 150: in a chain,
/// every plate after it moves 30 to the right.
pub fn widened(plates: &Plates) -> System {
    let mut system = plates.system.clone();
    for equation in &mut system.equations {
        if let Equation::Offset { from, value, .. } = equation
            && *from == plates.lines[0][0].a.x
        {
            *value = 150.0;
        }
    }
    system
}

/// The plates with the last plate's upper right corner dragged 5 up and
/// held there, as a grip leaves it: its plate grows taller.
pub fn dragged(plates: &Plates) -> System {
    let mut system = plates.system.clone();
    let last = plates.lines.last().expect("a plate");
    let corner = last[1].b;
    system.values[corner.y] += 5.0;
    system.hold(corner.y);
    // Its height is no longer 80: the drag drives it, as a grip on a
    // dimensioned plate is refused otherwise; drop the height.
    system
        .equations
        .retain(|e| !matches!(e, Equation::Offset { from, .. } if *from == last[1].a.y));
    system
}
