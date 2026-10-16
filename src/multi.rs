//! The multi-send panel: a flat pool of preset commands with optional notes.
//!
//! The pool holds [`TOTAL_ENTRIES`] globally numbered slots.  The panel browses
//! them a page at a time, where the page size follows the height the table has
//! available, so the number of pages is derived rather than fixed.  Each entry
//! may be sent with a newline and/or as hex, carry a delay applied after it,
//! and be included when looping; the first ten of the current page can be bound
//! to the numeric keypad.

use serde::{Deserialize, Serialize};

/// Total number of slots in the send pool.
///
/// Fixed: the settings file cannot change it, and the page count is derived by
/// dividing it by whatever fits on a page.
pub const TOTAL_ENTRIES: usize = 100;

/// Page size assumed before the panel has measured itself.
const DEFAULT_PAGE_SIZE: usize = 10;

/// Sparse storage for the pool: trailing empty entries are not written.
mod entries_serde {
    use super::{MultiEntry, TOTAL_ENTRIES};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(
        entries: &[MultiEntry],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let used = entries
            .iter()
            .rposition(|entry| *entry != MultiEntry::default())
            .map_or(0, |index| index + 1);
        entries[..used].serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<MultiEntry>, D::Error> {
        let mut entries = Vec::<MultiEntry>::deserialize(deserializer)?;
        entries.resize(TOTAL_ENTRIES, MultiEntry::default());
        Ok(entries)
    }
}

/// A fresh, empty pool.
fn default_entries() -> Vec<MultiEntry> {
    vec![MultiEntry::default(); TOTAL_ENTRIES]
}

/// One preset command.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MultiEntry {
    /// The command text (hex form allowed when `hex` is set).
    pub text: String,
    /// Interpret `text` as hexadecimal bytes.
    pub hex: bool,
    /// Append CR/LF after the payload.
    pub newline: bool,
    /// Free-form description shown next to the entry.
    pub note: String,
    /// Delay applied *after* this entry is sent, before the next one, in
    /// milliseconds.  Lets a sequence give a device time to answer.
    pub delay_ms: u64,
    /// Include this entry when looping.
    pub enabled: bool,
}

impl MultiEntry {
    /// Whether this entry has anything to send.
    pub fn is_sendable(&self) -> bool {
        !self.text.trim().is_empty()
    }
}

/// The whole multi-send panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MultiSend {
    /// The pool, always [`TOTAL_ENTRIES`] long after [`MultiSend::normalize`].
    #[serde(default = "default_entries", with = "entries_serde")]
    pub entries: Vec<MultiEntry>,
    /// Currently displayed page.
    ///
    /// A view state, so it is not persisted: the page count follows the window
    /// height, and a remembered page number would not mean the same thing next
    /// time.
    #[serde(skip)]
    pub page: usize,
    /// How many rows fit on a page.  The panel measures this every frame.
    #[serde(skip)]
    pub page_size: usize,
    /// Loop through all enabled entries.
    ///
    /// This is a *run* state, not a preference, so it is deliberately not
    /// persisted: a ticked box that outlives a restart — or a port close —
    /// reads as "still looping" while nothing is running.
    #[serde(skip)]
    pub loop_enabled: bool,
    /// How many full passes the loop makes.  `None` means loop forever.
    pub loop_count: Option<u32>,
    /// Bind the current page's first ten entries to the numeric keypad.
    pub keypad_enabled: bool,
    /// Append CR/LF to entries that request it (CRLF by default).
    pub newline_cr: bool,
    pub newline_lf: bool,
}

impl Default for MultiSend {
    fn default() -> Self {
        Self {
            entries: default_entries(),
            page: 0,
            page_size: DEFAULT_PAGE_SIZE,
            loop_enabled: false,
            loop_count: None,
            keypad_enabled: false,
            newline_cr: true,
            newline_lf: true,
        }
    }
}

