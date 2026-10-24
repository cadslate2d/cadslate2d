//! The L1 corpus (#261): plain-language requests in English and Portuguese
//! — new geometry, edits of the selection, revisions, what the operations
//! cannot do, and what the engine refuses — each with the drawing it
//! should make, replayed from the model's recorded answers.
//!
//! `test-data/ai/l1/cases.jsonl` is written by hand: a case's prompt, the
//! drawing it starts from (`setup`, as intent operations), the objects
//! selected, the request it refines (`previous`), and what should come of
//! it — the drawing one of the `expected` documents makes, `"nothing"`, or
//! the engine refusing what `refused` asks. `requests.jsonl` is generated
//! from it: what the app would send, its context from `Session::ai_context`
//! (`UPDATE_SNAPSHOTS=1` writes it). The AI service's recorder asks the
//! model each request and writes `recordings.jsonl`; here each recorded
//! answer is run, and the drawing it makes compared with the expected one.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use cad_core::EntityId;
use cad_document::{Color, Entity};
use cad_intent::{Intent, parse};
use serde_json::{Value, json};

use crate::intent::RunError;
use crate::testing::Harness;

const DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-data/ai/l1");

/// What a case should come to.
enum Expected {
    /// The drawing one of these documents makes: a triangle may be one
    /// polyline or three lines.
    Drawing(Vec<Intent>),
    /// No document: the reply says why.
    Nothing,
    /// The engine refuses this, and the reply explains.
    Refused(Intent),
}

struct Case {
    name: String,
    /// `en` or `pt`: what the reply is to be written in.
    language: String,
    prompt: String,
    setup: Option<Intent>,
    select: Vec<u64>,
    previous: Option<Value>,
    expected: Expected,
}

fn document(operations: &Value) -> Intent {
    parse(&json!({ "version": cad_intent::VERSION, "operations": operations }).to_string())
        .unwrap_or_else(|error| panic!("{operations}: {error}"))
}

fn cases() -> Vec<Case> {
    let text = std::fs::read_to_string(format!("{DIRECTORY}/cases.jsonl")).unwrap();
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            let case: Value = serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("cases.jsonl:{}: {error}", index + 1));
            let expected = match (&case["expected"], &case["refused"]) {
                (Value::String(word), _) if word == "nothing" => Expected::Nothing,
                (Value::Array(alternatives), _) => {
                    Expected::Drawing(alternatives.iter().map(document).collect())
                }
                (_, refused @ Value::Array(_)) => Expected::Refused(document(refused)),
                _ => panic!("cases.jsonl:{}: no expected outcome", index + 1),
            };
            Case {
                name: case["name"].as_str().unwrap().to_owned(),
                language: case["lang"].as_str().unwrap().to_owned(),
                prompt: case["prompt"].as_str().unwrap().to_owned(),
                setup: case.get("setup").map(document),
                select: case
                    .get("select")
                    .and_then(Value::as_array)
                    .map(|ids| ids.iter().filter_map(Value::as_u64).collect())
                    .unwrap_or_default(),
                previous: case.get("previous").cloned(),
                expected,
            }
        })
        .collect()
}

/// The drawing a case starts from: millimetres, its setup, its selection.
fn start(case: &Case) -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    if let Some(setup) = &case.setup {
        h.session.run_intent(setup).unwrap();
    }
    let selected: Vec<EntityId> = case
        .select
        .iter()
        .map(|&id| EntityId::from_raw(id))
        .collect();
    h.session.select_only(&selected);
    h
}

/// What the app would send for `case`: the context is what the user is
/// asked to agree to, here agreed.
fn request(case: &Case) -> Value {
    let h = start(case);
    let context = h.session.ai_context();
    let mut request = json!({ "prompt": case.prompt });
    if let Some(unit) = context.unit {
        request["unit"] = json!(unit);
    }
    if !context.objects.is_empty() {
        let objects: Vec<Value> = context
            .objects
            .iter()
            .map(|object| {
                json!({
                    "id": object.id,
                    "kind": object.kind,
                    "geometry": object.geometry,
                    "layer": object.layer,
                })
            })
            .collect();
        request["context"] = json!({ "layers": context.layers, "objects": objects });
    }
    if let Some(previous) = &case.previous {
        request["previous"] = previous.clone();
    }
    request
}

