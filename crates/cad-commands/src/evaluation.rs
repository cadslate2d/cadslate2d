//! Evaluation of the AI input method (ADR-0037, #262): a corpus of
//! plain-language requests under `test-data/ai/`, one directory per level,
//! each case with what should come of it, scored against the model's
//! recorded answers.
//!
//! A level's `cases.jsonl` is written by hand: a case's prompt and language,
//! the drawing it starts from (`setup`, as intent operations), the objects
//! selected, the request it refines (`previous`), and what should come of
//! it — the drawing one of the `expected` documents makes, `"nothing"`, or
//! the engine refusing what `refused` asks. [`Corpus::requests`] is what the
//! app would send for each, its context from `Session::ai_context`: the AI
//! service's recorder asks the model each and writes the recordings, and
//! [`Corpus::score`] runs each recorded document from its case's drawing and
//! compares the drawing it makes with the expected one.
//!
//! CI scores the recordings committed beside the cases, which must all pass;
//! the AI service's nightly job scores the model's answers of the night
//! against them (`examples/ai-eval.rs`).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use cad_core::EntityId;
use cad_document::{Color, Entity};
use cad_intent::{Intent, parse};
use serde_json::{Value, json};

use crate::intent::RunError;
use crate::testing::Harness;

/// What a case should come to.
#[derive(Clone, Debug)]
pub enum Expected {
    /// The drawing one of these documents makes: a triangle may be one
    /// polyline or three lines.
    Drawing(Vec<Intent>),
    /// No document: the reply says why.
    Nothing,
    /// The engine refuses this, and the reply explains.
    Refused(Intent),
    /// A floor plan (#282) whose brief has these properties — what the
    /// request fixes, leaving the rest to the model — and that is drawn.
    /// The example is an answer that has them, which the corpus checks.
    Plan {
        wanted: Box<PlanWanted>,
        example: Intent,
    },
}

/// What a floor plan's brief must have. A room is matched by a pattern:
/// words separated by `|`, any of which its name contains, in any case —
/// `kitchen|cozinha` — and after a `!`, words none of which it does:
/// `bedroom|suite!bath`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanWanted {
    /// The plot's width and depth, each to 1%.
    pub terrain: Option<(f64, f64)>,
    /// How many rooms match each pattern.
    pub rooms: Vec<(String, usize)>,
    /// How many rooms in all, and how many pairs asked to open onto each
    /// other: a chain of three is three and two, whatever their names.
    pub count: Option<usize>,
    pub pairs: Option<usize>,
    /// The rooms' floor together, from and to.
    pub total: Option<(f64, f64)>,
    /// A room matching the pattern with its area from and to.
    pub areas: Vec<(String, f64, f64)>,
    /// Two rooms matching the patterns asked to open onto each other.
    pub adjacent: Vec<(String, String)>,
    /// Setbacks, each exact.
    pub setbacks: Vec<(String, f64)>,
    /// Which of the plans.
    pub alternative: Option<u32>,
    /// A plan drawn already is drawn again (#325): the document erases the
    /// selection — the plan's objects — before its `floor_plan`.
    pub replaces: bool,
    /// The windows' width, metres, exact (#324).
    pub windows: Option<f64>,
    /// A revision: every room and pairing of the brief revised is kept,
    /// each room's area too unless an `areas` pattern names it.
    pub keeps_previous: bool,
}

fn matches(pattern: &str, name: &str) -> bool {
    let name = name.to_lowercase();
    let any = |words: &str| {
        words
            .split('|')
            .any(|word| !word.trim().is_empty() && name.contains(&word.trim().to_lowercase()))
    };
    match pattern.split_once('!') {
        Some((wanted, unwanted)) => any(wanted) && !any(unwanted),
        None => any(pattern),
    }
}

