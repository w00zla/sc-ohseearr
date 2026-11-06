//! The terminal's texts the reader looks for, built from the game's `global.ini` by loc key (ADR 0007): no game
//! text is compiled in.

use sc_ohseearr_sc::{Locale, norm};

/// The loc keys of the texts the reader anchors on, in the order of `Labels`' fields.
pub const KEYS: [&str; 9] = [
    // The table head of a work order ("Materials Yielded (cSCU)")
    "refinery_ui_jobcard_table_materialsyielded",
    "refinery_ui_workordercard_title_processing",
    "refinery_ui_workordercard_title_completed",
    "refinery_ui_remainingtime",
    // Not confirmed as the terminal's own key (the column has no refinery key)
    "ui_quality",
    "refinery_ui_jobcard_table_yield",
    "refinery_ui_jobcard_table_todo",
    "refinery_ui_jobcard_table_done",
    // The terminal's title ("Refinement Center")
    "refinery_ui_refinementcenter",
];

/// The terminal's texts the OCR looks for, each normalized (`norm`) after cutting at a parenthesis (a trailing
/// "(cSCU)" is often a box of its own); the language pack's texts first, then the English table's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Labels {
    /// The table head of a work order (without the parenthesis)
    pub head: Vec<String>,
    pub processing: Vec<String>,
    pub completed: Vec<String>,
    pub time_remaining: Vec<String>,
    /// Columns
    pub quality: Vec<String>,
    pub yield_: Vec<String>,
    pub todo: Vec<String>,
    pub done: Vec<String>,
    /// The terminal's title ("Refinement Center")
    pub center: Vec<String>,
}

impl Labels {
    /// The labels of `locale`; `texts` are what it holds for the `KEYS`, in order.
    fn from_texts(texts: &[Vec<String>]) -> Self {
        let label = |i: usize| {
            let mut all: Vec<String> = Vec::new();
            for t in &texts[i] {
                let n = norm(t.split('(').next().unwrap_or(t));
                if !n.is_empty() && !all.contains(&n) {
                    all.push(n);
                }
            }
            all
        };
        Self {
            head: label(0),
            processing: label(1),
            completed: label(2),
            time_remaining: label(3),
            quality: label(4),
            yield_: label(5),
            todo: label(6),
            done: label(7),
            center: label(8),
        }
    }

    pub fn from_locale(locale: &Locale) -> Self {
        Self::from_texts(&texts_of(locale))
    }
}

fn texts_of(locale: &Locale) -> Vec<Vec<String>> {
    KEYS.iter().map(|k| locale.texts(k).into_iter().map(str::to_string).collect()).collect()
}

/// `Labels` kept while the locale's texts for the keys stay the same (a pointer would not do: a new locale may
/// reuse an address).
#[derive(Default)]
pub struct LabelCache {
    cached: Option<(Vec<Vec<String>>, Labels)>,
}

impl LabelCache {
    pub fn get(&mut self, locale: &Locale) -> &Labels {
        let texts = texts_of(locale);
        if self.cached.as_ref().is_none_or(|(t, _)| *t != texts) {
            let labels = Labels::from_texts(&texts);
            self.cached = Some((texts, labels));
        }
        &self.cached.as_ref().expect("set above").1
    }

    pub fn clear(&mut self) {
        self.cached = None;
    }
}
