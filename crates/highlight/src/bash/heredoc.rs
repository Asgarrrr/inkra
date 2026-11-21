//! Bash heredocs: the operator and its tag on a command line, the queue in the line state, and
//! the body lines up to each tag (code highlighting spec, D2).

use crate::TokenClass::{Operator, String};
use crate::scan::skip_spaces;
use crate::state::Heredoc;

use super::{Tokens, is_separator};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0100_0000_01b3;

/// FNV-1a, folded one byte at a time, so a tag hashes the same with or without its quotes.
fn fold(hash: u64, byte: u8) -> u64 {
    (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
}

fn tag_hash(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .fold(FNV_OFFSET, |hash, &byte| fold(hash, byte))
}

/// The tag word at `at`: its end, the hash of its text without quotes and backslashes, and
/// whether it had any. `None` when no word starts there.
fn tag(bytes: &[u8], at: usize) -> Option<(usize, u64, bool)> {
    let (mut end, mut hash, mut quoted) = (at, FNV_OFFSET, false);
    while let Some(&byte) = bytes.get(end) {
        match byte {
            b'\'' | b'"' => {
                quoted = true;
                let close = bytes[end + 1..]
                    .iter()
                    .position(|&other| other == byte)
                    .map_or(bytes.len(), |len| end + 1 + len);
                hash = bytes[end + 1..close]
                    .iter()
                    .fold(hash, |hash, &byte| fold(hash, byte));
                end = (close + 1).min(bytes.len());
            }
            b'\\' => {
                quoted = true;
                if let Some(&escaped) = bytes.get(end + 1) {
                    hash = fold(hash, escaped);
                }
                end = (end + 2).min(bytes.len());
            }
            byte if is_separator(byte) => break,
            _ => {
                hash = fold(hash, byte);
                end += 1;
            }
        }
    }
    (end > at).then_some((end, hash, quoted))
}

impl Tokens<'_> {
    /// `<<` or `<<-` at `at` and the tag after it; queues the heredoc while the queue has room.
    pub(super) fn heredoc(&mut self, at: usize) -> usize {
        let bytes = self.line.as_bytes();
        let strip_tabs = bytes.get(at + 2) == Some(&b'-');
        let op_end = at + 2 + usize::from(strip_tabs);
        self.sink.push(at..op_end, Operator);
        let start = skip_spaces(bytes, op_end);
        let Some((end, tag, quoted)) = tag(bytes, start) else {
            return op_end;
        };
        self.sink.push(start..end, String);
        if let Some(slot) = self.state.heredocs.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(Heredoc {
                tag,
                strip_tabs,
                expands: !quoted,
            });
        }
        end
    }

    /// A body line of `heredoc`, the first in the queue: the tag alone ends it.
    pub(super) fn body(&mut self, heredoc: Heredoc) {
        let text = if heredoc.strip_tabs {
            self.line.trim_start_matches('\t')
        } else {
            self.line
        };
        let from = self.line.len() - text.len();
        if tag_hash(text.as_bytes()) == heredoc.tag {
            self.sink.push(from..self.line.len(), String);
            self.state.heredocs = [self.state.heredocs[1], None];
        } else if heredoc.expands {
            self.expanding(from, from, None);
        } else {
            self.sink.push(from..self.line.len(), String);
        }
    }
}

#[cfg(test)]
mod tests;
