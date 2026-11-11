//! Removes secrets from text before it leaves the machine.
//!
//! Raw session lines carry arbitrary tool output, so the rules cover the token formats providers
//! issue, credentials in URLs and headers, and `.env`-style or JSON fields with secret names.
//! Replacements never contain a quote or backslash, so a JSON line stays valid JSON.
//!
//! [`VERSION`] names this rule set. Change it whenever the rules change: an uploaded segment
//! records the version that scrubbed it, so older data can be found and scrubbed again.
//! `rroflow raw scrub-check` tests the rules against real sessions and planted secrets.

pub(crate) mod plants;

use regex::{Regex, RegexSet, RegexSetBuilder};
use std::collections::BTreeMap;

pub(crate) const VERSION: &str = "rroflow.scrub/1";

/// Environment variables whose values are secrets wherever they appear.
const SECRET_VARIABLES: [&str; 7] = [
    "RROFLOW_INGEST_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "XAI_API_KEY",
    "RAINDROP_WRITE_KEY",
    "RAINDROP_QUERY_KEY",
    "RAINDROP_QUERY_API_KEY",
];

struct Rule {
    kind: &'static str,
    pattern: &'static str,
    /// The capture group holding the secret. 0 is the whole match.
    group: usize,
    /// Whether the text this rule found is really a secret.
    keep: fn(&str) -> bool,
}

fn always(_: &str) -> bool {
    true
}

/// Source code, shell references and templates, not values.
fn code_like(value: &str) -> bool {
    value.contains(['(', ')', '$', '<', '>', '{', '}', '[', ']', '%', '&'])
        || value.starts_with('-')
        || value.ends_with(':')
}

fn placeholder(value: &str) -> bool {
    const WORDS: [&str; 14] = [
        "changeme",
        "change_me",
        "example",
        "password",
        "secret",
        "token",
        "none",
        "null",
        "true",
        "false",
        "undefined",
        "string",
        "redacted",
        "required",
    ];
    let lower = value.to_ascii_lowercase();
    WORDS.contains(&lower.as_str())
        || lower.starts_with("your")
        || value
            .chars()
            .all(|c| c == value.chars().next().unwrap_or('x'))
}

/// Could be an assigned secret: not a placeholder, code or a plain number.
fn assigned(value: &str) -> bool {
    !placeholder(value) && !code_like(value) && !value.bytes().all(|b| b.is_ascii_digit())
}

/// Looks generated, rather than an ordinary word or identifier.
fn opaque(value: &str) -> bool {
    assigned(value) && (value.bytes().any(|b| b.is_ascii_digit()) || value.len() >= 24)
}

/// Cookie values are often percent-encoded, so only clear signs of code rule one out.
fn cookie_value(value: &str) -> bool {
    !value.contains(['(', ')', '{', '}', '<', '>', '$'])
}

/// The next character position after `at`.
fn after(text: &str, at: usize) -> usize {
    at + text[at..].chars().next().map_or(1, char::len_utf8)
}

/// True if the character at `at` is the letter of a JSON escape such as `\f`.
fn starts_on_escape(text: &str, at: usize) -> bool {
    let slashes = text.as_bytes()[..at]
        .iter()
        .rev()
        .take_while(|b| **b == b'\\')
        .count();
    slashes % 2 == 1
}

const fn whole(kind: &'static str, pattern: &'static str) -> Rule {
    Rule {
        kind,
        pattern,
        group: 0,
        keep: always,
    }
}

const fn value(kind: &'static str, pattern: &'static str, keep: fn(&str) -> bool) -> Rule {
    Rule {
        kind,
        pattern,
        group: 1,
        keep,
    }
}