impl PlanWanted {
    fn read(value: &Value) -> Result<Self, String> {
        let pair = |v: &Value| -> Option<(f64, f64)> { Some((v[0].as_f64()?, v[1].as_f64()?)) };
        let mut wanted = Self {
            terrain: value.get("terrain").and_then(pair),
            total: value.get("total").and_then(pair),
            alternative: value
                .get("alternative")
                .and_then(Value::as_u64)
                .map(|n| n as u32),
            keeps_previous: value["keeps_previous"].as_bool().unwrap_or(false),
            windows: value["windows"].as_f64(),
            replaces: value["replaces"].as_bool().unwrap_or(false),
            count: value["count"].as_u64().map(|n| n as usize),
            pairs: value["pairs"].as_u64().map(|n| n as usize),
            ..Self::default()
        };
        if let Some(rooms) = value["rooms"].as_object() {
            for (pattern, count) in rooms {
                let count = count.as_u64().ok_or("a room count is a number")?;
                wanted.rooms.push((pattern.clone(), count as usize));
            }
        }
        if let Some(areas) = value["areas"].as_object() {
            for (pattern, range) in areas {
                let (lo, hi) = pair(range).ok_or("an area range is two numbers")?;
                wanted.areas.push((pattern.clone(), lo, hi));
            }
        }
        if let Some(pairs) = value["adjacent"].as_array() {
            for p in pairs {
                let (a, b) = (p[0].as_str(), p[1].as_str());
                let (Some(a), Some(b)) = (a, b) else {
                    return Err("an adjacent pair is two patterns".into());
                };
                wanted.adjacent.push((a.to_owned(), b.to_owned()));
            }
        }
        if let Some(setbacks) = value["setbacks"].as_object() {
            for (side, v) in setbacks {
                wanted
                    .setbacks
                    .push((side.clone(), v.as_f64().ok_or("a setback is a number")?));
            }
        }
        Ok(wanted)
    }

    /// What the brief of `intent`'s floor plan lacks, if anything; the
    /// plan revised is `previous`'s, when there is one.
    pub fn missed(&self, intent: &Intent, previous: Option<&Intent>) -> Option<String> {
        let plan = |intent: &Intent| {
            intent.operations.iter().find_map(|op| match op {
                cad_intent::Operation::FloorPlan {
                    brief, alternative, ..
                } => Some((brief.clone(), *alternative)),
                _ => None,
            })
        };
        let Some((brief, alternative)) = plan(intent) else {
            return Some("no floor plan".into());
        };
        if self.replaces {
            let erases_first = matches!(
                intent.operations.first(),
                Some(cad_intent::Operation::Erase {
                    targets: cad_intent::Targets::Selection(_)
                })
            );
            if !erases_first {
                return Some("the plan drawn before is not erased first".into());
            }
        }
        let near = |a: f64, b: f64| (a - b).abs() <= 0.01 * b.abs().max(1.0);
        if let Some((w, d)) = self.terrain
            && !(near(brief.terrain.width, w) && near(brief.terrain.depth, d))
        {
            return Some(format!(
                "a terrain {} × {}, not {w} × {d}",
                brief.terrain.width, brief.terrain.depth
            ));
        }
        for (pattern, count) in &self.rooms {
            let found = brief
                .rooms
                .iter()
                .filter(|r| matches(pattern, &r.name))
                .count();
            if found != *count {
                return Some(format!("{found} rooms like {pattern}, not {count}"));
            }
        }
        if let Some(count) = self.count
            && brief.rooms.len() != count
        {
            return Some(format!("{} rooms, not {count}", brief.rooms.len()));
        }
        if let Some(pairs) = self.pairs
            && brief.adjacent.len() != pairs
        {
            return Some(format!("{} pairings, not {pairs}", brief.adjacent.len()));
        }
        let total: f64 = brief.rooms.iter().map(|r| r.area).sum();
        if let Some((lo, hi)) = self.total
            && !(lo..=hi).contains(&total)
        {
            return Some(format!("{total} m² in all, not {lo} to {hi}"));
        }
        for (pattern, lo, hi) in &self.areas {
            match brief.rooms.iter().find(|r| matches(pattern, &r.name)) {
                Some(room) if (*lo..=*hi).contains(&room.area) => {}
                Some(room) => {
                    return Some(format!(
                        "the {} is {} m², not {lo} to {hi}",
                        room.name, room.area
                    ));
                }
                None => return Some(format!("no room like {pattern}")),
            }
        }
        for (a, b) in &self.adjacent {
            let joined = brief
                .adjacent
                .iter()
                .any(|[x, y]| (matches(a, x) && matches(b, y)) || (matches(a, y) && matches(b, x)));
            if !joined {
                return Some(format!("nothing like {a} opens onto anything like {b}"));
            }
        }
        for (side, value) in &self.setbacks {
            let s = brief.setbacks.unwrap_or_default();
            let given = match side.as_str() {
                "front" => s.front,
                "back" => s.back,
                "left" => s.left,
                _ => s.right,
            };
            if (given - value).abs() > 1e-6 {
                return Some(format!("a {side} setback of {given}, not {value}"));
            }
        }
        if let Some(width) = self.windows
            && brief
                .windows
                .is_none_or(|given| (given - width).abs() > 1e-6)
        {
            return Some(match brief.windows {
                Some(given) => format!("windows {given} m wide, not {width}"),
                None => format!("no windows, not {width} m wide"),
            });
        }
        if let Some(wanted) = self.alternative
            && alternative.unwrap_or(1) != wanted
        {
            return Some(format!("plan {}, not {wanted}", alternative.unwrap_or(1)));
        }
        if self.keeps_previous {
            let Some((before, _)) = previous.and_then(plan) else {
                return Some("no plan to revise".into());
            };
            for room in &before.rooms {
                let resized = self.areas.iter().any(|(p, _, _)| matches(p, &room.name));
                match brief.rooms.iter().find(|r| r.name == room.name) {
                    None => return Some(format!("the {} is gone", room.name)),
                    Some(now) if !resized && (now.area - room.area).abs() > 1e-6 => {
                        return Some(format!("the {} changed size", room.name));
                    }
                    Some(_) => {}
                }
            }
            for pair in &before.adjacent {
                let kept = brief.adjacent.iter().any(|p| {
                    (p[0] == pair[0] && p[1] == pair[1]) || (p[0] == pair[1] && p[1] == pair[0])
                });
                if !kept {
                    return Some(format!(
                        "{} and {} no longer open onto each other",
                        pair[0], pair[1]
                    ));
                }
            }
            if before.terrain != brief.terrain && self.terrain.is_none() {
                return Some("the terrain changed".into());
            }
        }
        None
    }
}

