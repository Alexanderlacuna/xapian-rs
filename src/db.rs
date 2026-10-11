use crate::ffi::{self, ToCxxString, cxx_bytes};

use std::{path::Path, pin::Pin};

use autocxx::{cxx, prelude::*};
use bitflags::bitflags;
use bytes::Bytes;

/// A typed failure from a checked write operation.
///
/// `error_type` is the Xapian exception class name (for example
/// `DatabaseLockError` or `InvalidArgumentError`); `message` is the
/// exception's message. This is the write-side counterpart of
/// [`ParseError`][crate::ParseError] and follows the same shim pattern: the
/// C++ exception is caught in the `shim` layer, so callers get a typed error
/// instead of a process abort.
#[derive(Clone, Debug)]
pub struct DbError {
    /// The Xapian exception class name
    pub error_type: String,
    /// The exception message
    pub message: String,
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.error_type, self.message)
    }
}

impl std::error::Error for DbError {}

/// A read-only Xapian database
pub struct Database(Pin<Box<ffi::Database>>);

impl Database {
    /// Open a read-only Database at the provided path
    pub fn open(path: impl AsRef<Path>, backend: impl Into<Option<DbBackend>>) -> Self {
        let flags = backend.into().unwrap_or_default();
        Self(ffi::Database::new1(&path.as_ref().to_cxx_string(), flags.into()).within_box())
    }

    /// Close a Database
    pub fn close(&mut self) {
        self.0.as_mut().close()
    }

    /// Get the number of documents stored in the database
    pub fn doc_count(&self) -> u32 {
        self.0.get_doccount().into()
    }

    /// Detect whether a given term exists in the database
    pub fn term_exists(&self, term: impl AsRef<[u8]>) -> bool {
        cxx::let_cxx_string!(term = term);
        self.0.term_exists(&term)
    }
}

impl AsRef<ffi::Database> for Database {
    fn as_ref(&self) -> &ffi::Database {
        &self.0
    }
}

impl Clone for Database {
    fn clone(&self) -> Self {
        Self(ffi::shim::database_clone(&self.0).within_box())
    }
}

impl From<&WritableDatabase> for Database {
    fn from(value: &WritableDatabase) -> Self {
        Self(ffi::shim::database_clone(value.as_ref()).within_box())
    }
}

impl From<WritableDatabase> for Database {
    fn from(value: WritableDatabase) -> Self {
        Self::from(&value)
    }
}

/// A flag indicating how to handle the database already existing (or not)
#[repr(i32)]
#[derive(Default)]
pub enum DbAction {
    /// Open the database if it exists, create it otherwise
    #[default]
    CreateOrOpen = 0x00,
    /// Overwrite the database if it exists, create it otherwise
    CreateOrOverwrite = 0x01,
    /// Create a new database
    Create = 0x02,
    /// Open an existing database
    Open = 0x03,
}

impl From<DbAction> for autocxx::c_int {
    fn from(value: DbAction) -> Self {
        (value as i32).into()
    }
}

/// The type of backend to use for the database
#[repr(i32)]
#[derive(Default)]
pub enum DbBackend {
    #[default]
    /// Automatically select a backend
    Auto = 0x000,
    /// Use the Glass backend
    Glass = 0x100,
    /// Use the Chert backend
    Chert = 0x200,
    /// Use the stub backend
    Stub = 0x300,
    /// Use an in-memory database
    InMemory = 0x400,
}

impl From<DbBackend> for autocxx::c_int {
    fn from(value: DbBackend) -> Self {
        (value as i32).into()
    }
}

bitflags! {
    /// Various flags to modify writable database behavior
    pub struct DbFlags: u32 {
        /// Don't attempt to ensure changes have hit the disk
        const NO_SYNC = 0x04;
        /// Try to ensure changes have hit the disk
        const FULL_SYNC = 0x08;
        /// Update the database in-place
        const DANGEROUS = 0x10;
        /// Do not create a termlist table when creating the database
        const NO_TERMLIST = 0x20;
        /// If the database is already locked, retry it
        const RETRY_LOCK = 0x40;
    }
}

/// A Xapian database that can be read or written to
pub struct WritableDatabase(Pin<Box<ffi::WritableDatabase>>);

impl Default for WritableDatabase {
    /// Open a new, in-memory [`WritableDatabase`]
    fn default() -> Self {
        Self::inmemory()
    }
}

