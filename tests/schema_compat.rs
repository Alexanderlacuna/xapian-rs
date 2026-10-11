//! Schema-compatibility GOLD test: rebuild reference documents in Rust and
//! compare generated vs reference term lists and value slots.
//!
//! Three documents from the read-only reference index are rebuilt with the
//! writer configuration derived from the authoritative indexer source
//! (`~/project/genenetwork3/scripts/index-genenetwork`):
//!
//!   * `Qphenotype:10583:bxdpublish`
//!   * `Qphenotype:11331:bxdpublish`
//!   * the first hit of `Query("XTgene")`, recorded as
//!     `Qgene:100001_at:hc_u_0304_r` (dataset `HC_U_0304_R`,
//!     name `100001_at`).
//!
//! The generator consumes the document's source data; the reference index is
//! only ever read for comparison, never for generation.
//!
//! Two tiers are run:
//!
//! * `gold_docdata_*` (spec-literal): generate from the stored document data
//!   alone and report exactly which reference terms are reproducible from it
//!   and which are not. The stored data is LOSSY: the indexer pops fields
//!   before `set_data` (phenotypes: Abstract/Title/abbreviation/Lab_code and
//!   the raw authors string; genes: probe_target_description/alias/genbankid/
//!   unigeneid) and the RDF dictionaries (XRF/XWK) never appear in it, so the
//!   stored data cannot reproduce those terms.
//! * `gold_authoritative_*`: generate from the indexer's ACTUAL inputs — the
//!   stored data for surviving fields, plus the authoritative source values
//!   the indexer indexes but does not store (read from the GeneNetwork
//!   database via `derive-source-fields.py`), plus the RDF dictionaries held
//!   by the fork and fed from the authoritative RDF cache via
//!   `derive-dictionary.py`. These must match the reference exactly
//!   (termlist set, wdf, positions; every value slot 0..=8), which is the
//!   schema-compatibility proof.
//!
//! Reference index path: `$GN3_REF_INDEX` or the default build path.

use std::collections::BTreeMap;
use std::path::PathBuf;

use bytes::Bytes;
use serde_json::Value;
use xapian_rs::{
    Database, DbAction, DbBackend, Document, Enquire, Query, SimpleStopper, Stem, StopStrategy,
    TermGenerator, WritableDatabase,
};

const REF_DEFAULT: &str = "/home/kabui/spikes/index-self-build/xapian-index";

/// `(wdf, sorted positions)` for a single term.
type TermList = BTreeMap<String, (u32, Vec<u32>)>;

fn ref_index() -> PathBuf {
    std::env::var("GN3_REF_INDEX")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(REF_DEFAULT))
}

fn open_ref() -> Database {
    let path = ref_index();
    assert!(
        path.exists(),
        "reference index not found at {} (set GN3_REF_INDEX)",
        path.display()
    );
    Database::open(&path, Some(DbBackend::Glass))
}

fn doc_by_term(db: &Database, term: &str) -> Document {
    let mut enquire = Enquire::new(db);
    enquire.set_query(Query::term(term, None, None), None);
    let mset = enquire.mset(0, 1, None, None);
    mset.matches()
        .next()
        .unwrap_or_else(|| panic!("reference index has no document for {term}"))
        .document()
}

/// The writer's TermGenerator, configured exactly as the indexer: stemmer
/// `en`, default stemming strategy (never set), stop strategy `STOP_ALL`, and
/// an empty `SimpleStopper` (the non-functional stopper configuration is
/// reproduced verbatim, never "fixed").
fn writer_termgen() -> TermGenerator {
    let mut termgen = TermGenerator::default();
    termgen.set_stemmer(Stem::for_language("en"));
    termgen.set_stopper_strategy(StopStrategy::All);
    termgen.set_stopper(SimpleStopper::new());
    termgen
}

