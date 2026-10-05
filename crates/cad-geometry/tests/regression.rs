//! Runs the geometry regression corpora in `test-data/geometry/`.
//!
//! A corpus is plain text, one case per line, so that recording a fixed bug
//! is a one-line change (DESIGN §21.3). Every file goes through the same
//! runner, [`run`], which checks every case and reports each one that fails
//! as `test-data/geometry/<file>:<line>: <why>`.
//!
//! Besides comparing with the recorded outcome, each kind of case checks a
//! property that must hold whatever the outcome is — so a case recorded
//! from the implementation is still held to something independent of it.
//!
//! `REPORT_CORPUS=1 cargo test -p cad-geometry --test regression -- --nocapture`
//! prints what every case produces, in corpus syntax, for recording new ones.

use std::f64::consts::TAU;
use std::path::Path;

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;
use cad_geometry::{
    Arc, Circle, Corner, CornerError, Path as Polyline, PathSpan, Primitive, Segment,
    distance_to_line, extend_arc, extend_segment, fillet_curves, offset_arc, offset_circle,
    offset_path, offset_segment, path_spans, trim_arc_parts, trim_circle_parts, trim_segment_parts,
};

const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

/// How far a computed value may be from the recorded one. Looser than the
/// geometric tolerance because recorded values are typed to finite precision.
const AGREEMENT: f64 = 1e-6;

/// What checking one case comes to: nothing, or why it failed.
type Check = Result<(), String>;

/// How one kind of case is checked.
type CaseCheck = dyn Fn(&str) -> Check;

// ── the runner ──

/// Checks every case in `corpus`. Returns how many there were and, for each
/// that failed, where it is and why.
fn failures(file: &str, corpus: &str, check: &CaseCheck) -> (usize, Vec<String>) {
    let mut cases = 0;
    let mut failed = Vec::new();
    for (index, text) in corpus.lines().enumerate() {
        let text = text.trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        cases += 1;
        if let Err(why) = check(text) {
            failed.push(format!(
                "test-data/geometry/{file}:{}: {why}\n    {text}",
                index + 1
            ));
        }
    }
    (cases, failed)
}

/// Runs the corpus file `file`, which must hold at least `minimum` cases: a
/// corpus only grows.
fn run(file: &str, minimum: usize, check: &CaseCheck) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/geometry")
        .join(file);
    let corpus = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let (cases, failed) = failures(file, &corpus, check);
    assert!(
        failed.is_empty(),
        "{} of {cases} cases failed:\n{}",
        failed.len(),
        failed.join("\n")
    );
    assert!(
        cases >= minimum,
        "test-data/geometry/{file} should not shrink: {cases} cases, at least {minimum} expected"
    );
}

/// Prints what a case produced when `REPORT_CORPUS` is set.
fn report(case: &str, produced: &str) {
    if std::env::var_os("REPORT_CORPUS").is_some() {
        let inputs = case.split("=>").next().unwrap_or(case).trim();
        println!("{inputs} => {produced}");
    }
}

// ── reading cases ──

fn split<'a>(text: &'a str, separator: &str) -> Result<(&'a str, &'a str), String> {
    text.split_once(separator)
        .map(|(before, after)| (before.trim(), after.trim()))
        .ok_or_else(|| format!("missing `{separator}`"))
}

fn numbers(text: &str) -> Result<Vec<f64>, String> {
    text.split_whitespace()
        .map(|token| {
            token
                .parse()
                .map_err(|_| format!("`{token}` is not a number"))
        })
        .collect()
}

fn point(text: &str) -> Result<Point2, String> {
    match numbers(text)?.as_slice() {
        [x, y] => Ok(Point2::new(*x, *y)),
        _ => Err(format!("`{text}` is not a point")),
    }
}

fn primitive(text: &str) -> Result<Primitive, String> {
    let (kind, rest) = split(text, " ").map_err(|_| format!("cannot read `{text}`"))?;
    match (kind, numbers(rest)?.as_slice()) {
        ("segment", [x0, y0, x1, y1]) => Ok(Primitive::Segment(Segment::new(
            Point2::new(*x0, *y0),
            Point2::new(*x1, *y1),
        ))),
        ("circle", [x, y, radius]) => {
            Ok(Primitive::Circle(Circle::new(Point2::new(*x, *y), *radius)))
        }
        ("arc", [x, y, radius, start, sweep]) => Ok(Primitive::Arc(Arc::new(
            Point2::new(*x, *y),
            *radius,
            start.to_radians(),
            sweep.to_radians(),
        ))),
        _ => Err(format!("cannot read `{text}`")),
    }
}

