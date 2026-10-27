//! The index-colour palette: what a colour number looks like.
//!
//! Index colours are the 255 numbered colours drafting has always used —
//! 1 red, 2 yellow, 3 green, 4 cyan, 5 blue, 6 magenta, 7 "white" — and
//! that DXF stores in group 62. The red, green, and blue of each are data,
//! reproduced here from the table the ezdxf library (1.4.4) ships as
//! `ezdxf.colors.DXF_DEFAULT_COLORS`.
//!
//! One entry is not a colour at all. **Colour 7 is the foreground**: light
//! on a dark background and dark on a light one. What that is depends on
//! the theme, which Rust does not know (DESIGN §7.1), so it is resolved to
//! [`Paint::Foreground`] and left to whoever draws.

use crate::{Color, Layer};

/// The number of the colour that means "the foreground".
pub const FOREGROUND_INDEX: u8 = 7;

/// What to draw with, once `ByLayer` and `ByBlock` are resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Paint {
    /// Whatever stands out from the background: the theme's choice.
    Foreground,
    /// This red, green, and blue.
    Rgb([u8; 3]),
}

/// The paint of index colour `index`. Colour 7 — and 0, which is not a
/// colour an object can have — is the foreground.
pub const fn index_paint(index: u8) -> Paint {
    if index == FOREGROUND_INDEX || index == 0 {
        Paint::Foreground
    } else {
        Paint::Rgb(INDEX_RGB[index as usize])
    }
}

/// The index colour nearest to a red, green, and blue: the one at the
/// least distance in RGB, the lower number on a tie. This is what a true
/// colour from a file becomes (ADR-0018): the drawing holds the palette
/// and nothing else.
pub fn nearest_index(rgb: [u8; 3]) -> u8 {
    let distance = |candidate: [u8; 3]| -> u32 {
        rgb.iter()
            .zip(candidate)
            .map(|(a, b)| {
                let d = i32::from(*a) - i32::from(b);
                (d * d) as u32
            })
            .sum()
    };
    (1..=255u8)
        .min_by_key(|index| distance(INDEX_RGB[usize::from(*index)]))
        .expect("the palette is not empty")
}

impl Color {
    /// The paint of an object with this colour on `layer`, outside any
    /// block. This is the one place the indirections are resolved
    /// (ADR-0013): `ByLayer` is the layer's colour; `ByBlock`, with no
    /// block reference to take it from, is the foreground — as is a layer
    /// whose own colour is an indirection.
    pub const fn paint_on(self, layer: &Layer) -> Paint {
        match self {
            Color::Index(index) => index_paint(index),
            Color::ByLayer => match layer.color {
                Color::Index(index) => index_paint(index),
                Color::ByLayer | Color::ByBlock => Paint::Foreground,
            },
            Color::ByBlock => Paint::Foreground,
        }
    }
}

/// The seven colours that have names, by number.
const NAMES: [(&str, u8); 7] = [
    ("red", 1),
    ("yellow", 2),
    ("green", 3),
    ("cyan", 4),
    ("blue", 5),
    ("magenta", 6),
    ("white", 7),
];

/// Common colours by name, beyond the seven, as red, green, and blue: each
/// read as the index colour nearest it (#348). Black is not among them: on
/// the drawing it is the foreground, white, 7.
const MORE_NAMES: [(&str, [u8; 3]); 26] = [
    ("orange", [255, 165, 0]),
    ("gold", [255, 215, 0]),
    ("brown", [165, 42, 42]),
    ("pink", [255, 192, 203]),
    ("purple", [128, 0, 128]),
    ("violet", [238, 130, 238]),
    ("gray", [128, 128, 128]),
    ("grey", [128, 128, 128]),
    ("silver", [192, 192, 192]),
    ("navy", [0, 0, 128]),
    ("teal", [0, 128, 128]),
    ("olive", [128, 128, 0]),
    ("maroon", [128, 0, 0]),
    ("lime", [0, 255, 0]),
    ("crimson", [220, 20, 60]),
    ("tan", [210, 180, 140]),
    ("turquoise", [64, 224, 208]),
    ("indigo", [75, 0, 130]),
    ("coral", [255, 127, 80]),
    ("salmon", [250, 128, 114]),
    ("khaki", [240, 230, 140]),
    ("bronze", [205, 127, 50]),
    ("copper", [184, 115, 51]),
    ("amber", [255, 191, 0]),
    ("beige", [245, 245, 220]),
    ("aqua", [0, 255, 255]),
];

