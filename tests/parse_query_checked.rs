//! Tests for `QueryParser::parse_query_checked` and `ParseError`.
//!
//! The checked-parse API is this fork's addition over upstream: Xapian
//! parser errors surface as `Err(ParseError)` instead of panicking.
//! The guaranteed-throw shapes below (dangling operator, invalid
//! range bounds) are parser errors in every Xapian 1.4.x line; note
//! that some odd-looking inputs (e.g. "a&&b") do NOT throw under
//! libxapian 1.4.29 and are therefore not asserted here.

use xapian_rs::{NumberRangeProcessor, ParseError, QueryParser, Stem, StemStrategy};

fn build_parser() -> QueryParser {
    let mut qp = QueryParser::default();
    qp.set_stemmer(Stem::for_language("en"));
    qp.set_stemming_strategy(StemStrategy::AllZ);
    qp.add_boolean_prefix::<&str, &str>("species", Some("XS"), None);
    qp.add_rangeprocessor("mean:", 0u32, NumberRangeProcessor, false, false, None);
    qp
}

#[test]
fn dangling_operator_returns_typed_error() {
    let mut qp = build_parser();
    match qp.parse_query_checked::<&str>("alcohol AND", None, None) {
        Err(ParseError { error_type, message }) => {
            assert_eq!(error_type, "QueryParserError");
            assert!(
                message.starts_with("Syntax:"),
                "unexpected message: {message}"
            );
        }
        other => panic!("expected Err, got {other:?}"),
    }
}

#[test]
fn invalid_range_bounds_return_typed_error() {
    let mut qp = build_parser();
    match qp.parse_query_checked::<&str>("mean:abc..def", None, None) {
        Err(ParseError { error_type, .. }) => {
            assert_eq!(error_type, "QueryParserError");
        }
        other => panic!("expected Err, got {other:?}"),
    }
}

#[test]
fn valid_queries_parse_ok() {
    let mut qp = build_parser();
    assert!(qp.parse_query_checked::<&str>("alcohol", None, None).is_ok());
    assert!(qp
        .parse_query_checked::<&str>("species:human", None, None)
        .is_ok());
    assert!(qp
        .parse_query_checked::<&str>("mean:10..20", None, None)
        .is_ok());
}

#[test]
fn parse_error_display_carries_type_and_message() {
    let mut qp = build_parser();
    let err = qp
        .parse_query_checked::<&str>("alcohol AND", None, None)
        .expect_err("dangling operator must error");
    let text = err.to_string();
    assert!(text.starts_with("QueryParserError:"), "got: {text}");
    assert!(text.len() > "QueryParserError:".len());
}

// Note: there is deliberately no test asserting that the unchecked
// `parse_query` panics on parser errors -- because it does not. The
// C++ exception unwinds into Rust frames without unwind tables, so
// std::terminate is called and the PROCESS ABORTS (verified: the
// should_panic variant of this test killed the whole test binary
// with SIGABRT). Callers must use `parse_query_checked` for any
// untrusted input.