/// Primitives separated by `;`.
fn primitives(text: &str) -> Result<Vec<Primitive>, String> {
    text.split(';').map(|part| primitive(part.trim())).collect()
}

fn segment_of(primitive: &Primitive) -> Result<Segment, String> {
    match primitive {
        Primitive::Segment(segment) => Ok(*segment),
        other => Err(format!("{} is not a segment", show(other))),
    }
}

// ── comparing and showing ──

fn near(a: Point2, b: Point2) -> bool {
    a.distance_to(b) < AGREEMENT
}

fn same(a: &Primitive, b: &Primitive) -> bool {
    match (a, b) {
        (Primitive::Segment(a), Primitive::Segment(b)) => {
            near(a.start, b.start) && near(a.end, b.end)
        }
        (Primitive::Circle(a), Primitive::Circle(b)) => {
            near(a.center, b.center) && (a.radius - b.radius).abs() < AGREEMENT
        }
        (Primitive::Arc(a), Primitive::Arc(b)) => {
            near(a.center, b.center)
                && (a.radius - b.radius).abs() < AGREEMENT
                && near(a.start_point(), b.start_point())
                && near(a.end_point(), b.end_point())
                && (a.sweep - b.sweep).abs() < AGREEMENT
        }
        _ => false,
    }
}

/// A primitive in corpus syntax.
fn show(primitive: &Primitive) -> String {
    match primitive {
        Primitive::Segment(s) => format!(
            "segment {} {} {} {}",
            s.start.x, s.start.y, s.end.x, s.end.y
        ),
        Primitive::Circle(c) => format!("circle {} {} {}", c.center.x, c.center.y, c.radius),
        Primitive::Arc(a) => format!(
            "arc {} {} {} {} {}",
            a.center.x,
            a.center.y,
            a.radius,
            a.start.to_degrees(),
            a.sweep.to_degrees()
        ),
    }
}

fn show_all(primitives: &[Primitive]) -> String {
    if primitives.is_empty() {
        return "nothing".to_owned();
    }
    let shown: Vec<String> = primitives.iter().map(show).collect();
    shown.join(" ; ")
}

/// `got` is `want`, piece for piece and in order.
fn expect_all(got: &[Primitive], want: &[Primitive]) -> Check {
    let matches = got.len() == want.len() && got.iter().zip(want).all(|(a, b)| same(a, b));
    if matches {
        Ok(())
    } else {
        Err(format!("gave {}", show_all(got)))
    }
}

fn expect(holds: bool, why: impl FnOnce() -> String) -> Check {
    if holds { Ok(()) } else { Err(why()) }
}

fn length(primitive: &Primitive) -> f64 {
    match primitive {
        Primitive::Segment(segment) => segment.length(),
        Primitive::Arc(arc) => arc.radius * arc.sweep,
        Primitive::Circle(circle) => TAU * circle.radius,
    }
}

/// The two ends of an open primitive.
fn ends(primitive: &Primitive) -> Option<(Point2, Point2)> {
    match primitive {
        Primitive::Segment(segment) => Some((segment.start, segment.end)),
        Primitive::Arc(arc) => Some((arc.start_point(), arc.end_point())),
        Primitive::Circle(_) => None,
    }
}

// ── intersections ──

/// `<primitive> | <primitive> => <x y> [<x y>]`, or `=> none`.
/// Property: the answer does not depend on the order of the two.
fn intersection_case(text: &str) -> Check {
    let (inputs, outcome) = split(text, "=>")?;
    let (first, second) = split(inputs, "|")?;
    let (a, b) = (primitive(first)?, primitive(second)?);
    let want: Vec<Point2> = if outcome == "none" {
        Vec::new()
    } else {
        let values = numbers(outcome)?;
        if !values.len().is_multiple_of(2) {
            return Err("expected points need an x and a y".to_owned());
        }
        values
            .chunks(2)
            .map(|pair| Point2::new(pair[0], pair[1]))
            .collect()
    };

    let describe = |hits: &[Point2]| {
        if hits.is_empty() {
            return "none".to_owned();
        }
        let shown: Vec<String> = hits
            .iter()
            .map(|hit| format!("{} {}", hit.x, hit.y))
            .collect();
        shown.join(" ")
    };
    report(text, &describe(a.intersect(&b).as_slice()));
    for (order, hits) in [("a|b", a.intersect(&b)), ("b|a", b.intersect(&a))] {
        let got = hits.as_slice();
        let matches = got.len() == want.len()
            && want
                .iter()
                .all(|point| got.iter().any(|g| near(*g, *point)));
        expect(matches, || format!("({order}) gave {}", describe(got)))?;
    }
    Ok(())
}