/// `#RRGGBB`, as red, green, and blue.
fn hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.strip_prefix('#')?;
    if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?])
}

impl Color {
    /// Reads a colour as it is typed: a number from 1 to 255, one of the
    /// seven names (`red`, `yellow`, `green`, `cyan`, `blue`, `magenta`,
    /// `white`), `ByLayer`, or `ByBlock`; or another common name (`gold`,
    /// `orange`, `silver`, …) or `#RRGGBB`, either as the index colour
    /// nearest it. Case does not count. `None` for anything else, 0 and
    /// 256 included.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.eq_ignore_ascii_case("bylayer") {
            return Some(Color::ByLayer);
        }
        if text.eq_ignore_ascii_case("byblock") {
            return Some(Color::ByBlock);
        }
        if let Some((_, index)) = NAMES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(text))
        {
            return Some(Color::Index(*index));
        }
        let rgb = MORE_NAMES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(text))
            .map(|(_, rgb)| *rgb)
            .or_else(|| hex(text));
        if let Some(rgb) = rgb {
            return Some(Color::Index(nearest_index(rgb)));
        }
        // Digits only: "+3", "3.0", and " 3 " inside are not colour numbers.
        if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        match text.parse::<u8>() {
            Ok(index) if index >= 1 => Some(Color::Index(index)),
            _ => None,
        }
    }

    /// The colour as it is shown: `ByLayer`, `ByBlock`, `red`, or a number.
    pub fn describe(self) -> String {
        match self {
            Color::ByLayer => "ByLayer".to_owned(),
            Color::ByBlock => "ByBlock".to_owned(),
            Color::Index(index) => NAMES
                .iter()
                .find(|(_, named)| *named == index)
                .map_or_else(|| index.to_string(), |(name, _)| (*name).to_owned()),
        }
    }

    /// Whether this is a colour in itself, not a reference to a layer's or
    /// a block's: what a layer's own colour must be.
    pub const fn is_concrete(self) -> bool {
        matches!(self, Color::Index(_))
    }
}

