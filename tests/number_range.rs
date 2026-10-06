//! NumberRangeProcessor bound precision.
//!
//! Bounds parsed as f32 lose exactness above 2^24 (f32 has a 24-bit
//! mantissa); gn3's C++ range processor uses double and preserves the
//! query text exactly. The fixtures below discriminate: a slot value
//! of 16777216.5 lies inside the exact f64 bounds but outside the
//! f32-rounded ones, so the match count flips with the fix.

use xapian_rs::{
    Document, Enquire, NumberRangeProcessor, Query, QueryParser, ToValue, WritableDatabase,
};

fn db_with_value(value: f64) -> WritableDatabase {
    let mut db = WritableDatabase::inmemory();
    let mut doc = Document::default();
    doc.set_value(0, value);
    let idterm = "Q:v";
    doc.add_boolean_term(idterm);
    db.replace_document_by_term(idterm, &doc);
    db
}

fn mean_parser() -> QueryParser {
    let mut qp = QueryParser::default();
    qp.add_rangeprocessor("mean:", 0u32, NumberRangeProcessor, false, false, None);
    qp
}

fn hits(db: &WritableDatabase, query: &Query) -> u32 {
    let mut enq = Enquire::new(db);
    enq.set_query(query, None);
    enq.mset(0, 10, 10, None).size()
}

#[test]
fn large_integer_lower_bound_stays_exact() {
    // 16777217 rounds to 16777216 as f32; the document's 16777216.5
    // must NOT satisfy a lower bound of 16777217.
    let db = db_with_value(16777216.5);
    let mut qp = mean_parser();
    let query = qp
        .parse_query_checked::<&str>("mean:16777217..", None, None)
        .expect("open-ended range must parse");
    assert_eq!(hits(&db, &query), 0);
}

#[test]
fn fractional_upper_bound_stays_exact() {
    // 16777216.75 rounds to 16777216.0 as f32; the document's
    // 16777216.5 must satisfy this upper bound.
    let db = db_with_value(16777216.5);
    let mut qp = mean_parser();
    let query = qp
        .parse_query_checked::<&str>("mean:..16777216.75", None, None)
        .expect("open-ended range must parse");
    assert_eq!(hits(&db, &query), 1);
}

#[test]
fn fractional_bound_round_trips() {
    // Round-3 measurement: 123456789.25 shifted by +2.75 as f32.
    let db = db_with_value(123456789.25);
    let mut qp = mean_parser();
    let query = qp
        .parse_query_checked::<&str>("mean:123456789.25..", None, None)
        .expect("range must parse");
    assert_eq!(hits(&db, &query), 1);
}

#[test]
fn small_bounds_still_match() {
    // Control: typical mean/year magnitudes are unaffected.
    let db = db_with_value(5.0);
    let mut qp = mean_parser();
    let query = qp
        .parse_query_checked::<&str>("mean:5..5", None, None)
        .expect("closed range must parse");
    assert_eq!(hits(&db, &query), 1);
}
