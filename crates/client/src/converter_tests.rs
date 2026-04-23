//! Unit tests for `converter`. Loaded via `#[path]` from
//! `converter.rs` so the tests live in their own file without forcing
//! the module into a directory layout.

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