/// Red, green, and blue of each index colour. Entry 0 is unused.
#[rustfmt::skip]
const INDEX_RGB: [[u8; 3]; 256] = [
    [0, 0, 0], [255, 0, 0], [255, 255, 0], [0, 255, 0], // 0
    [0, 255, 255], [0, 0, 255], [255, 0, 255], [255, 255, 255], // 4
    [128, 128, 128], [192, 192, 192], [255, 0, 0], [255, 127, 127], // 8
    [165, 0, 0], [165, 82, 82], [127, 0, 0], [127, 63, 63], // 12
    [76, 0, 0], [76, 38, 38], [38, 0, 0], [38, 19, 19], // 16
    [255, 63, 0], [255, 159, 127], [165, 41, 0], [165, 103, 82], // 20
    [127, 31, 0], [127, 79, 63], [76, 19, 0], [76, 47, 38], // 24
    [38, 9, 0], [38, 23, 19], [255, 127, 0], [255, 191, 127], // 28
    [165, 82, 0], [165, 124, 82], [127, 63, 0], [127, 95, 63], // 32
    [76, 38, 0], [76, 57, 38], [38, 19, 0], [38, 28, 19], // 36
    [255, 191, 0], [255, 223, 127], [165, 124, 0], [165, 145, 82], // 40
    [127, 95, 0], [127, 111, 63], [76, 57, 0], [76, 66, 38], // 44
    [38, 28, 0], [38, 33, 19], [255, 255, 0], [255, 255, 127], // 48
    [165, 165, 0], [165, 165, 82], [127, 127, 0], [127, 127, 63], // 52
    [76, 76, 0], [76, 76, 38], [38, 38, 0], [38, 38, 19], // 56
    [191, 255, 0], [223, 255, 127], [124, 165, 0], [145, 165, 82], // 60
    [95, 127, 0], [111, 127, 63], [57, 76, 0], [66, 76, 38], // 64
    [28, 38, 0], [33, 38, 19], [127, 255, 0], [191, 255, 127], // 68
    [82, 165, 0], [124, 165, 82], [63, 127, 0], [95, 127, 63], // 72
    [38, 76, 0], [57, 76, 38], [19, 38, 0], [28, 38, 19], // 76
    [63, 255, 0], [159, 255, 127], [41, 165, 0], [103, 165, 82], // 80
    [31, 127, 0], [79, 127, 63], [19, 76, 0], [47, 76, 38], // 84
    [9, 38, 0], [23, 38, 19], [0, 255, 0], [127, 255, 127], // 88
    [0, 165, 0], [82, 165, 82], [0, 127, 0], [63, 127, 63], // 92
    [0, 76, 0], [38, 76, 38], [0, 38, 0], [19, 38, 19], // 96
    [0, 255, 63], [127, 255, 159], [0, 165, 41], [82, 165, 103], // 100
    [0, 127, 31], [63, 127, 79], [0, 76, 19], [38, 76, 47], // 104
    [0, 38, 9], [19, 88, 23], [0, 255, 127], [127, 255, 191], // 108
    [0, 165, 82], [82, 165, 124], [0, 127, 63], [63, 127, 95], // 112
    [0, 76, 38], [38, 76, 57], [0, 38, 19], [19, 88, 28], // 116
    [0, 255, 191], [127, 255, 223], [0, 165, 124], [82, 165, 145], // 120
    [0, 127, 95], [63, 127, 111], [0, 76, 57], [38, 76, 66], // 124
    [0, 38, 28], [19, 88, 88], [0, 255, 255], [127, 255, 255], // 128
    [0, 165, 165], [82, 165, 165], [0, 127, 127], [63, 127, 127], // 132
    [0, 76, 76], [38, 76, 76], [0, 38, 38], [19, 88, 88], // 136
    [0, 191, 255], [127, 223, 255], [0, 124, 165], [82, 145, 165], // 140
    [0, 95, 127], [63, 111, 127], [0, 57, 76], [38, 66, 126], // 144
    [0, 28, 38], [19, 88, 88], [0, 127, 255], [127, 191, 255], // 148
    [0, 82, 165], [82, 124, 165], [0, 63, 127], [63, 95, 127], // 152
    [0, 38, 76], [38, 57, 126], [0, 19, 38], [19, 28, 88], // 156
    [0, 63, 255], [127, 159, 255], [0, 41, 165], [82, 103, 165], // 160
    [0, 31, 127], [63, 79, 127], [0, 19, 76], [38, 47, 126], // 164
    [0, 9, 38], [19, 23, 88], [0, 0, 255], [127, 127, 255], // 168
    [0, 0, 165], [82, 82, 165], [0, 0, 127], [63, 63, 127], // 172
    [0, 0, 76], [38, 38, 126], [0, 0, 38], [19, 19, 88], // 176
    [63, 0, 255], [159, 127, 255], [41, 0, 165], [103, 82, 165], // 180
    [31, 0, 127], [79, 63, 127], [19, 0, 76], [47, 38, 126], // 184
    [9, 0, 38], [23, 19, 88], [127, 0, 255], [191, 127, 255], // 188
    [82, 0, 165], [124, 82, 165], [63, 0, 127], [95, 63, 127], // 192
    [38, 0, 76], [57, 38, 126], [19, 0, 38], [28, 19, 88], // 196
    [191, 0, 255], [223, 127, 255], [124, 0, 165], [145, 82, 165], // 200
    [95, 0, 127], [111, 63, 127], [57, 0, 76], [66, 38, 76], // 204
    [28, 0, 38], [88, 19, 88], [255, 0, 255], [255, 127, 255], // 208
    [165, 0, 165], [165, 82, 165], [127, 0, 127], [127, 63, 127], // 212
    [76, 0, 76], [76, 38, 76], [38, 0, 38], [88, 19, 88], // 216
    [255, 0, 191], [255, 127, 223], [165, 0, 124], [165, 82, 145], // 220
    [127, 0, 95], [127, 63, 111], [76, 0, 57], [76, 38, 66], // 224
    [38, 0, 28], [88, 19, 88], [255, 0, 127], [255, 127, 191], // 228
    [165, 0, 82], [165, 82, 124], [127, 0, 63], [127, 63, 95], // 232
    [76, 0, 38], [76, 38, 57], [38, 0, 19], [88, 19, 28], // 236
    [255, 0, 63], [255, 127, 159], [165, 0, 41], [165, 82, 103], // 240
    [127, 0, 31], [127, 63, 79], [76, 0, 19], [76, 38, 47], // 244
    [38, 0, 9], [88, 19, 23], [0, 0, 0], [101, 101, 101], // 248
    [102, 102, 102], [153, 153, 153], [204, 204, 204], [255, 255, 255], // 252
];

