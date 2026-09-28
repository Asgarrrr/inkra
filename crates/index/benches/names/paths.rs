//! Path generators shared by the `names` and `find` benches.

const WORDS: &[&str] = &[
    "projet",
    "réunion",
    "été",
    "notes",
    "plan",
    "journal",
    "idée",
    "café",
    "budget",
    "équipe",
    "archive",
    "lecture",
    "voyage",
    "santé",
    "recherche",
    "daily",
    "meeting",
    "design",
    "draft",
    "ideas",
    "review",
    "roadmap",
    "inbox",
    "reference",
];

/// A fixed LCG, so every run benches the same vault.
pub(crate) struct Lcg(pub(crate) u64);

impl Lcg {
    pub(crate) fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        usize::try_from(self.0 >> 33).unwrap_or(0) % below
    }
}

pub(crate) fn folders(lcg: &mut Lcg, depth: usize) -> String {
    (0..depth)
        .map(|_| WORDS[lcg.next(WORDS.len())])
        .collect::<Vec<_>>()
        .join("/")
}
