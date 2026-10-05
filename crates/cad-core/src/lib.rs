//! Shared primitives: points, bounds, entity identity, tolerances, and view math.
//!
//! Everything here is needed by at least two otherwise-unrelated crates
//! (ADR-0001). Nothing here knows about entities, commands, or rendering.

// Exact float comparison is deliberate in tests; production code goes through `tolerance`.
#![cfg_attr(test, allow(clippy::float_cmp))]

mod aabb;
mod id;
mod point;
pub mod tolerance;
mod units;
mod view;

pub use aabb::Aabb;
pub use id::EntityId;
pub use point::{Point2, Vec2};
pub use units::LengthUnit;
pub use view::{ClipTransform, PixelTolerance, ScreenPoint, View};