/// One request of the corpus and what should come of it.
#[derive(Clone, Debug)]
pub struct Case {
    pub name: String,
    /// `en` or `pt`: what the reply is to be written in.
    pub language: String,
    pub prompt: String,
    pub setup: Option<Intent>,
    /// The IDs selected, or every entity the setup made (`"all"`).
    pub select: Selected,
    pub previous: Option<Value>,
    pub expected: Expected,
    /// A request for options (AI service #10): how few and how many may
    /// come, each wanted as one answer would be, and no two drawing the
    /// same. A case without it wants no options.
    pub options: Option<(usize, usize)>,
}

/// What a case selects before its request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selected {
    Ids(Vec<u64>),
    /// Everything the setup drew: a plan accepted, say (#325).
    All,
}

/// One level's cases: `l1`, AI commands.
#[derive(Clone, Debug)]
pub struct Corpus {
    pub level: String,
    pub cases: Vec<Case>,
}

fn document(operations: &Value) -> Result<Intent, String> {
    parse(&json!({ "version": cad_intent::VERSION, "operations": operations }).to_string())
        .map_err(|error| format!("{operations}: {error}"))
}

impl Corpus {
    /// Every level under `root`, `test-data/ai`: each directory with a
    /// `cases.jsonl`, in name order.
    pub fn levels(root: &Path) -> Result<Vec<Self>, String> {
        let mut directories: Vec<_> = std::fs::read_dir(root)
            .map_err(|error| format!("{}: {error}", root.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.join("cases.jsonl").is_file())
            .collect();
        directories.sort();
        directories.iter().map(|path| Self::read(path)).collect()
    }

    /// The level in `directory`, named after it.
    pub fn read(directory: &Path) -> Result<Self, String> {
        let path = directory.join("cases.jsonl");
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let level = directory
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        Ok(Self {
            level,
            cases: Self::parse(&text)?,
        })
    }

    /// The cases of a `cases.jsonl`.
    pub fn parse(text: &str) -> Result<Vec<Case>, String> {
        text.lines()
            .enumerate()
            .filter(|(_, line)| !line.trim().is_empty())
            .map(|(index, line)| {
                let at = |error: String| format!("cases.jsonl:{}: {error}", index + 1);
                let case: Value = serde_json::from_str(line).map_err(|e| at(e.to_string()))?;
                let text = |key: &str| {
                    case[key]
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| at(format!("no {key}")))
                };
                let expected = match (&case["expected"], &case["refused"]) {
                    _ if case.get("plan").is_some() => Expected::Plan {
                        wanted: Box::new(PlanWanted::read(&case["plan"]).map_err(at)?),
                        example: document(&case["example"]).map_err(at)?,
                    },
                    (Value::String(word), _) if word == "nothing" => Expected::Nothing,
                    (Value::Array(alternatives), _) => Expected::Drawing(
                        alternatives
                            .iter()
                            .map(document)
                            .collect::<Result<_, _>>()
                            .map_err(at)?,
                    ),
                    (_, refused @ Value::Array(_)) => {
                        Expected::Refused(document(refused).map_err(at)?)
                    }
                    _ => return Err(at("no expected outcome".to_owned())),
                };
                Ok(Case {
                    name: text("name")?,
                    language: text("lang")?,
                    prompt: text("prompt")?,
                    setup: case.get("setup").map(document).transpose().map_err(at)?,
                    select: match &case["select"] {
                        Value::String(all) if all == "all" => Selected::All,
                        Value::Array(ids) => {
                            Selected::Ids(ids.iter().filter_map(Value::as_u64).collect())
                        }
                        _ => Selected::Ids(Vec::new()),
                    },
                    previous: case.get("previous").cloned(),
                    expected,
                    options: match &case["options"] {
                        Value::Null => None,
                        Value::Array(range) => match range.as_slice() {
                            [least, most] => least.as_u64().zip(most.as_u64()),
                            _ => None,
                        }
                        .map(|(least, most)| (least as usize, most as usize))
                        .filter(|(least, most)| 2 <= *least && least <= most)
                        .map(Some)
                        .ok_or_else(|| at("options are [least, most], from 2".to_owned()))?,
                        options => options
                            .as_u64()
                            .filter(|n| *n >= 2)
                            .map(|n| Some((n as usize, n as usize)))
                            .ok_or_else(|| at("options are a count from 2".to_owned()))?,
                    },
                })
            })
            .collect()
    }