#[test]
fn intersection_corpus() {
    run("intersections.txt", 20, &intersection_case);
}

// ── polyline offsets ──

fn polyline(kind: &str, vertices: &str) -> Result<Polyline, String> {
    let mut path = Polyline {
        vertices: Vec::new(),
        bulges: Vec::new(),
        closed: match kind {
            "open" => false,
            "closed" => true,
            other => return Err(format!("`{other}` is neither open nor closed")),
        },
    };
    for vertex in vertices.split_whitespace() {
        let values: Result<Vec<f64>, String> = vertex
            .split(',')
            .map(|token| {
                token
                    .parse()
                    .map_err(|_| format!("`{token}` is not a number"))
            })
            .collect();
        match values?.as_slice() {
            [x, y] => {
                path.vertices.push(Point2::new(*x, *y));
                path.bulges.push(0.0);
            }
            [x, y, bulge] => {
                path.vertices.push(Point2::new(*x, *y));
                path.bulges.push(*bulge);
            }
            _ => return Err(format!("cannot read vertex `{vertex}`")),
        }
    }
    Ok(path)
}

/// `<name> : open|closed : <vertices> : <distance> => <paths> <total length>`.
/// Property: every point of the result is the offset distance from the
/// source (ADR-0011).
fn polyline_offset_case(text: &str) -> Check {
    let (inputs, outcome) = split(text, "=>")?;
    let fields: Vec<&str> = inputs.split(':').map(str::trim).collect();
    let [_name, kind, vertices, distance] = fields.as_slice() else {
        return Err("expected name : open|closed : vertices : distance".to_owned());
    };
    let source = polyline(kind, vertices)?;
    let signed: f64 = distance
        .parse()
        .map_err(|_| format!("`{distance}` is not a distance"))?;

    let offsets = offset_path(&source, signed);
    let total: f64 = offsets
        .iter()
        .flat_map(path_spans)
        .map(|span| length(&span.primitive()))
        .sum();
    report(text, &format!("{} {total}", offsets.len()));

    let shape: Vec<Primitive> = path_spans(&source)
        .iter()
        .map(PathSpan::primitive)
        .collect();
    for span in offsets.iter().flat_map(path_spans) {
        for step in 0..=20 {
            let at = span.point_at(f64::from(step) / 20.0);
            let nearest = shape
                .iter()
                .map(|primitive| primitive.distance_to_point(at))
                .fold(f64::INFINITY, f64::min);
            expect((nearest - signed.abs()).abs() < AGREEMENT, || {
                format!("{at:?} is {nearest} from the source, not {}", signed.abs())
            })?;
        }
    }

    let (count, recorded) = split(outcome, " ")?;
    let count: usize = count
        .parse()
        .map_err(|_| format!("`{count}` is not a number of paths"))?;
    let recorded: f64 = recorded
        .parse()
        .map_err(|_| format!("`{recorded}` is not a length"))?;
    expect(offsets.len() == count, || {
        format!("gave {} paths: {offsets:?}", offsets.len())
    })?;
    expect(
        (total - recorded).abs() < AGREEMENT * (1.0 + recorded),
        || format!("total length {total}"),
    )
}

#[test]
fn polyline_offset_corpus() {
    run("polyline-offsets.txt", 24, &polyline_offset_case);
}

// ── offsets of lines, circles, and arcs ──

