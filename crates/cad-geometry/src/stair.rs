//! Stairs (#331): sized by Blondel's rule and drawn in plan and in section.
//!
//! **Sizing.** A stair climbs a floor-to-floor height in equal risers. The
//! fewest risers that keep each at most the highest allowed are taken, and
//! the tread is what makes two risers and a tread Blondel's step — 63 cm by
//! default — but never shallower than the least tread: 280 cm climbs in 16
//! risers of 17.5 and treads of 28. A stair has one tread fewer than it has
//! risers; the last riser lands on the floor above.
//!
//! **Plan.** Straight; L, turning a quarter at a square landing; U, turning
//! back at a landing as wide as both flights; helical, its treads sectors
//! round a newel, measured on the walking line two thirds of the way out.
//! Each is drawn as its outline, a line across each tread, the walking line
//! with an arrow at its top, and the cut line where a plan 1.2 m above the
//! floor cuts the stair.
//!
//! **Section.** The profile of risers and treads, with the floors below and
//! above and the underside of the stair a waist below the nosings; for an L
//! and a helix, the profile unrolled along the walking line, landings flat.
//! A U is cut as a building section along its flights cuts it (#358): the
//! first flight and the landing cut, the second flight seen beyond, coming
//! back over the first in the same length — not unrolled to twice it.
//!
//! **Across.** A straight or U stair cut square to its flights at a
//! distance along the climb (#358): each flight the cut meets, cut at its
//! height there; the steps it climbs to beyond, and the landing.
//!
//! **In a room.** [`fit`] finds where a straight, L, or U stair goes in a
//! rectangular room: against a corner, turned a quarter at a time, with
//! free floor at its foot to step onto it and clear of what must be kept
//! clear — the floor a door swings over.
//!
//! Lengths are in metres here; the caller scales them to the drawing.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Aabb, Point2, Vec2};

/// What a stair is to climb, and the rule it is sized by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rule {
    /// Two risers and a tread, metres: 0.63 by default.
    pub step: f64,
    /// The highest riser allowed.
    pub max_riser: f64,
    /// The shallowest tread allowed.
    pub min_tread: f64,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            step: 0.63,
            max_riser: 0.18,
            min_tread: 0.28,
        }
    }
}

/// A stair's risers and treads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sizing {
    pub risers: usize,
    pub riser: f64,
    pub tread: f64,
}

impl Sizing {
    /// How many treads: one fewer than the risers.
    pub fn treads(&self) -> usize {
        self.risers - 1
    }
    /// The going: the treads' depth together, landings left out.
    pub fn going(&self) -> f64 {
        self.tread * self.treads() as f64
    }
    /// As a stair's label says it: risers and their height, the tread, in
    /// centimetres — `16 R × 17.5 / T 28.0`.
    pub fn words(&self) -> String {
        format!(
            "{} R × {:.1} / T {:.1}",
            self.risers,
            self.riser * 100.0,
            self.tread * 100.0
        )
    }
    /// Blondel's step as sized: two risers and a tread.
    pub fn step(&self) -> f64 {
        2.0 * self.riser + self.tread
    }
}

/// Why a stair cannot be sized or drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StairError {
    /// A size that is not one: the height, the width, a radius.
    NotPositive(&'static str),
    /// The rule cannot be kept: the step is less than two of the highest
    /// risers and the least tread.
    Rule,
    /// A helix's newel is as wide as the stair or wider.
    Newel,
    /// A helix climbs less than [`HEADROOM`] in a full turn: one would hit
    /// one's head on the stair above. Carries what a turn climbs.
    Headroom(f64),
    /// A cut across is drawn for a straight or U stair only.
    Across,
}

/// What a helix must climb in one full turn: the headroom under the stair
/// above.
pub const HEADROOM: f64 = 2.0;

/// The stair that climbs `height` by `rule`.
pub fn size(height: f64, rule: Rule) -> Result<Sizing, StairError> {
    if !(height.is_finite() && height > 0.0) {
        return Err(StairError::NotPositive("height"));
    }
    if !(rule.step > 0.0 && rule.max_riser > 0.0 && rule.min_tread > 0.0) {
        return Err(StairError::Rule);
    }
    // The fewest risers no higher than allowed; more while the tread the
    // step leaves is too shallow, as long as risers shrink.
    let mut risers = ((height / rule.max_riser) - 1e-9).ceil().max(2.0) as usize;
    loop {
        let riser = height / risers as f64;
        let tread = rule.step - 2.0 * riser;
        if tread >= rule.min_tread - 1e-9 {
            return Ok(Sizing {
                risers,
                riser,
                tread,
            });
        }
        if risers > 1000 {
            return Err(StairError::Rule);
        }
        risers += 1;
    }
}

/// The kinds of stair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Straight,
    /// Turning a quarter to the left at a landing.
    L,
    /// Turning back to the left at a landing.
    U,
    /// Round a newel, counter-clockwise going up.
    Helical,
}

/// What a stair is drawn from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stair {
    pub kind: Kind,
    pub height: f64,
    pub width: f64,
    /// A helix's newel radius.
    pub newel: f64,
    pub rule: Rule,
}

/// A piece of a stair's drawing, in metres from the foot of the stair: its
/// first riser's middle, the stair climbing along +y.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Line(Point2, Point2),
    /// Counter-clockwise from `start` to `end`, radians.
    Arc {
        center: Point2,
        radius: f64,
        start: f64,
        end: f64,
    },
    /// A closed or open polyline of straight segments.
    Polyline {
        points: Vec<Point2>,
        closed: bool,
    },
    /// A label's text at a point.
    Text {
        at: Point2,
        height: f64,
        words: String,
    },
}

