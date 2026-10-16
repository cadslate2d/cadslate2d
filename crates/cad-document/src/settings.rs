//! Drawing settings: the saved state of a drawing that is neither an entity
//! nor a table row (DESIGN §11, §19.1).
//!
//! They are part of the drawing — written to a file, restored by undo —
//! unlike session state such as the view or the snap modes.

use cad_core::LengthUnit;

use crate::dim_style::DimStyleId;
use crate::layer::LayerId;
use crate::point::PointStyle;
use crate::text_style::TextStyleId;
use crate::{Color, Linetype, Lineweight};

/// The settings of one drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawingSettings {
    /// The layer new objects are put on.
    pub current_layer: LayerId,
    /// The colour new objects are given: `ByLayer` unless set otherwise.
    pub current_color: Color,
    /// The linetype new objects are given: `ByLayer` unless set otherwise.
    pub current_linetype: Linetype,
    /// The lineweight new objects are given: `ByLayer` unless set otherwise.
    pub current_lineweight: Lineweight,
    /// The style new dimensions are drawn by (`DIMSTYLE`).
    pub current_dim_style: DimStyleId,
    /// The style new texts take (`STYLE`, `$TEXTSTYLE`; #221).
    pub current_text_style: TextStyleId,
    /// What every linetype pattern is multiplied by (`LTSCALE`): the one
    /// knob that fits dashes to the size of what is drawn. Positive.
    pub ltscale: f64,
    /// What the drawing's numbers are numbers of, or `None` for a unitless
    /// drawing. It says what the geometry means and changes none of it: a
    /// line 25 long is 25 long in any unit. A typed length with a unit
    /// suffix is converted into this (DESIGN §14).
    pub unit: Option<LengthUnit>,
    /// How every point is drawn (`PTYPE`; `$PDMODE`, `$PDSIZE`; #198).
    pub point_style: PointStyle,
    /// Whether wipeouts show their frames (`WIPEOUT Frames`; DXF
    /// `WIPEOUTVARIABLES`; #226). On by default, as in the classic
    /// programs: a frameless wipeout is hard to find.
    pub wipeout_frame: bool,
}

impl DrawingSettings {
    /// The `LTSCALE` of a new drawing: patterns at their defined lengths.
    pub const DEFAULT_LTSCALE: f64 = 1.0;
}

impl Default for DrawingSettings {
    /// New objects go on layer `0`, with everything by their layer; the
    /// drawing is unitless, and patterns are at their defined lengths.
    fn default() -> Self {
        Self {
            current_layer: LayerId::FIRST,
            current_color: Color::ByLayer,
            current_linetype: Linetype::ByLayer,
            current_lineweight: Lineweight::ByLayer,
            current_dim_style: DimStyleId::FIRST,
            current_text_style: TextStyleId::FIRST,
            ltscale: Self::DEFAULT_LTSCALE,
            unit: None,
            point_style: PointStyle::default(),
            wipeout_frame: true,
        }
    }
}

/// A reversible change of the drawing's settings: the whole of them before
/// and after. They are few and small; one shape covers every setting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SettingsChange {
    pub before: DrawingSettings,
    pub after: DrawingSettings,
}

impl SettingsChange {
    /// The change that exactly undoes this one.
    pub fn inverse(&self) -> Self {
        Self {
            before: self.after,
            after: self.before,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_drawing_draws_on_layer_zero() {
        assert_eq!(DrawingSettings::default().current_layer, LayerId::FIRST);
        assert_eq!(DrawingSettings::default().unit, None, "and has no unit");
        assert_eq!(DrawingSettings::default().current_color, Color::ByLayer);
        assert_eq!(
            DrawingSettings::default().current_linetype,
            Linetype::ByLayer
        );
        assert_eq!(DrawingSettings::default().ltscale, 1.0);
    }

    #[test]
    fn a_settings_change_is_undone_by_swapping_its_ends() {
        let change = SettingsChange {
            before: DrawingSettings::default(),
            after: DrawingSettings {
                current_layer: LayerId::from_raw(3),
                current_color: Color::Index(1),
                current_linetype: Linetype::CONTINUOUS,
                current_lineweight: Lineweight::Hundredths(25),
                current_dim_style: DimStyleId::from_raw(2),
                current_text_style: TextStyleId::from_raw(1),
                ltscale: 2.5,
                unit: Some(LengthUnit::Metres),
                point_style: PointStyle::new(35, 4.0),
                wipeout_frame: false,
            },
        };
        assert_eq!(change.inverse().before, change.after);
        assert_eq!(change.inverse().inverse(), change);
    }
}