/// Specific formats first, so a secret is named for what it is. Quoted values stop at a quote or
/// backslash, which also ends a value inside JSON-escaped text.
const RULES: &[Rule] = &[
    whole(
        "private_key",
        r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----(?:[A-Za-z0-9+/=\s:,.-]|\\+[nrt])*?-----END [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----",
    ),
    // A key cut off by a tool's output limit, with no END line.
    whole(
        "private_key",
        r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----(?:[A-Za-z0-9+/=\s]|\\+[nr])*",
    ),
    whole(
        "rroflow_credential",
        r"\brr(?:ok|op|oi|na|nv|nc|nr|nd)_[A-Za-z0-9_-]{16,}",
    ),
    whole(
        "rroflow_credential",
        r"\brrsetup-[A-Z0-9]{5}(?:-[A-Z0-9]{5}){2,}",
    ),
    whole(
        "api_key",
        r"\bsk-(?:ant-|proj-|svcacct-|admin-)?[A-Za-z0-9_-]{16,}",
    ),
    whole("api_key", r"\bxai-[A-Za-z0-9_-]{16,}"),
    whole("stripe_key", r"\b[spr]k_(?:live|test)_[A-Za-z0-9]{10,}"),
    whole("stripe_key", r"\bwhsec_[A-Za-z0-9+/=]{16,}"),
    whole("github_token", r"\bgh[pousr]_[A-Za-z0-9]{20,}"),
    whole("github_token", r"\bgithub_pat_[A-Za-z0-9_]{20,}"),
    whole("gitlab_token", r"\bglpat-[A-Za-z0-9_-]{16,}"),
    whole("slack_token", r"\bxox[abeoprs]-[A-Za-z0-9-]{10,}"),
    whole("slack_token", r"\bxapp-[A-Za-z0-9-]{10,}"),
    whole(
        "slack_webhook",
        r"hooks\.slack\.com/services/[A-Za-z0-9/_-]{16,}",
    ),
    whole("aws_key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
    whole("google_key", r"\bAIza[0-9A-Za-z_-]{30,}"),
    whole("google_key", r"\bGOCSPX-[A-Za-z0-9_-]{16,}"),
    whole("google_key", r"\bya29\.[A-Za-z0-9._-]{20,}"),
    whole("package_token", r"\bnpm_[A-Za-z0-9]{30,}"),
    whole("package_token", r"\bpypi-[A-Za-z0-9_-]{40,}"),
    whole("huggingface_token", r"\bhf_[A-Za-z0-9]{30,}"),
    whole(
        "sendgrid_key",
        r"\bSG\.[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{16,}",
    ),
    whole(
        "jwt",
        // The signature is optional: a header and payload alone still expose the claims.
        r"\beyJ[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}(?:\.[A-Za-z0-9_-]{6,})?",
    ),
    value(
        "url_password",
        r#"\b[a-zA-Z][a-zA-Z0-9+.-]*://[^\s:/@"'\\]*:([^\s@/"'\\]+)@"#,
        always,
    ),
    value(
        "authorization",
        r#"(?i)authorization\\*"?\s*[:=]\s*\\*"?(?:bearer\s+|basic\s+|token\s+)?([^\s"'\\,;]{8,})"#,
        assigned,
    ),
    value(
        "authorization",
        r"(?i)\b(?:bearer|basic)\s+([A-Za-z0-9._~+/=-]{8,})",
        opaque,
    ),
    value(
        "cookie",
        // `name=value` pairs only, so ordinary text after the header is not swept in.
        r#"(?i)\b(?:set-)?cookie\\*"?\s*:\s*\\*"?([^\s;=,"\\]+=[^\s;,"\\]+(?:;\s*[^\s;=,"\\]+=[^\s;,"\\]*)*)"#,
        cookie_value,
    ),
    // A JSON field with a secret's name, at any depth of escaping.
    value(
        "secret_field",
        r#"(?i)\\*"(?:[a-z0-9_.-]*(?:secret|password|passwd|passphrase|api[_-]?key|access[_-]?key|private[_-]?key|auth[_-]?token|access[_-]?token|refresh[_-]?token|id[_-]?token|session[_-]?token|bearer[_-]?token|credential)[a-z0-9_.-]*|token)\\*"\s*:\s*\\*"([^"\\]{6,})\\*""#,
        assigned,
    ),
    // `.env` and shell assignments use upper-case names, a strong signal, so any value counts.
    value(
        "secret_assignment",
        r#"\b[A-Z0-9_]*(?:SECRET|PASSWORD|PASSWD|PASSPHRASE|TOKEN|API_?KEY|ACCESS_?KEY|PRIVATE_?KEY|CREDENTIALS?|DATABASE_URL|DSN)[A-Z0-9_]*=(?:\\*["'])?([^\s"'\\,;]{4,})"#,
        assigned,
    ),
    // Lower-case names are also common in source code, so the value must look generated.
    value(
        "secret_assignment",
        r#"(?i)\b[a-z0-9_.-]*(?:secret|password|passwd|passphrase|api[_-]?key|access[_-]?key|private[_-]?key|auth[_-]?token|access[_-]?token|refresh[_-]?token)[a-z0-9_.-]*\s*[=:]\s*(?:\\*["'])?([^\s"'\\,;)&]{6,})"#,
        opaque,
    ),
    value(
        "command_flag",
        r#"(?i)--(?:password|passwd|token|api-key|apikey|secret|access-key|auth-token)[= ]\s*(?:\\*["'])?([^\s"'\\]{4,})"#,
        assigned,
    ),
    // SHA-1 commits are 40 characters and stay. A longer run may be a key with no known prefix;
    // digests are lost with it, on purpose.
    whole("long_hex", r"\b[a-f0-9]{48,}\b"),
];

pub(crate) struct Scrubber {
    any: RegexSet,
    rules: Vec<(&'static Rule, Regex)>,
    literals: Vec<String>,
}

/// Scrubbed text, and how many secrets of each kind it held.
pub(crate) struct Scrubbed {
    pub(crate) text: String,
    pub(crate) found: BTreeMap<&'static str, u64>,
}

const LITERAL_KIND: &str = "configured_secret";

impl Scrubber {
    /// The built-in rules, plus the values of known secret variables and `RROFLOW_REDACT`.
    pub(crate) fn new() -> Self {
        let mut literals: Vec<String> = SECRET_VARIABLES
            .iter()
            .filter_map(|name| std::env::var(name).ok())
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
        Self::with_literals(literals)
    }

    pub(crate) fn with_literals(mut literals: Vec<String>) -> Self {
        literals.retain(|s| s.len() >= 8);
        Self {
            any: RegexSetBuilder::new(RULES.iter().map(|rule| rule.pattern))
                .size_limit(128 * 1024 * 1024)
                .build()
                .expect("valid scrub rules"),
            rules: RULES
                .iter()
                .map(|rule| (rule, Regex::new(rule.pattern).expect("valid scrub rule")))
                .collect(),
            literals,
        }
    }

    fn run(&self, text: &str, marker: impl Fn(&str) -> String) -> Scrubbed {
        let mut found = BTreeMap::new();
        let mut out = text.to_string();
        for literal in &self.literals {
            let hits = out.matches(literal.as_str()).count() as u64;
            if hits > 0 {
                out = out.replace(literal.as_str(), &marker(LITERAL_KIND));
                *found.entry(LITERAL_KIND).or_insert(0) += hits;
            }
        }
        // One pass finds which rules can apply, so most lines run none of them.
        let matched = self.any.matches(&out);
        for index in matched.iter() {
            let (rule, regex) = &self.rules[index];
            let (mut rewritten, mut last, mut hits, mut at) = (String::new(), 0, 0, 0);
            while at <= out.len() {
                let Some(captures) = regex.captures_at(&out, at) else {
                    break;
                };
                let whole = captures.get(0).expect("a match has a whole span");
                let next = after(&out, whole.start());
                let Some(secret) = captures.get(rule.group) else {
                    at = next;
                    continue;
                };
                if secret.as_str().contains("[REDACTED") || !(rule.keep)(secret.as_str()) {
                    // Not a secret, but a real one may begin inside what this match covered.
                    at = secret.start().max(next);
                    continue;
                }
                // Keep both halves of an escape, so the line stays valid JSON.
                let start = if starts_on_escape(&out, secret.start()) {
                    after(&out, secret.start())
                } else {
                    secret.start()
                };
                rewritten.push_str(&out[last..start]);
                rewritten.push_str(&marker(rule.kind));
                last = secret.end();
                hits += 1;
                at = whole.end().max(next);
            }
            if hits > 0 {
                rewritten.push_str(&out[last..]);
                out = rewritten;
                *found.entry(rule.kind).or_insert(0) += hits;
            }
        }
        Scrubbed { text: out, found }
    }

    /// Replace each secret with a marker naming its kind.
    pub(crate) fn scrub(&self, text: &str) -> Scrubbed {
        self.run(text, |kind| format!("[REDACTED:{kind}]"))
    }

    /// Replace each secret with a plain `[REDACTED]`.
    pub(crate) fn redact(&self, text: &str) -> String {
        self.run(text, |_| "[REDACTED]".to_string()).text
    }

    /// True when `text` holds something that would be replaced.
    pub(crate) fn finds(&self, text: &str) -> bool {
        !self.scrub(text).found.is_empty()
    }
}

#[cfg(test)]
mod tests;