/// What a stair's drawing holds, sorted by what it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    /// The outline and the treads.
    pub treads: Vec<Piece>,
    /// The walking line and its arrow.
    pub walking: Vec<Piece>,
    /// The cut line.
    pub cut: Vec<Piece>,
    /// The riser count and size.
    pub label: Vec<Piece>,
    /// In a section, what is seen beyond the cut: a U's second flight.
    pub beyond: Vec<Piece>,
}

/// The height a plan cuts a stair at.
pub const CUT_HEIGHT: f64 = 1.2;
/// The waist: the stair's thickness under its nosings, in section.
pub const WAIST: f64 = 0.15;
/// The floors' thickness in section.
pub const SLAB: f64 = 0.2;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

/// A walking-line arrow at `tip`, pointing along `towards`.
fn arrow(tip: Point2, towards: Vec2, size: f64) -> Piece {
    let u = towards * (1.0 / towards.length());
    let n = Vec2::new(-u.y, u.x);
    let back = tip - u * size;
    Piece::Polyline {
        points: vec![back + n * (size * 0.4), tip, back - n * (size * 0.4)],
        closed: false,
    }
}

/// The label: risers and their size, treads and theirs, in centimetres.
fn label(sizing: &Sizing, at: Point2) -> Piece {
    Piece::Text {
        at,
        height: 0.12,
        words: sizing.words(),
    }
}

/// Which tread a plan at [`CUT_HEIGHT`] cuts: the first whose riser tops
/// it, counted from the foot.
fn cut_tread(sizing: &Sizing) -> usize {
    ((CUT_HEIGHT / sizing.riser).floor() as usize).clamp(1, sizing.treads().max(1))
}

/// A cut line across a flight: a zigzag break from `a` to `b`.
fn break_line(a: Point2, b: Point2) -> Piece {
    let d = b - a;
    let m = a + d * 0.5;
    let n = Vec2::new(-d.y, d.x) * (0.08 / d.length());
    let along = d * (0.06 / d.length());
    Piece::Polyline {
        points: vec![a, m - along + n, m + along - n, b],
        closed: false,
    }
}

impl Stair {
    fn check(&self) -> Result<Sizing, StairError> {
        if !(self.width.is_finite() && self.width > 0.0) {
            return Err(StairError::NotPositive("width"));
        }
        if self.kind == Kind::Helical {
            if !(self.newel.is_finite() && self.newel > 0.0) {
                return Err(StairError::NotPositive("newel radius"));
            }
            if self.newel >= self.width {
                return Err(StairError::Newel);
            }
            let sizing = size(self.height, self.rule)?;
            // A turn's treads on the walking line, and what they climb.
            let walking = self.newel + (self.width - self.newel) * 2.0 / 3.0;
            let per_turn = std::f64::consts::TAU * walking / sizing.tread;
            let climb = per_turn * sizing.riser;
            if climb < HEADROOM {
                return Err(StairError::Headroom(climb));
            }
            return Ok(sizing);
        }
        size(self.height, self.rule)
    }

    /// The stair's sizing.
    pub fn sizing(&self) -> Result<Sizing, StairError> {
        self.check()
    }

    /// How the treads are split between flights: the first flight's, the
    /// second's (none for a straight stair or a helix).
    fn flights(&self, sizing: &Sizing) -> (usize, usize) {
        match self.kind {
            Kind::Straight | Kind::Helical => (sizing.treads(), 0),
            // One riser climbs onto the landing, the landing is a tread
            // deep as the stair is wide: the treads less one, halved.
            Kind::L | Kind::U => {
                let rest = sizing.treads().saturating_sub(1);
                (rest.div_ceil(2), rest / 2)
            }
        }
    }

    /// The box the stair takes in plan, in its own frame: its flights and
    /// landings, for a helix the circle it turns in.
    pub fn footprint(&self) -> Result<Aabb, StairError> {
        let sizing = self.check()?;
        let (w, g, half) = (self.width, sizing.tread, self.width / 2.0);
        let (first, second) = self.flights(&sizing);
        let (a, b) = (g * first as f64, g * second as f64);
        Ok(match self.kind {
            Kind::Straight => Aabb::from_points(p(-half, 0.0), p(half, sizing.going())),
            Kind::L => Aabb::from_points(p(-half - b, 0.0), p(half, a + w)),
            Kind::U => Aabb::from_points(p(-half - w, (a - b).min(0.0)), p(half, a + w)),
            Kind::Helical => {
                let c = p(-(self.newel + w) / 2.0, 0.0);
                Aabb::from_points(c - Vec2::new(w, w), c + Vec2::new(w, w))
            }
        })
    }

