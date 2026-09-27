mod display;
mod front_matter;
mod links;
mod markup;
mod parse;
mod reveal;
mod styles;

pub use display::{DisplayLine, DisplayOffset};
pub use front_matter::front_matter;
pub use links::{Link, LinkKind, link_at, links};
pub use markup::markup;
pub use parse::{Heading, Markdown, Span, parse};
pub use reveal::{Conceal, is_active, touched};
pub use styles::Styles;