impl MultiSend {
    /// Ensure the pool has the expected shape after loading old settings.
    pub fn normalize(&mut self) {
        self.entries.resize(TOTAL_ENTRIES, MultiEntry::default());
        for entry in &mut self.entries {
            entry.delay_ms = entry.delay_ms.min(3_600_000);
        }
        // Zero passes would look like a loop that silently does nothing.
        self.loop_count = self.loop_count.map(|n| n.clamp(1, 1_000_000));
        self.page_size = self.page_size.max(1);
        self.clamp_page();
    }

    /// Borrow one entry by its global index.
    pub fn entry(&self, index: usize) -> Option<&MultiEntry> {
        self.entries.get(index)
    }

    /// Mutably borrow one entry by its global index.
    pub fn entry_mut(&mut self, index: usize) -> Option<&mut MultiEntry> {
        self.entries.get_mut(index)
    }

    /// The delay to apply after sending `index`.
    pub fn delay_after(&self, index: usize) -> u64 {
        self.entry(index).map_or(0, |entry| entry.delay_ms)
    }

    /// How many entries fit on one page.
    pub fn page_size(&self) -> usize {
        self.page_size.max(1)
    }

    /// The number of pages the pool spans at the current page size.
    pub fn page_count(&self) -> usize {
        TOTAL_ENTRIES.div_ceil(self.page_size())
    }

    /// The global indices shown on the current page.
    pub fn page_range(&self) -> std::ops::Range<usize> {
        let start = self.page * self.page_size();
        let end = (start + self.page_size()).min(TOTAL_ENTRIES);
        start..end
    }

    /// Keep the current page inside the range the window now allows.
    pub fn clamp_page(&mut self) {
        self.page = self.page.min(self.page_count() - 1);
    }

    pub fn first_page(&mut self) {
        self.page = 0;
    }

    pub fn previous_page(&mut self) {
        self.page = self.page.saturating_sub(1);
    }

    pub fn next_page(&mut self) {
        self.page = (self.page + 1).min(self.page_count() - 1);
    }

    pub fn last_page(&mut self) {
        self.page = self.page_count() - 1;
    }

