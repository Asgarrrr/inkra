//! What the backlinks panel holds of one source's read.

use crate::context::{LinkContext, link_context};
use crate::{Backlink, Read};

/// One source's contexts, or none when its read does not prove the links' ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slot {
    /// One context per occurrence, in document order.
    Read {
        contexts: Vec<LinkContext>,
    },
    NoContext,
}

impl Slot {
    /// `Read` only for a text at the source's stamp that holds every occurrence's range.
    #[must_use]
    pub fn land(backlink: &Backlink, read: &Read) -> Self {
        let Read::Text { text, stamp } = read else {
            return Self::NoContext;
        };
        if *stamp != backlink.candidate().stamp {
            return Self::NoContext;
        }
        backlink
            .occurrences()
            .map(|link| link_context(text, link.range.clone()))
            .collect::<Option<Vec<_>>>()
            .map_or(Self::NoContext, |contexts| Self::Read { contexts })
    }
}