    /// The stair in plan: from its foot at the origin, climbing along +y.
    pub fn plan(&self) -> Result<Drawing, StairError> {
        let sizing = self.check()?;
        let (w, g) = (self.width, sizing.tread);
        let half = w / 2.0;
        let mut out = Drawing::default();
        let cut = cut_tread(&sizing);
        match self.kind {
            Kind::Straight => {
                let length = sizing.going();
                out.treads.push(Piece::Polyline {
                    points: vec![
                        p(-half, 0.0),
                        p(half, 0.0),
                        p(half, length),
                        p(-half, length),
                    ],
                    closed: true,
                });
                for k in 1..sizing.treads() {
                    let y = g * k as f64;
                    out.treads.push(Piece::Line(p(-half, y), p(half, y)));
                }
                out.walking
                    .push(Piece::Line(p(0.0, g / 2.0), p(0.0, length)));
                out.walking
                    .push(arrow(p(0.0, length), Vec2::new(0.0, 1.0), 0.15));
                let y = g * cut as f64 + g / 2.0;
                out.cut
                    .push(break_line(p(-half, y - g / 2.0), p(half, y + g / 2.0)));
                out.label.push(label(&sizing, p(half + 0.1, 0.0)));
            }
            Kind::L | Kind::U => {
                let (first, second) = self.flights(&sizing);
                let a = g * first as f64;
                // The first flight, then the landing a stair's width square.
                out.treads.push(Piece::Polyline {
                    points: vec![p(-half, 0.0), p(half, 0.0), p(half, a), p(-half, a)],
                    closed: true,
                });
                for k in 1..first {
                    let y = g * k as f64;
                    out.treads.push(Piece::Line(p(-half, y), p(half, y)));
                }
                let b = g * second as f64;
                let (landing, flight, walk_end, arrow_dir) = if self.kind == Kind::L {
                    // Turning left: the second flight runs along -x from
                    // the landing's left side.
                    let landing = vec![p(-half, a), p(half, a), p(half, a + w), p(-half, a + w)];
                    let flight = vec![
                        p(-half, a),
                        p(-half, a + w),
                        p(-half - b, a + w),
                        p(-half - b, a),
                    ];
                    for k in 1..second.max(1) {
                        let x = -half - g * k as f64;
                        out.treads.push(Piece::Line(p(x, a), p(x, a + w)));
                    }
                    (
                        landing,
                        flight,
                        p(-half - b, a + half),
                        Vec2::new(-1.0, 0.0),
                    )
                } else {
                    // Turning back: the landing spans both flights, the
                    // second flight beside the first, coming down along -y.
                    let gap = 0.0;
                    let left = -half - gap - w;
                    let landing = vec![p(left, a), p(half, a), p(half, a + w), p(left, a + w)];
                    let flight = vec![p(left, a), p(-half, a), p(-half, a - b), p(left, a - b)];
                    for k in 1..second.max(1) {
                        let y = a - g * k as f64;
                        out.treads.push(Piece::Line(p(left, y), p(-half, y)));
                    }
                    (landing, flight, p(left + half, a - b), Vec2::new(0.0, -1.0))
                };
                out.treads.push(Piece::Polyline {
                    points: landing,
                    closed: true,
                });
                out.treads.push(Piece::Polyline {
                    points: flight,
                    closed: true,
                });
                // The walking line up the first flight, round the landing,
                // along the second.
                let turn = p(0.0, a + half);
                let mut walk = vec![p(0.0, g / 2.0), turn];
                if self.kind == Kind::U {
                    walk.push(p(-half - w / 2.0, a + half));
                }
                walk.push(walk_end);
                out.walking.push(Piece::Polyline {
                    points: walk,
                    closed: false,
                });
                out.walking.push(arrow(walk_end, arrow_dir, 0.15));
                if cut < first {
                    let y = g * cut as f64 + g / 2.0;
                    out.cut
                        .push(break_line(p(-half, y - g / 2.0), p(half, y + g / 2.0)));
                }
                out.label.push(label(&sizing, p(half + 0.1, 0.0)));
            }
            Kind::Helical => {
                let (r0, r1) = (self.newel, self.width);
                // The walking line two thirds of the way from the newel.
                let rw = r0 + (r1 - r0) * 2.0 / 3.0;
                let step = g / rw;
                let n = sizing.treads();
                let sweep = step * n as f64;
                // The foot's first riser along -x of the centre... the
                // stair starts at angle -90°, its first riser the radius
                // there, the centre at the origin shifted so the foot's
                // middle is at the origin.
                let center = p(-(r0 + r1) / 2.0, 0.0);
                let start = 0.0_f64;
                let at = |radius: f64, angle: f64| {
                    center + Vec2::new(radius * angle.cos(), radius * angle.sin())
                };
                out.treads.push(Piece::Arc {
                    center,
                    radius: r0,
                    start: 0.0,
                    end: std::f64::consts::TAU,
                });
                out.treads.push(Piece::Arc {
                    center,
                    radius: r1,
                    start,
                    end: start + sweep,
                });
                for k in 0..=n {
                    let angle = start + step * k as f64;
                    out.treads.push(Piece::Line(at(r0, angle), at(r1, angle)));
                }
                out.walking.push(Piece::Arc {
                    center,
                    radius: rw,
                    start: start + step / 2.0,
                    end: start + sweep,
                });
                let end = start + sweep;
                out.walking
                    .push(arrow(at(rw, end), Vec2::new(-end.sin(), end.cos()), 0.15));
                let angle = start + step * cut as f64 + step / 2.0;
                out.cut.push(break_line(at(r0, angle), at(r1, angle)));
                out.label.push(label(&sizing, at(r1 + 0.1, start)));
            }
        }
        Ok(out)
    }

    /// What a building section through the stair's first flight cuts
    /// (#338): its profile of risers and treads and its underside, the
    /// foot at the origin, climbing along +x — the whole stair when it is
    /// straight; the first flight and its landing, as deep as the stair is
    /// wide, for an L or a U.
    pub fn cut_profile(&self) -> Result<[Vec<Point2>; 2], StairError> {
        let sizing = self.check()?;
        let (h, g) = (sizing.riser, sizing.tread);
        let (first, _) = self.flights(&sizing);
        let (risers, landing) = match self.kind {
            Kind::L | Kind::U => (first + 1, Some(self.width)),
            Kind::Straight | Kind::Helical => (sizing.risers, None),
        };
        let mut profile = vec![p(0.0, 0.0)];
        let (mut x, mut y) = (0.0, 0.0);
        for k in 0..risers {
            y += h;
            profile.push(p(x, y));
            if k + 1 == risers {
                if let Some(depth) = landing {
                    x += depth;
                    profile.push(p(x, y));
                }
                break;
            }
            x += g;
            profile.push(p(x, y));
        }
        let pitch = (g * g + h * h).sqrt() / g;
        let mut underside = vec![p(0.0, 0.0)];
        for pair in profile.windows(2) {
            if pair[1].y > pair[0].y {
                underside.push(pair[1] - Vec2::new(0.0, WAIST * pitch));
            } else if pair[1].x - pair[0].x > g + 1e-9 {
                underside.push(pair[1] - Vec2::new(0.0, WAIST));
            }
        }
        Ok([profile, underside])
    }