fn snapshot(doc: &Document) -> TermList {
    let mut terms = TermList::new();
    for term in doc.terms() {
        let name: &str = term.as_ref();
        let mut positions: Vec<u32> = term.positions().map(u32::from).collect();
        positions.sort_unstable();
        terms.insert(name.to_string(), (term.wdf(), positions));
    }
    terms
}

/// Raw bytes of slots 0..=8; `None` means the slot is absent.
fn raw_values(doc: &Document) -> [Option<Vec<u8>>; 9] {
    let mut out: [Option<Vec<u8>>; 9] = Default::default();
    for slot in 0u32..=8 {
        out[slot as usize] = doc
            .value::<Bytes>(slot)
            .map(|value| value.expect("Bytes deserialization is infallible").to_vec());
    }
    out
}

fn parse_data(doc: &Document) -> Value {
    serde_json::from_slice(&doc.data()).expect("reference docdata is not valid JSON")
}

fn field_str(data: &Value, key: &str) -> Option<String> {
    data.get(key).and_then(Value::as_str).map(str::to_string)
}

/// The indexer indexes the raw SQL authors string (comma separated) before
/// splitting it for storage, so rejoin the stored list with ", ".
fn authors_text(data: &Value) -> Option<String> {
    match data.get("authors") {
        Some(Value::Array(items)) => Some(
            items
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        ),
        Some(Value::String(text)) => Some(text.clone()),
        _ => None,
    }
}

/// Look up an indexer input: prefer the authoritative source value when
/// supplied, else fall back to the (possibly absent) stored data field.
fn source_or_data(fields: Option<&Value>, data: &Value, key: &str) -> Option<String> {
    fields
        .and_then(|fields| fields.get(key))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| field_str(data, key))
}

fn set_value_if_present(doc: &mut Document, slot: u32, data: &Value, key: &str) {
    if let Some(number) = data.get(key).and_then(Value::as_f64) {
        doc.set_value(slot, number);
    }
}

