//! Linetypes: the second of the drawing's tables (DESIGN §11, §23.2, §32;
//! ADR-0015).
//!
//! A linetype is a name and a *pattern*: dash and gap lengths in drawing
//! units, alternately, which repeat along an entity's path. A dash of no
//! length is a dot. `Continuous` — no pattern — is the table's permanent
//! first row, so `Linetype::CONTINUOUS` always names a row that exists.
//! The drawing's `LTSCALE` multiplies every pattern; nothing here is in
//! pixels, so a pattern keeps its place on the geometry at every zoom.
//!
//! The patterns a user can load are the standard ones every drafting
//! program ships; complex linetypes, `.lin` files, and per-object scale
//! are out of scope for now.

use crate::names::{NameError, Named, check_name};
use crate::table::{Table, TableChange, TableId};
use crate::{Layer, Linetype};

/// A row of the linetype table.
#[derive(Clone, Debug, PartialEq)]
pub struct LinetypeDef {
    pub name: String,
    /// How the pattern looks, as text: `__ __ __`.
    pub description: String,
    /// Dash, gap, dash, gap, … in drawing units. Empty for a continuous
    /// line; otherwise an even number of lengths, up to
    /// [`LinetypeDef::MAX_LENGTHS`], none negative, not all zero.
    pub pattern: Vec<f64>,
}

impl LinetypeDef {
    /// The name of the linetype every drawing has.
    pub const CONTINUOUS: &'static str = "Continuous";
    /// The most dash and gap lengths a pattern may have.
    pub const MAX_LENGTHS: usize = 8;

    /// The permanent first row: an unbroken line.
    pub fn continuous() -> Self {
        Self {
            name: Self::CONTINUOUS.to_owned(),
            description: "Solid line".to_owned(),
            pattern: Vec::new(),
        }
    }

    /// The patterns that can be loaded by name. Lengths are those of the
    /// standard definitions, in drawing units.
    pub fn built_in() -> [Self; 6] {
        let def = |name: &str, description: &str, pattern: &[f64]| Self {
            name: name.to_owned(),
            description: description.to_owned(),
            pattern: pattern.to_vec(),
        };
        [
            def("Dashed", "__ __ __ __ __ __ __", &[0.5, 0.25]),
            def("Hidden", "_ _ _ _ _ _ _ _ _ _", &[0.25, 0.125]),
            def("Center", "____ _ ____ _ ____ _", &[1.25, 0.25, 0.25, 0.25]),
            def(
                "Phantom",
                "_____ _ _ _____ _ _ ____",
                &[1.25, 0.25, 0.25, 0.25, 0.25, 0.25],
            ),
            def("Dot", ". . . . . . . . . . . .", &[0.0, 0.25]),
            def("Dashdot", "__ . __ . __ . __ . __", &[0.5, 0.25, 0.0, 0.25]),
        ]
    }

    /// The built-in linetype of that name, whatever its case.
    pub fn built_in_named(name: &str) -> Option<Self> {
        Self::built_in()
            .into_iter()
            .find(|def| crate::names::same_name(&def.name, name))
    }

    /// Whether the row is one the table can hold.
    pub fn check(&self) -> Result<(), LinetypeError> {
        check_name(&self.name).map_err(LinetypeError::Name)?;
        if self.pattern.len() > Self::MAX_LENGTHS {
            return Err(LinetypeError::TooLong);
        }
        if !self.pattern.len().is_multiple_of(2) {
            return Err(LinetypeError::Odd);
        }
        if self
            .pattern
            .iter()
            .any(|length| !length.is_finite() || *length < 0.0)
        {
            return Err(LinetypeError::Negative);
        }
        if !self.pattern.is_empty() && self.period() <= 0.0 {
            return Err(LinetypeError::NoPeriod);
        }
        Ok(())
    }

    /// The length of one repeat of the pattern; zero for a continuous line.
    pub fn period(&self) -> f64 {
        // Not `sum`, whose empty result is negative zero.
        self.pattern
            .iter()
            .fold(0.0, |total, length| total + length)
    }

    /// Whether the line is drawn unbroken.
    pub fn is_continuous(&self) -> bool {
        self.pattern.is_empty()
    }
}

