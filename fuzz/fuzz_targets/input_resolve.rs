//! The input grammar (#177): any text at any kind of prompt, with any
//! keywords and any drawing unit, resolves to an input or a diagnostic,
//! the same way twice, and never panics.
#![no_main]

use arbitrary::Arbitrary;
use cad_core::LengthUnit;
use cad_input::{CommandAliases, ExpectedInput, InputKind, Keyword, parse_point, resolve};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct Case {
    /// Which kind of prompt.
    kind: u8,
    /// Which of the keywords below the prompt offers.
    keywords: u8,
    /// Whether the prompt takes direct-distance entry.
    direct_distance: bool,
    /// The drawing's unit, if it has one.
    unit: Option<u8>,
    text: String,
}

const KEYWORDS: [Keyword; 6] = [
    Keyword::new("Close"),
    Keyword::new("Undo"),
    Keyword::new("2P"),
    Keyword::new("?"),
    Keyword::new("SCale"),
    Keyword::new("Select"),
];

fn expected(case: &Case) -> ExpectedInput {
    let base = match case.kind % 8 {
        0 => ExpectedInput::command(),
        1 => ExpectedInput::point(),
        2 => ExpectedInput::distance(),
        3 => ExpectedInput::factor(),
        4 => ExpectedInput::angle(),
        5 => ExpectedInput::object(),
        6 => ExpectedInput::text(),
        _ => ExpectedInput::new(InputKind::Nothing),
    };
    let chosen: Vec<Keyword> = KEYWORDS
        .iter()
        .enumerate()
        .filter(|(i, _)| case.keywords & (1 << i) != 0)
        .map(|(_, keyword)| *keyword)
        .collect();
    let with_keywords = base.with_keywords(&chosen);
    if case.direct_distance {
        with_keywords.with_direct_distance()
    } else {
        with_keywords
    }
}

fn aliases() -> CommandAliases {
    let mut aliases = CommandAliases::new();
    aliases.register("LINE", &["L"]).expect("fresh");
    aliases.register("CIRCLE", &["C"]).expect("fresh");
    aliases.register("DIMLINEAR", &["DLI", "DIMLIN"]).expect("fresh");
    aliases
}

fuzz_target!(|case: Case| {
    let expected = expected(&case);
    let aliases = aliases();
    let unit = case
        .unit
        .map(|index| LengthUnit::ALL[usize::from(index) % LengthUnit::ALL.len()]);
    let first = resolve(&case.text, &expected, &aliases, unit);
    let second = resolve(&case.text, &expected, &aliases, unit);
    assert_eq!(first, second, "resolution is deterministic");
    let _ = parse_point(&case.text);
});
