//! The game's texts in the player's language.
//!
//! `user.cfg`'s `g_language` picks `data/Localization/<language>/global.ini` in the channel folder (LIVE, PTU, …).
//! A language pack lives there: a complete `global.ini`, a translation or one rewording the English texts (like
//! StarStrings). Without one the game uses its built-in English texts, which live inside `Data.p4k`, not on disk:
//! extract them once per game build with `scripts/extract-global-ini.{ps1,sh}` (ADR 0007) and pass the file as
//! the English table.
//!
//! No text of the game is compiled in: readers name what they look for by loc key, `Locale::texts` gives the
//! texts the player can see for it. Keys are case-insensitive; a pack may mark its entries with a suffix
//! (`key,P=value`), which is dropped.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Instant, SystemTime};

/// Letters and digits only, upper case: how readers compare what the OCR read with the game's texts (spacing,
/// punctuation and case vary in the read).
pub fn norm(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_uppercase).collect()
}

/// The `g_language` of the game folder's `user.cfg`; None when it sets none (the game's English).
pub fn language(game_dir: &Path) -> Option<String> {
    std::fs::read_to_string(game_dir.join("user.cfg")).ok().and_then(|cfg| language_of(&cfg))
}

/// `g_language` of a `user.cfg` ("g_language = german_(germany)").
fn language_of(cfg: &str) -> Option<String> {
    cfg.lines().find_map(|line| {
        let line = line.split(';').next()?.trim();
        let (key, value) = line.split_once('=')?;
        (key.trim().eq_ignore_ascii_case("g_language") && !value.trim().is_empty()).then(|| value.trim().to_string())
    })
}

/// Where the game reads a language pack from (it may not exist: no pack installed).
pub fn pack_path(game_dir: &Path) -> PathBuf {
    let language = language(game_dir).unwrap_or_else(|| "english".into());
    // On Linux (Wine) the case matters: the game's folder is "data", some installs have "Data"
    let dir =
        ["data", "Data"].iter().map(|d| game_dir.join(d)).find(|d| d.is_dir()).unwrap_or_else(|| game_dir.join("data"));
    dir.join("Localization").join(language).join("global.ini")
}

/// One `global.ini`: loc key (lower case) → text (`\n` as a space).
#[derive(Debug, Clone, Default)]
pub struct LocTable {
    values: HashMap<String, String>,
}

impl LocTable {
    pub fn parse(text: &str) -> Self {
        let mut values = HashMap::new();
        for line in text.lines() {
            let Some((key, value)) = line.trim_start_matches('\u{feff}').split_once('=') else { continue };
            let key = key.split(',').next().unwrap_or(key).trim().to_ascii_lowercase();
            if !key.is_empty() {
                values.insert(key, value.trim().replace("\\n", " "));
            }
        }
        Self { values }
    }

    /// The file, read as UTF-8 (invalid bytes replaced).
    pub fn load(path: &Path) -> std::io::Result<Self> {
        Ok(Self::parse(&String::from_utf8_lossy(&std::fs::read(path)?)))
    }

    /// The text of `key` (any case).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(&key.to_ascii_lowercase()).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocaleError {
    /// Neither a language pack nor an English table: the game's texts are unknown
    NoLanguageTable,
    /// A table could not be read
    Io(PathBuf, String),
}

impl fmt::Display for LocaleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoLanguageTable => write!(f, "no language table: no language pack and no English global.ini given"),
            Self::Io(path, e) => write!(f, "{}: {e}", path.display()),
        }
    }
}

impl std::error::Error for LocaleError {}

impl From<LocaleError> for String {
    fn from(e: LocaleError) -> Self {
        e.to_string()
    }
}

/// The texts the player can see: the language pack's and the English table's (either may be missing, not both).
#[derive(Debug)]
pub struct Locale {
    pack: Option<LocTable>,
    english: Option<LocTable>,
    // Normalized text → keys, over both tables (made on first use: ~100k entries)
    index: OnceLock<HashMap<String, Vec<String>>>,
}

impl Locale {
    pub fn new(pack: Option<LocTable>, english: Option<LocTable>) -> Result<Self, LocaleError> {
        if pack.is_none() && english.is_none() {
            return Err(LocaleError::NoLanguageTable);
        }
        Ok(Self { pack, english, index: OnceLock::new() })
    }

    /// The game folder's language pack (if any) and the English table at `english` (if given and present).
    pub fn from_game_dir(game_dir: &Path, english: Option<&Path>) -> Result<Self, LocaleError> {
        let load = |path: &Path| -> Result<Option<LocTable>, LocaleError> {
            if !path.is_file() {
                return Ok(None);
            }
            LocTable::load(path).map(Some).map_err(|e| LocaleError::Io(path.to_path_buf(), e.to_string()))
        };
        let pack = load(&pack_path(game_dir))?;
        let english = match english {
            Some(path) => load(path)?,
            None => None,
        };
        Self::new(pack, english)
    }

    pub fn has_pack(&self) -> bool {
        self.pack.is_some()
    }

    pub fn has_english(&self) -> bool {
        self.english.is_some()
    }

