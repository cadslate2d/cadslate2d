//! The codes DXF text carries inside its words (DESIGN §24.2; #145).
//!
//! A TEXT's `%%d`, `%%p`, and `%%c` are the degree, plus-minus, and
//! diameter signs; `%%u` and `%%o` switch underlining and overlining;
//! `%%` and three digits is a character by its code (#397). An
//! MTEXT's words are an inline language: `\P` breaks a paragraph, `\~` is
//! a space that does not break, braces group, and a dozen backslash codes
//! change the font, height, colour, stacking, and more. The drawing holds
//! words and line breaks only, so the words are kept, the breaks are kept,
//! a stacked fraction becomes `a/b`, and every other code is dropped — and
//! the report says so.

/// `text` with the TEXT codes resolved; whether anything was dropped. A
/// TEXT is one line: a line break or another control character but a tab,
/// which a DWG's length-counted string may hold, is a space, and dropped
/// formatting (#430) — as the writer puts one in the file.
pub(crate) fn plain_text(text: &str) -> (String, bool) {
    let control = |c: char| c.is_control() && c != '\t';
    let spaced: String;
    let mut dropped = false;
    let text = if text.chars().any(control) {
        dropped = true;
        spaced = text
            .chars()
            .map(|c| if control(c) { ' ' } else { c })
            .collect();
        spaced.as_str()
    } else {
        text
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("%%") {
        out.push_str(&rest[..at]);
        // `%%065` is `A`: a character of the code page by its number,
        // as AutoCAD reads it (#397). One that is not printable is not
        // words, and is dropped.
        let digits = rest[at + 2..]
            .get(..3)
            .filter(|d| d.bytes().all(|b| b.is_ascii_digit()));
        if let Some(digits) = digits {
            let character = digits
                .parse::<u8>()
                .ok()
                .map(crate::import::windows_1252)
                .filter(|c| !c.is_control());
            match character {
                Some(c) => out.push(c),
                None => dropped = true,
            }
            rest = &rest[at + 5..];
            continue;
        }
        let code = rest[at + 2..].chars().next();
        rest = &rest[at + 2 + code.map_or(0, char::len_utf8)..];
        match code.map(|c| c.to_ascii_lowercase()) {
            Some('d') => out.push('°'),
            Some('p') => out.push('±'),
            Some('c') => out.push('Ø'),
            Some('%') => out.push('%'),
            // Underline and overline: a look, not words.
            Some('u' | 'o') => dropped = true,
            _ => dropped = true,
        }
    }
    out.push_str(rest);
    (out, dropped)
}

/// An MTEXT's words with the formatting taken out: paragraph breaks become
/// line breaks; whether any formatting was dropped.
pub(crate) fn plain_mtext(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut dropped = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' | '}' => {}
            '\\' => match chars.next() {
                // `\X` puts what follows under a dimension's line: a
                // line break, with no argument (#418).
                Some('P') | Some('N') | Some('X') => out.push('\n'),
                Some('~') => out.push(' '),
                Some('\\') => out.push('\\'),
                Some('{') => out.push('{'),
                Some('}') => out.push('}'),
                // A stacked fraction: numerator, a divider, denominator.
                Some('S') => {
                    dropped = true;
                    let mut stack = String::new();
                    for d in chars.by_ref() {
                        if d == ';' {
                            break;
                        }
                        stack.push(d);
                    }
                    let (top, bottom) = stack
                        .split_once(['^', '/', '#'])
                        .unwrap_or((stack.as_str(), ""));
                    out.push_str(top.trim());
                    if !bottom.is_empty() {
                        out.push('/');
                        out.push_str(bottom.trim());
                    }
                }
                // Codes with an argument up to the semicolon.
                Some('f' | 'F' | 'H' | 'C' | 'c' | 'T' | 'Q' | 'W' | 'A' | 'p') => {
                    dropped = true;
                    for d in chars.by_ref() {
                        if d == ';' {
                            break;
                        }
                    }
                }
                // Switches with no argument, and anything unknown.
                Some(_) => dropped = true,
                None => dropped = true,
            },
            _ => out.push(c),
        }
    }
    let (resolved, codes) = plain_text(&out);
    (resolved, dropped || codes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_codes_become_their_signs_and_looks_are_dropped() {
        assert_eq!(plain_text("45%%d"), ("45°".to_owned(), false));
        assert_eq!(plain_text("%%c20 %%p0.1"), ("Ø20 ±0.1".to_owned(), false));
        assert_eq!(plain_text("100%%%"), ("100%".to_owned(), false));
        assert_eq!(plain_text("%%uunder%%u"), ("under".to_owned(), true));
        assert_eq!(plain_text("%%zodd"), ("odd".to_owned(), true));
        assert_eq!(plain_text("plain"), ("plain".to_owned(), false));
        assert_eq!(plain_text("ends %%"), ("ends ".to_owned(), true));
    }

    /// `%%` and three digits is a character by its number in the code
    /// page (#397); one that is not printable, or past 255, is dropped.
    /// Fewer digits are not a code of this kind.
    #[test]
    fn a_percent_code_with_three_digits_is_a_character() {
        assert_eq!(plain_text("%%065BC"), ("ABC".to_owned(), false));
        assert_eq!(
            plain_text("%%0372 %%176 %%128"),
            ("%2 ° €".to_owned(), false)
        );
        // A line feed is not words: dropped, not a line split.
        assert_eq!(plain_text("1%%0102"), ("12".to_owned(), true));
        assert_eq!(plain_text("a%%999b"), ("ab".to_owned(), true));
        assert_eq!(plain_text("%%06"), ("6".to_owned(), true));
        assert_eq!(plain_mtext("x %%065;"), ("x A;".to_owned(), false));
    }

    #[test]
    fn mtext_keeps_words_and_breaks_and_drops_formatting() {
        assert_eq!(plain_mtext("one\\Ptwo"), ("one\ntwo".to_owned(), false));
        assert_eq!(plain_mtext("tied\\~up"), ("tied up".to_owned(), false));
        assert_eq!(
            plain_mtext("a\\\\b \\{c\\}"),
            ("a\\b {c}".to_owned(), false)
        );
        assert_eq!(
            plain_mtext("{\\fArial|b1|i0;Bold} and \\H5;tall"),
            ("Bold and tall".to_owned(), true)
        );
        assert_eq!(
            plain_mtext("\\C1;red \\Lunder\\l"),
            ("red under".to_owned(), true)
        );
        assert_eq!(plain_mtext("\\Sa^ b; over"), ("a/b over".to_owned(), true));
        assert_eq!(plain_mtext("\\S1/2;"), ("1/2".to_owned(), true));
        assert_eq!(plain_mtext("\\Sonly;"), ("only".to_owned(), true));
        assert_eq!(plain_mtext("45%%d\\Pnext"), ("45°\nnext".to_owned(), false));
        // A dimension's text under its line follows `\X`, which takes no
        // argument (#418).
        assert_eq!(
            plain_mtext("ABOVE\\XBELOW"),
            ("ABOVE\nBELOW".to_owned(), false)
        );
        assert_eq!(plain_mtext("trailing\\"), ("trailing".to_owned(), true));
        assert_eq!(
            plain_mtext("\\pxi-2,l2;indented"),
            ("indented".to_owned(), true)
        );
    }
}