/// Rebuild a phenotype document, mirroring `index_phenotypes`.
///
/// `source` supplies the raw, non-stored input fields when available.
fn build_phenotype(data: &Value, source: Option<&Value>) -> Document {
    let mut doc = Document::default();
    {
        let mut termgen = writer_termgen();
        termgen.set_document(&doc);

        // Values, in the source's order (no mb slot for phenotypes).
        set_value_if_present(&mut doc, 0, data, "mean");
        set_value_if_present(&mut doc, 1, data, "lrs");
        set_value_if_present(&mut doc, 3, data, "geno_mb");
        set_value_if_present(&mut doc, 4, data, "additive");
        set_value_if_present(&mut doc, 5, data, "year");
        set_value_if_present(&mut doc, 6, data, "nsamples");

        // Free text WITH positions. Authors use the raw comma-separated string.
        let authors = source
            .and_then(|s| s.get("authors"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| authors_text(data));
        for key in ["description", "authors", "dataset"] {
            let text = if key == "authors" {
                authors.clone()
            } else {
                field_str(data, key)
            };
            if let Some(text) = text {
                termgen.index_text::<&str>(&text, None, None);
                termgen.increase_termpos(None);
            }
        }
        for key in ["Abstract", "Title"] {
            if let Some(text) = source_or_data(source, data, key) {
                termgen.index_text::<&str>(&text, None, None);
                termgen.increase_termpos(None);
            }
        }
        // Free text WITHOUT positions (unprefixed).
        for key in ["species", "group", "inbredsetcode"] {
            if let Some(text) = field_str(data, key) {
                termgen.index_text_without_positions::<&str>(&text, None, None);
            }
        }
        for key in ["abbreviation", "Lab_code"] {
            if let Some(text) = source_or_data(source, data, key) {
                termgen.index_text_without_positions::<&str>(&text, None, None);
            }
        }
        // Prefixed: index_text (WITH positions) for A and XDS, without positions
        // for the rest, exactly as the source.
        if let Some(text) = field_str(data, "species") {
            termgen.index_text_without_positions::<&str>(&text, Some(0u32), Some("XS"));
        }
        if let Some(text) = field_str(data, "group") {
            termgen.index_text_without_positions::<&str>(&text, Some(0u32), Some("XG"));
        }
        if let Some(text) = authors {
            termgen.index_text::<&str>(&text, Some(0u32), Some("A"));
        }
        if let Some(text) = field_str(data, "geno_chr") {
            termgen.index_text_without_positions::<&str>(&text, Some(0u32), Some("XPC"));
        }
        if let Some(text) = field_str(data, "dataset") {
            termgen.index_text::<&str>(&text, Some(0u32), Some("XDS"));
        }
    }
    doc
}

/// Rebuild a gene document, mirroring `index_genes`.
///
/// `rif`/`wiki` are the indexer's RDF-derived dictionary texts for this
/// document's `(species, symbol)`; `source` supplies the non-stored fields.
fn build_gene(
    data: &Value,
    source: Option<&Value>,
    rif: Option<&str>,
    wiki: Option<&str>,
) -> Document {
    let mut doc = Document::default();
    {
        let mut termgen = writer_termgen();
        termgen.set_document(&doc);

        set_value_if_present(&mut doc, 0, data, "mean");
        set_value_if_present(&mut doc, 1, data, "lrs");
        set_value_if_present(&mut doc, 2, data, "mb");
        set_value_if_present(&mut doc, 3, data, "geno_mb");
        set_value_if_present(&mut doc, 4, data, "additive");

        for key in ["description", "tissue", "dataset"] {
            if let Some(text) = field_str(data, key) {
                termgen.index_text::<&str>(&text, None, None);
                termgen.increase_termpos(None);
            }
        }
        if let Some(text) = source_or_data(source, data, "probe_target_description") {
            termgen.index_text::<&str>(&text, None, None);
            termgen.increase_termpos(None);
        }
        for key in ["name", "symbol", "species", "group"] {
            if let Some(text) = field_str(data, key) {
                termgen.index_text_without_positions::<&str>(&text, None, None);
            }
        }
        for key in ["alias", "genbankid", "unigeneid"] {
            if let Some(text) = source_or_data(source, data, key) {
                termgen.index_text_without_positions::<&str>(&text, None, None);
            }
        }
        if let Some(text) = field_str(data, "species") {
            termgen.index_text_without_positions::<&str>(&text, Some(0u32), Some("XS"));
        }
        if let Some(text) = field_str(data, "group") {
            termgen.index_text_without_positions::<&str>(&text, Some(0u32), Some("XG"));
        }
        if let Some(text) = field_str(data, "tissue") {
            termgen.index_text::<&str>(&text, Some(0u32), Some("XI"));
        }
        if let Some(text) = field_str(data, "dataset") {
            termgen.index_text::<&str>(&text, Some(0u32), Some("XDS"));
        }
        if let Some(text) = field_str(data, "symbol") {
            termgen.index_text_without_positions::<&str>(&text, Some(0u32), Some("XY"));
        }
        if let Some(text) = field_str(data, "chr") {
            termgen.index_text_without_positions::<&str>(&text, Some(0u32), Some("XC"));
        }
        if let Some(text) = field_str(data, "geno_chr") {
            termgen.index_text_without_positions::<&str>(&text, Some(0u32), Some("XPC"));
        }
        if let Some(text) = rif {
            termgen.index_text_without_positions::<&str>(text, Some(0u32), Some("XRF"));
        }
        if let Some(text) = wiki {
            termgen.index_text_without_positions::<&str>(text, Some(0u32), Some("XWK"));
        }
    }
    doc
}

fn identity_term(doctype: &str, name: &str, dataset: &str) -> String {
    format!(
        "Q{}:{}",
        doctype,
        format!("{name}:{dataset}").to_lowercase()
    )
}

fn add_identity_terms(doc: &mut Document, doctype: &str, idterm: &str) {
    doc.add_boolean_term(format!("XT{doctype}"));
    doc.add_boolean_term(idterm);
}

/// Write `doc` through the fork's write API into a fresh index, commit, and
/// reopen it read-only. Returns the directory (for cleanup) and the reopened
/// read-only database.
fn write_and_reopen(idterm: &str, doc: &Document) -> (PathBuf, Database) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "xapian-rs-gold-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    {
        let mut db = WritableDatabase::open(
            &dir,
            Some(DbAction::CreateOrOverwrite),
            Some(DbBackend::Glass),
            None,
            None,
        );
        db.replace_document_by_term(idterm, doc);
        db.commit();
        db.close();
    }
    let reopened = Database::open(&dir, Some(DbBackend::Glass));
    (dir, reopened)
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