impl Named for LinetypeDef {
    fn name(&self) -> &str {
        &self.name
    }
}

/// Why a row cannot be a linetype.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinetypeError {
    Name(NameError),
    /// More than [`LinetypeDef::MAX_LENGTHS`] lengths.
    TooLong,
    /// A dash without its gap.
    Odd,
    /// A length that is negative or not a number.
    Negative,
    /// Every length is zero: the pattern would never repeat.
    NoPeriod,
}

impl std::fmt::Display for LinetypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Name(error) => write!(f, "{}", error.explain("linetype")),
            Self::TooLong => write!(
                f,
                "A linetype can have at most {} dashes and gaps.",
                LinetypeDef::MAX_LENGTHS
            ),
            Self::Odd => write!(f, "A linetype pairs every dash with a gap."),
            Self::Negative => write!(f, "A dash or gap cannot be negative."),
            Self::NoPeriod => write!(f, "A linetype needs a dash or gap with some length."),
        }
    }
}

pub type LinetypeId = TableId<LinetypeDef>;
pub type LinetypeTable = Table<LinetypeDef>;
/// A reversible change to one linetype: loaded or removed.
pub type LinetypeChange = TableChange<LinetypeDef>;

/// A new drawing's linetypes: just `Continuous`.
pub(crate) fn initial_linetypes() -> LinetypeTable {
    Table::starting_with(LinetypeDef::continuous())
}

impl Linetype {
    /// The linetype every drawing has: the table's permanent first row.
    pub const CONTINUOUS: Self = Self::Named(LinetypeId::FIRST);

    /// Reads a linetype as it is typed: `ByLayer`, `ByBlock`, or the name
    /// of a row of `table`. Case does not count. `None` for a name the
    /// table does not have.
    pub fn parse(text: &str, table: &LinetypeTable) -> Option<Self> {
        let text = text.trim();
        if text.eq_ignore_ascii_case("bylayer") {
            return Some(Self::ByLayer);
        }
        if text.eq_ignore_ascii_case("byblock") {
            return Some(Self::ByBlock);
        }
        table.find(text).map(Self::Named)
    }

    /// The linetype as it is shown: `ByLayer`, `ByBlock`, or the row's
    /// name — or its number, should the row be gone.
    pub fn describe(self, table: &LinetypeTable) -> String {
        match self {
            Self::ByLayer => "ByLayer".to_owned(),
            Self::ByBlock => "ByBlock".to_owned(),
            Self::Named(id) => table
                .get(id)
                .map_or_else(|| id.to_string(), |def| def.name.clone()),
        }
    }

    /// Whether this is a linetype of its own rather than a reference to
    /// the layer's or the block's.
    pub fn is_concrete(self) -> bool {
        matches!(self, Self::Named(_))
    }