    /// The stair in section: the foot at the origin, the floor below along
    /// y = 0, the floor above at the height, climbing along +x. A U is cut
    /// along its flights ([`Self::along`]); the others unrolled along their
    /// walking line.
    pub fn section(&self) -> Result<Drawing, StairError> {
        if self.kind == Kind::U {
            return self.along();
        }
        self.unrolled()
    }

    /// A U cut along its first flight, as a building section through it
    /// cuts it (#358): that flight and the landing cut, one closed outline
    /// from the nosings to the underside a waist below; the second flight
    /// beyond, coming back from the landing over the first to the floor
    /// above; the floor above reached where it ends. Its length is the
    /// flights' and the landing's — the stair's footprint — never twice it.
    fn along(&self) -> Result<Drawing, StairError> {
        let sizing = self.check()?;
        let (h, g, w) = (sizing.riser, sizing.tread, self.width);
        let (first, second) = self.flights(&sizing);
        let (a, b) = (g * first as f64, g * second as f64);
        let landing = h * (first + 1) as f64;
        let top = h * sizing.risers as f64;
        let pitch = (g * g + h * h).sqrt() / g;
        // The first flight up to the landing, and the landing.
        let mut nosings = vec![p(0.0, 0.0)];
        let (mut x, mut y) = (0.0, 0.0);
        for k in 0..=first {
            y += h;
            nosings.push(p(x, y));
            x += if k == first { w } else { g };
            nosings.push(p(x, y));
        }
        let mut cut = nosings.clone();
        // The underside back to the foot: flat under the landing, then a
        // waist below the nosings, square to the pitch.
        cut.push(p(a + w, landing - WAIST));
        cut.push(p(a, landing - WAIST));
        // At the foot the underside meets the floor, never below it.
        cut.push(p(0.0, (h - WAIST * pitch).max(0.0)));
        cut.push(p(0.0, 0.0));
        let mut out = Drawing::default();
        out.treads.push(Piece::Polyline {
            points: cut,
            closed: true,
        });
        // The second flight beyond: from the landing's near edge back
        // towards the foot, a riser and a tread each, the last riser to
        // the floor above.
        let mut back = vec![p(a, landing)];
        let (mut x, mut y) = (a, landing);
        for j in 0..=second {
            y += h;
            back.push(p(x, y));
            if j == second {
                break;
            }
            x -= g;
            back.push(p(x, y));
        }
        out.beyond.push(Piece::Polyline {
            points: back,
            closed: false,
        });
        out.beyond.push(Piece::Line(
            p(a, landing + h - WAIST * pitch),
            p(a - b, top - WAIST * pitch),
        ));
        // The floors: below, from before the foot; above, from where the
        // second flight arrives, back over the foot.
        out.treads.push(Piece::Polyline {
            points: vec![p(-1.0, 0.0), p(0.0, 0.0), p(0.0, -SLAB), p(-1.0, -SLAB)],
            closed: false,
        });
        out.treads.push(Piece::Polyline {
            points: vec![
                p(a - b, top),
                p(a - b - 1.0, top),
                p(a - b - 1.0, top - SLAB),
                p(a - b, top - SLAB),
            ],
            closed: false,
        });
        out.label.push(label(&sizing, p(0.0, -SLAB - 0.25)));
        Ok(out)
    }

    /// A straight or U stair cut square to its flights `at` metres along
    /// the climb from its foot (#358), seen looking up the first flight:
    /// across, the stair's own x — the first flight from −w/2 to w/2, a U's
    /// second beside it from −3w/2 to −w/2 — and up, the height from the
    /// floor below. Each flight the cut meets is cut: from the tread there
    /// down to its underside, a waist below the nosings. Beyond it, the
    /// nosings of the first flight's steps still to climb, and the landing;
    /// a U's second flight, when the cut is short of it, as the nosings it
    /// climbs by.
    pub fn across(&self, at: f64) -> Result<Drawing, StairError> {
        if !matches!(self.kind, Kind::Straight | Kind::U) {
            return Err(StairError::Across);
        }
        let sizing = self.check()?;
        let (h, g, w) = (sizing.riser, sizing.tread, self.width);
        let half = w / 2.0;
        let pitch = (g * g + h * h).sqrt() / g;
        let (first, second) = match self.kind {
            Kind::U => self.flights(&sizing),
            _ => (sizing.treads(), 0),
        };
        let a = g * first as f64;
        let b = g * second as f64;
        let landing = (self.kind == Kind::U).then(|| h * (first + 1) as f64);
        let mut out = Drawing::default();
        let block = |x0: f64, x1: f64, low: f64, high: f64| Piece::Polyline {
            points: vec![p(x0, low), p(x1, low), p(x1, high), p(x0, high)],
            closed: true,
        };
        // The first flight: the step the cut is on, and those beyond.
        let last = if landing.is_some() {
            first
        } else {
            sizing.treads()
        };
        if (0.0..g * last as f64).contains(&at) {
            let k = (at / g).floor() as usize;
            let tread = h * (k + 1) as f64;
            let under = (h + at * h / g - WAIST * pitch).max(0.0);
            out.treads.push(block(-half, half, under, tread));
            for j in k + 1..last {
                let y = h * (j + 1) as f64;
                out.beyond.push(Piece::Line(p(-half, y), p(half, y)));
            }
        }
        if let Some(level) = landing {
            let left = -half - w;
            if (a..=a + w).contains(&at) {
                out.treads.push(block(left, half, level - WAIST, level));
            } else if at < a {
                out.beyond.push(Piece::Line(p(left, level), p(half, level)));
            }
            // The second flight: cut where the cut meets it, its nosings
            // seen when the cut is short of it.
            if (a - b..a).contains(&at) {
                let d = a - at;
                let j = (d / g).floor() as usize;
                let tread = level + h * (j + 1) as f64;
                let under = level + h + d * h / g - WAIST * pitch;
                out.treads.push(block(left, -half, under, tread));
            } else if at < a - b {
                for j in 0..=second {
                    let y = level + h * (j + 1) as f64;
                    out.beyond.push(Piece::Line(p(left, y), p(-half, y)));
                }
            }
        }
        // The floor below, across the stair.
        let left = if landing.is_some() { -half - w } else { -half };
        out.treads.push(Piece::Polyline {
            points: vec![
                p(left - 0.3, 0.0),
                p(half + 0.3, 0.0),
                p(half + 0.3, -SLAB),
                p(left - 0.3, -SLAB),
            ],
            closed: false,
        });
        out.label.push(label(&sizing, p(left, -SLAB - 0.25)));
        Ok(out)
    }

