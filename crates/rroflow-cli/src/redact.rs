//! Secret redaction applied to every string before it leaves the machine.

use regex::Regex;

pub(crate) struct Redactor {
    pub(crate) patterns: Vec<Regex>,
    pub(crate) literals: Vec<String>,
}

impl Redactor {
    pub(crate) fn new() -> Self {
        let pats = [
            r"(?i)bearer\s+[A-Za-z0-9._~+/=-]{8,}",
            r"sk-[A-Za-z0-9_-]{16,}",
            r"xai-[A-Za-z0-9_-]{16,}",
            r"gh[pousr]_[A-Za-z0-9]{20,}",
            r"xox[abprs]-[A-Za-z0-9-]{10,}",
            r"AKIA[0-9A-Z]{16}",
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
            r"\b[a-f0-9]{48,}\b",
        ];
        let mut literals: Vec<String> = [
            "RROFLOW_INGEST_KEY",
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "XAI_API_KEY",
            "RAINDROP_WRITE_KEY",
            "RAINDROP_QUERY_KEY",
            "RAINDROP_QUERY_API_KEY",
        ]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .collect();
        if let Ok(extra) = std::env::var("RROFLOW_REDACT") {
            literals.extend(
                extra
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(String::from),
            );
        }
        literals.retain(|s| s.len() >= 8);
        Self {
            patterns: pats
                .iter()
                .map(|p| Regex::new(p).expect("valid regex"))
                .collect(),
            literals,
        }
    }
    /// Replace every known secret shape and literal with `[REDACTED]`.
    pub(crate) fn apply(&self, s: &str) -> String {
        let mut out = s.to_string();
        for l in &self.literals {
            out = out.replace(l.as_str(), "[REDACTED]");
        }
        for p in &self.patterns {
            out = p.replace_all(&out, "[REDACTED]").into_owned();
        }
        out
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
    r.literals.iter().any(|l| s.contains(l.as_str())) || r.patterns.iter().any(|p| p.is_match(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_known_secret_shapes() {
        let r = Redactor {
            patterns: Redactor::new().patterns,
            literals: vec!["25b1106c-d759-49c9".into()],
        };
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
