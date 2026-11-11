//! The leak test: `rroflow raw scrub-check`.
//!
//! Scrubs a copy of every stored line in memory, plants fresh fake secrets into real lines, and
//! checks that none survive and that JSON stays valid. It writes nothing, sends nothing, and
//! prints counts only, never session text.

use super::{Record, open_segment, segment_paths};
use crate::scrub::plants::{self, TEMPLATES};
use crate::scrub::{self, Scrubber};
use anyhow::{Result, anyhow};
use regex::{Regex, RegexSet};
use serde::de::IgnoredAny;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Broad, short shapes that scrubbed text should rarely still contain. Written separately from
/// the scrubber's rules so a mistake in one is not repeated in the other. Most hits are harmless,
/// such as a bare prefix in documentation, so these are counted for review and do not fail the
/// check. The length of what followed tells a bare prefix from something that needs a look.
const RESIDUE: &[(&str, &str)] = &[
    ("openai_or_anthropic", r"\bsk-[A-Za-z0-9_-]*"),
    ("xai", r"\bxai-[A-Za-z0-9_-]*"),
    ("github", r"\b(?:gh[pousr]_|github_pat_)[A-Za-z0-9_]*"),
    ("gitlab", r"\bglpat-[A-Za-z0-9_-]*"),
    ("slack", r"\bxox[a-z]-[A-Za-z0-9-]*"),
    ("aws", r"\b(?:AKIA|ASIA)[0-9A-Z]*"),
    ("google", r"\b(?:AIza|GOCSPX-|ya29\.)[0-9A-Za-z._-]*"),
    ("stripe", r"\b(?:[spr]k_(?:live|test)_|whsec_)[A-Za-z0-9]*"),
    ("package", r"\b(?:npm_|pypi-|hf_)[A-Za-z0-9_-]*"),
    (
        "rroflow",
        r"\b(?:rr(?:ok|op|oi|na|nv|nc|nr|nd)_|rrsetup-)[A-Za-z0-9_-]*",
    ),
    ("jwt", r"\beyJ[A-Za-z0-9_-]*\.[A-Za-z0-9_-]+"),
    ("private_key", r"BEGIN [A-Z0-9 ]*PRIVATE KEY"),
];

fn bucket(length: usize) -> &'static str {
    match length {
        0..16 => "under_16",
        16..32 => "16_to_31",
        _ => "32_or_more",
    }
}

fn string_leaves<'a>(value: &'a mut Value, out: &mut Vec<&'a mut String>) {
    match value {
        Value::String(text) => out.push(text),
        Value::Array(items) => items.iter_mut().for_each(|item| string_leaves(item, out)),
        Value::Object(fields) => fields
            .values_mut()
            .for_each(|item| string_leaves(item, out)),
        _ => {}
    }
}

/// Puts `planted` at the start, middle or end of one of the line's strings.
fn insert(value: &mut Value, planted: &str, turn: usize) -> bool {
    let mut leaves = Vec::new();
    string_leaves(value, &mut leaves);
    if leaves.is_empty() {
        return false;
    }
    let count = leaves.len();
    let leaf = &mut leaves[turn % count];
    let at = match turn % 3 {
        0 => 0,
        1 => {
            let mut middle = leaf.len() / 2;
            while !leaf.is_char_boundary(middle) {
                middle -= 1;
            }
            middle
        }
        _ => leaf.len(),
    };
    leaf.insert_str(at, &format!(" {planted} "));
    true
}

fn add<K: Ord>(into: &mut BTreeMap<K, u64>, from: BTreeMap<K, u64>) {
    for (key, count) in from {
        *into.entry(key).or_insert(0) += count;
    }
}

#[derive(Default)]
struct Tally {
    lines: u64,
    json_lines: u64,
    lines_changed: u64,
    binary_lines: u64,
    omitted_lines: u64,
    unreadable_records: u64,
    broke_json: u64,
    planted: u64,
    survived: u64,
    found: BTreeMap<&'static str, u64>,
    /// Which kinds of secret were found in lines that stopped being valid JSON.
    broke_json_with: BTreeMap<&'static str, u64>,
    planted_by_kind: BTreeMap<&'static str, u64>,
    survived_by_kind: BTreeMap<&'static str, u64>,
    residue: BTreeMap<&'static str, BTreeMap<&'static str, u64>>,
}

impl Tally {
    fn merge(&mut self, other: Tally) {
        self.lines += other.lines;
        self.json_lines += other.json_lines;
        self.lines_changed += other.lines_changed;
        self.binary_lines += other.binary_lines;
        self.omitted_lines += other.omitted_lines;
        self.unreadable_records += other.unreadable_records;
        self.broke_json += other.broke_json;
        self.planted += other.planted;
        self.survived += other.survived;
        add(&mut self.found, other.found);
        add(&mut self.broke_json_with, other.broke_json_with);
        add(&mut self.planted_by_kind, other.planted_by_kind);
        add(&mut self.survived_by_kind, other.survived_by_kind);
        for (shape, buckets) in other.residue {
            add(self.residue.entry(shape).or_default(), buckets);
        }
    }

