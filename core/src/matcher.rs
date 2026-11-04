//! Name matching. Each candidate gets a match class (how "intentional" the
//! match looks) and a fuzzy score used as a tie-breaker inside a class.

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Exact name.
pub const EXACT: u8 = 4;
/// The name starts with the query.
pub const PREFIX: u8 = 3;
/// Every query word starts a word of the name, or the query is a prefix
/// of the name's initials ("vsc" for Visual Studio Code).
pub const WORDS: u8 = 2;
/// Letters appear in order somewhere.
pub const FUZZY: u8 = 1;
const FUZZY_MIN_PER_CHAR: u32 = 20;

pub struct NameMatcher {
    lower: String,
    words: Vec<String>,
    letters: u32,
    pattern: Pattern,
    matcher: Matcher,
    buf: Vec<char>,
}

impl NameMatcher {
    pub fn new(query: &str) -> Self {
        Self::with_config(query, Config::DEFAULT)
    }

    pub fn for_paths(query: &str) -> Self {
        Self::with_config(query, Config::DEFAULT.match_paths())
    }

    fn with_config(query: &str, config: Config) -> Self {
        let lower = query.trim().to_lowercase();
        let words = lower.split_whitespace().map(str::to_owned).collect();
        NameMatcher {
            letters: lower.chars().filter(|c| !c.is_whitespace()).count() as u32,
            pattern: Pattern::new(&lower, CaseMatching::Ignore, Normalization::Smart, AtomKind::Fuzzy),
            lower,
            words,
            matcher: Matcher::new(config),
            buf: Vec::new(),
        }
    }

    /// `(class, score)` or `None` when the name doesn't match at all.
    pub fn score(&mut self, name: &str) -> Option<(u8, u32)> {
        if self.lower.is_empty() {
            return None;
        }
        let fuzzy = self.pattern.score(Utf32Str::new(name, &mut self.buf), &mut self.matcher)?;
        let c = class(&self.lower, &self.words, name);
        // Scattered letters ("saf" in "Counter-Strike Global Offensive") score
        // under ~20 per query letter; real abbreviations score above.
        if c == FUZZY && fuzzy < FUZZY_MIN_PER_CHAR * self.letters {
            return None;
        }
        Some((c, fuzzy))
    }

    /// Best match over a name and its aliases.
    pub fn score_any<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) -> Option<(u8, u32)> {
        names.into_iter().filter_map(|n| self.score(n)).max()
    }
}

fn class(lower_q: &str, q_words: &[String], name: &str) -> u8 {
    let t = name.to_lowercase();
    if t == lower_q {
        return EXACT;
    }
    if t.starts_with(lower_q) {
        return PREFIX;
    }
    let words = split_words(name);
    let all_words_prefix = q_words
        .iter()
        .all(|qw| words.iter().any(|w| w.to_lowercase().starts_with(qw.as_str())));
    let initials: String = words.iter().filter_map(|w| w.chars().next()).flat_map(char::to_lowercase).collect();
    if all_words_prefix || (!lower_q.contains(' ') && initials.starts_with(lower_q)) {
        return WORDS;
    }
    FUZZY
}

/// Split on non-alphanumerics and lower→upper case changes ("iTerm" → i, Term).
pub fn split_words(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut prev_lower = false;
    for (i, c) in s.char_indices() {
        if !c.is_alphanumeric() {
            if let Some(st) = start.take() {
                out.push(&s[st..i]);
            }
            prev_lower = false;
            continue;
        }
        match start {
            None => start = Some(i),
            Some(st) if c.is_uppercase() && prev_lower => {
                out.push(&s[st..i]);
                start = Some(i);
            }
            _ => {}
        }
        prev_lower = c.is_lowercase() || c.is_numeric();
    }
    if let Some(st) = start {
        out.push(&s[st..]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cls(q: &str, name: &str) -> Option<u8> {
        NameMatcher::new(q).score(name).map(|(c, _)| c)
    }

    #[test]
    fn classes() {
        assert_eq!(cls("safari", "Safari"), Some(EXACT));
        assert_eq!(cls("saf", "Safari"), Some(PREFIX));
        assert_eq!(cls("code", "Visual Studio Code"), Some(WORDS));
        assert_eq!(cls("vsc", "Visual Studio Code"), Some(WORDS));
        assert_eq!(cls("vis cod", "Visual Studio Code"), Some(WORDS));
        assert_eq!(cls("term", "iTerm"), Some(WORDS));
        assert_eq!(cls("sfr", "Safari"), Some(FUZZY));
        assert_eq!(cls("xyz", "Safari"), None);
        assert_eq!(cls("", "Safari"), None);
    }

    #[test]
    fn scattered_fuzzy_matches_are_dropped() {
        assert_eq!(cls("saf", "Counter-Strike Global Offensive"), None);
        assert_eq!(cls("term", "Cisco AnyConnect Secure Mobility Client"), None);
        assert_eq!(cls("chrm", "Google Chrome"), Some(FUZZY));
        assert_eq!(cls("photshp", "Adobe Photoshop 2025"), Some(FUZZY));
    }

    #[test]
    fn words_split_camel_case() {
        assert_eq!(split_words("iTerm2"), vec!["i", "Term2"]);
        assert_eq!(split_words("System Settings"), vec!["System", "Settings"]);
        assert_eq!(split_words("VLC-media_player"), vec!["VLC", "media", "player"]);
    }
}
