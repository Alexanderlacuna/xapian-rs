//! Tests for processor declines in the FieldProcessor and
//! RangeProcessor bridges.
//!
//! A processor that cannot produce a query must map to Xapian's
//! MatchNothing -- never to an OP_INVALID query, which the parser
//! accepts but Enquire rejects with an uncatchable C++ exception
//! that aborts the whole process (SIGABRT). Verified end-to-end
//! against an in-memory index: a declined field matches nothing,
//! and per Xapian's boolean semantics MatchNothing OR q == q.

use bytes::Bytes;
use xapian_rs::{
    Document, Enquire, FieldProcessor, Query, QueryParser, RangeProcessor, TermGenerator,
    ToValue, WritableDatabase,
};

/// Field processor that accepts numeric input and declines the rest
/// (the shape of gn4's position processor: None when the value does
/// not parse).
#[derive(Clone)]
struct NumericFieldProcessor;

impl FieldProcessor for NumericFieldProcessor {
    fn process(&self, term: &str) -> Option<Query> {
        term.parse::<f64>()
            .ok()
            .map(|v| Query::value_range(0, v.serialize(), v.serialize()))
    }
}

/// Range processor that declines every range.
struct DecliningRangeProcessor;

impl RangeProcessor for DecliningRangeProcessor {
    fn process_range(&self, _start: &str, _end: &str) -> (Option<Bytes>, Option<Bytes>) {
        (None, None)
    }
}

/// In-memory index with one document: the term "alcohol" and the
/// slot-0 value 5.0.
fn one_doc_db() -> WritableDatabase {
    let mut db = WritableDatabase::inmemory();
    let mut doc = Document::default();
    let mut indexer = TermGenerator::default();
    indexer.set_document(&doc);
    indexer.index_text::<&str>("alcohol", None, None);
    doc.set_value(0, 5.0);
    let idterm = "Q:t1";
    doc.add_boolean_term(idterm);
    db.replace_document_by_term(idterm, &doc);
    db
}

fn hits(db: &WritableDatabase, query: &Query) -> u32 {
    let mut enq = Enquire::new(db);
    enq.set_query(query, None);
    enq.mset(0, 10, 10, None).size()
}

#[test]
fn declined_field_processor_matches_nothing() {
    let db = one_doc_db();
    let mut qp = QueryParser::default();
    qp.add_custom_boolean_prefix::<_, &str>("pos", NumericFieldProcessor, None);
    let query = qp
        .parse_query_checked::<&str>("pos:abc", None, None)
        .expect("declined field must parse, not abort");
    assert_eq!(hits(&db, &query), 0);
}

#[test]
fn match_nothing_or_query_is_query() {
    // Xapian documents MatchNothing as boolean false:
    // MatchNothing | q == q.
    let db = one_doc_db();
    let mut qp = QueryParser::default();
    qp.add_custom_boolean_prefix::<_, &str>("pos", NumericFieldProcessor, None);
    let query = qp
        .parse_query_checked::<&str>("pos:abc OR alcohol", None, None)
        .expect("combined query must parse");
    assert_eq!(hits(&db, &query), 1);
}

#[test]
fn accepted_field_processor_still_matches() {
    // Control: the happy path through the same bridge is intact.
    let db = one_doc_db();
    let mut qp = QueryParser::default();
    qp.add_custom_boolean_prefix::<_, &str>("pos", NumericFieldProcessor, None);
    let query = qp
        .parse_query_checked::<&str>("pos:5", None, None)
        .expect("numeric field must parse");
    assert_eq!(hits(&db, &query), 1);
}

#[test]
fn declined_range_processor_matches_nothing() {
    // Xapian's documented protocol for an unhandled range is a
    // default-constructed query (MatchNothing), never OP_INVALID.
    let db = one_doc_db();
    let mut qp = QueryParser::default();
    qp.add_rangeprocessor("val:", 0u32, DecliningRangeProcessor, false, false, None);
    let query = qp
        .parse_query_checked::<&str>("val:1..2", None, None)
        .expect("declined range must parse, not abort");
    assert_eq!(hits(&db, &query), 0);
}
