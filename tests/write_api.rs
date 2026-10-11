//! Unit tests for the write-API additions to the fork:
//!
//!   * `TermGenerator::index_text_without_positions`
//!   * `SimpleStopper` (concrete Xapian stopper, `impl Stopper`)
//!   * `Document::clear_values`
//!   * `WritableDatabase::{replace_document_by_term_checked,
//!     delete_document_by_term_checked, replace_document_checked,
//!     delete_document_checked}` and the `DbError` type
//!
//! The heavier negative controls (a, b, c from the lane) live in
//! `examples/write_controls.rs`, which can be run in RED sabotage modes.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use xapian_rs::{
    Database, DbAction, DbBackend, DbError, Document, SimpleStopper, Stem, StopStrategy, Stopper,
    TermGenerator, ToValue, WritableDatabase,
};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn fresh_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "xapian-rs-writeapi-{}-{}-{}",
        std::process::id(),
        tag,
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn open_writable(dir: &PathBuf) -> WritableDatabase {
    WritableDatabase::open(
        dir,
        Some(DbAction::CreateOrOverwrite),
        Some(DbBackend::Glass),
        None,
        None,
    )
}

fn doc_with(idterm: &str) -> Document {
    let mut doc = Document::default();
    doc.set_data(format!("data for {idterm}"));
    doc.add_boolean_term(idterm);
    doc
}

fn termgen() -> TermGenerator {
    let mut termgen = TermGenerator::default();
    termgen.set_stemmer(Stem::for_language("en"));
    termgen.set_stopper_strategy(StopStrategy::All);
    termgen.set_stopper(SimpleStopper::new());
    termgen
}

fn terms_of(doc: &Document) -> Vec<(String, u32, usize)> {
    let mut out: Vec<(String, u32, usize)> = doc
        .terms()
        .map(|term| {
            let name: &str = term.as_ref();
            (name.to_string(), term.wdf(), term.positions_vec().len())
        })
        .collect();
    out.sort();
    out
}

#[test]
fn index_text_without_positions_adds_terms_without_positions() {
    let doc = Document::default();
    {
        let mut termgen = termgen();
        termgen.set_document(&doc);
        termgen.index_text_without_positions::<&str>("hello world", None, None);
        termgen.index_text_without_positions::<&str>("mouse", Some(0u32), Some("XS"));
    }
    let terms = terms_of(&doc);
    let find = |name: &str| terms.iter().find(|(n, _, _)| n == name).cloned();
    // Unstemmed terms are present with wdf 1 and NO positions.
    assert_eq!(find("hello"), Some(("hello".into(), 1, 0)));
    assert_eq!(find("world"), Some(("world".into(), 1, 0)));
    // Stemmed terms are present too.
    assert!(find("Zhello").is_some());
    // Prefixed with wdf increment 0.
    assert_eq!(find("XSmouse"), Some(("XSmouse".into(), 0, 0)));
}

#[test]
fn simple_stopper_is_empty_and_usable_with_set_stopper() {
    let stopper = SimpleStopper::new();
    // An empty SimpleStopper is not a stopword for anything.
    assert!(!stopper.is_stopword("the"));
    assert!(!stopper.is_stopword("and"));

    // STOP_ALL with the (empty) SimpleStopper stops nothing: "the"/"and" are
    // still indexed verbatim, matching the reference indexer.
    let doc = Document::default();
    {
        let mut termgen = termgen();
        termgen.set_document(&doc);
        termgen.index_text::<&str>("the and mice", None, None);
    }
    let terms = terms_of(&doc);
    assert!(terms.iter().any(|(n, _, _)| n == "the"));
    assert!(terms.iter().any(|(n, _, _)| n == "and"));
    assert!(terms.iter().any(|(n, _, _)| n == "Zthe"));
}

#[test]
fn clear_values_removes_all_slots() {
    let mut doc = Document::default();
    doc.set_value(0u32, 1.5f64);
    doc.set_value(3u32, "text".to_string());
    assert!(doc.value::<f64>(0u32).is_some());
    assert!(doc.value::<String>(3u32).is_some());
    doc.clear_values();
    for slot in 0u32..=8 {
        assert!(doc.value::<bytes::Bytes>(slot).is_none(), "slot {slot}");
    }
    // Terms are unaffected by clearing values.
    assert_eq!(doc.terms().count(), 0);
    doc.add_boolean_term("T1");
    doc.clear_values();
    assert_eq!(doc.terms().count(), 1);
}

#[test]
fn checked_replace_and_delete_round_trip() {
    let dir = fresh_dir("checked");
    let idterm = "Qgene:1:ds";
    {
        let mut db = open_writable(&dir);
        db.replace_document_by_term_checked(idterm, &doc_with(idterm))
            .expect("checked replace succeeds");
        db.commit();
    }
    let reopened = Database::open(&dir, Some(DbBackend::Glass));
    assert_eq!(reopened.doc_count(), 1);
    drop(reopened);

    // Deleting an absent term is a silent no-op upstream and through the
    // checked API: no error, count unchanged.
    {
        let mut db = WritableDatabase::open(
            &dir,
            Some(DbAction::Open),
            Some(DbBackend::Glass),
            None,
            None,
        );
        db.delete_document_by_term_checked("Qgene:absent:ds")
            .expect("absent delete is a no-op, not an error");
        db.commit();
        // Deleting a present term removes the document.
        db.delete_document_by_term_checked(idterm)
            .expect("present delete succeeds");
        db.commit();
        db.close();
    }
    let reopened = Database::open(&dir, Some(DbBackend::Glass));
    assert_eq!(reopened.doc_count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn checked_write_reports_typed_error_for_invalid_term() {
    let dir = fresh_dir("typerror");
    let mut db = open_writable(&dir);
    // An empty unique term is invalid; the checked API must surface a typed
    // DbError instead of aborting the process.
    match db.replace_document_by_term_checked("", &Document::default()) {
        Ok(_) => panic!("empty unique term unexpectedly accepted"),
        Err(DbError {
            error_type,
            message,
        }) => {
            assert!(!error_type.is_empty(), "error_type should name the class");
            assert!(!message.is_empty(), "message should be populated");
        }
    }
    db.close();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn f64_value_round_trip_and_order() {
    let mut doc = Document::default();
    let a = 0.1f64;
    let b = 2.5f64;
    doc.set_value(0u32, a);
    doc.set_value(1u32, b);
    let back_a: f64 = doc.value::<f64>(0u32).unwrap().unwrap();
    let back_b: f64 = doc.value::<f64>(1u32).unwrap().unwrap();
    assert_eq!(back_a, a, "exact f64 round-trip");
    assert_eq!(back_b, b, "exact f64 round-trip");
    // sortable_serialise preserves order: serialised bytes sort with the values.
    let ser_a = a.serialize();
    let ser_b = b.serialize();
    assert!(ser_a < ser_b, "serialised floats must preserve a < b");
}