    fn note_broken(&mut self, found: &BTreeMap<&'static str, u64>) {
        self.broke_json += 1;
        for kind in found.keys() {
            *self.broke_json_with.entry(kind).or_insert(0) += 1;
        }
    }
}

struct Check {
    scrubber: Scrubber,
    residue_any: RegexSet,
    residue: Vec<(&'static str, Regex)>,
    every: u64,
}

impl Check {
    fn line(&self, tally: &mut Tally, line: &str) {
        tally.lines += 1;
        let scrubbed = self.scrubber.scrub(line);
        if scrubbed.text != line {
            tally.lines_changed += 1;
        }
        for (kind, count) in &scrubbed.found {
            *tally.found.entry(kind).or_insert(0) += count;
        }
        for index in self.residue_any.matches(&scrubbed.text).iter() {
            let (shape, regex) = &self.residue[index];
            for hit in regex.find_iter(&scrubbed.text) {
                *tally
                    .residue
                    .entry(shape)
                    .or_default()
                    .entry(bucket(hit.len()))
                    .or_insert(0) += 1;
            }
        }
        let Ok(mut value) = serde_json::from_str::<Value>(line) else {
            return;
        };
        tally.json_lines += 1;
        if serde_json::from_str::<IgnoredAny>(&scrubbed.text).is_err() {
            tally.note_broken(&scrubbed.found);
        }
        if !tally.json_lines.is_multiple_of(self.every) {
            return;
        }
        let turn = tally.planted as usize;
        let planted = plants::plant(turn);
        if !insert(&mut value, &planted.text, turn) {
            return;
        }
        tally.planted += 1;
        *tally.planted_by_kind.entry(planted.kind).or_insert(0) += 1;
        let out = self.scrubber.scrub(&value.to_string());
        if plants::survives(&out.text, &planted.secret) {
            tally.survived += 1;
            *tally.survived_by_kind.entry(planted.kind).or_insert(0) += 1;
        }
        if serde_json::from_str::<IgnoredAny>(&out.text).is_err() {
            tally.note_broken(&out.found);
        }
    }

    fn segment(&self, tally: &mut Tally, path: &Path) -> Result<()> {
        let mut reader = open_segment(path)?;
        let mut bytes = Vec::new();
        loop {
            bytes.clear();
            if reader.read_until(b'\n', &mut bytes)? == 0 {
                return Ok(());
            }
            let Ok(record) = serde_json::from_slice::<Record>(&bytes) else {
                tally.unreadable_records += 1;
                continue;
            };
            match (&record.line, &record.line_hex) {
                (Some(line), _) => self.line(tally, line),
                (None, Some(_)) => tally.binary_lines += 1,
                (None, None) => tally.omitted_lines += 1,
            }
        }
    }

    /// Works through the segments on every core; each worker takes the next one unclaimed.
    fn segments(&self, paths: &[PathBuf]) -> Result<Tally> {
        let next = AtomicUsize::new(0);
        let workers = std::thread::available_parallelism().map_or(1, usize::from);
        let results: Vec<Result<Tally>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..workers)
                .map(|_| {
                    scope.spawn(|| {
                        let mut tally = Tally::default();
                        while let Some(path) = paths.get(next.fetch_add(1, Ordering::Relaxed)) {
                            self.segment(&mut tally, path)?;
                        }
                        Ok(tally)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle
                        .join()
                        .unwrap_or_else(|_| Err(anyhow!("a scrub check worker stopped")))
                })
                .collect()
        });
        let mut total = Tally::default();
        for result in results {
            total.merge(result?);
        }
        Ok(total)
    }
}

/// `every` plants one secret in every Nth JSON line; 1 plants one in each.
pub(super) fn run(store: &Path, every: u64) -> Result<Value> {
    let check = Check {
        scrubber: Scrubber::new(),
        residue_any: RegexSet::new(RESIDUE.iter().map(|(_, pattern)| *pattern))
            .expect("valid residue shapes"),
        residue: RESIDUE
            .iter()
            .map(|(shape, pattern)| (*shape, Regex::new(pattern).expect("valid residue shape")))
            .collect(),
        every: every.max(1),
    };
    let tally = check.segments(&segment_paths(&store.join("raw"))?)?;
    let every_kind_planted = tally.planted_by_kind.len() == TEMPLATES.len();
    Ok(json!({
        "ok": tally.survived == 0
            && tally.broke_json == 0
            && tally.unreadable_records == 0
            && every_kind_planted,
        "scrubber": scrub::VERSION,
        "lines": tally.lines,
        "json_lines": tally.json_lines,
        "lines_changed": tally.lines_changed,
        "binary_lines_not_scrubbed": tally.binary_lines,
        "omitted_lines": tally.omitted_lines,
        "unreadable_records": tally.unreadable_records,
        "found": tally.found,
        "broke_json": tally.broke_json,
        "broke_json_with": tally.broke_json_with,
        "planted": tally.planted,
        "planted_kinds": tally.planted_by_kind.len(),
        "kinds_available": TEMPLATES.len(),
        "survived": tally.survived,
        "survived_by_kind": tally.survived_by_kind,
        "residue_for_review": tally.residue,
    }))
}