/// Runs `intent` from the case's start: the drawing it makes, or why not.
fn outcome(case: &Case, intent: &Intent) -> Result<Vec<String>, RunError> {
    let mut h = start(case);
    h.session.run_intent(intent)?;
    Ok(describe(&h))
}

/// A number to the hundredth, without a negative zero.
fn n(value: f64) -> String {
    let text = format!("{value:.2}");
    if text == "-0.00" {
        "0.00".to_owned()
    } else {
        text
    }
}

fn p(point: cad_core::Point2) -> String {
    format!("({}, {})", n(point.x), n(point.y))
}

fn degrees(radians: f64) -> String {
    n(radians.to_degrees().rem_euclid(360.0) % 360.0)
}

/// A polyline as the same text whichever vertex it is drawn from and
/// whichever way: each vertex with the bulge of the segment from it.
fn polyline(vertices: &[cad_core::Point2], bulges: &[f64], closed: bool) -> String {
    let count = vertices.len();
    let forward: Vec<(String, String)> =
        (0..count).map(|i| (p(vertices[i]), n(bulges[i]))).collect();
    let backward: Vec<(String, String)> = (0..count)
        .rev()
        .map(|i| {
            let bulge = if i == 0 && !closed {
                0.0
            } else {
                -bulges[(i + count - 1) % count]
            };
            (p(vertices[i]), n(bulge))
        })
        .collect();
    let text = |sequence: &[(String, String)]| {
        sequence
            .iter()
            .enumerate()
            .map(|(i, (point, bulge))| {
                if bulge == "0.00" || (!closed && i + 1 == count) {
                    point.clone()
                } else {
                    format!("{point} bulge {bulge}")
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut candidates = Vec::new();
    for sequence in [forward, backward] {
        if closed {
            for start in 0..count {
                let mut turned = sequence.clone();
                turned.rotate_left(start);
                candidates.push(text(&turned));
            }
        } else {
            candidates.push(text(&sequence));
        }
    }
    let shape = candidates.into_iter().min().unwrap_or_default();
    format!(
        "polyline {} {shape}",
        if closed { "closed" } else { "open" }
    )
}

/// The drawing as sorted lines of text: each object's geometry, layer, and
/// colour, then the layers and the current one. Two drawings that look
/// the same and are on the same layers describe the same, however their
/// objects were made.
fn describe(h: &Harness) -> Vec<String> {
    let drawing = h.session.drawing();
    let mut lines: Vec<String> = drawing
        .entities()
        .map(|(id, entity)| {
            let mut line = match entity {
                Entity::Line(l) => {
                    let (a, b) = (p(l.start), p(l.end));
                    format!("line {} {}", a.clone().min(b.clone()), a.max(b))
                }
                Entity::Circle(c) => format!("circle {} radius {}", p(c.center), n(c.radius)),
                Entity::Arc(a) => format!(
                    "arc {} radius {} from {} to {}",
                    p(a.center),
                    n(a.radius),
                    degrees(a.start_angle),
                    degrees(a.end_angle)
                ),
                Entity::Polyline(pl) => {
                    let bulges: Vec<f64> = pl.bulges().collect();
                    polyline(&pl.vertices, &bulges, pl.closed)
                }
                Entity::Text(t) => format!(
                    "text {:?} at {} height {} rotation {}",
                    t.content,
                    p(t.insertion),
                    n(t.height),
                    degrees(t.rotation)
                ),
                // Where its text and line are placed is the drawer's
                // choice; what it measures is not.
                Entity::Dimension(d) => {
                    format!("dimension {} {}", d.measure.kind(), n(d.value()))
                }
                other => format!("{other:?}"),
            };
            let properties = drawing.properties(id).unwrap_or_default();
            if let Some(layer) = drawing.layers().get(properties.layer)
                && layer.name != "0"
            {
                let _ = write!(line, " layer {}", layer.name);
            }
            if properties.color != Color::ByLayer {
                let _ = write!(line, " colour {}", properties.color.describe());
            }
            line
        })
        .collect();
    lines.sort();
    let layers = drawing.layers();
    let mut names: Vec<&str> = layers
        .iter()
        .map(|(_, layer)| layer.name.as_str())
        .collect();
    names.sort_unstable();
    lines.push(format!("layers {}", names.join(", ")));
    if let Some(current) = layers.get(drawing.current_layer()) {
        lines.push(format!("current layer {}", current.name));
    }
    lines
}

/// The corpus is what #261 says: at least 50 requests, in both languages;
/// every case's expected documents run, and its refusals are refused.
#[test]
fn every_case_expects_one_drawing_or_a_refusal() {
    let cases = cases();
    assert!(cases.len() >= 50, "{} cases", cases.len());
    for language in ["-en", "-pt"] {
        assert!(
            cases.iter().filter(|c| c.name.ends_with(language)).count() >= 25,
            "{language}"
        );
    }
    let mut names = std::collections::BTreeSet::new();
    for case in &cases {
        assert!(names.insert(case.name.clone()), "{} twice", case.name);
        match &case.expected {
            Expected::Drawing(alternatives) => {
                for alternative in alternatives {
                    if let Err(error) = outcome(case, alternative) {
                        panic!("{}: {error}", case.name);
                    }
                }
            }
            Expected::Refused(intent) => assert!(
                matches!(outcome(case, intent), Err(RunError::Refused { .. })),
                "{}: not refused",
                case.name
            ),
            Expected::Nothing => {}
        }
    }
}

/// The reason a refused case's explanation is asked with: what stopped the
/// proposal, as the app sends it — the engine's own words.
fn reason(case: &Case) -> Option<String> {
    match &case.expected {
        Expected::Refused(intent) => match outcome(case, intent) {
            Err(error @ RunError::Refused { .. }) => Some(error.to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// What the recorder asks the model: generated from the cases, so a change
/// to what the app sends shows here and in a re-recording.
#[test]
fn the_requests_are_the_recorded_ones() {
    let mut generated = String::new();
    for case in cases() {
        let mut line = json!({ "name": case.name, "request": request(&case) });
        if let Some(reason) = reason(&case) {
            line["explain"] = json!(reason);
        }
        generated.push_str(&line.to_string());
        generated.push('\n');
    }
    let path = format!("{DIRECTORY}/requests.jsonl");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &generated).unwrap();
    }
    let recorded = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        generated == recorded,
        "the requests changed: write them with UPDATE_SNAPSHOTS=1, then record the model's answers again"
    );
}

/// Words that are common in one of the languages a reply could slip into
/// and rare in the others.
const LANGUAGES: [(&str, &[&str]); 3] = [
    (
        "en",
        &[
            "the", "a", "i", "and", "with", "will", "to", "of", "it", "these", "this", "can",
            "cannot", "so", "they", "be", "between", "would", "is", "are", "at", "by", "from",
        ],
    ),
    (
        "pt",
        &[
            "o", "os", "a", "as", "um", "uma", "com", "em", "não", "vou", "do", "da", "dos", "das",
            "e", "até", "para", "você", "será", "ao", "na", "é",
        ],
    ),
    (
        "es",
        &[
            "el", "los", "la", "las", "con", "en", "hasta", "del", "y", "una", "se", "que", "por",
        ],
    ),
];

/// The language `text` is most likely written in, by its common words;
/// any other script is none of them.
fn language(text: &str) -> &'static str {
    if text.chars().any(|c| {
        c.is_alphabetic()
            && !c.is_ascii()
            && !"áâãàçéêíóôõúüñ".contains(c.to_lowercase().next().unwrap_or(c))
    }) {
        return "other";
    }
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphabetic())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    LANGUAGES
        .iter()
        .max_by_key(|(_, common)| {
            words
                .iter()
                .filter(|word| common.contains(&word.as_str()))
                .count()
        })
        .map_or("other", |(language, _)| language)
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
        language("Extendí la línea horizontal hasta que se encuentre con la línea vertical."),
        "es"
    );
    assert_eq!(
        language("Создам сетку из 12 окружностей радиусом 5 мм."),
        "other"
    );
}

/// The model's answer in a recording: the provider's last attempt, as
/// the service reads it.
fn answer(raw: &Value) -> (String, Option<Value>) {
    let content: Value = serde_json::from_str(raw["content"].as_str().unwrap_or("")).unwrap();
    let reply = content["reply"].as_str().unwrap_or("").to_owned();
    let intent = content
        .get("intent")
        .filter(|intent| !intent.is_null())
        .cloned();
    (reply, intent)
}

/// Every recorded answer makes the expected drawing, proposes nothing where
/// nothing can be done, and is refused — and explained — where the engine
/// refuses.
#[test]
fn the_recorded_answers_make_the_expected_drawings() {
    let text = std::fs::read_to_string(format!("{DIRECTORY}/recordings.jsonl")).unwrap();
    let recordings: BTreeMap<String, Value> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let recording: Value = serde_json::from_str(line).unwrap();
            (recording["name"].as_str().unwrap().to_owned(), recording)
        })
        .collect();
    let mut misses = Vec::new();
    for case in cases() {
        let Some(recording) = recordings.get(&case.name) else {
            misses.push(format!("{}: not recorded", case.name));
            continue;
        };
        if recording["request"] != request(&case) {
            misses.push(format!("{}: recorded for another request", case.name));
            continue;
        }
        let (reply, intent) = answer(recording["raw"].as_array().unwrap().last().unwrap());
        if language(&reply) != case.language {
            misses.push(format!(
                "{}: the reply is not in its language: {reply}",
                case.name
            ));
        }
        let intent = match intent.map(|intent| parse(&intent.to_string())) {
            Some(Ok(intent)) => Some(intent),
            Some(Err(error)) => {
                misses.push(format!("{}: invalid document: {error}", case.name));
                continue;
            }
            None => None,
        };
        match (&case.expected, intent) {
            (Expected::Drawing(alternatives), Some(intent)) => {
                let wanted: Vec<Vec<String>> = alternatives
                    .iter()
                    .map(|alternative| outcome(&case, alternative).unwrap())
                    .collect();
                match outcome(&case, &intent) {
                    Ok(made) if wanted.contains(&made) => {}
                    Ok(made) => misses.push(format!(
                        "{}: made\n    {}\n  wanted\n    {}",
                        case.name,
                        made.join("\n    "),
                        wanted[0].join("\n    ")
                    )),
                    Err(error) => misses.push(format!("{}: {error}", case.name)),
                }
            }
            (Expected::Drawing(_), None) => {
                misses.push(format!("{}: nothing proposed: {reply}", case.name));
            }
            (Expected::Nothing, None) => {}
            (Expected::Nothing, Some(_)) => {
                misses.push(format!("{}: proposed what cannot be done", case.name));
            }
            (Expected::Refused(_), intent) => {
                // Refused by the engine, or seen to be impossible by the
                // model: either way nothing is proposed, and the reply
                // says why.
                if let Some(intent) = intent
                    && !matches!(outcome(&case, &intent), Err(RunError::Refused { .. }))
                {
                    misses.push(format!("{}: not refused", case.name));
                }
                let explained = recording["explanation"]["content"]
                    .as_str()
                    .and_then(|content| serde_json::from_str::<Value>(content).ok())
                    .and_then(|content| content["reply"].as_str().map(str::to_owned));
                match explained {
                    Some(reply) if language(&reply) == case.language => {}
                    Some(reply) => misses.push(format!(
                        "{}: the explanation is not in its language: {reply}",
                        case.name
                    )),
                    None => misses.push(format!("{}: the refusal is not explained", case.name)),
                }
            }
        }
    }
    assert!(misses.is_empty(), "{}", misses.join("\n"));
}
