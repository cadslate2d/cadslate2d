//! Recall of previously submitted input (DESIGN §13.4: Up / Down).

/// Submitted lines with a recall cursor, like a shell history.
#[derive(Debug, Default)]
pub(crate) struct InputHistory {
    entries: Vec<String>,
    /// Index of the entry currently recalled; `None` means "at the new line".
    cursor: Option<usize>,
}

impl InputHistory {
    /// Records a submitted line and returns the cursor to the new line.
    /// Blank lines and immediate repeats are not recorded.
    pub(crate) fn record(&mut self, line: &str) {
        let line = line.trim();
        if !line.is_empty() && self.entries.last().map(String::as_str) != Some(line) {
            self.entries.push(line.to_owned());
        }
        self.cursor = None;
    }

    /// Steps back to an older entry. `None` when there is nothing older.
    pub(crate) fn previous(&mut self) -> Option<&str> {
        let index = match self.cursor {
            None => self.entries.len().checked_sub(1)?,
            Some(index) => index.checked_sub(1)?,
        };
        self.cursor = Some(index);
        Some(&self.entries[index])
    }

    /// Steps forward to a newer entry. Stepping past the newest returns an
    /// empty string (the new line). `None` when already there.
    pub(crate) fn next(&mut self) -> Option<&str> {
        let index = self.cursor? + 1;
        if index < self.entries.len() {
            self.cursor = Some(index);
            Some(&self.entries[index])
        } else {
            self.cursor = None;
            Some("")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(lines: &[&str]) -> InputHistory {
        let mut history = InputHistory::default();
        lines.iter().for_each(|line| history.record(line));
        history
    }

    #[test]
    fn an_empty_history_recalls_nothing() {
        let mut history = InputHistory::default();
        assert_eq!(history.previous(), None);
        assert_eq!(history.next(), None);
    }

    #[test]
    fn previous_walks_back_and_stops_at_the_oldest() {
        let mut history = history(&["L", "0,0", "@5,5"]);
        assert_eq!(history.previous(), Some("@5,5"));
        assert_eq!(history.previous(), Some("0,0"));
        assert_eq!(history.previous(), Some("L"));
        assert_eq!(history.previous(), None);
    }

    #[test]
    fn next_walks_forward_and_ends_on_an_empty_line() {
        let mut history = history(&["L", "0,0"]);
        history.previous();
        history.previous();
        assert_eq!(history.next(), Some("0,0"));
        assert_eq!(history.next(), Some(""));
        assert_eq!(history.next(), None);
    }

    #[test]
    fn blank_lines_and_immediate_repeats_are_not_recorded() {
        let mut history = history(&["L", "", "  ", "L", " L ", "0,0", "L"]);
        assert_eq!(history.previous(), Some("L"));
        assert_eq!(history.previous(), Some("0,0"));
        assert_eq!(history.previous(), Some("L"));
        assert_eq!(history.previous(), None);
    }

    #[test]
    fn recording_resets_the_cursor() {
        let mut history = history(&["a", "b"]);
        history.previous();
        history.previous();
        history.record("c");
        assert_eq!(history.previous(), Some("c"));
    }
}
