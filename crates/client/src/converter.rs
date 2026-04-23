//! Pure-logic word-list conversion.
//!
//! Takes the content of an external file (CSV / TSV / JSON / plain text),
//! parses it into a category → words map, and emits a string in the
//! `lists/`-compatible format understood by `main::load_words`.
//!
//! No I/O beyond whatever the caller hands us as `&str` content — every
//! function here is deterministic and unit-testable.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// Which parser to use. Detected from the file extension by
/// `detect_format`; the caller can still pass a different one if it wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
    Csv,
    Tsv,
    Json,
    Text,
}

/// Category → words. `BTreeMap` so the emitted output order is
/// deterministic and testable.
pub type ParsedList = BTreeMap<String, Vec<String>>;

/// Failure modes shared by every parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertError {
    EmptyFile,
    InvalidJson(String),
    JsonWrongShape,
    EmptyAfterParse,
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvertError::EmptyFile => write!(f, "The source file is empty."),
            ConvertError::InvalidJson(msg) => write!(f, "Invalid JSON: {}", msg),
            ConvertError::JsonWrongShape => write!(
                f,
                "JSON must be an object mapping category names to arrays of strings."
            ),
            ConvertError::EmptyAfterParse => {
                write!(f, "No words found in the source after parsing.")
            }
        }
    }
}

impl std::error::Error for ConvertError {}

/// Result of inspecting a CSV/TSV's header row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CsvHeaderAnalysis {
    /// Columns were resolved without user input — either because the file
    /// has a single column (word_col=0, category_col=None) or because one
    /// of the two column headers matched a word-like name.
    AutoResolved {
        word_col: usize,
        category_col: Option<usize>,
        headers: Vec<String>,
    },
    /// Column layout couldn't be auto-resolved — either two columns with
    /// unrecognized headers, or more than two columns. Caller must prompt
    /// the user to pick the word column and (optionally) the category
    /// column.
    NeedsPick { headers: Vec<String> },
}

/// Fallback category used when the source has no category information
/// (single-column CSV/TSV, plain text file, or a row with an empty
/// category cell).
const GENERAL: &str = "General";

const WORD_LIKE_HEADERS: &[&str] = &["word", "name", "entry", "term"];

/// Infer the format from a filename extension. Returns None for anything
/// we don't know how to parse — the caller turns that into a clean
/// user-facing error.
pub fn detect_format(path: &Path) -> Option<SourceFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "csv" => Some(SourceFormat::Csv),
        "tsv" => Some(SourceFormat::Tsv),
        "json" => Some(SourceFormat::Json),
        "txt" => Some(SourceFormat::Text),
        _ => None,
    }
}

/// Inspect the header row of a CSV/TSV to decide whether column
/// assignment needs a user prompt.
pub fn analyze_csv_headers(
    content: &str,
    delimiter: char,
) -> Result<CsvHeaderAnalysis, ConvertError> {
    let header_line = first_non_blank_line(content).ok_or(ConvertError::EmptyFile)?;
    let headers = split_row(header_line, delimiter);

    match headers.len() {
        0 => Err(ConvertError::EmptyFile),
        1 => Ok(CsvHeaderAnalysis::AutoResolved {
            word_col: 0,
            category_col: None,
            headers,
        }),
        2 => {
            let is_word_like = |h: &str| {
                let trimmed = h.trim().to_ascii_lowercase();
                WORD_LIKE_HEADERS.iter().any(|w| w == &trimmed)
            };
            if is_word_like(&headers[0]) {
                Ok(CsvHeaderAnalysis::AutoResolved {
                    word_col: 0,
                    category_col: Some(1),
                    headers,
                })
            } else if is_word_like(&headers[1]) {
                Ok(CsvHeaderAnalysis::AutoResolved {
                    word_col: 1,
                    category_col: Some(0),
                    headers,
                })
            } else {
                Ok(CsvHeaderAnalysis::NeedsPick { headers })
            }
        }
        _ => Ok(CsvHeaderAnalysis::NeedsPick { headers }),
    }
}