impl WritableDatabase {
    /// Open a database for updates
    ///
    /// Automatically selects the appropriate backend to use
    pub fn open(
        path: impl AsRef<Path>,
        action: impl Into<Option<DbAction>>,
        backend: impl Into<Option<DbBackend>>,
        flags: impl Into<Option<DbFlags>>,
        block_size: impl Into<Option<i32>>,
    ) -> Self {
        let action = action.into().unwrap_or_default();
        let backend = backend.into().unwrap_or_default();
        let flags = flags.into().map(|f| f.bits()).unwrap_or(0);
        let flags = action as i32 | backend as i32 | flags as i32;

        let block_size = block_size.into().unwrap_or(0);
        Self(
            ffi::WritableDatabase::new1(
                &path.as_ref().to_cxx_string(),
                flags.into(),
                block_size.into(),
            )
            .within_box(),
        )
    }

    /// Create a new, in-memory WritableDatabase
    pub fn inmemory() -> Self {
        Self(ffi::InMemory::open().within_box())
    }

    /// Add shards from another `WritableDatabase`
    pub fn add_database(&mut self, db: impl AsRef<ffi::WritableDatabase>) {
        self.0.as_mut().add_database(db.as_ref())
    }

    /// Add a new document to the database
    pub fn add_document(&mut self, doc: impl AsRef<ffi::Document>) -> crate::DocId {
        unsafe { crate::DocId::new_unchecked(self.0.as_mut().add_document(doc.as_ref())) }
    }

    /// Add a word to the spelling dictionary
    pub fn add_spelling(&self, word: impl AsRef<str>, increment: impl Into<Option<u32>>) {
        let increment = increment.into().unwrap_or(1);
        cxx::let_cxx_string!(word = word.as_ref());
        self.0.add_spelling(&word, increment.into())
    }

    /// Add a synonym for a term
    pub fn add_synonym(&self, term: impl AsRef<str>, synonym: impl AsRef<str>) {
        cxx::let_cxx_string!(term = term.as_ref());
        cxx::let_cxx_string!(synonym = synonym.as_ref());
        self.0.add_synonym(&term, &synonym);
    }

    /// Begin a transaction
    pub fn begin_transaction(&mut self, flushed: impl Into<Option<bool>>) {
        let flushed = flushed.into().unwrap_or(true);
        self.0.as_mut().begin_transaction(flushed)
    }

    /// Abort the transaction currently in progress
    pub fn cancel_transaction(&mut self) {
        self.0.as_mut().cancel_transaction()
    }

    /// Complete the transaction currently in progress
    pub fn commit_transaction(&mut self) {
        self.0.as_mut().commit_transaction()
    }

    /// Close the database
    pub fn close(&mut self) {
        let db: Pin<&mut ffi::Database> = unsafe { ffi::upcast(self.0.as_mut()) };
        db.close()
    }

    /// Commit any pending modifications made to the database
    pub fn commit(&mut self) {
        self.0.as_mut().commit()
    }

    /// Delete the document (if any) matching the specified [`DocId`][crate::DocId] from the database
    pub fn delete_document(&mut self, id: impl Into<crate::DocId>) {
        self.0.as_mut().delete_document(ffi::docid::from(id.into()))
    }

    /// Delete any documents indexed by the specified term from the database
    pub fn delete_document_by_term(&mut self, term: impl AsRef<str>) {
        cxx::let_cxx_string!(term = term.as_ref());
        self.0.as_mut().delete_document1(&term)
    }

    /// Get the number of documents in the database
    pub fn doc_count(&self) -> u32 {
        let db: &ffi::Database = self.as_ref();
        db.get_doccount().into()
    }

    /// Get the user-specified metadata associated with a given key
    pub fn metadata(&self, key: impl AsRef<str>) -> Bytes {
        let db: &ffi::Database = self.as_ref();
        cxx::let_cxx_string!(key = key.as_ref());
        cxx_bytes(&db.get_metadata(&key))
    }

    /// Retrieve a read-only `Database` instance backed by this `WritableDatabase`
    pub fn read_only(&self) -> Database {
        Database::from(self)
    }

    /// Remove a word from the spelling dictionary
    pub fn remove_spelling(&self, word: impl AsRef<str>, decrement: impl Into<Option<u32>>) {
        let decrement = decrement.into().unwrap_or(1);
        cxx::let_cxx_string!(word = word.as_ref());
        self.0.remove_spelling(&word, decrement.into());
    }

    /// Remove the given synonym for the specified term
    pub fn remove_synonym(&self, term: impl AsRef<str>, synonym: impl AsRef<str>) {
        cxx::let_cxx_string!(term = term.as_ref());
        cxx::let_cxx_string!(synonym = synonym.as_ref());
        self.0.remove_synonym(&term, &synonym);
    }

    /// Replace the document (if any) matching the specified [`DocId`][crate::DocId] from the database with the specified `doc`
    pub fn replace_document(
        &mut self,
        id: impl Into<crate::DocId>,
        doc: impl AsRef<ffi::Document>,
    ) {
        self.0
            .as_mut()
            .replace_document(ffi::docid::from(id.into()), doc.as_ref())
    }