/// `<object> : <distance> @ <side x y> => <object>`, or `=> nothing`.
/// Property: the result is the same kind of thing, `distance` away —
/// a parallel line of the same length, a concentric circle or arc.
fn offset_case(text: &str) -> Check {
    let (inputs, outcome) = split(text, "=>")?;
    let (shape, side) = split(inputs, "@")?;
    let (object, distance) = split(shape, ":")?;
    let (object, side) = (primitive(object)?, point(side)?);
    let distance: f64 = distance
        .parse()
        .map_err(|_| format!("`{distance}` is not a distance"))?;

    let got: Option<Primitive> = match &object {
        Primitive::Segment(segment) => {
            offset_segment(segment, distance, side).map(Primitive::Segment)
        }
        Primitive::Circle(circle) => {
            offset_circle(circle, distance, side, TOLERANCE).map(Primitive::Circle)
        }
        Primitive::Arc(arc) => offset_arc(arc, distance, side, TOLERANCE).map(Primitive::Arc),
    };
    let got: Vec<Primitive> = got.into_iter().collect();
    report(text, &show_all(&got));

    if let (Some(result), Primitive::Segment(source)) = (got.first(), &object) {
        let result = segment_of(result)?;
        for end in [result.start, result.end] {
            let away = distance_to_line(source, end).unwrap_or(f64::NAN);
            expect((away - distance).abs() < AGREEMENT, || {
                format!("{end:?} is {away} from the line")
            })?;
        }
        expect(
            (result.length() - source.length()).abs() < AGREEMENT,
            || "the length changed".to_owned(),
        )?;
    }
    if let (Some(Primitive::Circle(result)), Primitive::Circle(source)) = (got.first(), &object) {
        expect(
            ((result.radius - source.radius).abs() - distance).abs() < AGREEMENT
                && near(result.center, source.center),
            || "not concentric at the distance".to_owned(),
        )?;
    }
    if let (Some(Primitive::Arc(result)), Primitive::Arc(source)) = (got.first(), &object) {
        expect(
            ((result.radius - source.radius).abs() - distance).abs() < AGREEMENT
                && near(result.center, source.center)
                && (result.sweep - source.sweep).abs() < AGREEMENT,
            || "not concentric at the distance with the same angles".to_owned(),
        )?;
    }

    let want = if outcome == "nothing" {
        Vec::new()
    } else {
        primitives(outcome)?
    };
    expect_all(&got, &want)
}

#[test]
fn offset_corpus() {
    run("offsets.txt", 12, &offset_case);
}

// ── trims ──

/// `<object> / <cutter> [; <cutter>…] @ <pick x y> => <piece> [; <piece>…]`,
/// or `=> nothing` when the object cannot be trimmed there.
/// Property: what is kept and what is removed add up to the object, and
/// what is removed is the part nearest the pick.
fn trim_case(text: &str) -> Check {
    let (inputs, outcome) = split(text, "=>")?;
    let (shapes, pick) = split(inputs, "@")?;
    let (object, cutters) = split(shapes, "/")?;
    let (object, cutters, pick) = (primitive(object)?, primitives(cutters)?, point(pick)?);
    let cuts: Vec<Point2> = cutters
        .iter()
        .flat_map(|cutter| object.intersect(cutter).as_slice().to_vec())
        .collect();

    let parts: Option<(Vec<Primitive>, Primitive)> = match &object {
        Primitive::Segment(segment) => {
            trim_segment_parts(segment, &cuts, pick, TOLERANCE).map(|parts| {
                let kept = parts.kept.into_iter().map(Primitive::Segment).collect();
                (kept, Primitive::Segment(parts.removed))
            })
        }
        Primitive::Arc(arc) => trim_arc_parts(arc, &cuts, pick, TOLERANCE).map(|parts| {
            let kept = parts.kept.into_iter().map(Primitive::Arc).collect();
            (kept, Primitive::Arc(parts.removed))
        }),
        Primitive::Circle(circle) => {
            trim_circle_parts(circle, &cuts, pick, TOLERANCE).map(|parts| {
                let kept = parts.kept.into_iter().map(Primitive::Arc).collect();
                (kept, Primitive::Arc(parts.removed))
            })
        }
    };
    let Some((kept, removed)) = parts else {
        report(text, "nothing");
        return expect(outcome == "nothing", || "gave nothing".to_owned());
    };
    report(text, &show_all(&kept));

    let total: f64 = kept.iter().map(length).sum::<f64>() + length(&removed);
    expect((total - length(&object)).abs() < AGREEMENT, || {
        format!(
            "kept and removed add up to {total}, not {}",
            length(&object)
        )
    })?;
    let to_removed = removed.distance_to_point(pick);
    expect(
        kept.iter()
            .all(|piece| to_removed <= piece.distance_to_point(pick) + AGREEMENT),
        || "a kept piece is nearer the pick than what was removed".to_owned(),
    )?;

    if outcome == "nothing" {
        return Err(format!("gave {}", show_all(&kept)));
    }
    expect_all(&kept, &primitives(outcome)?)
}

#[test]
fn trim_corpus() {
    run("trims.txt", 14, &trim_case);
}

// ── extends ──

