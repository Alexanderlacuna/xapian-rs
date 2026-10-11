//! Mandatory negative controls (a, b, c) for the write-API lane.
//!
//! Default run executes each control with its CORRECT expectation and exits 0:
//!   cargo run --example write_controls
//!
//! Sabotage runs deliberately break the scenario while keeping the assertion,
//! so the assertion MUST fail (a panic => non-zero exit):
//!   cargo run --example write_controls -- --red-a   # two different idterms
//!   cargo run --example write_controls -- --red-b   # delete a PRESENT term
//!   cargo run --example write_controls -- --red-c   # reversed float order
//!
//! Evidence is the process exit status captured directly (no pipes).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use xapian_rs::{Database, DbAction, DbBackend, Document, ToValue, WritableDatabase};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn fresh_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "xapian-rs-controls-{}-{tag}-{}",
        std::process::id(),
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

/// Control (a): replace_document twice with the SAME identity term on a fresh
/// index; a REOPENED read-only handle must report doc_count == 1. Uncommitted
/// changes are visible to the same writable handle, so the read must be a fresh
/// read-only open.
fn control_a(red: bool) {
    let dir = fresh_dir("a");
    let (first, second) = if red {
        // Sabotage: two DIFFERENT identity terms create two documents.
        ("Qgene:dup:ds", "Qgene:dup2:ds")
    } else {
        ("Qgene:dup:ds", "Qgene:dup:ds")
    };
    {
        let mut db = open_writable(&dir);
        db.replace_document_by_term(first, &doc_with(first));
        db.replace_document_by_term(second, &doc_with(second));
        db.commit();
        db.close();
    }
    let reopened = Database::open(&dir, Some(DbBackend::Glass));
    let count = reopened.doc_count();
    drop(reopened);
    println!("control a: reopened doc_count = {count} (expect 1)");
    assert_eq!(count, 1, "control a FAILED: replace twice yielded {count}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Control (b): delete_document_by_term on an ABSENT term is a silent no-op
/// upstream; doc_count must be unchanged and nothing must abort.
fn control_b(red: bool) {
    let dir = fresh_dir("b");
    let present = "Qgene:present:ds";
    {
        let mut db = open_writable(&dir);
        db.replace_document_by_term(present, &doc_with(present));
        db.replace_document_by_term("Qgene:other:ds", &doc_with("Qgene:other:ds"));
        db.commit();
        db.close();
    }
    let before = Database::open(&dir, Some(DbBackend::Glass)).doc_count();
    {
        let mut db = WritableDatabase::open(
            &dir,
            Some(DbAction::Open),
            Some(DbBackend::Glass),
            None,
            None,
        );
        if red {
            // Sabotage: delete a PRESENT term, then still expect no change.
            db.delete_document_by_term(present);
        } else {
            db.delete_document_by_term("Qgene:absent:ds");
        }
        db.commit();
        db.close();
    }
    let after = Database::open(&dir, Some(DbBackend::Glass)).doc_count();
    println!("control b: doc_count before={before} after={after} (expect unchanged)");
    assert_eq!(
        after, before,
        "control b FAILED: doc_count changed {before}->{after}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Control (c): f64 slot round-trip is exact, and sortable serialisation
/// preserves order for a < b.
fn control_c(red: bool) {
    let mut doc = Document::default();
    let a = 0.1f64;
    let b = 2.5f64;
    doc.set_value(0u32, a);
    doc.set_value(1u32, b);
    let back_a: f64 = doc.value::<f64>(0u32).unwrap().unwrap();
    let back_b: f64 = doc.value::<f64>(1u32).unwrap().unwrap();
    println!("control c: round-trip {a}->{back_a}, {b}->{back_b}");
    assert_eq!(back_a, a, "control c FAILED: round-trip not exact");
    assert_eq!(back_b, b, "control c FAILED: round-trip not exact");
    let (ser_a, ser_b) = (a.serialize(), b.serialize());
    println!("control c: ser({a})={ser_a:?} ser({b})={ser_b:?} (expect ser(a) < ser(b))");
    if red {
        // Sabotage: reversed order expectation.
        assert!(
            ser_a > ser_b,
            "control c FAILED(RED): expected reversed order"
        );
    } else {
        assert!(ser_a < ser_b, "control c FAILED: order not preserved");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let red = args.get(1).map(String::as_str);
    let (red_a, red_b, red_c) = match red {
        Some("--red-a") => (true, false, false),
        Some("--red-b") => (false, true, false),
        Some("--red-c") => (false, false, true),
        Some(other) => {
            eprintln!("unknown mode {other:?}");
            std::process::exit(2);
        }
        None => (false, false, false),
    };

    control_a(red_a);
    control_b(red_b);
    control_c(red_c);
    println!("ALL CONTROLS OK");
}