#[cfg(test)]
mod tests {
    #[test]
    fn every_palette_colour_is_its_own_nearest_index() {
        for index in 1..=255u8 {
            let nearest = super::nearest_index(super::INDEX_RGB[usize::from(index)]);
            assert_eq!(
                super::INDEX_RGB[usize::from(nearest)],
                super::INDEX_RGB[usize::from(index)],
                "{index} → {nearest}"
            );
        }
        // Off the palette: orange is 30, a dark teal is a dark cyan.
        assert_eq!(super::nearest_index([255, 128, 0]), 30);
        assert_eq!(super::nearest_index([250, 5, 5]), 1);
        assert_ne!(super::nearest_index([0, 128, 128]), 7);
    }

    use super::*;

    #[test]
    fn the_first_colours_are_the_ones_everyone_knows() {
        let named = [
            (1, [255, 0, 0]),
            (2, [255, 255, 0]),
            (3, [0, 255, 0]),
            (4, [0, 255, 255]),
            (5, [0, 0, 255]),
            (6, [255, 0, 255]),
            (8, [128, 128, 128]),
            (9, [192, 192, 192]),
        ];
        for (index, rgb) in named {
            assert_eq!(index_paint(index), Paint::Rgb(rgb), "colour {index}");
        }
        assert_eq!(index_paint(255), Paint::Rgb([255, 255, 255]));
    }

    #[test]
    fn colour_seven_is_the_foreground_not_a_fixed_white() {
        assert_eq!(index_paint(7), Paint::Foreground);
        assert_eq!(index_paint(0), Paint::Foreground);
        // Every other number is a definite colour.
        for index in (1..=255u8).filter(|index| *index != 7) {
            assert!(
                matches!(index_paint(index), Paint::Rgb(_)),
                "colour {index}"
            );
        }
    }

    #[test]
    fn the_hues_run_through_the_palette_in_tens() {
        // 10 to 249: twenty-four hues of ten shades, the first of each at
        // full strength — red, then round to red again.
        assert_eq!(index_paint(10), Paint::Rgb([255, 0, 0]));
        assert_eq!(index_paint(50), Paint::Rgb([255, 255, 0]));
        assert_eq!(index_paint(90), Paint::Rgb([0, 255, 0]));
        assert_eq!(index_paint(130), Paint::Rgb([0, 255, 255]));
        assert_eq!(index_paint(170), Paint::Rgb([0, 0, 255]));
        assert_eq!(index_paint(210), Paint::Rgb([255, 0, 255]));
        // Even shades are pure, and darker down the ten.
        assert_eq!(index_paint(12), Paint::Rgb([165, 0, 0]));
        assert_eq!(index_paint(18), Paint::Rgb([38, 0, 0]));
    }

