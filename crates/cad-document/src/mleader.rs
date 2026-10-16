//! Multileaders (#240, ADR-0033): leaders from several places to one
//! note, with a dogleg at the landing, drawn by a multileader style.
//!
//! A multileader stores what defines it — its leaders, the landing they
//! meet at, its content, its style — and is laid out afresh from the style
//! whenever it is drawn, picked, or bounded (ADR-0020). The style table
//! has a permanent `Standard` row.

use cad_core::{Aabb, Point2, Vec2};
use cad_geometry::{Arrowhead, Segment, Transform, normalized};

use crate::block::BlockId;
use crate::names::Named;
use crate::table::{Table, TableChange, TableId};
use crate::text::{Justification, TextEntity};

/// A multileader style. Lengths are in drawing units.
#[derive(Clone, Debug, PartialEq)]
pub struct MLeaderStyle {
    pub name: String,
    /// The length of an arrowhead. Positive.
    pub arrow_size: f64,
    /// Between the dogleg's end and the note. Not negative.
    pub landing_gap: f64,
    /// The horizontal line at the landing; none when zero. Not negative.
    pub dogleg_length: f64,
    /// The height of a text note. Positive.
    pub text_height: f64,
}

/// Why a multileader style cannot be as given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MLeaderStyleError {
    /// The named length must be a positive number.
    NotPositive(&'static str),
    /// The named length must not be negative.
    Negative(&'static str),
}

impl MLeaderStyleError {
    /// The refusal in the user's terms.
    pub fn explain(self) -> String {
        match self {
            Self::NotPositive(what) => format!("The {what} must be greater than zero."),
            Self::Negative(what) => format!("The {what} cannot be negative."),
        }
    }
}

impl MLeaderStyle {
    /// The name of the style every drawing has.
    pub const STANDARD: &'static str = "Standard";

    /// A style with the defaults: arrowheads and text 2.5, a gap of 1, a
    /// dogleg of 5.
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            arrow_size: 2.5,
            landing_gap: 1.0,
            dogleg_length: 5.0,
            text_height: 2.5,
        }
    }

    /// Whether a multileader can be drawn by these values. The name is
    /// checked where the table is.
    pub fn check(&self) -> Result<(), MLeaderStyleError> {
        let positive = |value: f64, what| {
            (value.is_finite() && value > 0.0)
                .then_some(())
                .ok_or(MLeaderStyleError::NotPositive(what))
        };
        let not_negative = |value: f64, what| {
            (value.is_finite() && value >= 0.0)
                .then_some(())
                .ok_or(MLeaderStyleError::Negative(what))
        };
        positive(self.arrow_size, "arrow size")?;
        positive(self.text_height, "text height")?;
        not_negative(self.landing_gap, "landing gap")?;
        not_negative(self.dogleg_length, "dogleg length")?;
        Ok(())
    }
}

impl Named for MLeaderStyle {
    fn name(&self) -> &str {
        &self.name
    }
}

pub type MLeaderStyleId = TableId<MLeaderStyle>;
pub type MLeaderStyleTable = Table<MLeaderStyle>;
/// A reversible change to one multileader style.
pub type MLeaderStyleChange = TableChange<MLeaderStyle>;

/// A new drawing's multileader styles: just `Standard`.
pub(crate) fn initial_mleader_styles() -> MLeaderStyleTable {
    Table::starting_with(MLeaderStyle::named(MLeaderStyle::STANDARD))
}

/// What a multileader points out.
#[derive(Clone, Debug, PartialEq)]
pub enum MLeaderContent {
    /// Nothing: the leaders and the dogleg alone.
    None,
    /// Words, at the style's text height.
    Text(String),
    /// A block, placed at the landing's end at this scale (#241).
    Block { block: BlockId, scale: f64 },
}

/// A multileader.
#[derive(Clone, Debug, PartialEq)]
pub struct MLeaderEntity {
    /// Each leader: its arrowhead's tip, then its bends; it runs on to
    /// the landing. Never empty, each at least one point.
    pub leaders: Vec<Vec<Point2>>,
    /// Where every leader meets, and the dogleg starts.
    pub landing: Point2,
    pub content: MLeaderContent,
    pub style: MLeaderStyleId,
}

