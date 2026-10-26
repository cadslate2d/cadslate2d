//! The brief a layout is made from: a terrain, its setbacks, the rooms and
//! what each needs, which rooms open onto which, and the walls. Lengths in
//! metres, areas in square metres. Read from JSON — a spec file, or the
//! intent IR's operation — and checked before any search.

use serde::{Deserialize, Serialize};

/// A plan's brief.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spec {
    pub terrain: Terrain,
    #[serde(default)]
    pub setbacks: Setbacks,
    pub rooms: Vec<Room>,
    /// Pairs of rooms, by name, that must share a wall with a door in it.
    #[serde(default)]
    pub adjacent: Vec<[String; 2]>,
    #[serde(default = "Spec::default_exterior_wall")]
    pub exterior_wall: f64,
    #[serde(default = "Spec::default_interior_wall")]
    pub interior_wall: f64,
    /// The clear width of a door.
    #[serde(default = "Spec::default_door")]
    pub door: f64,
    /// The most floor the search may add as circulation — a hall or
    /// passage the rooms alone cannot be shaped or reached without — as a
    /// share of the rooms' floor. Zero forbids it.
    #[serde(default = "Spec::default_max_circulation")]
    pub max_circulation: f64,
    /// The windows' width (#324): with it, a window on each outside wall
    /// of each room; without it, one of the default width in each room
    /// that asks for an outside wall.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<f64>,
    /// The search's seed: the same brief and seed make the same plan.
    #[serde(default)]
    pub seed: u64,
}

/// The plot: a rectangle, its front along the x axis from the origin.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Terrain {
    pub width: f64,
    pub depth: f64,
}

/// How far the building keeps from each edge of the terrain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setbacks {
    #[serde(default)]
    pub front: f64,
    #[serde(default)]
    pub back: f64,
    #[serde(default)]
    pub left: f64,
    #[serde(default)]
    pub right: f64,
}

/// A room: its name, its floor area inside its walls, and its shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Room {
    pub name: String,
    pub area: f64,
    /// Its narrower side, inside the walls, is at least this.
    #[serde(default = "Room::default_min_width")]
    pub min_width: f64,
    /// Its longer side over its shorter is at most this.
    #[serde(default = "Room::default_max_aspect")]
    pub max_aspect: f64,
    /// It wants an outside wall — a window — when it can have one.
    #[serde(default)]
    pub exterior: bool,
    /// The way in: an outside wall with a door. The first room when none
    /// says so.
    #[serde(default)]
    pub entrance: bool,
}

impl Spec {
    fn default_exterior_wall() -> f64 {
        0.25
    }
    fn default_interior_wall() -> f64 {
        0.15
    }
    fn default_door() -> f64 {
        0.9
    }
    pub(crate) fn default_max_circulation() -> f64 {
        0.15
    }

    /// A brief from its JSON — a spec file, or a brief typed or sent —
    /// read and checked.
    pub fn from_json(text: &str) -> Result<Self, SpecError> {
        let spec: Self = serde_json::from_str(text)
            .map_err(|e| SpecError(format!("The brief cannot be read: {e}")))?;
        spec.check()?;
        Ok(spec)
    }

    /// The brief as one line of JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a brief is plain data")
    }

    /// The index of the way in.
    pub fn entrance(&self) -> usize {
        self.rooms.iter().position(|r| r.entrance).unwrap_or(0)
    }

    /// The room called `name`.
    pub fn room(&self, name: &str) -> Option<usize> {
        self.rooms.iter().position(|r| r.name == name)
    }

    /// The adjacent pairs, by index; names already checked.
    pub(crate) fn adjacent_pairs(&self) -> Vec<(usize, usize)> {
        self.adjacent
            .iter()
            .filter_map(|[a, b]| Some((self.room(a)?, self.room(b)?)))
            .collect()
    }

    /// What makes the brief unreadable, before any search: a number that is
    /// not a size, a name twice or not there, setbacks that leave nothing.
    pub fn check(&self) -> Result<(), SpecError> {
        let positive = |what: String, value: f64| {
            if value.is_finite() && value > 0.0 {
                Ok(())
            } else {
                Err(SpecError(format!("{what} must be greater than zero")))
            }
        };
        let not_negative = |what: &str, value: f64| {
            if value.is_finite() && value >= 0.0 {
                Ok(())
            } else {
                Err(SpecError(format!("the {what} setback cannot be negative")))
            }
        };
        positive("The terrain's width".into(), self.terrain.width)?;
        positive("The terrain's depth".into(), self.terrain.depth)?;
        not_negative("front", self.setbacks.front)?;
        not_negative("back", self.setbacks.back)?;
        not_negative("left", self.setbacks.left)?;
        not_negative("right", self.setbacks.right)?;
        positive("The exterior wall's thickness".into(), self.exterior_wall)?;
        positive("The interior wall's thickness".into(), self.interior_wall)?;
        positive("The door's width".into(), self.door)?;
        if let Some(width) = self.windows {
            positive("The windows' width".into(), width)?;
        }
        if !(self.max_circulation.is_finite() && (0.0..=1.0).contains(&self.max_circulation)) {
            return Err(SpecError(
                "The circulation's share must be between 0 and 1".into(),
            ));
        }
        if self.rooms.is_empty() {
            return Err(SpecError("The brief has no rooms".into()));
        }
        if self.rooms.len() > MAX_ROOMS {
            return Err(SpecError(format!("A plan takes at most {MAX_ROOMS} rooms")));
        }
        for (i, room) in self.rooms.iter().enumerate() {
            if room.name.trim().is_empty() {
                return Err(SpecError(format!("Room {} has no name", i + 1)));
            }
            if self.rooms[..i].iter().any(|r| r.name == room.name) {
                return Err(SpecError(format!("Two rooms are called {}", room.name)));
            }
            positive(format!("The {}'s area", room.name), room.area)?;
            positive(format!("The {}'s minimum width", room.name), room.min_width)?;
            if !(room.max_aspect.is_finite() && room.max_aspect >= 1.0) {
                return Err(SpecError(format!(
                    "The {}'s aspect ratio must be 1 or more",
                    room.name
                )));
            }
        }
        if self.rooms.iter().filter(|r| r.entrance).count() > 1 {
            return Err(SpecError("Only one room can be the entrance".into()));
        }
        for [a, b] in &self.adjacent {
            for name in [a, b] {
                if self.room(name).is_none() {
                    return Err(SpecError(format!("No room is called {name}")));
                }
            }
            if a == b {
                return Err(SpecError(format!("The {a} cannot open onto itself")));
            }
        }
        Ok(())
    }
}

impl Room {
    fn default_min_width() -> f64 {
        1.5
    }
    fn default_max_aspect() -> f64 {
        2.5
    }

    /// A room of `area`, with the defaults.
    pub fn new(name: impl Into<String>, area: f64) -> Self {
        Self {
            name: name.into(),
            area,
            min_width: Self::default_min_width(),
            max_aspect: Self::default_max_aspect(),
            exterior: false,
            entrance: false,
        }
    }
}

/// The most rooms a brief takes: the search's time grows with them.
pub const MAX_ROOMS: usize = 24;

/// Why a brief cannot be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecError(pub String);

impl std::fmt::Display for SpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.", self.0)
    }
}

impl std::error::Error for SpecError {}
