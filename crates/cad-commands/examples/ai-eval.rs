//! Scores the AI corpus (#262): every level under `test-data/ai`, against
//! the recordings committed beside it, or against another night's.
//!
//! ```text
//! ai-eval ROOT                     replay: every case must pass (scripts/check.sh)
//! ai-eval ROOT --recordings DIR    a live run, DIR/<level>.jsonl, against the
//!                                  committed recordings as the baseline: a
//!                                  language falling below it is a regression
//!     [--json FILE] [--markdown FILE] [--prices INPUT,CACHED,OUTPUT]
//! ```
//!
//! Prints a table per level and language. Exits 1 when a replayed case
//! misses, 2 when a live run regresses.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use cad_commands::evaluation::{Corpus, Score};
use serde_json::{Value, json};

struct Options {
    root: PathBuf,
    recordings: Option<PathBuf>,
    json: Option<PathBuf>,
    markdown: Option<PathBuf>,
    /// US dollars per million tokens: input, cached input, output.
    prices: Option<[f64; 3]>,
}

fn options() -> Result<Options, String> {
    let mut args = std::env::args().skip(1);
    let mut options = Options {
        root: PathBuf::new(),
        recordings: None,
        json: None,
        markdown: None,
        prices: None,
    };
    let mut root = None;
    while let Some(arg) = args.next() {
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
        match arg.as_str() {
            "--recordings" => options.recordings = Some(value(&arg)?.into()),
            "--json" => options.json = Some(value(&arg)?.into()),
            "--markdown" => options.markdown = Some(value(&arg)?.into()),
            "--prices" => {
                let numbers: Vec<f64> = value(&arg)?
                    .split(',')
                    .map(|n| n.trim().parse::<f64>().map_err(|e| e.to_string()))
                    .collect::<Result<_, _>>()?;
                options.prices = Some(
                    numbers
                        .try_into()
                        .map_err(|_| "--prices is INPUT,CACHED,OUTPUT".to_owned())?,
                );
            }
            other if root.is_none() && !other.starts_with("--") => root = Some(other.into()),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    options.root = root.ok_or(
        "usage: ai-eval ROOT [--recordings DIR] [--json FILE] [--markdown FILE] [--prices I,C,O]",
    )?;
    Ok(options)
}

fn read(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn percent(passed: usize, total: usize) -> String {
    if total == 0 {
        "—".to_owned()
    } else {
        format!("{:.1}%", 100.0 * passed as f64 / total as f64)
    }
}

fn cost(score: &Score, prices: Option<[f64; 3]>) -> Option<f64> {
    let [input, cached, output] = prices?;
    let tokens = score.tokens();
    Some(
        (tokens.input.saturating_sub(tokens.cached_input) as f64 * input
            + tokens.cached_input as f64 * cached
            + tokens.output as f64 * output)
            / 1e6,
    )
}

fn main() -> ExitCode {
    let options = match options() {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(64);
        }
    };
    let levels = match Corpus::levels(&options.root) {
        Ok(levels) => levels,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::FAILURE;
        }
    };
    let live = options.recordings.is_some();
    let mut markdown = String::from(if live {
        "## AI corpus, live\n\n| Level | Language | Passed | Rate | Baseline |\n| --- | --- | --- | --- | --- |\n"
    } else {
        "## AI corpus, replayed\n\n| Level | Language | Passed | Rate |\n| --- | --- | --- | --- |\n"
    });
    let mut reports = Vec::<Value>::new();
    let mut misses = String::new();
    let (mut missed, mut regressed) = (false, false);
    for level in &levels {
        let baseline = level.score(&read(
            &options.root.join(&level.level).join("recordings.jsonl"),
        ));
        let score = match &options.recordings {
            Some(directory) => {
                level.score(&read(&directory.join(format!("{}.jsonl", level.level))))
            }
            None => baseline.clone(),
        };
        let before: std::collections::BTreeMap<String, (usize, usize)> = baseline
            .by_language()
            .into_iter()
            .map(|(language, passed, total)| (language, (passed, total)))
            .collect();
        for (language, passed, total) in score.by_language() {
            let line = format!(
                "{} {language}: {passed}/{total} ({})",
                level.level,
                percent(passed, total)
            );
            println!("{line}");
            let _ = write!(
                markdown,
                "| {} | {language} | {passed}/{total} | {} |",
                level.level,
                percent(passed, total)
            );
            if live {
                let (was, of) = before.get(&language).copied().unwrap_or_default();
                let _ = write!(markdown, " {} |", percent(was, of));
            }
            markdown.push('\n');
        }
        let regressions = score.regressions(&baseline);
        for (language, now, was) in &regressions {
            println!(
                "{} {language}: REGRESSION, {:.1}% against the baseline's {:.1}%",
                level.level,
                now * 100.0,
                was * 100.0
            );
        }
        regressed |= live && !regressions.is_empty();
        missed |= score.passed() < score.results.len();
        for result in score.misses() {
            let why = result.miss.as_deref().unwrap_or("");
            println!("  miss {}: {why}", result.name);
            let _ = writeln!(misses, "- `{}`: {}", result.name, why.replace('\n', " "));
        }
        let tokens = score.tokens();
        let spent = cost(&score, options.prices);
        println!(
            "{} tokens: {} input ({} cached), {} output{}",
            level.level,
            tokens.input,
            tokens.cached_input,
            tokens.output,
            spent.map_or_else(String::new, |dollars| format!(", ${dollars:.4}"))
        );
        let mut report = score.to_json();
        report["cost_usd"] = json!(spent);
        if live {
            report["baseline"] = baseline.to_json();
            report["regressions"] = json!(regressions
                .iter()
                .map(|(language, now, was)| json!({ "language": language, "rate": now, "baseline": was }))
                .collect::<Vec<_>>());
        }
        reports.push(report);
    }
    if !misses.is_empty() {
        let _ = write!(markdown, "\n### Misses\n\n{misses}");
    }
    if regressed {
        markdown.push_str("\n**A regression against the recorded baseline.**\n");
    }
    if let Some(path) = &options.json {
        let _ = std::fs::write(
            path,
            serde_json::to_string_pretty(&reports).unwrap_or_default(),
        );
    }
    if let Some(path) = &options.markdown {
        let _ = std::fs::write(path, &markdown);
    }
    if regressed {
        ExitCode::from(2)
    } else if missed && !live {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