    /// What the recorder asks the model, one line per case: what the app
    /// would send, and for a refusal the engine's reason, which the
    /// explanation is asked with.
    pub fn requests(&self) -> String {
        let mut lines = String::new();
        for case in &self.cases {
            let mut line = json!({ "name": case.name, "request": request(case) });
            if let Some(reason) = reason(case) {
                line["explain"] = json!(reason);
            }
            lines.push_str(&line.to_string());
            lines.push('\n');
        }
        lines
    }

    /// What is wrong with the cases themselves: an expected document that
    /// does not run, a refusal the engine does not refuse, a name twice.
    pub fn faults(&self) -> Vec<String> {
        let mut faults = Vec::new();
        let mut names = std::collections::BTreeSet::new();
        for case in &self.cases {
            if !names.insert(case.name.as_str()) {
                faults.push(format!("{}: named twice", case.name));
            }
            match &case.expected {
                Expected::Drawing(alternatives) => {
                    for alternative in alternatives {
                        if let Err(error) = outcome(case, alternative) {
                            faults.push(format!("{}: {error}", case.name));
                        }
                    }
                    if let Some((least, _)) = case.options
                        && alternatives.len() < least
                    {
                        faults.push(format!(
                            "{}: fewer drawings expected than the {least} options wanted",
                            case.name
                        ));
                    }
                }
                Expected::Refused(intent) => {
                    if reason(case).is_none() {
                        faults.push(format!("{}: {intent:?} is not refused", case.name));
                    }
                }
                Expected::Plan { wanted, example } => {
                    if let Err(error) = outcome(case, example) {
                        faults.push(format!("{}: the example: {error}", case.name));
                    }
                    // Options of a plan are its brief's plans: the example's
                    // brief has as many, each drawn differently.
                    if let Some((least, _)) = case.options {
                        let mut drawn = Vec::new();
                        for plan in 1..=least as u32 {
                            let mut option = example.clone();
                            for operation in &mut option.operations {
                                if let cad_intent::Operation::FloorPlan { alternative, .. } =
                                    operation
                                {
                                    *alternative = Some(plan);
                                }
                            }
                            match outcome(case, &option) {
                                Ok(made) if drawn.contains(&made) => faults.push(format!(
                                    "{}: the example's plan {plan} is another's",
                                    case.name
                                )),
                                Ok(made) => drawn.push(made),
                                Err(error) => faults.push(format!(
                                    "{}: the example's plan {plan}: {error}",
                                    case.name
                                )),
                            }
                        }
                    }
                    if let Some(missed) = wanted.missed(example, previous(case).as_ref()) {
                        faults.push(format!("{}: the example has {missed}", case.name));
                    }
                }
                Expected::Nothing => {}
            }
        }
        faults
    }