    /// The newline suffix for multi-send entries, in byte form.
    pub fn newline_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(2);
        if self.newline_cr {
            out.push(b'\r');
        }
        if self.newline_lf {
            out.push(b'\n');
        }
        out
    }

    /// Encode one entry for transmission.
    ///
    /// Returns `Err` with a user-facing message when a hex entry is malformed,
    /// which is the one case the original tool also refuses to send.
    pub fn encode_entry(&self, entry: &MultiEntry) -> Result<Vec<u8>, String> {
        let mut bytes = if entry.hex {
            crate::hex::parse(&entry.text).map_err(|e| e.to_string())?
        } else {
            entry.text.as_bytes().to_vec()
        };
        if entry.newline {
            bytes.extend_from_slice(&self.newline_bytes());
        }
        Ok(bytes)
    }

    /// Global indices of the checked, non-empty entries, in order.
    ///
    /// Drives both the loop and the one-shot "send checked" action, and spans
    /// the whole pool rather than the current page.
    pub fn enabled_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.enabled && e.is_sendable())
            .map(|(i, _)| i)
            .collect()
    }

    /// Serialise the pool to a portable JSON document.
    pub fn export_json(&self) -> Result<String, String> {
        #[derive(Serialize)]
        struct Doc<'a> {
            version: u32,
            entries: &'a [MultiEntry],
        }
        let used = self
            .entries
            .iter()
            .rposition(|entry| *entry != MultiEntry::default())
            .map_or(0, |index| index + 1);
        serde_json::to_string_pretty(&Doc {
            version: 2,
            entries: &self.entries[..used],
        })
        .map_err(|e| e.to_string())
    }

    /// Load entries from a JSON document.
    ///
    /// Accepts both the current `entries` shape and the legacy `pages` shape,
    /// so documents exported before the flat pool still import.
    pub fn import_json(&mut self, text: &str) -> Result<usize, String> {
        #[derive(Deserialize)]
        struct Doc {
            #[serde(default)]
            entries: Vec<MultiEntry>,
            #[serde(default)]
            pages: Vec<Vec<MultiEntry>>,
        }
        let doc: Doc =
            serde_json::from_str(text).map_err(|e| crate::i18n::tr1("解析失败: {}", e))?;
        let imported = if doc.entries.is_empty() {
            doc.pages.into_iter().flatten().collect()
        } else {
            doc.entries
        };
        if imported.is_empty() {
            return Err(crate::i18n::tr("文件中没有条目").to_owned());
        }
        let count = imported.iter().filter(|entry| entry.is_sendable()).count();
        self.entries = imported;
        self.normalize();
        self.page = 0;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pool_holds_the_fixed_slot_count() {
        let m = MultiSend::default();
        assert_eq!(m.entries.len(), TOTAL_ENTRIES);
    }

    #[test]
    fn paging_clamps_at_both_ends() {
        let mut m = MultiSend {
            page_size: 10,
            ..Default::default()
        };
        m.previous_page();
        assert_eq!(m.page, 0);
        m.first_page();
        assert_eq!(m.page, 0);
        for _ in 0..50 {
            m.next_page();
        }
        assert_eq!(m.page, m.page_count() - 1);
        m.last_page();
        assert_eq!(m.page, m.page_count() - 1);
    }

    #[test]
    fn page_count_divides_the_pool_rounding_up() {
        let m = MultiSend {
            page_size: 25,
            ..Default::default()
        };
        assert_eq!(m.page_count(), 4, "100 / 25");
        let m = MultiSend {
            page_size: 30,
            ..Default::default()
        };
        assert_eq!(m.page_count(), 4, "the last page is partial");
        let m = MultiSend {
            page_size: 200,
            ..Default::default()
        };
        assert_eq!(m.page_count(), 1);
    }

    #[test]
    fn the_page_range_stays_inside_the_pool() {
        let m = MultiSend {
            page: 3,
            page_size: 30,
            ..Default::default()
        };
        assert_eq!(m.page_range(), 90..100);
    }

    #[test]
    fn encodes_text_entry_with_newline() {
        let m = MultiSend::default();
        let entry = MultiEntry {
            text: "AT".to_owned(),
            newline: true,
            ..Default::default()
        };
        assert_eq!(m.encode_entry(&entry).unwrap(), b"AT\r\n");
    }

    #[test]
    fn encodes_hex_entry() {
        let m = MultiSend::default();
        let entry = MultiEntry {
            text: "01 02 0A".to_owned(),
            hex: true,
            newline: false,
            ..Default::default()
        };
        assert_eq!(m.encode_entry(&entry).unwrap(), vec![0x01, 0x02, 0x0A]);
    }

    #[test]
    fn rejects_malformed_hex_entry() {
        let m = MultiSend::default();
        let entry = MultiEntry {
            text: "0G".to_owned(),
            hex: true,
            ..Default::default()
        };
        assert!(m.encode_entry(&entry).is_err());
    }

    #[test]
    fn enabled_indices_span_the_whole_pool() {
        let mut m = MultiSend::default();
        for (index, (text, enabled)) in [("a", true), ("b", false), ("", true), ("c", true)]
            .into_iter()
            .enumerate()
        {
            let entry = m.entry_mut(index).expect("entry exists");
            entry.text = text.to_owned();
            entry.enabled = enabled;
        }
        // And one far away, to prove the pool is not page-limited.
        let far = m.entry_mut(TOTAL_ENTRIES - 1).expect("entry exists");
        far.text = "z".to_owned();
        far.enabled = true;
        assert_eq!(m.enabled_indices(), vec![0, 3, TOTAL_ENTRIES - 1]);
    }

    #[test]
    fn json_round_trip_preserves_entries() {
        let mut m = MultiSend::default();
        *m.entry_mut(3).expect("entry exists") = MultiEntry {
            text: "0A0B".to_owned(),
            hex: true,
            newline: true,
            note: "备注".to_owned(),
            delay_ms: 0,
            enabled: true,
        };
        let json = m.export_json().unwrap();
        let mut loaded = MultiSend::default();
        let count = loaded.import_json(&json).unwrap();
        assert_eq!(count, 1, "only the one non-empty entry is counted");
        assert_eq!(loaded.entries[3], m.entries[3]);
    }

    #[test]
    fn storage_is_sparse_and_padded_back_on_load() {
        let mut m = MultiSend::default();
        m.entry_mut(3).expect("entry").text = "A".to_owned();
        let json = serde_json::to_string(&m).unwrap();
        let back: MultiSend = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries.len(), TOTAL_ENTRIES, "padded back to full");
        assert_eq!(back.entries[3].text, "A");

        // A default pool writes no trailing empties at all.
        let empty = serde_json::to_string(&MultiSend::default()).unwrap();
        assert!(
            empty.contains("\"entries\":[]"),
            "an empty pool must not write a hundred blank entries: {empty}"
        );
    }

    /// Run states are not written and cannot be resurrected from an old file.
    #[test]
    fn run_states_are_not_persisted() {
        let m = MultiSend {
            loop_enabled: true,
            page: 4,
            ..Default::default()
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(
            !json.contains("loop_enabled") && !json.contains("\"page\""),
            "run/view states must not be written: {json}"
        );

        let back: MultiSend = serde_json::from_str(&json).unwrap();
        assert!(!back.loop_enabled);
        assert_eq!(back.page, 0);

        // A file written by an older build still carries the field; it must not
        // resurrect a stale tick either.
        let legacy = json.replace("\"entries\"", "\"loop_enabled\":true,\"entries\"");
        let back: MultiSend = serde_json::from_str(&legacy).unwrap();
        assert!(!back.loop_enabled, "an old file must not restore a tick");
    }

    #[test]
    fn import_accepts_the_legacy_page_shape() {
        let json = r#"{"version":1,"pages":[[{"text":"a"}],[{"text":"b"},{"text":"c"}]]}"#;
        let mut m = MultiSend::default();
        let count = m.import_json(json).unwrap();
        assert_eq!(count, 3);
        assert_eq!(m.entries[0].text, "a");
        assert_eq!(m.entries[1].text, "b");
        assert_eq!(m.entries[2].text, "c");
        assert_eq!(m.entries.len(), TOTAL_ENTRIES);
    }

    #[test]
    fn import_rejects_junk() {
        let mut m = MultiSend::default();
        assert!(m.import_json("not json").is_err());
        assert!(m.import_json("{}").is_err());
    }

    #[test]
    fn normalize_repairs_a_short_or_oversized_pool() {
        let mut m = MultiSend {
            entries: vec![MultiEntry::default(); 2],
            page: 99,
            page_size: 0,
            ..Default::default()
        };
        m.normalize();
        assert_eq!(m.entries.len(), TOTAL_ENTRIES);
        assert_eq!(m.page_size(), 1);
        assert_eq!(m.page, TOTAL_ENTRIES - 1);

        m.entries = vec![MultiEntry::default(); TOTAL_ENTRIES + 50];
        m.normalize();
        assert_eq!(m.entries.len(), TOTAL_ENTRIES);
    }

    #[test]
    fn per_entry_delay_defaults_to_zero_and_is_reported() {
        let mut m = MultiSend::default();
        assert_eq!(m.delay_after(0), 0);
        m.entry_mut(3).expect("entry").delay_ms = 250;
        assert_eq!(m.delay_after(3), 250);
    }

    #[test]
    fn loop_count_defaults_to_infinite_and_is_clamped() {
        assert_eq!(MultiSend::default().loop_count, None, "infinite by default");

        let mut m = MultiSend {
            loop_count: Some(0),
            ..Default::default()
        };
        m.normalize();
        assert_eq!(m.loop_count, Some(1), "zero would loop never");

        m.loop_count = Some(u32::MAX);
        m.normalize();
        assert_eq!(m.loop_count, Some(1_000_000));
    }

    #[test]
    fn newline_bytes_follow_flags() {
        let mut m = MultiSend {
            newline_cr: false,
            newline_lf: true,
            ..Default::default()
        };
        assert_eq!(m.newline_bytes(), vec![b'\n']);
        m.newline_cr = true;
        m.newline_lf = false;
        assert_eq!(m.newline_bytes(), vec![b'\r']);
        m.newline_cr = false;
        assert!(m.newline_bytes().is_empty());
    }
}
