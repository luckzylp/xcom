//! The multi-send panel: an on-demand list of preset commands with notes.
//!
//! Entries are added by the user and can be removed or dragged into a new
//! order; there is no fixed slot count.  Each entry carries its own options: it
//! may be sent with a newline and/or as hex, carry a delay after it, and be
//! included when looping.  "Send checked" and looping span the whole list, and
//! the entries are numbered by their current position in it.

use serde::{Deserialize, Serialize};

/// Format version written by this build.
///
/// The list used to be a fixed 100-slot pool.  Files without a version predate
/// that change, so their entries are discarded on load: keeping 100 mostly
/// empty slots would defeat the on-demand model, and the slot order meant
/// nothing outside the old paging UI.
pub const FORMAT_VERSION: u32 = 2;

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
    /// Format marker; see [`FORMAT_VERSION`].
    pub version: u32,
    /// The entries, in display and send order.
    pub entries: Vec<MultiEntry>,
    /// Loop through all enabled entries.
    ///
    /// This is a *run* state, not a preference, so it is deliberately not
    /// persisted: a ticked box that outlives a restart — or a port close —
    /// reads as "still looping" while nothing is running.
    #[serde(skip)]
    pub loop_enabled: bool,
    /// How many full passes the loop makes.  `None` means loop forever.
    pub loop_count: Option<u32>,
    /// Append CR/LF to entries that request it (CRLF by default).
    pub newline_cr: bool,
    pub newline_lf: bool,
}

impl Default for MultiSend {
    fn default() -> Self {
        Self {
            version: FORMAT_VERSION,
            entries: Vec::new(),
            loop_enabled: false,
            loop_count: None,
            newline_cr: true,
            newline_lf: true,
        }
    }
}

impl MultiSend {
    /// Repair a loaded document and apply the on-demand-list migration.
    pub fn normalize(&mut self) {
        if self.version < FORMAT_VERSION {
            // Pre-on-demand files held a fixed pool of 100 slots; the blank
            // ones are noise and the filled positions were only meaningful in
            // the old paging UI. Start the new list empty as agreed.
            self.entries.clear();
            self.version = FORMAT_VERSION;
        }
        for entry in &mut self.entries {
            entry.delay_ms = entry.delay_ms.min(3_600_000);
        }
        // Zero passes would look like a loop that silently does nothing.
        self.loop_count = self.loop_count.map(|n| n.clamp(1, 1_000_000));
    }

    /// Append a blank entry and return its index.
    pub fn add_entry(&mut self) -> usize {
        self.entries.push(MultiEntry::default());
        self.entries.len() - 1
    }

    /// Remove the entry at `index`, if it exists.
    pub fn remove_entry(&mut self, index: usize) -> Option<MultiEntry> {
        (index < self.entries.len()).then(|| self.entries.remove(index))
    }

    /// Move `from` so it lands before the entry currently at `insertion`.
    ///
    /// `insertion` is an index into the list *before* the move, so dropping
    /// below the last entry is `len`.  Moving an entry onto itself, or just
    /// after itself, is a no-op.
    pub fn move_entry(&mut self, from: usize, insertion: usize) {
        if from >= self.entries.len() {
            return;
        }
        let mut target = insertion.min(self.entries.len());
        if target > from {
            target -= 1;
        }
        if target == from || target >= self.entries.len() {
            return;
        }
        let entry = self.entries.remove(from);
        self.entries.insert(target, entry);
    }

    /// Borrow one entry by its current index.
    pub fn entry(&self, index: usize) -> Option<&MultiEntry> {
        self.entries.get(index)
    }

    /// Mutably borrow one entry by its current index.
    pub fn entry_mut(&mut self, index: usize) -> Option<&mut MultiEntry> {
        self.entries.get_mut(index)
    }