    /// #135: colours are entered by number or by the seven standard names.
    #[test]
    fn colours_are_read_by_number_by_name_and_by_reference() {
        let cases = [
            ("1", Color::Index(1)),
            ("7", Color::Index(7)),
            ("42", Color::Index(42)),
            ("255", Color::Index(255)),
            ("007", Color::Index(7)),
            ("red", Color::Index(1)),
            ("Yellow", Color::Index(2)),
            ("GREEN", Color::Index(3)),
            ("cyan", Color::Index(4)),
            ("blue", Color::Index(5)),
            ("magenta", Color::Index(6)),
            ("white", Color::Index(7)),
            (" blue ", Color::Index(5)),
            ("bylayer", Color::ByLayer),
            ("ByLayer", Color::ByLayer),
            ("BYBLOCK", Color::ByBlock),
        ];
        for (typed, color) in cases {
            assert_eq!(Color::parse(typed), Some(color), "{typed:?}");
        }
        for typed in [
            "",
            "0",
            "256",
            "1000",
            "-1",
            "+3",
            "3.0",
            "1e2",
            "#12345",
            "#GGGGGG",
            "ffd700",
            "re",
            "reds",
            "by layer",
            "1 2",
            "٣",
            "99999999999999999999",
        ] {
            assert_eq!(Color::parse(typed), None, "{typed:?}");
        }
    }

    /// Gold, orange, a hex colour: each is the index colour nearest it
    /// (#348), so a drawing asked for in red and gold has both.
    #[test]
    fn common_names_and_hex_read_as_the_nearest_index_colour() {
        let nearest = |rgb| Some(Color::Index(nearest_index(rgb)));
        assert_eq!(Color::parse("gold"), nearest([255, 215, 0]));
        assert_eq!(Color::parse("Gold"), Color::parse("#FFD700"));
        assert_eq!(Color::parse("#ffd700"), nearest([255, 215, 0]));
        assert_eq!(Color::parse("orange"), nearest([255, 165, 0]));
        assert_eq!(Color::parse("#FF0000"), Some(Color::Index(1)));
        // Gold is not plain yellow, nor red: a colour of its own.
        let gold = Color::parse("gold").unwrap();
        assert_ne!(gold, Color::Index(2));
        assert_ne!(gold, Color::Index(1));
        assert!(Color::parse("purple").is_some());
        assert_eq!(Color::parse("black"), None);
    }

    #[test]
    fn a_colour_is_shown_by_its_name_if_it_has_one_and_reads_back_as_itself() {
        assert_eq!(Color::Index(1).describe(), "red");
        assert_eq!(Color::Index(7).describe(), "white");
        assert_eq!(Color::Index(8).describe(), "8");
        assert_eq!(Color::Index(255).describe(), "255");
        assert_eq!(Color::ByLayer.describe(), "ByLayer");
        assert_eq!(Color::ByBlock.describe(), "ByBlock");
        let every = (1..=255u8)
            .map(Color::Index)
            .chain([Color::ByLayer, Color::ByBlock]);
        for color in every {
            assert_eq!(Color::parse(&color.describe()), Some(color), "{color:?}");
        }
        assert!(Color::Index(3).is_concrete());
        assert!(!Color::ByLayer.is_concrete() && !Color::ByBlock.is_concrete());
    }

    #[test]
    fn by_layer_takes_the_layers_colour_and_by_block_the_foreground() {
        let red = Layer {
            color: Color::Index(1),
            ..Layer::named("red")
        };
        let plain = Layer::named("plain");
        assert_eq!(Color::ByLayer.paint_on(&red), Paint::Rgb([255, 0, 0]));
        assert_eq!(Color::ByLayer.paint_on(&plain), Paint::Foreground);
        assert_eq!(
            Color::Index(5).paint_on(&red),
            Paint::Rgb([0, 0, 255]),
            "its own colour wins"
        );
        assert_eq!(Color::Index(7).paint_on(&red), Paint::Foreground);
        assert_eq!(Color::ByBlock.paint_on(&red), Paint::Foreground);
        let odd = Layer {
            color: Color::ByLayer,
            ..Layer::named("odd")
        };
        assert_eq!(Color::ByLayer.paint_on(&odd), Paint::Foreground);
    }
}