    /// The texts of `key` the player may see: the pack's first, then the English one (when it differs). Empty
    /// when neither table has the key.
    pub fn texts(&self, key: &str) -> Vec<&str> {
        let mut all: Vec<&str> = Vec::new();
        for t in [&self.pack, &self.english].into_iter().flatten() {
            if let Some(v) = t.get(key)
                && !all.contains(&v)
            {
                all.push(v);
            }
        }
        all
    }

    /// The keys whose text reads like `text` (compared by `norm`), in either table; key order unspecified. A
    /// read name ("Mil/3/A Blizzard") resolves to its game identifiers this way.
    pub fn keys_for(&self, text: &str) -> Vec<&str> {
        let index = self.index.get_or_init(|| {
            let mut index: HashMap<String, Vec<String>> = HashMap::new();
            for t in [&self.pack, &self.english].into_iter().flatten() {
                for (key, value) in &t.values {
                    let keys = index.entry(norm(value)).or_default();
                    if !keys.contains(key) {
                        keys.push(key.clone());
                    }
                }
            }
            index
        });
        index.get(&norm(text)).map(|keys| keys.iter().map(String::as_str).collect()).unwrap_or_default()
    }
}

/// A `Locale` kept current for a game folder: read again when `user.cfg`, the pack or the English table changed
/// (checked at most every `check_secs`).
pub struct LocaleSource {
    locale: Option<Result<Locale, LocaleError>>,
    stamp: Option<Stamp>,
    checked: Option<Instant>,
    pub check_secs: u64,
}

type Stamp = (PathBuf, Option<SystemTime>, Option<SystemTime>, Option<PathBuf>, Option<SystemTime>);

impl Default for LocaleSource {
    fn default() -> Self {
        Self { locale: None, stamp: None, checked: None, check_secs: 10 }
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl LocaleSource {
    /// The locale of `game_dir` with the English table at `english`.
    pub fn get(&mut self, game_dir: &Path, english: Option<&Path>) -> Result<&Locale, LocaleError> {
        let due = !self.checked.is_some_and(|t| t.elapsed().as_secs() < self.check_secs);
        if due || self.locale.is_none() {
            self.checked = Some(Instant::now());
            let pack = pack_path(game_dir);
            let stamp = (
                pack.clone(),
                modified(&game_dir.join("user.cfg")),
                modified(&pack),
                english.map(Path::to_path_buf),
                english.and_then(modified),
            );
            if self.stamp.as_ref() != Some(&stamp) || self.locale.is_none() {
                let locale = Locale::from_game_dir(game_dir, english);
                match &locale {
                    Ok(l) => log::info!(
                        "language tables: pack {} ({}), English {}",
                        pack.display(),
                        if l.has_pack() { "found" } else { "none" },
                        if l.has_english() { "found" } else { "none" }
                    ),
                    Err(e) => log::warn!("language tables: {e}"),
                }
                self.locale = Some(locale);
                self.stamp = Some(stamp);
            }
        }
        match self.locale.as_ref().expect("set above") {
            Ok(l) => Ok(l),
            Err(e) => Err(e.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_from_user_cfg() {
        assert_eq!(
            language_of("r_DisplayInfo = 0\ng_language = german_(germany)\n").as_deref(),
            Some("german_(germany)")
        );
        assert_eq!(language_of("G_LANGUAGE=english ; comment").as_deref(), Some("english"));
        assert_eq!(language_of("g_languageAudio = english").as_deref(), None);
        assert_eq!(language_of(""), None);
    }

    #[test]
    fn table_with_pack_suffix_any_case_and_bom() {
        let t = LocTable::parse(
            "\u{feff}refinery_ui_RemainingTime=Verbleibende Zeit\nitems_commodities_raw_silicon,P=Silicon (Raw)\nx=a\\nb\n",
        );
        assert_eq!(t.get("refinery_ui_remainingtime"), Some("Verbleibende Zeit"));
        assert_eq!(t.get("ITEMS_COMMODITIES_RAW_SILICON"), Some("Silicon (Raw)"));
        assert_eq!(t.get("x"), Some("a b"));
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn texts_pack_first_then_english() {
        let pack = LocTable::parse("a=Gewonnene Materialien\nb=Completed\n");
        let english = LocTable::parse("a=Materials Yielded\nb=Completed\nc=Time Remaining\n");
        let l = Locale::new(Some(pack), Some(english)).unwrap();
        assert_eq!(l.texts("a"), ["Gewonnene Materialien", "Materials Yielded"]);
        assert_eq!(l.texts("b"), ["Completed"]);
        assert_eq!(l.texts("c"), ["Time Remaining"]);
        assert!(l.texts("d").is_empty());
    }

    #[test]
    fn no_table_is_an_error() {
        assert_eq!(Locale::new(None, None).unwrap_err(), LocaleError::NoLanguageTable);
    }

    #[test]
    fn keys_for_a_read_name() {
        let english =
            LocTable::parse("item_name_a=Mil/3/A Blizzard\nitem_name_b=Blizzard\nitem_desc=Mil 3 A Blizzard\n");
        let l = Locale::new(None, Some(english)).unwrap();
        let mut keys = l.keys_for("MIL/3/A  blizzard");
        keys.sort();
        assert_eq!(keys, ["item_desc", "item_name_a"]);
        assert!(l.keys_for("Glacier").is_empty());
    }
}