/// Why a multileader cannot be as given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MLeaderError {
    /// It has no leader, or a leader with no point.
    NoLeader,
    /// A point that is not finite.
    NotFinite,
}

impl std::fmt::Display for MLeaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoLeader => write!(f, "a multileader needs a leader"),
            Self::NotFinite => write!(f, "a multileader's points must be finite"),
        }
    }
}

impl std::error::Error for MLeaderError {}

/// A multileader drawn: what the renderer draws, picking feels, and the
/// bounds hold.
#[derive(Clone, Debug, PartialEq)]
pub struct MLeaderLayout {
    /// The leaders' segments, then the dogleg.
    pub lines: Vec<Segment>,
    /// One per leader, at its tip.
    pub arrowheads: Vec<Arrowhead>,
    /// A text note, placed past the dogleg.
    pub text: Option<TextEntity>,
    /// Where a block note's base point goes, and which way the note reads
    /// from the dogleg: +1 right, −1 left.
    pub note_at: Point2,
    pub reads: f64,
}

impl MLeaderLayout {
    /// Every straight piece: the lines and the arrowheads' outlines.
    pub fn segments(&self) -> impl Iterator<Item = Segment> + '_ {
        self.lines
            .iter()
            .copied()
            .chain(self.arrowheads.iter().flat_map(|arrow| arrow.segments()))
    }

    pub fn bounds(&self) -> Aabb {
        let mut bounds = self
            .segments()
            .map(|segment| segment.bounds())
            .fold(Aabb::from_points(self.note_at, self.note_at), |a, b| {
                a.union(&b)
            });
        if let Some(text) = &self.text {
            bounds = bounds.union(&text.bounds());
        }
        bounds
    }

    /// The distance to the nearest line, an arrowhead, or the note's box.
    pub fn distance_to(&self, point: Point2) -> f64 {
        self.segments()
            .map(|segment| segment.distance_to_point(point))
            .fold(
                self.text
                    .as_ref()
                    .map_or(f64::INFINITY, |text| text.distance_to(point)),
                f64::min,
            )
    }
}

impl MLeaderEntity {
    /// One leader from `tip` to `landing`, saying `text`, in `Standard`.
    pub fn new(tip: Point2, landing: Point2, content: MLeaderContent) -> Self {
        Self {
            leaders: vec![vec![tip]],
            landing,
            content,
            style: MLeaderStyleId::FIRST,
        }
    }

    #[must_use]
    pub fn in_style(self, style: MLeaderStyleId) -> Self {
        Self { style, ..self }
    }

    pub fn check(&self) -> Result<(), MLeaderError> {
        if self.leaders.is_empty() || self.leaders.iter().any(Vec::is_empty) {
            return Err(MLeaderError::NoLeader);
        }
        let finite = self.points().iter().all(|point| point.is_finite());
        if !finite {
            return Err(MLeaderError::NotFinite);
        }
        if let MLeaderContent::Block { scale, .. } = self.content
            && !(scale.is_finite() && scale > 0.0)
        {
            return Err(MLeaderError::NotFinite);
        }
        Ok(())
    }

    /// Every point it stores, in grip order: each leader's in turn, then
    /// the landing.
    pub fn points(&self) -> Vec<Point2> {
        self.leaders
            .iter()
            .flatten()
            .copied()
            .chain(std::iter::once(self.landing))
            .collect()
    }

    /// The arrowheads' tips: what a multileader snaps to.
    pub fn tips(&self) -> Vec<Point2> {
        self.leaders
            .iter()
            .filter_map(|leader| leader.first().copied())
            .collect()
    }

    /// This multileader with point `index` (as [`Self::points`] orders
    /// them) at `to`.
    pub fn with_point(&self, index: usize, to: Point2) -> Option<Self> {
        let mut moved = self.clone();
        let mut at = index;
        for leader in &mut moved.leaders {
            if at < leader.len() {
                leader[at] = to;
                return Some(moved);
            }
            at -= leader.len();
        }
        (at == 0).then_some(Self {
            landing: to,
            ..moved
        })
    }