    /// Scores the recordings in `recordings` (the AI service's JSON lines),
    /// case by case.
    pub fn score(&self, recordings: &str) -> Score {
        let recorded: BTreeMap<String, Value> = recordings
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|recording| Some((recording["name"].as_str()?.to_owned(), recording)))
            .collect();
        let results = self
            .cases
            .iter()
            .map(|case| {
                let recording = recorded.get(&case.name);
                CaseResult {
                    name: case.name.clone(),
                    language: case.language.clone(),
                    miss: match recording {
                        Some(recording) => judge(case, recording).err(),
                        None => Some("not recorded".to_owned()),
                    },
                    tokens: recording.map(tokens).unwrap_or_default(),
                }
            })
            .collect();
        Score {
            level: self.level.clone(),
            results,
        }
    }
}

/// The drawing a case starts from: millimetres, its setup, its selection.
fn start(case: &Case) -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "mm"]);
    if let Some(setup) = &case.setup {
        h.session
            .run_intent(setup)
            .unwrap_or_else(|error| panic!("{}: the setup does not run: {error}", case.name));
    }
    let selected: Vec<EntityId> = match &case.select {
        Selected::Ids(ids) => ids.iter().map(|&id| EntityId::from_raw(id)).collect(),
        Selected::All => h.session.drawing().entities().map(|(id, _)| id).collect(),
    };
    h.session.select_only(&selected);
    h
}

/// What the app would send for `case`: the context is what the user is
/// asked to agree to, here agreed.
pub fn request(case: &Case) -> Value {
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
        if !context.parameters.is_empty() {
            request["context"]["parameters"] = json!(context.parameters);
        }
        if !context.groups.is_empty() {
            let groups: Vec<Value> = context
                .groups
                .iter()
                .map(|group| {
                    json!({
                        "name": group.name,
                        "description": group.description,
                        "members": group.members,
                    })
                })
                .collect();
            request["context"]["groups"] = json!(groups);
        }
    }
    if let Some(previous) = &case.previous {
        request["previous"] = previous.clone();
    }
    request
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

/// The document a revision revises, when the case is one.
/// The plan a case revises: the proposal's, or — a plan drawn already —
/// the setup's (#325).
fn previous(case: &Case) -> Option<Intent> {
    match case.previous.as_ref().and_then(|p| p.get("intent")) {
        Some(intent) => parse(&intent.to_string()).ok(),
        None => case.setup.clone().filter(|setup| {
            setup
                .operations
                .iter()
                .any(|op| matches!(op, cad_intent::Operation::FloorPlan { .. }))
        }),
    }
}

