//! The AI corpus (#261, #262) replayed: every level's cases hold, the
//! requests are the ones recorded, and every recorded answer of the model
//! is what its case wants. `crate::evaluation` says how a case is judged.

use std::path::{Path, PathBuf};

use crate::evaluation::{Corpus, Expected, language};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/ai")
}

fn levels() -> Vec<Corpus> {
    let levels = Corpus::levels(&root()).unwrap();
    assert!(!levels.is_empty());
    levels
}

/// L2 is what #274 says: at least 40 requests for parts, placements,
/// constraints, and parametric edits, in both languages, refusals the
/// solver explains among them.
#[test]
fn l2_asks_for_parts_and_parametric_edits() {
    let l2 = Corpus::read(&root().join("l2")).unwrap();
    assert!(l2.cases.len() >= 40, "{} cases", l2.cases.len());
    for language in ["en", "pt"] {
        assert!(
            l2.cases.iter().filter(|c| c.language == language).count() >= 20,
            "{language}"
        );
    }
    assert!(
        l2.cases
            .iter()
            .any(|c| matches!(c.expected, Expected::Refused(_)))
    );
}

/// L1 is what #261 says: at least 50 requests, in both languages, with
/// requests the operations cannot do and refusals among them; and no case
/// is wrong itself.
#[test]
fn every_case_expects_one_drawing_or_a_refusal() {
    let l1 = Corpus::read(&root().join("l1")).unwrap();
    assert!(l1.cases.len() >= 50, "{} cases", l1.cases.len());
    for language in ["en", "pt"] {
        assert!(
            l1.cases.iter().filter(|c| c.language == language).count() >= 25,
            "{language}"
        );
    }
    assert!(
        l1.cases
            .iter()
            .any(|c| matches!(c.expected, Expected::Nothing))
    );
    assert!(
        l1.cases
            .iter()
            .any(|c| matches!(c.expected, Expected::Refused(_)))
    );
    for level in levels() {
        let faults = level.faults();
        assert!(faults.is_empty(), "{}: {}", level.level, faults.join("\n"));
    }
}

/// What the recorder asks the model: generated from the cases, so a change
/// to what the app sends shows here and in a re-recording.
#[test]
fn the_requests_are_the_recorded_ones() {
    for level in levels() {
        let generated = level.requests();
        let path = root().join(&level.level).join("requests.jsonl");
        if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
            std::fs::write(&path, &generated).unwrap();
        }
        let recorded = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            generated == recorded,
            "{}: the requests changed: write them with UPDATE_SNAPSHOTS=1, then record the model's answers again",
            level.level
        );
    }
}

#[test]
fn the_language_of_a_reply_is_told_by_its_common_words() {
    assert_eq!(
        language("I’ll draw a circle with radius 25 centred at (100, 100)."),
        "en"
    );
    assert_eq!(
        language("Vou desenhar um círculo de raio 25 com centro em 100,100."),
        "pt"
    );
    assert_eq!(
        language(
            "A proposta criará uma linha de [0, 0] até [150, 50] mm. Nada será feito até você aceitá-la."
        ),
        "pt"
    );
    assert_eq!(
        language("Extendí la línea horizontal hasta que se encuentre con la línea vertical."),
        "es"
    );
    assert_eq!(
        language("Создам сетку из 12 окружностей радиусом 5 мм."),
        "other"
    );
    // A diameter's sign is a drafter's, in any language.
    assert_eq!(
        language("I’ll create six Ø10 holes on a 100 mm pitch circle."),
        "en"
    );
    assert_eq!(
        language("A proposta cria seis furos de Ø10 num círculo primitivo de 100 mm."),
        "pt"
    );
}

/// Every recorded answer makes the expected drawing, proposes nothing where
/// nothing can be done, and is refused — and explained — where the engine
/// refuses, in the request's language.
#[test]
fn the_recorded_answers_make_the_expected_drawings() {
    for level in levels() {
        let path = root().join(&level.level).join("recordings.jsonl");
        let score = level.score(&std::fs::read_to_string(path).unwrap());
        let misses: Vec<String> = score
            .misses()
            .map(|r| format!("{}: {}", r.name, r.miss.as_deref().unwrap_or("")))
            .collect();
        assert!(misses.is_empty(), "{}", misses.join("\n"));
        assert!(score.tokens().input > 0);
    }
}

/// A model that does worse than the baseline is a regression when a
/// language's rate falls by more than the tolerance; one stray miss in
/// thirty-five is not.
#[test]
fn a_regression_is_a_language_falling_below_the_baseline() {
    let l1 = Corpus::read(&root().join("l1")).unwrap();
    let recorded = std::fs::read_to_string(root().join("l1/recordings.jsonl")).unwrap();
    let baseline = l1.score(&recorded);
    assert!(baseline.regressions(&baseline).is_empty());
    let mut worse = baseline.clone();
    let missed = |score: &mut crate::evaluation::Score, name: &str| {
        let result = score.results.iter_mut().find(|r| r.name == name).unwrap();
        result.miss = Some("made something else".to_owned());
    };
    missed(&mut worse, "circle-pt");
    assert!(worse.regressions(&baseline).is_empty(), "one miss in 35");
    missed(&mut worse, "line-pt");
    missed(&mut worse, "rectangle-pt");
    let regressions = worse.regressions(&baseline);
    assert_eq!(regressions.len(), 1);
    assert_eq!(regressions[0].0, "pt");
    assert_eq!(worse.passed(), 67);
    assert_eq!(worse.to_json()["misses"].as_array().unwrap().len(), 3);
    // Nothing recorded at all is every case missed.
    assert_eq!(l1.score("").passed(), 0);
}
