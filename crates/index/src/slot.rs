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

impl Slot {
    /// `Read` only for a text at the source's stamp that holds every occurrence's range.
    #[must_use]
    pub fn land(backlink: &Backlink, read: &Read) -> Self {
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
