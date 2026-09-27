//! Path generators shared by the `names` and `find` benches.

const KINDS: &[&str] = &["Meeting", "Call", "Lunch", "Review", "Workshop"];
const PEOPLE: &[&str] = &[
    "Hélène", "Marc", "Zoé", "Jérôme", "Anaïs", "Paul", "Léa", "Thomas",
];
const TOPICS: &[&str] = &[
    "the budget",
    "the roadmap",
    "l'été",
    "la réunion",
    "the design",
    "l'équipe",
    "la santé",
    "the trip",
];
pub(crate) const WORDS: &[&str] = &[
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

/// About 56 chars: two or three folders, then "Meeting with Zoé about the roadmap 12".
pub(crate) fn long_stem(lcg: &mut Lcg, note: usize) -> String {
    let depth = 2 + lcg.next(2);
    let folders = folders(lcg, depth);
    let kind = KINDS[lcg.next(KINDS.len())];
    let person = PEOPLE[lcg.next(PEOPLE.len())];
    let topic = TOPICS[lcg.next(TOPICS.len())];
    format!("{folders}/{kind} with {person} about {topic} {note}.md")
}
