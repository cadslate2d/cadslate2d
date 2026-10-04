//! The command-line log: echoed input, prompts, and messages.

use std::collections::VecDeque;

/// A bounded scrollback. Lines are numbered from the start of the session so
/// the UI can ask for "everything after line N" and append, instead of
/// re-reading the whole log on every change.
#[derive(Debug)]
pub(crate) struct CommandLog {
    lines: VecDeque<String>,
    /// Sequence number of `lines[0]`.
    first: u64,
}

impl CommandLog {
    /// Lines kept before the oldest are dropped.
    pub(crate) const CAPACITY: usize = 500;

    pub(crate) fn new() -> Self {
        Self {
            lines: VecDeque::new(),
            first: 0,
        }
    }

    pub(crate) fn push(&mut self, line: String) {
        if self.lines.len() == Self::CAPACITY {
            self.lines.pop_front();
            self.first += 1;
        }
        self.lines.push_back(line);
    }

    /// Total number of lines ever written.
    pub(crate) fn total(&self) -> u64 {
        self.first + self.lines.len() as u64
    }

    /// Lines with sequence number `>= since`, oldest first. Lines that have
    /// already scrolled out of the buffer are skipped.
    pub(crate) fn since(&self, since: u64) -> impl Iterator<Item = &str> {
        let skip = since.saturating_sub(self.first) as usize;
        self.lines.iter().skip(skip).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(log: &CommandLog, since: u64) -> Vec<&str> {
        log.since(since).collect()
    }

    #[test]
    fn lines_are_returned_from_a_sequence_number() {
        let mut log = CommandLog::new();
        for line in ["a", "b", "c"] {
            log.push(line.to_owned());
        }
        assert_eq!(log.total(), 3);
        assert_eq!(collect(&log, 0), vec!["a", "b", "c"]);
        assert_eq!(collect(&log, 2), vec!["c"]);
        assert_eq!(collect(&log, 3), Vec::<&str>::new());
        assert_eq!(collect(&log, 99), Vec::<&str>::new());
    }

    #[test]
    fn the_oldest_lines_scroll_out_but_numbering_continues() {
        let mut log = CommandLog::new();
        for n in 0..CommandLog::CAPACITY + 10 {
            log.push(n.to_string());
        }
        assert_eq!(log.total(), (CommandLog::CAPACITY + 10) as u64);
        // Asking from before the retained window yields the whole window.
        assert_eq!(log.since(0).count(), CommandLog::CAPACITY);
        assert_eq!(log.since(0).next(), Some("10"));
        assert_eq!(
            collect(&log, log.total() - 1),
            vec![(CommandLog::CAPACITY + 9).to_string()]
        );
    }
}
