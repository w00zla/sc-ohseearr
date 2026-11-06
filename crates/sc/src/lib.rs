//! The Star Citizen layer: what every screen reader of the game needs, whatever the screen.
//!
//! - `locale`: the texts the game shows come from its `global.ini` (a language pack, else the English table
//!   extracted from the game); readers name their anchors by loc key and resolve read names to loc keys.
//! - `ui`: the game's UI scales with the frame's height; screens drawn as a panel around the frame's centre.
//! - `parse`: the game's ways of writing values (qualities, SCU amounts, stack counts, durations).

pub mod locale;
pub mod parse;
pub mod ui;

pub use locale::{LocTable, Locale, LocaleError, LocaleSource, norm};
pub use ui::CenterUi;
