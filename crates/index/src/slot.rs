//! What the backlinks panel holds of one source's read.

use inkra_vault::Stamp;

use crate::context::{LinkContext, link_context};
use crate::{Backlink, READ_FAILURES, Read};

/// One source's contexts, or none when its read does not prove the links' ranges; either
/// holds only for the backlink it was read for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slot {
    /// One context per occurrence, in document order.
    Read {
        of: Backlink,
        contexts: Vec<LinkContext>,
    },
    NoContext {
        of: Backlink,
    },
    /// A read that failed holds while the source keeps its stamp, whatever its slice.
    Failed {
        stamp: Stamp,
    },
}

/// A read landed in a slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landed {
    pub slot: Slot,
    /// Whether the source reloads: once per stamp, so `NoContext` at a stamp asks only once.
    pub stale: bool,
    /// Whether the text cache drops the source's text at its stamp, as it disproved a range.
    pub forget: bool,
}

impl Slot {
    /// `previous` is the source's slot, valid or not: its stamp outlives a reload's new slice.
    #[must_use]
    pub fn land(previous: Option<&Self>, backlink: &Backlink, read: &Read) -> Landed {
        let slot = Self::of_read(backlink, read);
        let stamp = backlink.candidate().stamp;
        let no_context = matches!(slot, Self::NoContext { .. });
        let stale = no_context
            && !matches!(previous, Some(Self::NoContext { of }) if of.candidate().stamp == stamp);
        let forget = no_context && matches!(read, Read::Text { stamp: read, .. } if *read == stamp);
        Landed {
            slot,
            stale,
            forget,
        }
    }

    /// `Read` only for a text at the source's stamp that holds every occurrence's range.
    fn of_read(backlink: &Backlink, read: &Read) -> Self {
        let of = backlink.clone();
        let stamp = backlink.candidate().stamp;
        let text = match read {
            Read::Text { text, stamp: read } if *read == stamp => text,
            Read::Failed => return Self::Failed { stamp },
            Read::Text { .. } | Read::Gone | Read::NotUtf8 => return Self::NoContext { of },
        };
        match backlink
            .occurrences()
            .map(|link| link_context(text, link.range.clone()))
            .collect::<Option<Vec<_>>>()
        {
            Some(contexts) => Self::Read { of, contexts },
            None => Self::NoContext { of },
        }
    }

    /// Whether this slot was read for a backlink that `backlink` reads as.
    #[must_use]
    pub fn is_valid(&self, backlink: &Backlink) -> bool {
        match self {
            Self::Read { of, .. } | Self::NoContext { of } => of.reads_as(backlink),
            Self::Failed { stamp } => *stamp == backlink.candidate().stamp,
        }
    }
}

/// The failed reads of one panel's job; it stops at `READ_FAILURES`.
#[derive(Debug, Default)]
pub struct Failures(usize);

impl Failures {
    /// Counts `slot` if it failed; whether the job has failed enough to stop.
    #[must_use = "the job stops on true"]
    pub fn count(&mut self, slot: &Slot) -> bool {
        if matches!(slot, Slot::Failed { .. }) {
            self.0 += 1;
        }
        self.0 >= READ_FAILURES
    }
}