    /// The stair in section, unrolled along its walking line.
    fn unrolled(&self) -> Result<Drawing, StairError> {
        let sizing = self.check()?;
        let (h, g) = (sizing.riser, sizing.tread);
        let (first, _) = self.flights(&sizing);
        // The landing, for an L or a U, after the first flight and its
        // riser: as deep as the walking line runs round it.
        let landing = match self.kind {
            Kind::Straight | Kind::Helical => None,
            Kind::L => Some((first, self.width)),
            Kind::U => Some((first, 2.0 * self.width)),
        };
        let mut profile = vec![p(0.0, 0.0)];
        let (mut x, mut y) = (0.0, 0.0);
        for k in 0..sizing.risers {
            y += h;
            profile.push(p(x, y));
            if k + 1 == sizing.risers {
                break;
            }
            let deep = match landing {
                Some((at, depth)) if k == at => depth,
                _ => g,
            };
            x += deep;
            profile.push(p(x, y));
        }
        let top = p(x, y);
        let mut out = Drawing::default();
        out.treads.push(Piece::Polyline {
            points: profile.clone(),
            closed: false,
        });
        // The underside: under every nosing — each riser's top — by the
        // waist measured square to the flight's pitch, the landing alike.
        let pitch = (g * g + h * h).sqrt() / g;
        // A landing — a run deeper than a tread — is flat underneath too:
        // its far end under it by the waist.
        let mut underside: Vec<Point2> = Vec::new();
        for pair in profile.windows(2) {
            let rise = pair[1].y > pair[0].y;
            let landing = !rise && pair[1].x - pair[0].x > g + 1e-9;
            if rise {
                underside.push(pair[1] - Vec2::new(0.0, WAIST * pitch));
            } else if landing {
                underside.push(pair[1] - Vec2::new(0.0, WAIST));
            }
        }
        out.treads.push(Piece::Polyline {
            points: underside,
            closed: false,
        });
        // The floors: below, from before the foot; above, from the top.
        out.treads.push(Piece::Polyline {
            points: vec![p(-1.0, 0.0), p(0.0, 0.0), p(0.0, -SLAB), p(-1.0, -SLAB)],
            closed: false,
        });
        out.treads.push(Piece::Polyline {
            points: vec![
                top,
                p(top.x + 1.0, top.y),
                p(top.x + 1.0, top.y - SLAB),
                p(top.x, top.y - SLAB),
            ],
            closed: false,
        });
        out.label.push(label(&sizing, p(0.0, -SLAB - 0.25)));
        Ok(out)
    }
}

/// The free floor at a stair's foot, as deep as this, to step onto it.
pub const APPROACH: f64 = 0.9;

/// Where a stair goes in a room: its kind, its foot, and the quarter
/// turns of its frame (radians, counter-clockwise), climbing along its
/// frame's +y as [`Stair::plan`] draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub kind: Kind,
    pub foot: Point2,
    pub turn: f64,
}

/// `q` turned by `quarters` quarter turns counter-clockwise, exactly.
fn quarter(q: Point2, quarters: usize) -> Point2 {
    match quarters % 4 {
        0 => q,
        1 => p(-q.y, q.x),
        2 => p(-q.x, -q.y),
        _ => p(q.y, -q.x),
    }
}

fn turned(b: Aabb, quarters: usize) -> Aabb {
    let [a, _, c, _] = b.corners();
    Aabb::from_points(quarter(a, quarters), quarter(c, quarters))
}

fn shifted(b: Aabb, by: Vec2) -> Aabb {
    Aabb::from_points(b.min() + by, b.max() + by)
}

/// Whether two boxes share more than an edge.
fn overlaps(a: &Aabb, b: &Aabb) -> bool {
    a.min().x < b.max().x && b.min().x < a.max().x && a.min().y < b.max().y && b.min().y < a.max().y
}