    /// Which way the dogleg and note read from the landing: away from the
    /// leaders, as their last points lie; right when they lie on it.
    pub fn reads(&self) -> f64 {
        let left_of: f64 = self
            .leaders
            .iter()
            .filter_map(|leader| leader.last())
            .map(|last| last.x - self.landing.x)
            .sum();
        if left_of <= 0.0 { 1.0 } else { -1.0 }
    }

    /// The multileader laid out by `style`: each leader to the landing with
    /// an arrowhead at its tip, the dogleg, and a text note past it, its
    /// middle on the dogleg's line.
    pub fn layout(&self, style: &MLeaderStyle) -> MLeaderLayout {
        let mut lines = Vec::new();
        let mut arrowheads = Vec::new();
        for leader in &self.leaders {
            let path: Vec<Point2> = leader
                .iter()
                .copied()
                .chain(std::iter::once(self.landing))
                .collect();
            lines.extend(
                path.windows(2)
                    .filter(|pair| pair[0] != pair[1])
                    .map(|pair| Segment::new(pair[0], pair[1])),
            );
            let tip = path[0];
            if let Some(direction) = path[1..].iter().find_map(|next| normalized(tip - *next)) {
                arrowheads.push(Arrowhead::new(tip, direction, style.arrow_size));
            }
        }
        let reads = self.reads();
        let along = Vec2::new(reads, 0.0);
        let dogleg_end = self.landing + along * style.dogleg_length;
        if style.dogleg_length > 0.0 {
            lines.push(Segment::new(self.landing, dogleg_end));
        }
        let note_at = dogleg_end + along * style.landing_gap;
        let text = match &self.content {
            MLeaderContent::Text(words) if !words.trim().is_empty() => Some(
                TextEntity::new(
                    note_at - Vec2::new(0.0, style.text_height / 2.0),
                    style.text_height,
                    0.0,
                    words.clone(),
                )
                .justified(if reads > 0.0 {
                    Justification::Left
                } else {
                    Justification::Right
                }),
            ),
            _ => None,
        };
        MLeaderLayout {
            lines,
            arrowheads,
            text,
            note_at,
            reads,
        }
    }