fn render_term(name: &str, value: &(u32, Vec<u32>)) -> String {
    format!("  {name:<40} wdf={} pos={:?}", value.0, value.1)
}

fn log_dir() -> Option<PathBuf> {
    std::env::var("GN3_GOLD_LOG_DIR").ok().map(PathBuf::from)
}

fn write_log(name: &str, contents: &str) {
    if let Some(dir) = log_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join(name), contents);
    }
}

/// Exact comparison: termlist (set, wdf, positions) and every value slot.
fn assert_exact(
    label: &str,
    generated: &Document,
    reference: &Document,
    log: &mut String,
    logfile: &str,
) {
    use std::fmt::Write as _;

    writeln!(log, "DOC {label} [authoritative inputs]").unwrap();
    assert_eq!(
        generated.data(),
        reference.data(),
        "{label}: docdata round-trip differs"
    );
    writeln!(log, "  docdata round-trip byte-equal: true").unwrap();

    let gen_terms = snapshot(generated);
    let ref_terms = snapshot(reference);
    writeln!(log, "  generated terms: {}", gen_terms.len()).unwrap();
    writeln!(log, "  reference terms: {}", ref_terms.len()).unwrap();

    let mut only_gen = Vec::new();
    let mut differing = Vec::new();
    for (term, value) in &gen_terms {
        match ref_terms.get(term) {
            None => only_gen.push(render_term(term, value)),
            Some(reference_value) if reference_value != value => differing.push(format!(
                "  differs: {term}\n    generated {}\n    reference {}",
                render_term("", value),
                render_term("", reference_value)
            )),
            Some(_) => {}
        }
    }
    let extra: Vec<String> = ref_terms
        .iter()
        .filter(|(term, _)| !gen_terms.contains_key(*term))
        .map(|(term, value)| render_term(term, value))
        .collect();

    if !extra.is_empty() {
        writeln!(log, "  reference-only terms ({}):", extra.len()).unwrap();
        for line in &extra {
            writeln!(log, "{line}").unwrap();
        }
    }
    if !only_gen.is_empty() {
        writeln!(log, "  generated-only terms ({}):", only_gen.len()).unwrap();
        for line in &only_gen {
            writeln!(log, "{line}").unwrap();
        }
    }
    for line in &differing {
        writeln!(log, "{line}").unwrap();
    }

    let gen_values = raw_values(generated);
    let ref_values = raw_values(reference);
    writeln!(log, "  value slots (generated | reference):").unwrap();
    for slot in 0..9 {
        writeln!(
            log,
            "    slot {slot}: {} | {}",
            gen_values[slot]
                .as_deref()
                .map(hex)
                .unwrap_or_else(|| "<absent>".into()),
            ref_values[slot]
                .as_deref()
                .map(hex)
                .unwrap_or_else(|| "<absent>".into())
        )
        .unwrap();
    }

    write_log(logfile, log);
    print!("{log}");
    assert!(
        only_gen.is_empty() && differing.is_empty() && extra.is_empty(),
        "{label}: termlist mismatch (generated-only={} differing={} reference-only={}); see log",
        only_gen.len(),
        differing.len(),
        extra.len()
    );
    assert_eq!(
        gen_values, ref_values,
        "{label}: value-slot mismatch; see log"
    );
}