/// `<object> / <boundary> [; <boundary>…] @ <pick x y> => <object>`, or
/// `=> nothing` when it reaches no boundary. The end nearer the pick is the
/// one extended.
/// Property: the other end stays; the object is still all there; and the
/// new end is on a boundary.
fn extend_case(text: &str) -> Check {
    let (inputs, outcome) = split(text, "=>")?;
    let (shapes, pick) = split(inputs, "@")?;
    let (object, boundaries) = split(shapes, "/")?;
    let (object, boundaries, pick) = (primitive(object)?, primitives(boundaries)?, point(pick)?);
    let (start, end) = ends(&object).ok_or("a circle has no end to extend")?;
    let from_end = pick.distance_to(end) <= pick.distance_to(start);

    let got: Option<Primitive> = match &object {
        Primitive::Segment(segment) => {
            extend_segment(segment, from_end, &boundaries, TOLERANCE).map(Primitive::Segment)
        }
        Primitive::Arc(arc) => {
            extend_arc(arc, from_end, &boundaries, TOLERANCE).map(Primitive::Arc)
        }
        Primitive::Circle(_) => None,
    };
    let Some(got) = got else {
        report(text, "nothing");
        return expect(outcome == "nothing", || "gave nothing".to_owned());
    };
    report(text, &show(&got));

    let (new_start, new_end) = ends(&got).ok_or("the result has no ends")?;
    let (fixed, was, tip, old_tip) = if from_end {
        (new_start, start, new_end, end)
    } else {
        (new_end, end, new_start, start)
    };
    expect(near(fixed, was), || "the other end moved".to_owned())?;
    expect(got.distance_to_point(old_tip) < AGREEMENT, || {
        "the result does not pass through the old end".to_owned()
    })?;
    expect(
        boundaries
            .iter()
            .any(|boundary| boundary.distance_to_point(tip) < AGREEMENT),
        || format!("the new end {tip:?} is on no boundary"),
    )?;

    if outcome == "nothing" {
        return Err(format!("gave {}", show(&got)));
    }
    expect_all(&[got], &primitives(outcome)?)
}

#[test]
fn extend_corpus() {
    run("extends.txt", 10, &extend_case);
}

// ── fillets and chamfers ──

fn refusal(error: CornerError) -> &'static str {
    match error {
        CornerError::Parallel => "parallel",
        CornerError::TooLarge => "too-large",
        CornerError::NoSolution => "no-solution",
    }
}