/// Parse a CSV/TSV using explicit column indices. `category_col = None`
/// puts everything under `General`.
pub fn parse_csv_with_cols(
    content: &str,
    delimiter: char,
    word_col: usize,
    category_col: Option<usize>,
) -> Result<ParsedList, ConvertError> {
    let mut lines = content.lines().filter(|l| !l.trim().is_empty());

    // Skip the header row.
    if lines.next().is_none() {
        return Err(ConvertError::EmptyFile);
    }

    let mut raw: Vec<(String, String)> = Vec::new();
    for line in lines {
        let fields = split_row(line, delimiter);
        let Some(word_raw) = fields.get(word_col) else {
            continue;
        };
        let word = word_raw.trim().to_string();
        if word.is_empty() {
            continue;
        }
        let category = match category_col.and_then(|c| fields.get(c)) {
            Some(c) => {
                let t = c.trim();
                if t.is_empty() {
                    GENERAL.to_string()
                } else {
                    t.to_string()
                }
            }
            None => GENERAL.to_string(),
        };
        raw.push((category, word));
    }

    finalize(raw)
}

/// Parse a strict JSON shape: `{ "Category": ["word1", "word2"], ... }`.
pub fn parse_json(content: &str) -> Result<ParsedList, ConvertError> {
    if content.trim().is_empty() {
        return Err(ConvertError::EmptyFile);
    }
    let value: serde_json::Value =
        serde_json::from_str(content).map_err(|e| ConvertError::InvalidJson(e.to_string()))?;

    let serde_json::Value::Object(map) = value else {
        return Err(ConvertError::JsonWrongShape);
    };

    let mut raw: Vec<(String, String)> = Vec::new();
    for (category, vals) in map {
        let serde_json::Value::Array(items) = vals else {
            return Err(ConvertError::JsonWrongShape);
        };
        let cat_name = {
            let trimmed = category.trim();
            if trimmed.is_empty() {
                GENERAL.to_string()
            } else {
                trimmed.to_string()
            }
        };
        for item in items {
            let serde_json::Value::String(word) = item else {
                return Err(ConvertError::JsonWrongShape);
            };
            let word = word.trim().to_string();
            if word.is_empty() {
                continue;
            }
            raw.push((cat_name.clone(), word));
        }
    }

    finalize(raw)
}

/// Parse a plain-text newline-separated file. Everything lands under
/// `General`.
pub fn parse_text(content: &str) -> Result<ParsedList, ConvertError> {
    if content.trim().is_empty() {
        return Err(ConvertError::EmptyFile);
    }
    let mut raw: Vec<(String, String)> = Vec::new();
    for line in content.lines() {
        let word = line.trim();
        if word.is_empty() {
            continue;
        }
        raw.push((GENERAL.to_string(), word.to_string()));
    }
    finalize(raw)
}

/// Emit a `lists/`-compatible string. Deterministic category order
/// (BTreeMap), blank line between categories, ends with one trailing
/// newline.
pub fn emit_list(parsed: &ParsedList) -> String {
    let mut out = String::new();
    let mut first = true;
    for (cat, words) in parsed {
        if !first {
            out.push('\n');
        }
        first = false;
        out.push('[');
        out.push_str(cat);
        out.push_str("]\n");
        for w in words {
            out.push_str(w);
            out.push('\n');
        }
    }
    out
}

// ─── Internal helpers ──────────────────────────────────────────────────

/// Walk the raw (category, word) pairs, dedup case-insensitively across
/// the whole file (first occurrence wins, matching `load_words`), and
/// collect into a BTreeMap. Returns `EmptyAfterParse` if nothing survives.
fn finalize(raw: Vec<(String, String)>) -> Result<ParsedList, ConvertError> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: ParsedList = BTreeMap::new();
    for (category, word) in raw {
        let key = word.to_lowercase();
        if !seen.insert(key) {
            continue;
        }
        out.entry(category).or_default().push(word);
    }
    if out.is_empty() {
        return Err(ConvertError::EmptyAfterParse);
    }
    Ok(out)
}

fn first_non_blank_line(content: &str) -> Option<&str> {
    content
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .find(|l| !l.trim().is_empty())
}

/// Split a single row by `delimiter`. Fields wrapped in a matched pair of
/// double quotes have the quotes stripped. Embedded escaped quotes are
/// *not* handled — users with complex CSVs should export as JSON.
fn split_row(line: &str, delimiter: char) -> Vec<String> {
    let line = line.trim_end_matches('\r');
    line.split(delimiter)
        .map(|f| {
            let f = f.trim();
            if f.len() >= 2 && f.starts_with('"') && f.ends_with('"') {
                f[1..f.len() - 1].to_string()
            } else {
                f.to_string()
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "converter_tests.rs"]
mod tests;
