//! Embedded glossary parsing, lookup, suggestions, and plain-text rendering.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::terminal::{HelpColorMode, dim, table_heading};

const GLOSSARY_SOURCE: &str = include_str!("../docs/glossary.md");
const BASELINE_ENTRY_COUNT: usize = 113;

/// One parsed glossary entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlossaryEntry {
    pub key: String,
    pub term: String,
    pub class: String,
    pub aliases: Vec<String>,
    pub definition: String,
}

/// A contextual glossary-content error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlossaryError(String);

impl GlossaryError {
    fn at(line: usize, message: impl Into<String>) -> Self {
        Self(format!("glossary line {line}: {}", message.into()))
    }
}

impl fmt::Display for GlossaryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for GlossaryError {}

/// The deterministic result of a glossary lookup.
#[derive(Debug, Eq, PartialEq)]
pub enum LookupResult<'a> {
    Match(&'a GlossaryEntry),
    Ambiguous(Vec<&'a GlossaryEntry>),
    Miss(Vec<String>),
}

/// Parse and validate the glossary embedded in the binary.
pub fn embedded_entries() -> Result<Vec<GlossaryEntry>, GlossaryError> {
    let entries = parse_glossary(GLOSSARY_SOURCE)?;
    if entries.len() != BASELINE_ENTRY_COUNT {
        return Err(GlossaryError(format!(
            "glossary: expected {BASELINE_ENTRY_COUNT} baseline entries, found {}",
            entries.len()
        )));
    }
    Ok(entries)
}

/// Parse glossary Markdown without requiring its inherited checklist to be a manifest.
pub fn parse_glossary(source: &str) -> Result<Vec<GlossaryEntry>, GlossaryError> {
    let mut entries = Vec::new();
    let mut current: Option<(GlossaryEntry, usize, Vec<String>)> = None;
    let mut keys = HashSet::new();

    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        if raw_line.starts_with("###") {
            flush_entry(&mut current, &mut entries)?;
            let (term, key, class) = parse_heading(raw_line, line_number)?;
            if !keys.insert(key.clone()) {
                return Err(GlossaryError::at(
                    line_number,
                    format!("duplicate entry key {key:?}"),
                ));
            }
            current = Some((
                GlossaryEntry {
                    key,
                    term,
                    class,
                    aliases: Vec::new(),
                    definition: String::new(),
                },
                line_number,
                Vec::new(),
            ));
            continue;
        }

        let Some((entry, _, definition_lines)) = current.as_mut() else {
            continue;
        };
        let line = raw_line.trim();
        if let Some(value) = line.strip_prefix("- **Aliases:**") {
            entry.aliases = value
                .split(',')
                .map(str::trim)
                .filter(|alias| !alias.is_empty())
                .map(str::to_owned)
                .collect();
        } else if line.is_empty()
            || line == "---"
            || line.starts_with("## ")
            || line.starts_with("- **")
            || line.starts_with("**Prompt:**")
        {
            continue;
        } else {
            definition_lines.push(line.to_owned());
        }
    }
    flush_entry(&mut current, &mut entries)?;
    Ok(entries)
}

fn parse_heading(
    line: &str,
    line_number: usize,
) -> Result<(String, String, String), GlossaryError> {
    let malformed = || GlossaryError::at(line_number, "malformed entry heading");
    let body = line.strip_prefix("### ").ok_or_else(malformed)?;
    let (left, class) = body
        .strip_suffix(']')
        .and_then(|value| value.rsplit_once(" ["))
        .ok_or_else(malformed)?;
    let (term, key) = left
        .strip_suffix(')')
        .and_then(|value| value.rsplit_once(" (`"))
        .and_then(|(term, key)| key.strip_suffix('`').map(|key| (term, key)))
        .ok_or_else(malformed)?;
    let term = term.trim();
    let key = key.trim();
    let class = class.trim();
    if term.is_empty() || key.is_empty() || class.is_empty() {
        return Err(GlossaryError::at(
            line_number,
            "entry term, key, and class must be non-empty",
        ));
    }
    Ok((term.to_owned(), key.to_owned(), class.to_owned()))
}

fn flush_entry(
    current: &mut Option<(GlossaryEntry, usize, Vec<String>)>,
    entries: &mut Vec<GlossaryEntry>,
) -> Result<(), GlossaryError> {
    let Some((mut entry, line_number, definition_lines)) = current.take() else {
        return Ok(());
    };
    entry.definition = definition_lines.join(" ");
    if entry.definition.is_empty() {
        return Err(GlossaryError::at(
            line_number,
            format!("entry {:?} has an empty definition", entry.key),
        ));
    }
    entries.push(entry);
    Ok(())
}

