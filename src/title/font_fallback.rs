//! Per-codepoint font fallback discovery via `fc-match`.
//!
//! The `ab_glyph` and `skrifa` title renderers each load a single primary font.
//! When the title contains a codepoint not covered by that font (emoji, CJK,
//! many symbol ranges), this module finds an additional face that does cover it
//! using `fc-match -f '%{file}' :charset=HEX`, memory-maps it, and hands it back
//! for the backend to parse. Results are cached per-renderer; missing-coverage
//! lookups happen at most once per unique codepoint.

use std::{collections::HashSet, fs::File, process::Command};

/// Cache of fallback face mmaps and per-codepoint discovery state.
///
/// `fonts` is the ordered list of fallback faces; backends parse these into
/// their own font handles each render. Discovery is one-shot per codepoint
/// (`seen_chars`) and faces are deduped by file path (`paths`).
#[derive(Debug, Default)]
pub(crate) struct FallbackCache {
    pub fonts: Vec<memmap2::Mmap>,
    paths: HashSet<String>,
    seen_chars: HashSet<char>,
}

impl FallbackCache {
    /// For each char in `text` not already considered, call `covered` to ask
    /// whether the caller already has coverage. If not, run `fc-match` and
    /// push a deduped fallback face. The closure may inspect `&self.fonts`
    /// (already loaded so far) to know which faces are currently available;
    /// each char is checked against fallbacks pushed by earlier chars in the
    /// same call, so a single font can satisfy multiple codepoints.
    pub fn extend<F>(&mut self, text: &str, mut covered: F)
    where
        F: FnMut(char, &[memmap2::Mmap]) -> bool,
    {
        for c in text.chars() {
            if c.is_control() {
                continue;
            }
            if !self.seen_chars.insert(c) {
                continue;
            }
            if covered(c, &self.fonts) {
                continue;
            }
            let Some((path, mmap)) = find_font_for_char(c) else {
                continue;
            };
            if !self.paths.insert(path) {
                continue;
            }
            self.fonts.push(mmap);
        }
    }
}

/// `fc-match :charset=HEX` for the smallest font with `c` in its charset.
fn find_font_for_char(c: char) -> Option<(String, memmap2::Mmap)> {
    let pattern = format!(":charset={:X}", c as u32);
    let out = Command::new("fc-match")
        .arg("-f")
        .arg("%{file}")
        .arg(&pattern)
        .output()
        .ok()?;
    let path = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    if path.is_empty() {
        return None;
    }
    let file = File::open(&path).ok()?;
    // Safety: System font files are not expected to be mutated during use.
    let mmap = unsafe { memmap2::Mmap::map(&file).ok()? };
    Some((path, mmap))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn extend_dedupes_paths() {
        let mut cache = FallbackCache::default();
        // Force two ASCII chars to "miss" so fc-match is consulted for each.
        // Distinct ASCII letters resolve to the same default sans font on any
        // reasonable system, so the deduped fallback list should be ≤ 1.
        cache.extend("ab", |_, _| false);
        assert!(
            cache.fonts.len() <= 1,
            "two ASCII chars must share at most one fallback face, got {}",
            cache.fonts.len()
        );
    }

    #[test]
    fn extend_skips_already_seen_chars() {
        let mut cache = FallbackCache::default();
        let mut calls = 0;
        cache.extend("aa", |_, _| {
            calls += 1;
            true
        });
        assert_eq!(calls, 1, "the second 'a' should not invoke the closure");
    }

    #[test]
    fn extend_skips_control_chars() {
        let mut cache = FallbackCache::default();
        let mut calls = 0;
        cache.extend("\n\t\r", |_, _| {
            calls += 1;
            true
        });
        assert_eq!(calls, 0, "control chars should be skipped entirely");
    }
}
