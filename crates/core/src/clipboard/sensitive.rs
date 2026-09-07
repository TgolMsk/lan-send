//! Heuristic for secret-looking text that must not enter the history
//! (ADR-0011): a single token of 20–128 characters with high entropy that
//! is not a URL, a path or an e-mail address.

const MIN_LEN: usize = 20;
const MAX_LEN: usize = 128;
const MIN_ENTROPY_BITS: f64 = 3.5;

/// Whether `text` looks like a password, token or key.
pub fn looks_sensitive(text: &str) -> bool {
    let text = text.trim();
    let length = text.chars().count();
    if !(MIN_LEN..=MAX_LEN).contains(&length) {
        return false;
    }
    if text.chars().any(char::is_whitespace) {
        return false;
    }
    if text.contains("://") || text.starts_with('/') || text.contains('\\') || text.contains('@') {
        return false;
    }
    shannon_entropy(text) >= MIN_ENTROPY_BITS
}

/// Bits per character.
fn shannon_entropy(text: &str) -> f64 {
    let mut counts = std::collections::HashMap::new();
    let mut total = 0f64;
    for c in text.chars() {
        *counts.entry(c).or_insert(0f64) += 1.0;
        total += 1.0;
    }
    counts
        .values()
        .map(|count| {
            let p = count / total;
            -p * p.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_keys_but_not_prose_urls_or_paths() {
        assert!(looks_sensitive("sk-live-9fA3kQz8LmP2xR7vT1wY5bN0cH4dJ6e"));
        assert!(looks_sensitive("AKIAIOSFODNN7EXAMPLEXYZ123"));
        assert!(!looks_sensitive(
            "the quick brown fox jumps over the lazy dog"
        ));
        assert!(!looks_sensitive(
            "https://example.com/some/long/path?x=1&y=2"
        ));
        assert!(!looks_sensitive("/Users/someone/Documents/report.pdf"));
        assert!(!looks_sensitive("someone.longname@example-company.com"));
        assert!(!looks_sensitive("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(!looks_sensitive("short"));
    }
}