/// Look up a non-empty query using skout's tier precedence.
pub fn lookup<'a>(entries: &'a [GlossaryEntry], query: &str) -> LookupResult<'a> {
    let query = query.trim().to_lowercase();
    for entry in entries {
        if entry.key.to_lowercase() == query {
            return LookupResult::Match(entry);
        }
    }
    for entry in entries {
        if entry.term.to_lowercase() == query {
            return LookupResult::Match(entry);
        }
    }
    for entry in entries {
        if entry
            .aliases
            .iter()
            .any(|alias| alias.to_lowercase() == query)
        {
            return LookupResult::Match(entry);
        }
    }
    let matches: Vec<_> = entries
        .iter()
        .filter(|entry| {
            entry.key.to_lowercase().contains(&query)
                || entry.term.to_lowercase().contains(&query)
                || entry
                    .aliases
                    .iter()
                    .any(|alias| alias.to_lowercase().contains(&query))
        })
        .collect();
    match matches.len() {
        0 => LookupResult::Miss(suggest_keys(entries, &query, 3)),
        1 => LookupResult::Match(matches[0]),
        _ => LookupResult::Ambiguous(matches),
    }
}

/// Return the closest glossary keys by Unicode-scalar edit distance.
pub fn suggest_keys(entries: &[GlossaryEntry], query: &str, limit: usize) -> Vec<String> {
    let query = query.to_lowercase();
    let mut candidates: Vec<_> = entries
        .iter()
        .map(|entry| {
            (
                levenshtein(&query, &entry.key.to_lowercase()),
                entry.key.clone(),
            )
        })
        .collect();
    candidates.sort();
    candidates
        .into_iter()
        .take(limit)
        .map(|(_, key)| key)
        .collect()
}

fn levenshtein(left: &str, right: &str) -> usize {
    let left: Vec<_> = left.chars().collect();
    let right: Vec<_> = right.chars().collect();
    let mut previous: Vec<_> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (left_index, left_char) in left.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right.iter().enumerate() {
            let substitution = usize::from(left_char != right_char);
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(previous[right_index] + substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

/// Render one entry without a trailing newline or blank line.
pub fn render_entry(entry: &GlossaryEntry) -> String {
    render_entry_with_mode(entry, HelpColorMode::Plain)
}

/// Render one entry using skout's glossary color roles.
pub fn render_entry_with_mode(entry: &GlossaryEntry, mode: HelpColorMode) -> String {
    let header = format!("{} ({}) [{}]", entry.term, entry.key, entry.class);
    let mut lines = vec![table_heading(&header, mode)];
    if !entry.aliases.is_empty() {
        lines.push(dim(&format!("Aliases: {}", entry.aliases.join(", ")), mode));
    }
    lines.push(entry.definition.clone());
    lines.join("\n")
}

/// Render the complete glossary in deterministic class and key order.
pub fn render_full(entries: &[GlossaryEntry]) -> String {
    render_full_with_mode(entries, HelpColorMode::Plain)
}

/// Render the complete glossary using skout's glossary color roles.
pub fn render_full_with_mode(entries: &[GlossaryEntry], mode: HelpColorMode) -> String {
    let class_rank: HashMap<_, _> = ["baseball", "fantasy", "skout", "stat"]
        .into_iter()
        .enumerate()
        .map(|(rank, class)| (class, rank))
        .collect();
    let mut ordered: Vec<_> = entries.iter().collect();
    ordered.sort_by(|left, right| {
        let left_rank = class_rank.get(left.class.as_str()).copied();
        let right_rank = class_rank.get(right.class.as_str()).copied();
        left_rank
            .is_none()
            .cmp(&right_rank.is_none())
            .then_with(|| left_rank.cmp(&right_rank))
            .then_with(|| left.class.cmp(&right.class))
            .then_with(|| left.key.cmp(&right.key))
    });

    let mut output = String::new();
    let mut previous_class: Option<&str> = None;
    for entry in ordered {
        if previous_class != Some(entry.class.as_str()) {
            if !output.is_empty() {
                output.push_str("\n\n");
            }
            output.push_str(&table_heading(&entry.class.to_uppercase(), mode));
            output.push_str("\n\n");
            previous_class = Some(&entry.class);
        } else {
            output.push_str("\n\n");
        }
        output.push_str(&render_entry_with_mode(entry, mode));
    }
    output
}

/// Resolve a one-based interactive choice from ambiguous lookup matches.
pub fn select_match<'a>(entries: &[&'a GlossaryEntry], choice: &str) -> Option<&'a GlossaryEntry> {
    choice
        .trim()
        .parse::<usize>()
        .ok()
        .and_then(|number| number.checked_sub(1))
        .and_then(|index| entries.get(index).copied())
}