    #[must_use]
    pub fn transformed(&self, transform: &Transform) -> Self {
        let scale = transform.scale_factor();
        Self {
            leaders: self
                .leaders
                .iter()
                .map(|leader| leader.iter().map(|point| transform.apply(*point)).collect())
                .collect(),
            landing: transform.apply(self.landing),
            content: match &self.content {
                MLeaderContent::Block { block, scale: own } => MLeaderContent::Block {
                    block: *block,
                    scale: own * scale,
                },
                other => other.clone(),
            },
            style: self.style,
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn note(words: &str) -> MLeaderContent {
        MLeaderContent::Text(words.to_owned())
    }

    #[test]
    fn leaders_meet_at_the_landing_and_the_note_reads_away_from_them() {
        let style = MLeaderStyle::named("S");
        let mut mleader = MLeaderEntity::new(p(0.0, 0.0), p(20.0, 10.0), note("Hole"));
        mleader.leaders.push(vec![p(5.0, -10.0), p(10.0, 0.0)]);
        let layout = mleader.layout(&style);
        // One segment, two segments, then the dogleg.
        assert_eq!(layout.lines.len(), 4);
        assert_eq!(layout.lines[3], Segment::new(p(20.0, 10.0), p(25.0, 10.0)));
        assert_eq!(layout.arrowheads.len(), 2);
        assert_eq!(layout.arrowheads[1].tip, p(5.0, -10.0));
        let text = layout.text.unwrap();
        assert_eq!(text.insertion, p(26.0, 10.0 - 1.25));
        assert_eq!(text.justification, Justification::Left);
        assert_eq!(mleader.tips(), vec![p(0.0, 0.0), p(5.0, -10.0)]);

        // Leaders from the right: the note reads left.
        let left = MLeaderEntity::new(p(40.0, 0.0), p(20.0, 10.0), note("L"));
        let layout = left.layout(&style);
        assert_eq!(layout.lines[1], Segment::new(p(20.0, 10.0), p(15.0, 10.0)));
        assert_eq!(layout.text.unwrap().justification, Justification::Right);

        // No dogleg in the style, no note: the leaders alone.
        let bare = MLeaderEntity::new(p(0.0, 0.0), p(10.0, 10.0), MLeaderContent::None);
        let flat = MLeaderStyle {
            dogleg_length: 0.0,
            ..style.clone()
        };
        let layout = bare.layout(&flat);
        assert_eq!(layout.lines.len(), 1);
        assert!(layout.text.is_none());
    }

    #[test]
    fn grips_move_one_point_and_checks_refuse_what_cannot_be_drawn() {
        let mut mleader = MLeaderEntity::new(p(0.0, 0.0), p(20.0, 10.0), note("A"));
        mleader.leaders.push(vec![p(5.0, -10.0), p(10.0, 0.0)]);
        assert_eq!(mleader.points().len(), 4);
        let moved = mleader.with_point(2, p(11.0, 1.0)).unwrap();
        assert_eq!(moved.leaders[1][1], p(11.0, 1.0));
        let landed = mleader.with_point(3, p(30.0, 10.0)).unwrap();
        assert_eq!(landed.landing, p(30.0, 10.0));
        assert!(mleader.with_point(4, p(0.0, 0.0)).is_none());
        assert_eq!(mleader.check(), Ok(()));
        let none = MLeaderEntity {
            leaders: Vec::new(),
            ..mleader.clone()
        };
        assert_eq!(none.check(), Err(MLeaderError::NoLeader));
        let nan = MLeaderEntity::new(p(f64::NAN, 0.0), p(1.0, 1.0), note("x"));
        assert_eq!(nan.check(), Err(MLeaderError::NotFinite));
        assert!(MLeaderStyle::named("S").check().is_ok());
        let bad = MLeaderStyle {
            arrow_size: 0.0,
            ..MLeaderStyle::named("S")
        };
        assert_eq!(
            bad.check(),
            Err(MLeaderStyleError::NotPositive("arrow size"))
        );
    }

    proptest! {
        /// Every leader ends at the landing; every arrowhead's tip is a
        /// leader's first point; the dogleg reads away from the leaders;
        /// everything lies in the bounds; and the layout of a moved
        /// multileader is the moved layout.
        #[test]
        fn the_layout_follows_the_points_and_the_style(
            leaders in prop::collection::vec(prop::collection::vec((-100.0..100.0f64, -100.0..100.0f64), 1..4), 1..4),
            landing in (-100.0..100.0f64, -100.0..100.0f64),
            dogleg in 0.0..10.0f64, gap in 0.0..5.0f64,
            dx in -50.0..50.0f64, dy in -50.0..50.0f64,
        ) {
            let leaders: Vec<Vec<Point2>> = leaders
                .into_iter()
                .map(|points| points.into_iter().map(|(x, y)| p(x, y)).collect())
                .collect();
            let landing = p(landing.0, landing.1);
            let mleader = MLeaderEntity { leaders: leaders.clone(), landing, content: note("Ab"), style: MLeaderStyleId::FIRST };
            let style = MLeaderStyle { dogleg_length: dogleg, landing_gap: gap, ..MLeaderStyle::named("S") };
            let layout = mleader.layout(&style);
            for leader in &leaders {
                let reaches = layout.lines.iter().any(|line| line.end == landing) || leader.iter().all(|point| *point == landing);
                prop_assert!(reaches, "a leader to the landing");
            }
            for arrow in &layout.arrowheads {
                prop_assert!(leaders.iter().any(|leader| leader[0] == arrow.tip));
            }
            let text = layout.text.clone().unwrap();
            prop_assert!((text.insertion.x - (landing.x + layout.reads * (dogleg + gap))).abs() < 1e-9);
            let bounds = layout.bounds().expanded(1e-9);
            for segment in layout.segments() {
                prop_assert!(bounds.contains_point(segment.start) && bounds.contains_point(segment.end));
            }
            let moved = mleader.transformed(&Transform::translation(Vec2::new(dx, dy)));
            let moved_text = moved.layout(&style).text.unwrap();
            prop_assert!(moved_text.insertion.distance_to(text.insertion + Vec2::new(dx, dy)) < 1e-9);
        }
    }
}
