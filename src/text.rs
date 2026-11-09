//! Plain-text helpers: encodings, line endings, statistics and searching.

use std::ops::Range;

/// Line endings used by the file on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Eol {
    #[default]
    Lf,
    CrLf,
}

impl Eol {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::CrLf => "CRLF",
        }
    }

    /// Split a file's contents into the text we edit (always `\n`) and the
    /// line ending that should be restored on save.
    pub fn normalize(text: &str) -> (String, Self) {
        if text.contains("\r\n") {
            (text.replace("\r\n", "\n").replace('\r', "\n"), Self::CrLf)
        } else if text.contains('\r') {
            (text.replace('\r', "\n"), Self::Lf)
        } else {
            (text.to_owned(), Self::Lf)
        }
    }

    /// Convert our internal `\n` representation back to the file's line ending.
    pub fn encode(self, text: &str) -> String {
        match self {
            Self::Lf => text.to_owned(),
            Self::CrLf => text.replace('\n', "\r\n"),
        }
    }
}

/// Text encoding of the file. Files are always edited as UTF-8; the BOM is
/// remembered so that round-tripping a file does not silently change it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Encoding {
    #[default]
    Utf8,
    Utf8Bom,
}

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

impl Encoding {
    pub fn label(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf8Bom => "UTF-8 BOM",
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<(String, Self), String> {
        let (body, encoding) = match bytes.strip_prefix(BOM) {
            Some(rest) => (rest, Self::Utf8Bom),
            None => (bytes, Self::Utf8),
        };
        let text = std::str::from_utf8(body)
            .map_err(|err| format!("File is not valid UTF-8 (at byte {}).", err.valid_up_to()))?;
        Ok((text.to_owned(), encoding))
    }

    pub fn encode(self, text: &str) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(text.len() + 3);
        if self == Self::Utf8Bom {
            bytes.extend_from_slice(BOM);
        }
        bytes.extend_from_slice(text.as_bytes());
        bytes
    }
}

/// Everything in the status bar that only changes when the text changes.
#[derive(Clone, Debug, Default)]
pub struct TextStats {
    pub lines: usize,
    pub words: usize,
    pub chars: usize,
    /// Character offset of the start of each line.
    line_starts: Vec<usize>,
}

impl TextStats {
    pub fn compute(text: &str) -> Self {
        let mut line_starts = vec![0];
        let mut chars = 0;
        for (index, ch) in text.chars().enumerate() {
            chars += 1;
            if ch == '\n' {
                line_starts.push(index + 1);
            }
        }
        Self {
            lines: line_starts.len(),
            words: text.split_whitespace().count(),
            chars,
            line_starts,
        }
    }

    /// 1-based line and column for a character offset.
    pub fn line_col(&self, char_index: usize) -> (usize, usize) {
        let char_index = char_index.min(self.chars);
        let line = self
            .line_starts
            .partition_point(|&start| start <= char_index)
            - 1;
        (line + 1, char_index - self.line_starts[line] + 1)
    }

    /// Character offset where a 1-based line starts.
    pub fn line_start(&self, line: usize) -> usize {
        let index = line.clamp(1, self.lines) - 1;
        self.line_starts[index]
    }
}

/// Byte offset of a character offset.
pub fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// Character offset of a byte offset.
pub fn byte_to_char(text: &str, byte_index: usize) -> usize {
    text[..byte_index.min(text.len())].chars().count()
}

/// Non-overlapping matches of `needle` in `haystack`, as byte ranges.
///
/// Matching is byte-exact, with optional ASCII case folding.
pub fn find_matches(haystack: &str, needle: &str, case_sensitive: bool) -> Vec<Range<usize>> {
    let mut matches = Vec::new();
    if needle.is_empty() {
        return matches;
    }
    let needle_len = needle.len();
    let mut start = 0;
    while start + needle_len <= haystack.len() {
        if !haystack.is_char_boundary(start) {
            start += 1;
            continue;
        }
        // A candidate whose end lies inside a multibyte character is not a
        // match, but later boundaries may still be. Skip to the next character
        // instead of ending the search.
        let Some(candidate) = haystack.get(start..start + needle_len) else {
            start += haystack[start..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        let hit = if case_sensitive {
            candidate == needle
        } else {
            candidate.eq_ignore_ascii_case(needle)
        };
        if hit {
            matches.push(start..start + needle_len);
            start += needle_len;
        } else {
            start += haystack[start..].chars().next().map_or(1, char::len_utf8);
        }
    }
    matches
}

/// Format an integer with thousands separators.
pub fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eol_round_trip() {
        let (text, eol) = Eol::normalize("a\r\nb\r\nc");
        assert_eq!(text, "a\nb\nc");
        assert_eq!(eol, Eol::CrLf);
        assert_eq!(eol.encode(&text), "a\r\nb\r\nc");
    }

    #[test]
    fn lone_carriage_returns_are_normalized() {
        let (text, eol) = Eol::normalize("a\rb");
        assert_eq!(text, "a\nb");
        assert_eq!(eol, Eol::Lf);
    }

    #[test]
    fn encoding_bom_round_trip() {
        let raw = [BOM, b"hello".as_slice()].concat();
        let (text, encoding) = Encoding::decode(&raw).unwrap();
        assert_eq!(text, "hello");
        assert_eq!(encoding, Encoding::Utf8Bom);
        assert_eq!(encoding.encode(&text), raw);
    }

    #[test]
    fn invalid_utf8_is_reported() {
        assert!(Encoding::decode(&[0xFF, 0xFE]).is_err());
    }

    #[test]
    fn stats_and_positions() {
        let stats = TextStats::compute("one two\nthree\n");
        assert_eq!(stats.lines, 3);
        assert_eq!(stats.words, 3);
        assert_eq!(stats.chars, 14);
        assert_eq!(stats.line_col(0), (1, 1));
        assert_eq!(stats.line_col(8), (2, 1));
        assert_eq!(stats.line_col(13), (2, 6));
        assert_eq!(stats.line_start(2), 8);
    }

    #[test]
    fn empty_document_has_one_line() {
        let stats = TextStats::compute("");
        assert_eq!(stats.lines, 1);
        assert_eq!(stats.line_col(0), (1, 1));
    }

    #[test]
    fn char_byte_conversion() {
        let text = "žluťoučký kůň";
        let byte = char_to_byte(text, 3);
        assert_eq!(byte_to_char(text, byte), 3);
    }

    #[test]
    fn finds_non_overlapping_matches() {
        assert_eq!(find_matches("aaaa", "aa", true), vec![0..2, 2..4]);
        assert_eq!(find_matches("Hello hello", "hello", true), vec![6..11]);
        assert_eq!(
            find_matches("Hello hello", "hello", false),
            vec![0..5, 6..11]
        );
        assert!(find_matches("abc", "", false).is_empty());
    }

    #[test]
    fn find_respects_char_boundaries() {
        assert_eq!(find_matches("žluť", "luť", true), vec![2..6]);
    }

    #[test]
    fn find_skips_candidates_ending_inside_multibyte_characters() {
        // A one-byte needle after a multibyte prefix: the first candidate ends
        // inside "é"/"🙂" and must not abort the search.
        assert_eq!(find_matches("éa", "a", false), vec![2..3]);
        assert_eq!(find_matches("🙂foo", "foo", true), vec![4..7]);
    }

    #[test]
    fn find_continues_past_multibyte_text_between_matches() {
        assert_eq!(find_matches("aé a🙂a", "a", true), vec![0..1, 4..5, 9..10]);
        assert_eq!(find_matches("žluť luť", "luť", true), vec![2..6, 7..11]);
    }
}