    /// Replace any documents matching the given term
    pub fn replace_document_by_term(
        &mut self,
        term: impl AsRef<str>,
        doc: impl AsRef<ffi::Document>,
    ) -> u32 {
        cxx::let_cxx_string!(term = term.as_ref());
        self.0
            .as_mut()
            .replace_document1(&term, doc.as_ref())
            .into()
    }

    /// Replace any documents matching the given term, returning a typed
    /// [`DbError`] instead of aborting on a Xapian exception.
    ///
    /// The C++ exception is caught in the `shim` layer, so a failure (for
    /// example an invalid unique term) yields `Err(DbError { error_type,
    /// message })` with no unwind. On success the returned `u32` is the docid
    /// of the replaced document.
    pub fn replace_document_by_term_checked(
        &mut self,
        term: impl AsRef<str>,
        doc: impl AsRef<ffi::Document>,
    ) -> Result<u32, DbError> {
        cxx::let_cxx_string!(term = term.as_ref());
        let mut error_type = "".to_cxx_string();
        let mut message = "".to_cxx_string();
        let replaced = ffi::shim::writable_database_replace_document_by_term_checked(
            self.0.as_mut(),
            &term,
            doc.as_ref(),
            error_type.pin_mut(),
            message.pin_mut(),
        );
        let replaced: u32 = replaced.into();
        if error_type.as_bytes().is_empty() {
            Ok(replaced)
        } else {
            Err(DbError {
                error_type: error_type.to_string(),
                message: message.to_string(),
            })
        }
    }

    /// Delete any documents indexed by the specified term, returning the number
    /// of documents deleted or a typed [`DbError`] instead of aborting on a
    /// Xapian exception.
    pub fn delete_document_by_term_checked(
        &mut self,
        term: impl AsRef<str>,
    ) -> Result<(), DbError> {
        cxx::let_cxx_string!(term = term.as_ref());
        let mut error_type = "".to_cxx_string();
        let mut message = "".to_cxx_string();
        ffi::shim::writable_database_delete_document_by_term_checked(
            self.0.as_mut(),
            &term,
            error_type.pin_mut(),
            message.pin_mut(),
        );
        if error_type.as_bytes().is_empty() {
            Ok(())
        } else {
            Err(DbError {
                error_type: error_type.to_string(),
                message: message.to_string(),
            })
        }
    }

    /// Replace the document with the given docid, returning a typed [`DbError`]
    /// instead of aborting on a Xapian exception.
    pub fn replace_document_checked(
        &mut self,
        id: impl Into<crate::DocId>,
        doc: impl AsRef<ffi::Document>,
    ) -> Result<(), DbError> {
        let mut error_type = "".to_cxx_string();
        let mut message = "".to_cxx_string();
        ffi::shim::writable_database_replace_document_checked(
            self.0.as_mut(),
            ffi::docid::from(id.into()),
            doc.as_ref(),
            error_type.pin_mut(),
            message.pin_mut(),
        );
        if error_type.as_bytes().is_empty() {
            Ok(())
        } else {
            Err(DbError {
                error_type: error_type.to_string(),
                message: message.to_string(),
            })
        }
    }

    /// Delete the document with the given docid, returning a typed [`DbError`]
    /// instead of aborting on a Xapian exception.
    pub fn delete_document_checked(&mut self, id: impl Into<crate::DocId>) -> Result<(), DbError> {
        let mut error_type = "".to_cxx_string();
        let mut message = "".to_cxx_string();
        ffi::shim::writable_database_delete_document_checked(
            self.0.as_mut(),
            ffi::docid::from(id.into()),
            error_type.pin_mut(),
            message.pin_mut(),
        );
        if error_type.as_bytes().is_empty() {
            Ok(())
        } else {
            Err(DbError {
                error_type: error_type.to_string(),
                message: message.to_string(),
            })
        }
    }

    /// Wrap the function specified in `f` in a transaction
    pub fn transaction(
        &mut self,
        flushed: impl Into<Option<bool>>,
        mut f: impl FnMut(&mut WritableDatabase),
    ) {
        self.begin_transaction(flushed);
        f(self);
        self.commit_transaction();
    }
}

impl AsRef<ffi::Database> for WritableDatabase {
    fn as_ref(&self) -> &ffi::Database {
        ffi::shim::writable_database_upcast(&self.0)
    }
}

impl AsRef<ffi::WritableDatabase> for WritableDatabase {
    fn as_ref(&self) -> &ffi::WritableDatabase {
        &self.0
    }
}
