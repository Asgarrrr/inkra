//! A list marker's kind, as the markers a command writes copy it (list editing spec, D2, D4).

use crate::items::marker_end;

/// A marker as a sibling copies it: a bullet's byte, or a number with its digit count and its
/// delimiter (D2, rule 6, D4).
#[derive(Clone, Copy)]
pub(super) enum Kind {
    Bullet(u8),
    Number {
        value: u64,
        digits: usize,
        delimiter: u8,
    },
}

impl Kind {
    /// The marker at `marker`.
    pub(super) fn at(text: &str, marker: usize) -> Self {
        let end = marker_end(text, marker) - 1;
        let byte = text.as_bytes()[end];
        if end == marker {
            return Self::Bullet(byte);
        }
        #[expect(
            clippy::expect_used,
            reason = "CommonMark caps a list number at 9 digits"
        )]
        let value = text[marker..end]
            .parse::<u64>()
            .expect("a list number fits u64");
        Self::Number {
            value,
            digits: end - marker,
            delimiter: byte,
        }
    }

    /// A number list's first marker, `1` and `delimiter`.
    pub(super) fn first(delimiter: u8) -> Self {
        Self::Number {
            value: 1,
            digits: 1,
            delimiter,
        }
    }

    /// The marker `step` siblings past it, a number zero-padded to its digit count.
    pub(super) fn nth(self, step: u64) -> String {
        match self {
            Self::Bullet(byte) => char::from(byte).to_string(),
            Self::Number {
                value,
                digits,
                delimiter,
            } => format!("{:0digits$}{}", value + step, char::from(delimiter)),
        }
    }
}