/// `fillet <radius> : <object> @ <pick> | <object> @ <pick> => <first> ; <second> [; <arc>]`
/// `chamfer <first distance> <second distance> : <segment> @ <pick> | <segment> @ <pick>
///      => <first> ; <second> [; <bevel>]`
/// or `=> refused parallel|too-large|no-solution`.
///
/// Two segments go through `Corner`, anything else through `fillet_curves`
/// — the same choice the document makes.
/// Property: the joint ends on both objects; a fillet arc has the radius
/// asked for and leaves each object along its tangent.
fn corner_case(text: &str) -> Check {
    let (inputs, outcome) = split(text, "=>")?;
    let (operation, shapes) = split(inputs, ":")?;
    let (first, second) = split(shapes, "|")?;
    let picked = |text: &str| -> Result<(Primitive, Point2), String> {
        let (object, pick) = split(text, "@")?;
        Ok((primitive(object)?, point(pick)?))
    };
    let ((a, a_pick), (b, b_pick)) = (picked(first)?, picked(second)?);
    let (name, amounts) = split(operation, " ")?;
    let amounts = numbers(amounts)?;

    // The two objects as they become, and the joint between them.
    let result: Result<(Primitive, Primitive, Option<Primitive>), CornerError> =
        match (name, amounts.as_slice(), &a, &b) {
            ("fillet", [radius], Primitive::Segment(a), Primitive::Segment(b)) => {
                Corner::new(a, a_pick, b, b_pick, TOLERANCE)
                    .and_then(|corner| corner.fillet(*radius, TOLERANCE))
                    .map(|fillet| {
                        let [first, second] = fillet.lines.map(Primitive::Segment);
                        (first, second, fillet.arc.map(Primitive::Arc))
                    })
            }
            ("fillet", [radius], _, _) => fillet_curves(&a, a_pick, &b, b_pick, *radius, TOLERANCE)
                .map(|fillet| (fillet.first, fillet.second, fillet.arc.map(Primitive::Arc))),
            ("chamfer", [first, second], Primitive::Segment(a), Primitive::Segment(b)) => {
                Corner::new(a, a_pick, b, b_pick, TOLERANCE)
                    .and_then(|corner| corner.chamfer([*first, *second], TOLERANCE))
                    .map(|chamfer| {
                        let [first, second] = chamfer.lines.map(Primitive::Segment);
                        (first, second, chamfer.bevel.map(Primitive::Segment))
                    })
            }
            _ => return Err(format!("cannot read `{operation}` for these objects")),
        };

    let (first, second, joint) = match result {
        Ok(parts) => parts,
        Err(error) => {
            let said = format!("refused {}", refusal(error));
            report(text, &said);
            return expect(outcome == said, || said.clone());
        }
    };
    let got: Vec<Primitive> = [Some(first), Some(second), joint]
        .into_iter()
        .flatten()
        .collect();
    report(text, &show_all(&got));

    if let Some(joint) = &joint {
        let (start, end) = ends(joint).ok_or("the joint has no ends")?;
        for (object, name) in [(&first, "first"), (&second, "second")] {
            let touches = object.distance_to_point(start) < AGREEMENT
                || object.distance_to_point(end) < AGREEMENT;
            expect(touches, || {
                format!("the joint does not end on the {name} object")
            })?;
        }
    }
    if let (Some(Primitive::Arc(arc)), "fillet") = (&joint, name) {
        expect((arc.radius - amounts[0]).abs() < AGREEMENT, || {
            format!("the arc has radius {}", arc.radius)
        })?;
        // Tangent: each object's line or circle is exactly one radius from
        // the arc's centre (or, for a circle, the radii add or subtract).
        for object in [&first, &second] {
            let tangent = match object {
                Primitive::Segment(line) => distance_to_line(line, arc.center)
                    .is_some_and(|away| (away - arc.radius).abs() < AGREEMENT),
                Primitive::Circle(Circle { center, radius })
                | Primitive::Arc(Arc { center, radius, .. }) => {
                    let between = center.distance_to(arc.center);
                    (between - (radius + arc.radius)).abs() < AGREEMENT
                        || (between - (radius - arc.radius).abs()).abs() < AGREEMENT
                }
            };
            expect(tangent, || {
                format!("the arc is not tangent to {}", show(object))
            })?;
        }
    }

    if outcome.starts_with("refused") {
        return Err(format!("gave {}", show_all(&got)));
    }
    expect_all(&got, &primitives(outcome)?)
}

#[test]
fn corner_corpus() {
    run("fillets.txt", 14, &corner_case);
}

// ── the runner itself ──

/// The acceptance criterion of #129: a failing case names its file and line.
#[test]
fn a_failing_case_is_reported_with_its_file_and_line() {
    let corpus = "# a comment\n\
                  segment 0 0 10 10 | segment 0 10 10 0 => 5 5\n\
                  \n\
                  segment 0 0 10 10 | segment 0 10 10 0 => 6 6\n\
                  not a case at all\n";
    let (cases, failed) = failures("made-up.txt", corpus, &intersection_case);
    assert_eq!(cases, 3, "comments and blank lines are not cases");
    assert_eq!(failed.len(), 2, "{failed:?}");
    assert!(
        failed[0].starts_with("test-data/geometry/made-up.txt:4: (a|b) gave 5 5"),
        "{}",
        failed[0]
    );
    assert!(failed[0].ends_with("segment 0 0 10 10 | segment 0 10 10 0 => 6 6"));
    assert!(
        failed[1].starts_with("test-data/geometry/made-up.txt:5: missing `=>`"),
        "{}",
        failed[1]
    );
}

/// Every kind of case reports a wrong recorded outcome rather than passing.
#[test]
fn a_wrong_recorded_outcome_fails_in_every_corpus() {
    let wrong: [(&CaseCheck, &str); 5] = [
        (
            &offset_case,
            "segment 0 0 10 0 : 5 @ 0 9 => segment 0 6 10 6",
        ),
        (
            &trim_case,
            "segment 0 0 10 0 / segment 4 -5 4 5 @ 8 0 => segment 0 0 5 0",
        ),
        (
            &extend_case,
            "segment 0 0 10 0 / segment 20 -5 20 5 @ 9 0 => segment 0 0 21 0",
        ),
        (
            &corner_case,
            "fillet 0 : segment 5 0 50 0 @ 20 0 | segment 0 5 0 50 @ 0 20 => refused parallel",
        ),
        (
            &polyline_offset_case,
            "square : closed : 0,0 10,0 10,10 0,10 : 1 => 1 99",
        ),
    ];
    for (check, case) in wrong {
        assert!(check(case).is_err(), "`{case}` should fail");
    }
}
