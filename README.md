# xapian-rs

[![GitHub](https://img.shields.io/crates/l/xapian-rs)](https://github.com/torrancew/xapian-rs)
[![crates.io](https://img.shields.io/crates/d/xapian-rs)](https://crates.io/crates/xapian-rs)
[![docs.rs](https://docs.rs/xapian-rs/badge.svg)](https://docs.rs/xapian-rs)

`xapian-rs` provides a set of *low-level*, *mostly-ergonomic* Rust bindings for
the [Xapian](https://xapian.org) search library.

The bindings are provided by a mix of auto-generation (via
[`autocxx`](https://autocxx.rs)) and manual generation (via
[`cxx`](https://cxx.rs)). When necessary, small C++ shims are implemented to
work around incompatibilities between these tools and the Xapian codebase.

## Status / Stability

`xapian-rs` is currently immature, untested and incomplete. During the `0.x`
version series, no stability guarantees are provided for the API, and it may
change or break at any time. A small, limited real-world use case has been
implemented in [`pantry`](https://github.com/torrancew/pantry), which exercises
an interesting but small subset of the capabilities of Xapian:
- Indexing
- Searching
- Faceting

Some functionality is not provided at this time, including (but not limited to):
- `KeyMaker`

## Design

Where possible, `xapian-rs` tries to provide simple and ergonomic interactions
with idiomatic Rust code. However, Xapian is a C++ codebase which uses C++
idioms, and this does have some consequences on the current design (as do
limitations of the `autocxx` and `cxx`):
- Xapian primarily uses exceptions for error handling. `autocxx` does not
  currently support catching exceptions (though `cxx` does). In the current
  version, **any Xapian exception will trigger a panic in Rust code**. This
  will improve as the library evolves.
- Xapian uses C++ strings very heavily. C++ strings provide no encoding
  guarantees, while Rust strings are guaranteed to be valid UTF-8. These
  bindings currently handle this in a way that is inconsistent (though at times
  convenient). This will become more well-defined as the library evolves.
- Several Xapian types are exposed in a way that allows implementation via Rust
  traits. At present, these traits are generally implemented via `&self`
  references, and therefore interior mutability is often needed to implement
  interesting functionality.
- Some of these traits intentionally leak memory when passed to FFI today. This
  will improve as the library evolves.

## Examples

Several examples are provided in the `examples` directory. The `tests`
directory's integration tests are also useful.

## Fork notes

This fork (of torrancew/xapian-rs at 0.3.0) is maintained for
GeneNetwork4 and carries four changes:

- `QueryParser::parse_query_checked` and the `ParseError` type: a
  checked-parse API returning typed errors instead of aborting on
  Xapian parser exceptions. The unchecked `parse_query` does not
  merely panic on parser errors as previously documented: the C++
  exception unwinds into Rust frames and std::terminate ABORTS THE
  PROCESS (verified empirically). Use the checked variant for any
  untrusted input.
- cxx pinned to exactly 1.0.122: the upstream caret requirement now
  resolves to cxx 1.0.202, which requires rustc 1.88; the pin keeps
  this crate building on rustc 1.85 toolchains.
- Processor declines map to MatchNothing: a FieldProcessor or
  RangeProcessor returning None previously produced an OP_INVALID
  query that the parser accepted but Enquire rejected with an
  uncatchable exception, aborting the whole process (verified
  empirically). MatchNothing is Xapian's documented decline
  protocol; a declined range now falls back to text exactly like
  the C++ NumberValueRangeProcessor.
- NumberRangeProcessor parses range bounds as f64: as f32 they lost
  exactness above 2^24 (16777217 became 16777216), corrupting range
  endpoints.

Verified under Guix (rust 1.85.1, libxapian 1.4.29): unit tests pass,
and the checked-parse API returns the Xapian exception class and
message for malformed queries. Build note: autocxx needs libclang;
inside a Guix shell, point LIBCLANG_PATH at the clang package's lib
directory (for example via the profile that provides clang).