/// Spec-literal docdata-only comparison: every term generated from the stored
/// data must exist in the reference with the same-or-larger wdf (the popped
/// fields only ever add occurrences), and the reference-only residual is
/// reported in full.
fn assert_docdata_subset(
    label: &str,
    generated: &Document,
    reference: &Document,
    log: &mut String,
    logfile: &str,
) {
    use std::fmt::Write as _;

    let gen_terms = snapshot(generated);
    let ref_terms = snapshot(reference);
    writeln!(log, "DOC {label} [stored docdata only]").unwrap();
    writeln!(log, "  generated terms: {}", gen_terms.len()).unwrap();
    writeln!(log, "  reference terms: {}", ref_terms.len()).unwrap();

    let mut missing = Vec::new();
    let mut wdf_gt = Vec::new();
    for (term, value) in &gen_terms {
        match ref_terms.get(term) {
            None => missing.push(render_term(term, value)),
            Some(reference_value) => {
                if value.0 > reference_value.0 {
                    wdf_gt.push(format!(
                        "  {term}: generated wdf={} > reference wdf={}",
                        value.0, reference_value.0
                    ));
                }
            }
        }
    }
    let residual: Vec<(&String, &(u32, Vec<u32>))> = ref_terms
        .iter()
        .filter(|(term, _)| !gen_terms.contains_key(*term))
        .collect();
    writeln!(
        log,
        "  reference-only residual: {} terms (indexed from fields the indexer pops before set_data, and from the RDF dictionaries)",
        residual.len()
    )
    .unwrap();
    for (term, value) in &residual {
        writeln!(log, "{}", render_term(term, value)).unwrap();
    }
    if !missing.is_empty() {
        writeln!(log, "  MISSING from reference ({}):", missing.len()).unwrap();
        for line in &missing {
            writeln!(log, "{line}").unwrap();
        }
    }
    if !wdf_gt.is_empty() {
        writeln!(log, "  WDF GREATER than reference ({}):", wdf_gt.len()).unwrap();
        for line in &wdf_gt {
            writeln!(log, "{line}").unwrap();
        }
    }

    write_log(logfile, log);
    print!("{log}");
    assert!(
        missing.is_empty(),
        "{label}: generated terms absent from the reference ({}); see log",
        missing.len()
    );
    assert!(
        wdf_gt.is_empty(),
        "{label}: generated wdf exceeded the reference ({}); see log",
        wdf_gt.len()
    );
}

fn source_fixture() -> Value {
    serde_json::from_slice(include_bytes!("data/source_fields.json"))
        .expect("source_fields.json is not valid JSON")
}

#[test]
fn gold_docdata_phenotypes_are_consistent_subsets() {
    let db = open_ref();
    let mut log = String::new();
    for idterm in ["Qphenotype:10583:bxdpublish", "Qphenotype:11331:bxdpublish"] {
        let reference = doc_by_term(&db, idterm);
        let data = parse_data(&reference);
        let mut generated = build_phenotype(&data, None);
        generated.set_data(reference.data());
        add_identity_terms(&mut generated, "phenotype", idterm);
        let (dir, reopened) = write_and_reopen(idterm, &generated);
        let read_back = doc_by_term(&reopened, idterm);
        assert_docdata_subset(
            idterm,
            &read_back,
            &reference,
            &mut log,
            "gold-docdata-phenotypes.txt",
        );
        drop(reopened);
        let _ = std::fs::remove_dir_all(&dir);
    }
    write_log("gold-docdata-phenotypes.summary.txt", &log);
}