    /// The delay to apply after sending `index`.
    pub fn delay_after(&self, index: usize) -> u64 {
        self.entry(index).map_or(0, |entry| entry.delay_ms)
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

    /// Indices of the checked, non-empty entries, in list order.
    ///
    /// Drives both the loop and the one-shot "send checked" action.
    pub fn enabled_indices(&self) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.enabled && e.is_sendable())
            .map(|(i, _)| i)
            .collect()
    }

    /// Serialise the list to a portable JSON document.
    pub fn export_json(&self) -> Result<String, String> {
        #[derive(Serialize)]
        struct Doc<'a> {
            version: u32,
            entries: &'a [MultiEntry],
        }
        serde_json::to_string_pretty(&Doc {
            version: FORMAT_VERSION,
            entries: &self.entries,
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
        let imported: Vec<MultiEntry> = if doc.entries.is_empty() {
            doc.pages.into_iter().flatten().collect()
        } else {
            doc.entries
        };
        if imported.is_empty() {
            return Err(crate::i18n::tr("文件中没有条目").to_owned());
        }
        let count = imported.iter().filter(|entry| entry.is_sendable()).count();
        self.entries = imported;
        self.version = FORMAT_VERSION;
        self.normalize();
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(text: &str) -> MultiEntry {
        MultiEntry {
            text: text.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn a_new_list_starts_empty_and_current() {
        let m = MultiSend::default();
        assert!(m.entries.is_empty());
        assert_eq!(m.version, FORMAT_VERSION);
    }

    #[test]
    fn adding_and_removing_entries_renumbers_them() {
        let mut m = MultiSend {
            entries: vec![entry("a"), entry("b"), entry("c")],
            ..Default::default()
        };
        let added = m.add_entry();
        assert_eq!(added, 3);
        assert_eq!(m.entries.len(), 4);

        assert_eq!(m.remove_entry(1).map(|e| e.text), Some("b".to_owned()));
        assert_eq!(m.entries.len(), 3);
        assert_eq!(m.entries[1].text, "c");
        assert!(m.remove_entry(99).is_none());
    }

    #[test]
    fn reordering_moves_entries_to_the_insertion_point() {
        let mut m = MultiSend {
            entries: vec![entry("a"), entry("b"), entry("c"), entry("d")],
            ..Default::default()
        };

        // Drop "a" between "c" and "d".
        m.move_entry(0, 3);
        assert_eq!(
            m.entries
                .iter()
                .map(|e| e.text.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "c", "a", "d"]
        );

        // Drop "d" at the very top.
        m.move_entry(3, 0);
        assert_eq!(
            m.entries
                .iter()
                .map(|e| e.text.as_str())
                .collect::<Vec<_>>(),
            vec!["d", "b", "c", "a"]
        );

        // Dropping an entry onto itself, or just after itself, changes nothing.
        m.move_entry(1, 1);
        m.move_entry(1, 2);
        assert_eq!(
            m.entries
                .iter()
                .map(|e| e.text.as_str())
                .collect::<Vec<_>>(),
            vec!["d", "b", "c", "a"]
        );
    }

    #[test]
    fn legacy_settings_are_cleared() {
        let mut m = MultiSend {
            version: 0,
            entries: vec![entry("old"), entry("older")],
            ..Default::default()
        };
        m.normalize();
        assert!(m.entries.is_empty(), "the old fixed pool must be discarded");
        assert_eq!(m.version, FORMAT_VERSION);
    }

    #[test]
    fn current_settings_keep_their_entries() {
        let mut m = MultiSend {
            version: FORMAT_VERSION,
            entries: vec![entry("a")],
            ..Default::default()
        };
        m.normalize();
        assert_eq!(m.entries, vec![entry("a")]);
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
    fn enabled_indices_span_the_whole_list() {
        let mut m = MultiSend {
            entries: vec![entry("a"), entry("b"), entry(""), entry("c")],
            ..Default::default()
        };
        m.entries[0].enabled = true;
        m.entries[1].enabled = false;
        m.entries[2].enabled = true;
        m.entries[3].enabled = true;
        assert_eq!(m.enabled_indices(), vec![0, 3]);
    }

    #[test]
    fn json_round_trip_preserves_entries() {
        let mut m = MultiSend {
            entries: vec![entry("one"), entry("two")],
            ..Default::default()
        };
        m.entries[1].hex = true;
        m.entries[1].note = "备注".to_owned();
        m.entries[1].enabled = true;
        let json = m.export_json().unwrap();
        let mut loaded = MultiSend::default();
        let count = loaded.import_json(&json).unwrap();
        assert_eq!(count, 2);
        assert_eq!(loaded.entries, m.entries);
    }

    #[test]
    fn storage_writes_only_existing_entries() {
        // An empty list stays empty instead of writing a hundred blank slots.
        let empty = serde_json::to_string(&MultiSend::default()).unwrap();
        assert!(empty.contains("\"entries\":[]"), "{empty}");

        let m = MultiSend {
            entries: vec![entry("a"), entry("b")],
            ..Default::default()
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: MultiSend = serde_json::from_str(&json).unwrap();
        assert_eq!(back.entries, m.entries);
    }

    /// Run states are not written and cannot be resurrected from an old file.
    #[test]
    fn run_states_are_not_persisted() {
        let m = MultiSend {
            loop_enabled: true,
            ..Default::default()
        };
        let json = serde_json::to_string(&m).unwrap();
        assert!(!json.contains("loop_enabled"), "{json}");

        let back: MultiSend = serde_json::from_str(&json).unwrap();
        assert!(!back.loop_enabled);

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
        assert_eq!(m.entries.len(), 3);
        assert_eq!(m.entries[0].text, "a");
        assert_eq!(m.entries[1].text, "b");
        assert_eq!(m.entries[2].text, "c");
    }

    #[test]
    fn import_rejects_junk() {
        let mut m = MultiSend::default();
        assert!(m.import_json("not json").is_err());
        assert!(m.import_json("{}").is_err());
    }

    #[test]
    fn per_entry_delay_defaults_to_zero_and_is_reported() {
        let mut m = MultiSend::default();
        assert_eq!(m.delay_after(0), 0);
        m.entries = vec![entry("a")];
        m.entries[0].delay_ms = 250;
        assert_eq!(m.delay_after(0), 250);
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