/// Runs `intent` from the case's start: the drawing it makes, or why not.
pub fn outcome(case: &Case, intent: &Intent) -> Result<Vec<String>, RunError> {
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
pub fn describe(h: &Harness) -> Vec<String> {
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
                // Its text height too, as drawn: legible at the drawing's
                // size or not (#327).
                Entity::Dimension(d) => format!(
                    "dimension {} {} text {}",
                    d.measure.kind(),
                    n(d.value()),
                    n(d.layout(drawing.tables().dim_style(d.style)).text.height)
                ),
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
    // The constraints by what they hold, the parameters by name and value:
    // the same part made two ways holds the same.
    lines.extend(
        drawing
            .constraints()
            .iter()
            .map(|(_, c)| match c.kind.value() {
                Some(value) => format!("constraint {} = {}", c.name, n(value.abs())),
                None => {
                    let kind = format!("{:?}", c.kind);
                    let word = kind.split([' ', '{']).next().unwrap_or_default();
                    format!("constraint {}", word.to_lowercase())
                }
            }),
    );
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
            "e", "até", "para", "você", "será", "ao", "na", "é", "têm", "duas", "dois", "essas",
            "esses", "estas", "estes", "também", "mais", "pelo", "pela",
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
/// any other script is none of them. A drafter's `Ø` is no script.
pub fn language(text: &str) -> &'static str {
    if text.chars().any(|c| {
        c.is_alphabetic()
            && !c.is_ascii()
            && !"áâãàçéêíóôõúüñø".contains(c.to_lowercase().next().unwrap_or(c))
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

/// The model's answer in a recording: the provider's last attempt, as
/// the service reads it — the reply, the intent, and the other options.
fn answer(recording: &Value) -> Result<(String, Option<Value>, Vec<Value>), String> {
    let raw = recording["raw"]
        .as_array()
        .and_then(|raws| raws.last())
        .ok_or("no answer")?;
    let content: Value = serde_json::from_str(raw["content"].as_str().unwrap_or(""))
        .map_err(|error| format!("the answer is not JSON: {error}"))?;
    let reply = content["reply"].as_str().unwrap_or("").to_owned();
    let intent = content
        .get("intent")
        .filter(|intent| !intent.is_null())
        .cloned();
    let alternatives = content["alternatives"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    Ok((reply, intent, alternatives))
}

/// Whether one proposed document is what `case` wants; if so, the drawing
/// it makes.
fn wanted(case: &Case, intent: &Intent) -> Result<Vec<String>, String> {
    match &case.expected {
        Expected::Drawing(alternatives) => {
            let wanted: Vec<Vec<String>> = alternatives
                .iter()
                .filter_map(|alternative| outcome(case, alternative).ok())
                .collect();
            match outcome(case, intent) {
                Ok(made) if wanted.contains(&made) => Ok(made),
                Ok(made) => Err(format!(
                    "made\n    {}\n  wanted\n    {}",
                    made.join("\n    "),
                    wanted.first().map(|w| w.join("\n    ")).unwrap_or_default()
                )),
                Err(error) => Err(error.to_string()),
            }
        }
        Expected::Plan { wanted, .. } => {
            let made = outcome(case, intent).map_err(|error| error.to_string())?;
            match wanted.missed(intent, previous(case).as_ref()) {
                None => Ok(made),
                Some(missed) => Err(format!("the plan has {missed}")),
            }
        }
        Expected::Nothing => Err("proposed what cannot be done".to_owned()),
        Expected::Refused(_) => Err("not refused".to_owned()),
    }
}

/// Whether `recording` is what `case` wants: the expected drawing, nothing
/// proposed where nothing can be done, a refusal refused and explained —
/// and the reply in the request's language. A miss says why.
fn judge(case: &Case, recording: &Value) -> Result<(), String> {
    if recording["request"] != request(case) {
        return Err("recorded for another request".to_owned());
    }
    let (reply, intent, alternatives) = answer(recording)?;
    if language(&reply) != case.language {
        return Err(format!("the reply is not in its language: {reply}"));
    }
    let document = |value: Value| {
        parse(&value.to_string()).map_err(|error| format!("invalid document: {error}"))
    };
    let intent = intent.map(document).transpose()?;
    let alternatives: Vec<Intent> = alternatives
        .into_iter()
        .map(document)
        .collect::<Result<_, _>>()?;
    match (case.options, intent) {
        (None, _) if !alternatives.is_empty() => Err(format!(
            "offered {} options, none asked for",
            alternatives.len() + 1
        )),
        (Some(_), None) => Err(format!("nothing proposed: {reply}")),
        (Some((least, most)), Some(first)) => {
            let options: Vec<Intent> = std::iter::once(first).chain(alternatives).collect();
            if !(least..=most).contains(&options.len()) {
                return Err(format!("{} options, not {least} to {most}", options.len()));
            }
            let mut made = Vec::new();
            for (index, option) in options.iter().enumerate() {
                let drawing = wanted(case, option)
                    .map_err(|error| format!("option {}: {error}", index + 1))?;
                if made.contains(&drawing) {
                    return Err(format!("option {} draws the same as another", index + 1));
                }
                made.push(drawing);
            }
            Ok(())
        }
        (None, intent) => match (&case.expected, intent) {
            (Expected::Drawing(_) | Expected::Plan { .. }, Some(intent)) => {
                wanted(case, &intent).map(drop)
            }
            (Expected::Drawing(_) | Expected::Plan { .. }, None) => {
                Err(format!("nothing proposed: {reply}"))
            }
            (Expected::Nothing, None) => Ok(()),
            (Expected::Nothing, Some(_)) => Err("proposed what cannot be done".to_owned()),
            (Expected::Refused(_), intent) => {
                // Refused by the engine, or seen to be impossible by the model:
                // either way nothing is proposed, and the reply says why.
                if let Some(intent) = intent
                    && !matches!(outcome(case, &intent), Err(RunError::Refused { .. }))
                {
                    return Err("not refused".to_owned());
                }
                let explained = recording["explanation"]["content"]
                    .as_str()
                    .and_then(|content| serde_json::from_str::<Value>(content).ok())
                    .and_then(|content| content["reply"].as_str().map(str::to_owned));
                match explained {
                    Some(reply) if language(&reply) == case.language => Ok(()),
                    Some(reply) => Err(format!("the explanation is not in its language: {reply}")),
                    None => Err("the refusal is not explained".to_owned()),
                }
            }
        },
    }
}

/// Tokens a recording spent: every attempt and the explanation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tokens {
    pub input: u64,
    pub cached_input: u64,
    pub output: u64,
}

impl std::ops::AddAssign for Tokens {
    fn add_assign(&mut self, other: Self) {
        self.input += other.input;
        self.cached_input += other.cached_input;
        self.output += other.output;
    }
}

fn tokens(recording: &Value) -> Tokens {
    let mut spent = Tokens::default();
    let raws = recording["raw"].as_array().into_iter().flatten();
    for raw in raws.chain(std::iter::once(&recording["explanation"])) {
        let usage = &raw["usage"];
        spent += Tokens {
            input: usage["input"].as_u64().unwrap_or(0),
            cached_input: usage["cached_input"].as_u64().unwrap_or(0),
            output: usage["output"].as_u64().unwrap_or(0),
        };
    }
    spent
}

/// How one case did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseResult {
    pub name: String,
    pub language: String,
    /// Why it missed; `None` when it passed.
    pub miss: Option<String>,
    pub tokens: Tokens,
}

/// How a level did against one set of recordings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Score {
    pub level: String,
    pub results: Vec<CaseResult>,
}

/// How far a language's success rate may fall below the baseline's before
/// it is a regression: a model answers a little differently each night.
pub const REGRESSION_TOLERANCE: f64 = 0.05;

impl Score {
    /// Passed and total, per language, in name order.
    pub fn by_language(&self) -> Vec<(String, usize, usize)> {
        let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for result in &self.results {
            let count = counts.entry(&result.language).or_default();
            count.1 += 1;
            if result.miss.is_none() {
                count.0 += 1;
            }
        }
        counts
            .into_iter()
            .map(|(language, (passed, total))| (language.to_owned(), passed, total))
            .collect()
    }

    pub fn passed(&self) -> usize {
        self.results.iter().filter(|r| r.miss.is_none()).count()
    }

    pub fn misses(&self) -> impl Iterator<Item = &CaseResult> {
        self.results.iter().filter(|r| r.miss.is_some())
    }

    pub fn tokens(&self) -> Tokens {
        let mut total = Tokens::default();
        for result in &self.results {
            total += result.tokens;
        }
        total
    }

    /// The languages whose success rate fell below `baseline`'s by more
    /// than the tolerance, with both rates.
    pub fn regressions(&self, baseline: &Self) -> Vec<(String, f64, f64)> {
        let rate = |passed: usize, total: usize| {
            if total == 0 {
                1.0
            } else {
                passed as f64 / total as f64
            }
        };
        let before: BTreeMap<String, f64> = baseline
            .by_language()
            .into_iter()
            .map(|(language, passed, total)| (language, rate(passed, total)))
            .collect();
        self.by_language()
            .into_iter()
            .filter_map(|(language, passed, total)| {
                let now = rate(passed, total);
                let was = *before.get(&language)?;
                (now < was - REGRESSION_TOLERANCE).then_some((language, now, was))
            })
            .collect()
    }

    /// The score as JSON, for a report kept as an artifact.
    pub fn to_json(&self) -> Value {
        let tokens = self.tokens();
        json!({
            "level": self.level,
            "passed": self.passed(),
            "total": self.results.len(),
            "languages": self.by_language().into_iter().map(|(language, passed, total)| {
                json!({ "language": language, "passed": passed, "total": total })
            }).collect::<Vec<_>>(),
            "tokens": { "input": tokens.input, "cached_input": tokens.cached_input, "output": tokens.output },
            "misses": self.misses().map(|r| json!({ "name": r.name, "why": r.miss })).collect::<Vec<_>>(),
        })
    }
}