#[test]
fn gold_docdata_gene_is_a_consistent_subset() {
    let db = open_ref();
    let mut log = String::new();
    let (idterm, reference, data) = first_gene(&db);
    let mut generated = build_gene(&data, None, None, None);
    generated.set_data(reference.data());
    add_identity_terms(&mut generated, "gene", &idterm);
    let (dir, reopened) = write_and_reopen(&idterm, &generated);
    let read_back = doc_by_term(&reopened, &idterm);
    use std::fmt::Write as _;
    writeln!(
        log,
        "gene document: dataset={} name={} idterm={idterm}",
        field_str(&data, "dataset").unwrap_or_default(),
        field_str(&data, "name").unwrap_or_default()
    )
    .unwrap();
    assert_docdata_subset(
        &idterm,
        &read_back,
        &reference,
        &mut log,
        "gold-docdata-gene.txt",
    );
    write_log("gold-docdata-gene.summary.txt", &log);
    drop(reopened);
    let _ = std::fs::remove_dir_all(&dir);
}

fn first_gene(db: &Database) -> (String, Document, Value) {
    let mut enquire = Enquire::new(db);
    enquire.set_query(Query::term("XTgene", None, None), None);
    let mset = enquire.mset(0, 1, None, None);
    let reference = mset
        .matches()
        .next()
        .expect("reference index has no XTgene document")
        .document();
    let data = parse_data(&reference);
    let dataset = field_str(&data, "dataset").expect("gene docdata has no dataset");
    let name = field_str(&data, "name").expect("gene docdata has no name");
    let idterm = identity_term("gene", &name, &dataset);
    (idterm, reference, data)
}

#[test]
fn gold_authoritative_documents_match_reference_exactly() {
    let db = open_ref();
    let source = source_fixture();
    let dictionary: Value = serde_json::from_slice(include_bytes!("data/gene_dictionary.json"))
        .expect("gene_dictionary.json is not valid JSON");
    let mut log = String::new();

    for idterm in ["Qphenotype:10583:bxdpublish", "Qphenotype:11331:bxdpublish"] {
        let reference = doc_by_term(&db, idterm);
        let data = parse_data(&reference);
        let fields = source.get(idterm);
        let mut generated = build_phenotype(&data, fields);
        generated.set_data(reference.data());
        add_identity_terms(&mut generated, "phenotype", idterm);
        let (dir, reopened) = write_and_reopen(idterm, &generated);
        let read_back = doc_by_term(&reopened, idterm);
        assert_exact(
            idterm,
            &read_back,
            &reference,
            &mut log,
            "gold-authoritative-phenotypes.txt",
        );
        drop(reopened);
        let _ = std::fs::remove_dir_all(&dir);
    }

    let (idterm, reference, data) = first_gene(&db);
    let species = field_str(&data, "species").expect("gene docdata has no species");
    let symbol = field_str(&data, "symbol").expect("gene docdata has no symbol");
    let key = format!("{species}|{symbol}");
    let rif = dictionary
        .get(&key)
        .and_then(|v| v.get("rif"))
        .and_then(Value::as_str);
    let wiki = dictionary
        .get(&key)
        .and_then(|v| v.get("wiki"))
        .and_then(Value::as_str);
    let mut generated = build_gene(&data, source.get(&idterm), rif, wiki);
    generated.set_data(reference.data());
    add_identity_terms(&mut generated, "gene", &idterm);
    let (dir, reopened) = write_and_reopen(&idterm, &generated);
    let read_back = doc_by_term(&reopened, &idterm);
    use std::fmt::Write as _;
    writeln!(
        log,
        "gene document: dataset={} name={} species={} symbol={} idterm={idterm}",
        field_str(&data, "dataset").unwrap_or_default(),
        field_str(&data, "name").unwrap_or_default(),
        species,
        symbol
    )
    .unwrap();
    writeln!(log, "dictionary key {key}: rif={rif:?} wiki={wiki:?}").unwrap();
    assert_exact(
        &idterm,
        &read_back,
        &reference,
        &mut log,
        "gold-authoritative-gene.txt",
    );
    write_log("gold-authoritative.summary.txt", &log);
    print!("{log}");
    drop(reopened);
    let _ = std::fs::remove_dir_all(&dir);
}
