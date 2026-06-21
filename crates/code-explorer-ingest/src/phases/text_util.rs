//! Small UTF-8-safe string helpers shared across ingestion phases.

/// Largest index `<= i` that is a valid `char` boundary in `s` (clamped to `s.len()`).
///
/// Byte-offset slicing like `&s[..i]` panics when `i` lands inside a multi-byte
/// UTF-8 character. Callers that compute `i` arithmetically — a fixed cap
/// (`start + 2000`, `..500`) rather than a position returned by `find`/regex —
/// must snap it to a boundary first, or any non-ASCII content (em-dash, accents,
/// emoji) crashes the whole pipeline. `str::floor_char_boundary` is still unstable,
/// hence this local helper.
pub(crate) fn floor_char_boundary(s: &str, i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    let mut i = i;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floors_into_multibyte_char() {
        // em-dash '—' is 3 bytes occupying indices 2..5
        let s = "ab—cd";
        assert_eq!(floor_char_boundary(s, 3), 2); // inside the em-dash → snap back
        assert_eq!(floor_char_boundary(s, 4), 2); // inside the em-dash → snap back
        assert_eq!(floor_char_boundary(s, 2), 2); // already a boundary
        assert_eq!(floor_char_boundary(s, 5), 5); // boundary just after the em-dash
        assert_eq!(floor_char_boundary(s, 100), s.len()); // clamp past end
        // The result is always sliceable.
        for i in 0..=s.len() + 3 {
            let b = floor_char_boundary(s, i);
            let _ = &s[..b]; // must not panic
        }
    }
}
