/// The lines a leading front matter block spans, fences included, as pulldown-cmark 0.13 reads it;
/// reads no line past the closing fence.
#[must_use]
pub fn front_matter<'a>(lines: impl IntoIterator<Item = &'a str>) -> Option<usize> {
    let mut lines = lines.into_iter();
    let open = lines.next()?.strip_prefix("---")?;
    if !open.bytes().all(|byte| byte.is_ascii_whitespace()) {
        return None;
    }
    let first = lines.next()?;
    if is_blank(first) || closes(first) {
        return None;
    }
    lines.position(closes).map(|at| at + 3)
}

fn closes(line: &str) -> bool {
    line.strip_prefix("---")
        .or_else(|| line.strip_prefix("..."))
        .is_some_and(|rest| ends(rest.trim_start_matches(' ')))
}

fn is_blank(line: &str) -> bool {
    ends(line.trim_start_matches([' ', '\t', '\x0b', '\x0c']))
}

/// pulldown-cmark ends a line at a `\r` too.
fn ends(rest: &str) -> bool {
    rest.is_empty() || rest.starts_with('\r')
}

#[cfg(test)]
mod tests;