    /// The row an object with this linetype on `layer` is drawn with,
    /// outside any block. This is the one place the indirections are
    /// resolved (ADR-0013): `ByLayer` is the layer's linetype; `ByBlock`,
    /// with no block reference to take it from, is continuous — as is a
    /// layer whose own linetype is an indirection.
    pub fn row_on(self, layer: &Layer) -> LinetypeId {
        match self {
            Self::Named(id) => id,
            Self::ByLayer => match layer.linetype {
                Self::Named(id) => id,
                Self::ByLayer | Self::ByBlock => LinetypeId::FIRST,
            },
            Self::ByBlock => LinetypeId::FIRST,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_drawing_has_only_continuous_and_it_has_no_pattern() {
        let table = initial_linetypes();
        assert_eq!(table.len(), 1);
        let row = table.get(LinetypeId::FIRST).unwrap();
        assert_eq!(row.name, "Continuous");
        assert!(row.is_continuous());
        assert_eq!(row.period(), 0.0);
        assert_eq!(row.check(), Ok(()));
        assert_eq!(table.find("CONTINUOUS"), Some(LinetypeId::FIRST));
    }

    #[test]
    fn every_built_in_pattern_is_valid_and_distinctly_named() {
        let defs = LinetypeDef::built_in();
        let mut names: Vec<String> = defs.iter().map(|def| def.name.to_lowercase()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), defs.len());
        for def in &defs {
            assert_eq!(def.check(), Ok(()), "{}", def.name);
            assert!(def.period() > 0.0, "{}", def.name);
            assert!(!def.is_continuous());
            assert_ne!(def.name.to_lowercase(), "continuous");
        }
        assert_eq!(
            LinetypeDef::built_in_named("hidden").map(|def| def.pattern),
            Some(vec![0.25, 0.125])
        );
        // The dotted pattern has a dash of no length: a dot.
        assert_eq!(LinetypeDef::built_in_named("DOT").unwrap().pattern[0], 0.0);
        assert_eq!(LinetypeDef::built_in_named("solid"), None);
    }

    #[test]
    fn a_pattern_is_refused_when_it_could_not_be_drawn() {
        let def = |pattern: &[f64]| LinetypeDef {
            name: "x".to_owned(),
            description: String::new(),
            pattern: pattern.to_vec(),
        };
        assert_eq!(def(&[0.5]).check(), Err(LinetypeError::Odd));
        assert_eq!(def(&[0.5, -0.25]).check(), Err(LinetypeError::Negative));
        assert_eq!(def(&[0.5, f64::NAN]).check(), Err(LinetypeError::Negative));
        assert_eq!(def(&[0.0, 0.0]).check(), Err(LinetypeError::NoPeriod));
        assert_eq!(def(&[1.0; 10]).check(), Err(LinetypeError::TooLong));
        assert_eq!(
            def(&[0.0, 0.25]).check(),
            Ok(()),
            "a dot is a dash of no length"
        );
        let unnamed = LinetypeDef {
            name: String::new(),
            ..def(&[])
        };
        assert_eq!(unnamed.check(), Err(LinetypeError::Name(NameError::Empty)));
        assert_eq!(
            unnamed.check().unwrap_err().to_string(),
            "A linetype name cannot be empty."
        );
        assert!(
            [
                LinetypeError::TooLong,
                LinetypeError::Odd,
                LinetypeError::Negative,
                LinetypeError::NoPeriod
            ]
            .iter()
            .all(|error| error.to_string().ends_with('.'))
        );
    }

    #[test]
    fn a_linetype_is_read_as_typed_and_shown_by_name() {
        let mut table = initial_linetypes();
        let id = table.allocate();
        table.apply(&TableChange::Insert {
            id,
            row: LinetypeDef::built_in_named("Dashed").unwrap(),
        });
        assert_eq!(
            Linetype::parse(" bylayer ", &table),
            Some(Linetype::ByLayer)
        );
        assert_eq!(Linetype::parse("ByBlock", &table), Some(Linetype::ByBlock));
        assert_eq!(Linetype::parse("DASHED", &table), Some(Linetype::Named(id)));
        assert_eq!(
            Linetype::parse("continuous", &table),
            Some(Linetype::CONTINUOUS)
        );
        assert_eq!(Linetype::parse("hidden", &table), None, "not loaded");
        assert_eq!(Linetype::Named(id).describe(&table), "Dashed");
        assert_eq!(Linetype::ByLayer.describe(&table), "ByLayer");
        assert_eq!(
            Linetype::Named(LinetypeId::from_raw(9)).describe(&table),
            "9"
        );
        assert!(Linetype::Named(id).is_concrete() && !Linetype::ByLayer.is_concrete());
    }

    #[test]
    fn by_layer_resolves_to_the_layer_and_by_block_to_continuous() {
        let dashed = Linetype::Named(LinetypeId::from_raw(1));
        let mut layer = Layer::named("walls");
        assert_eq!(Linetype::ByLayer.row_on(&layer), LinetypeId::FIRST);
        layer.linetype = dashed;
        assert_eq!(Linetype::ByLayer.row_on(&layer), LinetypeId::from_raw(1));
        assert_eq!(Linetype::ByBlock.row_on(&layer), LinetypeId::FIRST);
        assert_eq!(Linetype::CONTINUOUS.row_on(&layer), LinetypeId::FIRST);
        // A layer whose linetype is itself an indirection has nothing to say.
        layer.linetype = Linetype::ByBlock;
        assert_eq!(Linetype::ByLayer.row_on(&layer), LinetypeId::FIRST);
    }
}
