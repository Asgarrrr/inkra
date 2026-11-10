//! What the backlinks panel holds of one source's read.

use crate::context::{LinkContext, link_context};
use crate::{Backlink, Read};

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
}

/// A read landed in a slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landed {
    pub slot: Slot,
    /// Whether the source reloads: once per stamp, so `NoContext` at a stamp asks only once.
    pub stale: bool,
}

impl Slot {
    /// `previous` is the source's slot, valid or not: its stamp outlives a reload's new slice.
    #[must_use]
    pub fn land(previous: Option<&Self>, backlink: &Backlink, read: &Read) -> Landed {
        let slot = Self::of_read(backlink, read);
        let stamp = backlink.candidate().stamp;
        let stale = matches!(slot, Self::NoContext { .. })
            && !matches!(previous, Some(Self::NoContext { of }) if of.candidate().stamp == stamp);
        Landed { slot, stale }
    }

    /// `Read` only for a text at the source's stamp that holds every occurrence's range.
    fn of_read(backlink: &Backlink, read: &Read) -> Self {
        let of = backlink.clone();
        let Read::Text { text, stamp } = read else {
            return Self::NoContext { of };
        };
        if *stamp != backlink.candidate().stamp {
            return Self::NoContext { of };
        }
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
        let (Self::Read { of, .. } | Self::NoContext { of }) = self;
        of.reads_as(backlink)
    }
}
