use crate::ffi;

use std::pin::Pin;

use autocxx::prelude::*;

#[derive(Clone)]
pub struct ESetIter<'eset> {
    size: (u32, u32),
    cursor_fwd: Pin<Box<ffi::ESetIterator>>,
    cursor_rev: Pin<Box<ffi::ESetIterator>>,
    eset: &'eset crate::ESet,
}

impl<'eset> ESetIter<'eset> {
    pub(crate) fn new(eset: &'eset crate::ESet) -> Self {
        let size = (eset.size(), 0);
        Self {
            size,
            cursor_fwd: eset.begin(),
            cursor_rev: eset.end(),
            eset,
        }
    }
}

impl Iterator for ESetIter<'_> {
    type Item = crate::Expansion;

    fn next(&mut self) -> Option<Self::Item> {
        // cursor_fwd starts out pointing to the first element
        // and therefore it yields an element before incrementing
        match &mut self.cursor_fwd {
            x if x == &self.cursor_rev || x == &self.eset.end() => None,
            c => {
                let item = crate::Expansion::new(c.clone());
                ffi::shim::eset_iterator_increment(c.as_mut());
                self.size.1 += 1;
                Some(item)
            }
        }
    }
}

impl DoubleEndedIterator for ESetIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        // cursor_rev starts out pointing after the last element
        // and therefore it decrements before it yields
        match &mut self.cursor_rev {
            x if x == &self.cursor_fwd || x == &self.eset.begin() => None,
            c => {
                ffi::shim::eset_iterator_decrement(c.as_mut());
                self.size.1 += 1;
                Some(crate::Expansion::new(c.clone()))
            }
        }
    }
}

#[derive(Clone)]
pub struct MSetIter<'mset> {
    size: (u32, u32),
    cursor_fwd: Pin<Box<ffi::MSetIterator>>,
    cursor_rev: Pin<Box<ffi::MSetIterator>>,
    mset: &'mset crate::MSet,
}

impl<'mset> MSetIter<'mset> {
    pub(crate) fn new(mset: &'mset crate::MSet) -> Self {
        let size = (mset.size(), 0);
        Self {
            size,
            cursor_fwd: mset.begin(),
            cursor_rev: mset.end(),
            mset,
        }
    }
}

impl Iterator for MSetIter<'_> {
    type Item = crate::Match;

    fn next(&mut self) -> Option<Self::Item> {
        // cursor_fwd starts out pointing to the first element
        // and therefore it yields an element before incrementing
        match &mut self.cursor_fwd {
            x if x == &self.cursor_rev || x == &self.mset.end() => None,
            c => {
                let item = crate::Match::new(c.clone());
                ffi::shim::mset_iterator_increment(c.as_mut());
                self.size.1 += 1;
                Some(item)
            }
        }
    }
}

impl DoubleEndedIterator for MSetIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        // cursor_rev starts out pointing after the last element
        // and therefore it decrements before it yields
        match &mut self.cursor_rev {
            x if x == &self.cursor_fwd || x == &self.mset.begin() => None,
            c => {
                ffi::shim::mset_iterator_decrement(c.as_mut());
                self.size.1 += 1;
                Some(crate::Match::new(c.clone()))
            }
        }
    }
}

#[derive(Clone)]
pub struct PositionIter {
    positions: std::vec::IntoIter<u32>,
}

impl PositionIter {
    pub(crate) fn new(term: &crate::Term) -> Self {
        // Back the iterator with the positions read eagerly when the term was
        // constructed. Reading the position list lazily (by iterating the
        // term iterator after the owning `TermIter` has advanced) associates
        // the wrong positions with the term and can over-run the list.
        Self {
            positions: term.positions_vec().into_iter(),
        }
    }
}

impl Iterator for PositionIter {
    type Item = crate::Position;

    fn next(&mut self) -> Option<Self::Item> {
        self.positions.next().map(Into::into)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.positions.size_hint()
    }
}

#[derive(Clone)]
pub struct SubqueryIter<'q> {
    parent: &'q ffi::Query,
    cursor: usize,
    length: usize,
}

impl<'q> SubqueryIter<'q> {
    pub fn new(query: &'q ffi::Query) -> SubqueryIter<'q> {
        Self {
            parent: query,
            cursor: 0,
            length: query.get_num_subqueries(),
        }
    }
}

impl Iterator for SubqueryIter<'_> {
    type Item = crate::Query;

    fn next(&mut self) -> Option<Self::Item> {
        (self.cursor < self.length).then(|| {
            let query = crate::Query::from_ffi(self.parent.get_subquery(self.cursor).within_box());
            self.cursor += 1;
            query
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.length - self.cursor;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for SubqueryIter<'_> {}

#[derive(Clone)]
pub struct TermIter {
    cursor: Pin<Box<ffi::TermIterator>>,
    end: Pin<Box<ffi::TermIterator>>,
}

impl TermIter {
    pub(crate) fn new(
        start: Pin<Box<ffi::TermIterator>>,
        end: Pin<Box<ffi::TermIterator>>,
    ) -> Self {
        Self { cursor: start, end }
    }
}

impl AsRef<ffi::TermIterator> for TermIter {
    fn as_ref(&self) -> &ffi::TermIterator {
        &self.cursor
    }
}

impl Iterator for TermIter {
    type Item = crate::Term;

    fn next(&mut self) -> Option<Self::Item> {
        match self.cursor == self.end {
            false => {
                let term = crate::Term::new(self.cursor.clone());
                ffi::shim::term_iterator_increment(self.cursor.as_mut());
                Some(term)
            }
            true => None,
        }
    }
}