/// Where `stair`, as the first of `kinds` that fits, goes in `room`: the
/// stair and the [`APPROACH`] at its foot inside the room, against one of
/// its corners or else against a wall or just past what is kept clear,
/// the stair itself overlapping none of `clear`. Kinds are tried in
/// order, then turns, then the corners before the other places. `None`
/// when no kind fits anywhere.
pub fn fit(stair: &Stair, kinds: &[Kind], room: Aabb, clear: &[Aabb]) -> Option<Placement> {
    let tolerance = LinearTolerance::DEFAULT
        .around(&[room.min(), room.max()])
        .value();
    let inside = room.expanded(tolerance);
    for &kind in kinds {
        let candidate = Stair { kind, ..*stair };
        let Ok(footprint) = candidate.footprint() else {
            continue;
        };
        let half = stair.width / 2.0;
        let approach = Aabb::from_points(p(-half, -APPROACH), p(half, 0.0));
        let taken = footprint.union(&approach);
        for quarters in 0..4 {
            let (stair_box, whole) = (turned(footprint, quarters), turned(taken, quarters));
            // Against a wall, or just past what must be kept clear: the
            // corners first.
            let walls_x = [room.min().x - whole.min().x, room.max().x - whole.max().x];
            let walls_y = [room.min().y - whole.min().y, room.max().y - whole.max().y];
            let past_x = clear
                .iter()
                .flat_map(|c| [c.max().x - stair_box.min().x, c.min().x - stair_box.max().x]);
            let past_y = clear
                .iter()
                .flat_map(|c| [c.max().y - stair_box.min().y, c.min().y - stair_box.max().y]);
            let xs: Vec<f64> = walls_x.into_iter().chain(past_x).collect();
            let ys: Vec<f64> = walls_y.into_iter().chain(past_y).collect();
            let corners = (0..2).flat_map(|i| (0..2).map(move |j| (i, j)));
            let along = (0..xs.len())
                .flat_map(|i| (0..ys.len()).map(move |j| (i, j)))
                .filter(|(i, j)| *i >= 2 || *j >= 2);
            for (i, j) in corners.chain(along) {
                let by = Vec2::new(xs[i], ys[j]);
                if inside.contains(&shifted(whole, by))
                    && !clear.iter().any(|c| overlaps(c, &shifted(stair_box, by)))
                {
                    return Some(Placement {
                        kind,
                        foot: Point2::ORIGIN + by,
                        turn: std::f64::consts::FRAC_PI_2 * quarters as f64,
                    });
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn blondels_rule_sizes_a_storey_of_280_in_sixteen_risers_of_17_5() {
        let s = size(2.8, Rule::default()).unwrap();
        assert_eq!(s.risers, 16);
        assert!(close(s.riser, 0.175) && close(s.tread, 0.28));
        assert!(close(s.step(), 0.63));
        assert_eq!(s.treads(), 15);
        assert!(close(s.going(), 4.2));
    }

    #[test]
    fn the_rule_holds_at_its_bounds_and_more_risers_keep_the_tread() {
        let rule = Rule::default();
        for height in [0.5, 1.0, 2.5, 2.7, 2.88, 3.0, 3.24, 4.5, 6.0] {
            let s = size(height, rule).unwrap();
            assert!(
                s.riser <= rule.max_riser + 1e-9,
                "{height}: riser {}",
                s.riser
            );
            assert!(
                s.tread >= rule.min_tread - 1e-9,
                "{height}: tread {}",
                s.tread
            );
            assert!(close(s.step(), rule.step), "{height}");
            assert!(close(s.riser * s.risers as f64, height));
            // The fewest that do: one fewer breaks a bound.
            if s.risers > 2 {
                let fewer = height / (s.risers - 1) as f64;
                assert!(fewer > rule.max_riser || rule.step - 2.0 * fewer < rule.min_tread);
            }
        }
        // Exactly the highest riser: 18 × 16 = 288 takes 16, treads 27…
        // too shallow, so 17 risers.
        let s = size(2.88, rule).unwrap();
        assert_eq!(s.risers, 17);
        assert!(size(0.0, rule).is_err());
        let impossible = Rule { step: 0.5, ..rule };
        // 2 × 0.18 + 0.28 = 0.64 > 0.5: only a riser of 0.11 leaves 0.28.
        let s = size(2.8, impossible).unwrap();
        assert!(s.riser <= 0.11 + 1e-9);
    }

    fn stair(kind: Kind) -> Stair {
        Stair {
            kind,
            height: 2.8,
            width: 0.9,
            newel: 0.15,
            rule: Rule::default(),
        }
    }

    #[allow(clippy::ptr_arg)]
    fn lines(d: &Drawing) -> usize {
        d.treads
            .iter()
            .filter(|p| matches!(p, Piece::Line(..)))
            .count()
    }

    #[test]
    fn a_straight_stair_in_plan_has_a_line_across_each_tread_and_its_walking_line() {
        let d = stair(Kind::Straight).plan().unwrap();
        // 15 treads: the outline and 14 lines between them.
        assert_eq!(lines(&d), 14);
        match &d.treads[0] {
            Piece::Polyline { points, closed } => {
                assert!(*closed);
                assert!(close(points[2].y, 4.2) && close(points[1].x - points[0].x, 0.9));
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(d.walking.len(), 2, "the line and its arrow");
        assert_eq!(d.cut.len(), 1);
        // Cut at 1.2 m: past the sixth riser (6 × 17.5 = 105), before the
        // seventh's 122.5.
        match &d.cut[0] {
            Piece::Polyline { points, .. } => {
                let mid = (points[0].y + points[3].y) / 2.0;
                assert!(mid > 0.28 * 6.0 && mid < 0.28 * 7.0, "{mid}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn l_u_and_helical_stairs_turn_with_their_treads_all_drawn() {
        for kind in [Kind::L, Kind::U] {
            let s = stair(kind);
            let sizing = s.sizing().unwrap();
            let (a, b) = s.flights(&sizing);
            // The landing takes one tread's riser: the rest in two flights.
            assert_eq!(a + b, sizing.treads() - 1, "{kind:?}");
            let d = s.plan().unwrap();
            assert_eq!(lines(&d), (a - 1) + (b - 1), "{kind:?}");
        }
        let d = stair(Kind::Helical).plan().unwrap();
        // A radius at each tread's edge: 16 for 15 treads.
        assert_eq!(lines(&d), 16);
        assert!(stair(Kind::Helical).plan().is_ok());
        let wide = Stair {
            newel: 1.0,
            ..stair(Kind::Helical)
        };
        assert_eq!(wide.plan(), Err(StairError::Newel));
        // A tight helix: a turn of its treads climbs too little to stand
        // under — 1.1 m wide round a 0.1 newel climbs 2.6; 0.6 wide, 1.4.
        let tight = Stair {
            width: 0.6,
            newel: 0.1,
            ..stair(Kind::Helical)
        };
        match tight.plan() {
            Err(StairError::Headroom(climb)) => assert!(climb < HEADROOM, "{climb}"),
            other => panic!("{other:?}"),
        }
        assert!(
            stair(Kind::Helical).plan().is_ok(),
            "0.9 round 0.15 climbs 2.56"
        );
    }

    #[test]
    fn a_section_climbs_the_height_in_its_risers_and_runs_its_treads() {
        let d = stair(Kind::Straight).section().unwrap();
        let Piece::Polyline { points, .. } = &d.treads[0] else {
            panic!()
        };
        // Foot, then a riser and a tread each, the last riser to the floor.
        assert_eq!(points.len(), 1 + 16 + 15);
        let top = points.last().unwrap();
        assert!(close(top.y, 2.8) && close(top.x, 4.2));
        // Every rise is a riser, every run a tread.
        for pair in points.windows(2) {
            let d = pair[1] - pair[0];
            assert!(close(d.x, 0.0) && close(d.y, 0.175) || close(d.y, 0.0) && close(d.x, 0.28));
        }
        // An L's landing is a stair's width deep, and flat underneath.
        let l = stair(Kind::L).section().unwrap();
        let Piece::Polyline { points, .. } = &l.treads[0] else {
            panic!()
        };
        let landing = points
            .windows(2)
            .find(|w| close(w[1].x - w[0].x, 0.9))
            .expect("the landing");
        let Piece::Polyline { points: under, .. } = &l.treads[1] else {
            panic!()
        };
        assert!(
            under
                .iter()
                .any(|q| close(q.x, landing[1].x) && close(q.y, landing[1].y - WAIST))
        );
    }

    fn points(piece: &Piece) -> Vec<Point2> {
        match piece {
            Piece::Line(a, b) => vec![*a, *b],
            Piece::Polyline { points, .. } => points.clone(),
            _ => Vec::new(),
        }
    }

    /// A U cut along its flights is as long as its footprint, not twice
    /// it (#358): the house's section drew its stair 7.18 m long in a
    /// 4 m stair zone, the profile unrolled.
    #[test]
    fn a_u_section_is_as_long_as_its_footprint_with_the_second_flight_beyond() {
        let u = Stair {
            height: 3.2,
            width: 1.2,
            ..stair(Kind::U)
        };
        let plan = u.footprint().unwrap();
        let d = u.section().unwrap();
        let cut = points(&d.treads[0]);
        let run = |q: &[Point2]| {
            q.iter().map(|q| q.x).fold(f64::INFINITY, f64::min)
                ..=q.iter().map(|q| q.x).fold(f64::NEG_INFINITY, f64::max)
        };
        // The first flight and the landing, from the foot to the landing's
        // far side: the footprint's length along the climb.
        let along = run(&cut);
        assert!(
            close(*along.start(), 0.0) && close(*along.end(), plan.max().y),
            "{along:?}"
        );
        // The second flight beyond, inside the same length, up to the floor
        // above.
        let back = points(&d.beyond[0]);
        let seen = run(&back);
        assert!(*seen.start() >= plan.min().y - 1e-9 && *seen.end() <= plan.max().y + 1e-9);
        assert!(back.iter().any(|q| close(q.y, 3.2)));
        // Every rise is a riser.
        let sizing = u.sizing().unwrap();
        for pair in back.windows(2) {
            let step = pair[1] - pair[0];
            assert!(close(step.x, 0.0) && close(step.y, sizing.riser) || close(step.y, 0.0));
        }
    }

    /// A cut across meets each flight at the height the plan puts it at
    /// (#358): the house's B-B drew one flight climbing across the house.
    #[test]
    fn a_cut_across_a_u_meets_each_flight_at_its_height_there() {
        let u = Stair {
            height: 3.2,
            width: 1.2,
            ..stair(Kind::U)
        };
        let sizing = u.sizing().unwrap();
        let (h, g, w) = (sizing.riser, sizing.tread, 1.2);
        let (first, second) = u.flights(&sizing);
        let a = g * first as f64;
        let level = h * (first + 1) as f64;
        // Inside both flights' reach: each cut, the second higher.
        let at = a - g * 1.5;
        assert!(at > a - g * second as f64);
        let d = u.across(at).unwrap();
        let blocks: Vec<Vec<Point2>> = d
            .treads
            .iter()
            .filter(|piece| matches!(piece, Piece::Polyline { closed: true, .. }))
            .map(points)
            .collect();
        assert_eq!(blocks.len(), 2, "{blocks:?}");
        let top = |q: &[Point2]| q.iter().map(|q| q.y).fold(f64::NEG_INFINITY, f64::max);
        let right = |q: &[Point2]| q.iter().map(|q| q.x).fold(f64::NEG_INFINITY, f64::max);
        let (one, two) = if right(&blocks[0]) > 0.0 {
            (&blocks[0], &blocks[1])
        } else {
            (&blocks[1], &blocks[0])
        };
        let k = (at / g).floor();
        assert!(close(top(one), h * (k + 1.0)));
        let j = ((a - at) / g).floor();
        assert!(close(top(two), level + h * (j + 1.0)));
        // The first flight between −w/2 and w/2, the second beside it.
        assert!(close(right(one), w / 2.0) && close(right(two), -w / 2.0));
        // The landing beyond, at its level.
        assert!(
            d.beyond
                .iter()
                .any(|piece| points(piece).iter().all(|q| close(q.y, level)))
        );
        // Through the landing: cut flat, across both flights.
        let through = u.across(a + w / 2.0).unwrap();
        let slab = points(&through.treads[0]);
        assert!(close(top(&slab), level) && close(right(&slab), w / 2.0));
        // An L is cut along it, not across.
        assert_eq!(stair(Kind::L).across(0.5), Err(StairError::Across));
    }

    fn room(w: f64, h: f64) -> Aabb {
        Aabb::from_points(p(0.0, 0.0), p(w, h))
    }

    const KINDS: [Kind; 3] = [Kind::Straight, Kind::U, Kind::L];

    #[test]
    fn a_straight_stair_fits_a_long_room_with_floor_at_its_foot() {
        // 15 treads of 28: a going of 4.2, and 0.9 to step on.
        let at = fit(&stair(Kind::Straight), &KINDS, room(1.0, 5.2), &[]).unwrap();
        assert_eq!(at.kind, Kind::Straight);
        assert!(close(at.foot.x, 0.45) && close(at.foot.y, APPROACH));
        assert!(close(at.turn, 0.0));
        // Too short by a centimetre: nothing fits a metre's width.
        assert_eq!(
            fit(&stair(Kind::Straight), &KINDS, room(1.0, 5.09), &[]),
            None
        );
    }

    #[test]
    fn a_room_too_short_for_a_straight_stair_takes_a_u_and_turns_it_to_fit() {
        let at = fit(&stair(Kind::Straight), &KINDS, room(3.0, 4.0), &[]).unwrap();
        assert_eq!(at.kind, Kind::U);
        // The U is 1.8 wide and 2.86 long, 3.76 with its approach: it fits
        // along y, not along x.
        let u = stair(Kind::U).footprint().unwrap();
        assert!(close(u.max().x - u.min().x, 1.8));
        assert!(close(u.max().y - u.min().y, 2.86));
        let turned = at.turn / std::f64::consts::FRAC_PI_2;
        assert!(close(turned.round(), turned) && (turned.round() as usize).is_multiple_of(2));
        // Laid on its side the room is too narrow.
        assert_eq!(fit(&stair(Kind::U), &[Kind::U], room(4.0, 1.7), &[]), None);
    }

    #[test]
    fn a_stair_keeps_clear_of_a_doors_swing_and_turns_away_from_it() {
        let door = Aabb::from_points(p(0.0, 4.5), p(1.0, 5.2));
        let at = fit(&stair(Kind::Straight), &KINDS, room(1.0, 5.2), &[door]).unwrap();
        // Climbing down the room, its foot by the door, its top away.
        assert!(close(at.turn, std::f64::consts::PI));
        assert!(close(at.foot.y, 4.2));
        let placed = turned(stair(Kind::Straight).footprint().unwrap(), 2);
        assert!(!shifted(placed, at.foot - Point2::ORIGIN).intersects(&door));
        // A door over the whole room leaves nowhere.
        assert_eq!(
            fit(
                &stair(Kind::Straight),
                &KINDS,
                room(1.0, 5.2),
                &[room(1.0, 5.2)]
            ),
            None
        );
    }

    #[test]
    fn a_cut_through_the_first_flight_climbs_to_its_landing_or_the_floor_above() {
        let straight = stair(Kind::Straight).cut_profile().unwrap();
        let top = straight[0].last().unwrap();
        assert!(close(top.y, 2.8) && close(top.x, 15.0 * 0.28));
        // A U's first flight: half the treads less one, then the landing
        // as deep as the stair is wide, at the height its risers reach.
        let u = stair(Kind::U).cut_profile().unwrap();
        let sizing = stair(Kind::U).sizing().unwrap();
        let first = (sizing.treads() - 1).div_ceil(2);
        let end = u[0].last().unwrap();
        assert!(close(end.y, (first + 1) as f64 * sizing.riser));
        assert!(close(end.x, first as f64 * sizing.tread + 0.9));
        // The underside stays under the profile.
        for profile in [straight, u] {
            for q in &profile[1][1..] {
                let above = profile[0]
                    .iter()
                    .filter(|r| (r.x - q.x).abs() < 1e-9)
                    .map(|r| r.y)
                    .fold(f64::NEG_INFINITY, f64::max);
                assert!(above > q.y, "{q:?}");
            }
        }
    }

    #[test]
    fn every_footprint_holds_its_plan() {
        for kind in [Kind::Straight, Kind::L, Kind::U, Kind::Helical] {
            let s = Stair {
                width: 1.2,
                ..stair(kind)
            };
            let b = s.footprint().unwrap().expanded(1e-9);
            for piece in s.plan().unwrap().treads {
                let points = match piece {
                    Piece::Line(a, c) => vec![a, c],
                    Piece::Polyline { points, .. } => points,
                    Piece::Arc { center, radius, .. } => vec![
                        center + Vec2::new(radius, radius),
                        center - Vec2::new(radius, radius),
                    ],
                    Piece::Text { .. } => vec![],
                };
                for q in points {
                    assert!(b.contains_point(q), "{kind:?}: {q:?} outside {b:?}");
                }
            }
        }
    }
}
