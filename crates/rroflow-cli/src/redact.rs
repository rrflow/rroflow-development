//! Secret redaction applied to every string before it leaves the machine. The rules live in
//! [`crate::scrub`].

use crate::scrub::Scrubber;

pub(crate) struct Redactor {
    scrubber: Scrubber,
}

impl Redactor {
    pub(crate) fn new() -> Self {
        Self {
            scrubber: Scrubber::new(),
        }
    }
    /// The built-in rules and these values, ignoring the environment.
    #[cfg(test)]
    pub(crate) fn with_literals(literals: Vec<String>) -> Self {
        Self {
            scrubber: Scrubber::with_literals(literals),
        }
    }
    /// Replace every known secret shape and literal with `[REDACTED]`.
    pub(crate) fn apply(&self, s: &str) -> String {
        self.scrubber.redact(s)
    }
}

/// Cut `s` to `max` characters and mark the cut.
pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    format!("{cut}...[truncated by rroflow]")
}

/// True when `s` still contains something the redactor would replace.
pub(crate) fn leaks(r: &Redactor, s: &str) -> bool {
    r.scrubber.finds(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_known_secret_shapes() {
        let r = Redactor::with_literals(vec!["25b1106c-d759-49c9".into()]);
        let s = r.apply(
            "Authorization: Bearer abcdefghijklmnop key=25b1106c-d759-49c9 sk-ABCDEFGHIJKLMNOPQRST",
        );
        assert!(
            !s.contains("abcdefghijklmnop") && !s.contains("25b1106c") && !s.contains("sk-ABC"),
            "{s}"
        );
    }
    #[test]
    fn leaks_detects_unredacted_secrets() {
        let r = Redactor::new();
        assert!(leaks(&r, "token sk-ABCDEFGHIJKLMNOPQRST here"));
        assert!(!leaks(&r, &r.apply("token sk-ABCDEFGHIJKLMNOPQRST here")));
    }
}
