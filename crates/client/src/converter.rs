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

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn detect_format_by_extension() {
        assert_eq!(detect_format(&p("foo.csv")), Some(SourceFormat::Csv));
        assert_eq!(detect_format(&p("foo.tsv")), Some(SourceFormat::Tsv));
        assert_eq!(detect_format(&p("foo.json")), Some(SourceFormat::Json));
        assert_eq!(detect_format(&p("foo.txt")), Some(SourceFormat::Text));
    }

    #[test]
    fn detect_format_is_case_insensitive() {
        assert_eq!(detect_format(&p("FOO.CSV")), Some(SourceFormat::Csv));
        assert_eq!(detect_format(&p("foo.Json")), Some(SourceFormat::Json));
        assert_eq!(detect_format(&p("foo.TXT")), Some(SourceFormat::Text));
    }

    #[test]
    fn detect_format_unknown_returns_none() {
        assert_eq!(detect_format(&p("foo.xlsx")), None);
        assert_eq!(detect_format(&p("foo")), None);
        assert_eq!(detect_format(&p("foo.md")), None);
    }

    // ─── parse_text ────────────────────────────────────────────────

    #[test]
    fn parse_text_trims_and_drops_blanks() {
        let input = "  Jon Snow  \n\nTyrion\n   \nDaenerys\n";
        let got = parse_text(input).unwrap();
        assert_eq!(got.len(), 1);
        let words = &got["General"];
        assert_eq!(words, &vec!["Jon Snow", "Tyrion", "Daenerys"]);
    }

    #[test]
    fn parse_text_dedups_case_insensitively() {
        let input = "Jon\njon\nJON\nTyrion\n";
        let got = parse_text(input).unwrap();
        assert_eq!(got["General"], vec!["Jon", "Tyrion"]);
    }

    #[test]
    fn parse_text_empty_file() {
        assert_eq!(parse_text(""), Err(ConvertError::EmptyFile));
        assert_eq!(parse_text("   \n\n  \n"), Err(ConvertError::EmptyFile));
    }

    #[test]
    fn parse_text_wraps_everything_under_general() {
        let got = parse_text("a\nb\nc\n").unwrap();
        assert_eq!(got.keys().collect::<Vec<_>>(), vec!["General"]);
    }

    // ─── parse_json ────────────────────────────────────────────────

    #[test]
    fn parse_json_valid_map_parses() {
        let input = r#"{"House Stark":["Jon","Arya"],"Dragons":["Drogon"]}"#;
        let got = parse_json(input).unwrap();
        assert_eq!(got["House Stark"], vec!["Jon", "Arya"]);
        assert_eq!(got["Dragons"], vec!["Drogon"]);
    }

    #[test]
    fn parse_json_top_level_array_is_wrong_shape() {
        let input = r#"["Jon","Arya"]"#;
        assert_eq!(parse_json(input), Err(ConvertError::JsonWrongShape));
    }

    #[test]
    fn parse_json_non_string_array_items_is_wrong_shape() {
        let input = r#"{"Cat":[1,2,3]}"#;
        assert_eq!(parse_json(input), Err(ConvertError::JsonWrongShape));
    }

    #[test]
    fn parse_json_non_array_value_is_wrong_shape() {
        let input = r#"{"Cat":"not-an-array"}"#;
        assert_eq!(parse_json(input), Err(ConvertError::JsonWrongShape));
    }

    #[test]
    fn parse_json_malformed_returns_invalid_json() {
        let err = parse_json("{not json").unwrap_err();
        assert!(matches!(err, ConvertError::InvalidJson(_)));
    }

    #[test]
    fn parse_json_empty_file() {
        assert_eq!(parse_json(""), Err(ConvertError::EmptyFile));
        assert_eq!(parse_json("   "), Err(ConvertError::EmptyFile));
    }

    #[test]
    fn parse_json_dedups_case_insensitively() {
        let input = r#"{"A":["Jon","jon"],"B":["JON"]}"#;
        let got = parse_json(input).unwrap();
        // First-seen wins; "Jon" lands in A, duplicates drop.
        assert_eq!(got["A"], vec!["Jon"]);
        assert!(!got.contains_key("B"));
    }

    // ─── analyze_csv_headers ───────────────────────────────────────

    #[test]
    fn analyze_csv_single_column() {
        let input = "name\nJon\nArya\n";
        match analyze_csv_headers(input, ',').unwrap() {
            CsvHeaderAnalysis::AutoResolved {
                word_col,
                category_col,
                headers,
            } => {
                assert_eq!(word_col, 0);
                assert_eq!(category_col, None);
                assert_eq!(headers, vec!["name"]);
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn analyze_csv_two_col_word_then_category() {
        let input = "Word,Category\nJon,Stark\n";
        match analyze_csv_headers(input, ',').unwrap() {
            CsvHeaderAnalysis::AutoResolved {
                word_col,
                category_col,
                ..
            } => {
                assert_eq!(word_col, 0);
                assert_eq!(category_col, Some(1));
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn analyze_csv_two_col_category_then_name() {
        let input = "Category,Name\nStark,Jon\n";
        match analyze_csv_headers(input, ',').unwrap() {
            CsvHeaderAnalysis::AutoResolved {
                word_col,
                category_col,
                ..
            } => {
                assert_eq!(word_col, 1);
                assert_eq!(category_col, Some(0));
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn analyze_csv_two_col_ambiguous_needs_pick() {
        let input = "Foo,Bar\na,b\n";
        match analyze_csv_headers(input, ',').unwrap() {
            CsvHeaderAnalysis::NeedsPick { headers } => {
                assert_eq!(headers, vec!["Foo", "Bar"]);
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn analyze_csv_word_like_match_is_case_insensitive() {
        let input = "ENTRY,Tag\nJon,hero\n";
        match analyze_csv_headers(input, ',').unwrap() {
            CsvHeaderAnalysis::AutoResolved { word_col, .. } => {
                assert_eq!(word_col, 0);
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn analyze_csv_more_than_two_cols_needs_pick() {
        let input = "a,b,c\n1,2,3\n";
        match analyze_csv_headers(input, ',').unwrap() {
            CsvHeaderAnalysis::NeedsPick { headers } => {
                assert_eq!(headers, vec!["a", "b", "c"]);
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn analyze_csv_many_cols_preserves_all_headers() {
        let input = "name,category,notes,source,extra\nJon,Stark,...,...,...\n";
        match analyze_csv_headers(input, ',').unwrap() {
            CsvHeaderAnalysis::NeedsPick { headers } => {
                assert_eq!(headers.len(), 5);
                assert_eq!(headers[0], "name");
                assert_eq!(headers[4], "extra");
            }
            other => panic!("unexpected: {:?}", other),
        }
    }

    #[test]
    fn parse_csv_explicit_cols_in_many_col_file() {
        // 4 cols, user picked word=1 (category), category=0 (name) — confirms
        // the parser honours explicit indices even past column 2.
        let input = "idx,name,desc,source\n1,Jon,hero,test\n2,Arya,girl,test\n";
        let got = parse_csv_with_cols(input, ',', 1, Some(3)).unwrap();
        assert_eq!(got["test"], vec!["Jon", "Arya"]);
    }

    #[test]
    fn parse_csv_many_col_file_with_none_category() {
        let input = "idx,name,desc,source\n1,Jon,hero,test\n2,Arya,girl,test\n";
        let got = parse_csv_with_cols(input, ',', 1, None).unwrap();
        assert_eq!(got["General"], vec!["Jon", "Arya"]);
    }

    #[test]
    fn analyze_csv_empty_file() {
        assert_eq!(analyze_csv_headers("", ','), Err(ConvertError::EmptyFile));
        assert_eq!(
            analyze_csv_headers("\n\n\n", ','),
            Err(ConvertError::EmptyFile)
        );
    }

    // ─── parse_csv_with_cols ───────────────────────────────────────

    #[test]
    fn parse_csv_word_then_category() {
        let input = "word,category\nJon,Stark\nDrogon,Dragons\nArya,Stark\n";
        let got = parse_csv_with_cols(input, ',', 0, Some(1)).unwrap();
        assert_eq!(got["Stark"], vec!["Jon", "Arya"]);
        assert_eq!(got["Dragons"], vec!["Drogon"]);
    }

    #[test]
    fn parse_csv_category_then_word() {
        let input = "category,name\nStark,Jon\nDragons,Drogon\n";
        let got = parse_csv_with_cols(input, ',', 1, Some(0)).unwrap();
        assert_eq!(got["Stark"], vec!["Jon"]);
        assert_eq!(got["Dragons"], vec!["Drogon"]);
    }

    #[test]
    fn parse_tsv_works() {
        let input = "word\tcategory\nJon\tStark\nDrogon\tDragons\n";
        let got = parse_csv_with_cols(input, '\t', 0, Some(1)).unwrap();
        assert_eq!(got["Stark"], vec!["Jon"]);
        assert_eq!(got["Dragons"], vec!["Drogon"]);
    }

    #[test]
    fn parse_csv_strips_matched_quotes() {
        let input = "word,category\n\"Jon Snow\",\"House Stark\"\n";
        let got = parse_csv_with_cols(input, ',', 0, Some(1)).unwrap();
        assert_eq!(got["House Stark"], vec!["Jon Snow"]);
    }

    #[test]
    fn parse_csv_missing_category_cell_falls_back_to_general() {
        let input = "word,category\nJon,\n";
        let got = parse_csv_with_cols(input, ',', 0, Some(1)).unwrap();
        assert_eq!(got["General"], vec!["Jon"]);
    }

    #[test]
    fn parse_csv_single_column_lands_under_general() {
        let input = "word\nJon\nArya\n";
        let got = parse_csv_with_cols(input, ',', 0, None).unwrap();
        assert_eq!(got.keys().collect::<Vec<_>>(), vec!["General"]);
        assert_eq!(got["General"], vec!["Jon", "Arya"]);
    }

    #[test]
    fn parse_csv_empty_after_parse() {
        let input = "word,category\n  ,  \n";
        assert_eq!(
            parse_csv_with_cols(input, ',', 0, Some(1)),
            Err(ConvertError::EmptyAfterParse)
        );
    }

    #[test]
    fn parse_csv_dedups_globally() {
        let input = "word,category\nJon,A\njon,B\n";
        let got = parse_csv_with_cols(input, ',', 0, Some(1)).unwrap();
        // First-seen wins; only "Jon" in category A survives.
        assert_eq!(got["A"], vec!["Jon"]);
        assert!(!got.contains_key("B"));
    }

    // ─── emit_list ─────────────────────────────────────────────────

    #[test]
    fn emit_list_deterministic_category_order() {
        let mut parsed: ParsedList = BTreeMap::new();
        parsed.insert("Zeta".to_string(), vec!["z1".to_string()]);
        parsed.insert(
            "Alpha".to_string(),
            vec!["a1".to_string(), "a2".to_string()],
        );
        let out = emit_list(&parsed);
        // BTreeMap iterates alphabetically.
        assert_eq!(out, "[Alpha]\na1\na2\n\n[Zeta]\nz1\n");
    }

    #[test]
    fn emit_list_blank_line_between_categories_no_trailing_blank() {
        let mut parsed: ParsedList = BTreeMap::new();
        parsed.insert("A".to_string(), vec!["x".to_string()]);
        parsed.insert("B".to_string(), vec!["y".to_string()]);
        let out = emit_list(&parsed);
        assert_eq!(out, "[A]\nx\n\n[B]\ny\n");
    }

    #[test]
    fn emit_list_single_category() {
        let mut parsed: ParsedList = BTreeMap::new();
        parsed.insert(
            "General".to_string(),
            vec!["a".to_string(), "b".to_string()],
        );
        let out = emit_list(&parsed);
        assert_eq!(out, "[General]\na\nb\n");
    }

    // ─── No-separator → General fallback (txt + single-col csv) ───

    #[test]
    fn text_file_all_under_general() {
        let got = parse_text("one\ntwo\nthree\n").unwrap();
        assert_eq!(got.keys().collect::<Vec<_>>(), vec!["General"]);
    }

    #[test]
    fn single_column_csv_all_under_general() {
        let got = parse_csv_with_cols("word\none\ntwo\n", ',', 0, None).unwrap();
        assert_eq!(got.keys().collect::<Vec<_>>(), vec!["General"]);
    }
}
