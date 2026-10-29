#[allow(clippy::struct_excessive_bools)]
pub struct HelpEntry {
    pub key: &'static str,
    pub desc: &'static str,
    pub indent: bool,
    pub todo_only: bool,
    pub waiting_too: bool,
    pub footer: Option<&'static str>,
    pub footer_key: Option<&'static str>,
}

impl HelpEntry {
    const fn is_visible(&self, is_todo: bool, is_waiting: bool) -> bool {
        !self.todo_only || is_todo || (self.waiting_too && is_waiting)
    }
}

// The `z` footer names what pressing it will do, so it reads as a toggle.
const SHOW_LATER: &str = "Show later";
const HIDE_LATER: &str = "Hide later";

pub const HELP_ENTRIES: &[HelpEntry] = &[
    HelpEntry {
        key: "hjkl",
        desc: "Navigate between columns and todos",
        indent: false,
        todo_only: false,
        waiting_too: false,
        footer: Some("Nav"),
        footer_key: Some("hjkl/Tab/S-Tab"),
    },
    HelpEntry {
        key: "Tab",
        desc: "Next mode",
        indent: false,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "S-Tab",
        desc: "Previous mode",
        indent: false,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "x",
        desc: "Complete selected todo",
        indent: false,
        todo_only: true,
        waiting_too: true,
        footer: Some("Done"),
        footer_key: None,
    },
    HelpEntry {
        key: "dd",
        desc: "Delete selected todo (and its detail .md file)",
        indent: false,
        todo_only: false,
        waiting_too: false,
        footer: Some("Del"),
        footer_key: None,
    },
    HelpEntry {
        key: "o",
        desc: "Open URLs in selected todo",
        indent: false,
        todo_only: false,
        waiting_too: false,
        footer: Some("URL"),
        footer_key: None,
    },
    HelpEntry {
        key: "f",
        desc: "Jump to visible todo by hint label (a-z, aa-zz)",
        indent: false,
        todo_only: false,
        waiting_too: false,
        footer: Some("Jump"),
        footer_key: None,
    },
    HelpEntry {
        key: "n",
        desc: "Now filter: energy (l/h/a) + time (s/m/l/a); nc clears",
        indent: false,
        todo_only: true,
        waiting_too: false,
        footer: Some("Now"),
        footer_key: None,
    },
    HelpEntry {
        key: "v",
        desc: "Toggle Project/Pick layout",
        indent: false,
        todo_only: true,
        waiting_too: false,
        footer: Some("Layout"),
        footer_key: None,
    },
    HelpEntry {
        key: "s",
        desc: "Send to... submenu",
        indent: false,
        todo_only: false,
        waiting_too: false,
        footer: Some("Send"),
        footer_key: None,
    },
    HelpEntry {
        key: "st",
        desc: "Send to todo.txt",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "sr",
        desc: "Send to ref.txt",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "si",
        desc: "Send to inbox.txt",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "ss",
        desc: "Send to someday.txt",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "sw",
        desc: "Send to waiting.txt",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "p",
        desc: "Set/clear priority submenu",
        indent: false,
        todo_only: false,
        waiting_too: false,
        footer: Some("Priority"),
        footer_key: None,
    },
    HelpEntry {
        key: "pa",
        desc: "Set priority (A)",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "pb",
        desc: "Set priority (B)",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "pc",
        desc: "Set priority (C)",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "pd",
        desc: "Set priority (D)",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "pe",
        desc: "Set priority (E)",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "px",
        desc: "Clear priority",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "t",
        desc: "Set tag submenu",
        indent: false,
        todo_only: false,
        waiting_too: false,
        footer: Some("Tag"),
        footer_key: None,
    },
    HelpEntry {
        key: "tis",
        desc: "Set time:short",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "tim",
        desc: "Set time:medium",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "til",
        desc: "Set time:long",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "tel",
        desc: "Set energy:low",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "teh",
        desc: "Set energy:high",
        indent: true,
        todo_only: false,
        waiting_too: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "z",
        desc: "Show/hide later items (future t: date)",
        indent: false,
        todo_only: false,
        waiting_too: false,
        footer: Some(SHOW_LATER),
        footer_key: None,
    },
    HelpEntry {
        key: "?",
        desc: "Toggle help",
        indent: false,
        todo_only: false,
        waiting_too: false,
        footer: Some("Help"),
        footer_key: None,
    },
    HelpEntry {
        key: "q",
        desc: "Quit",
        indent: false,
        todo_only: false,
        waiting_too: false,
        footer: Some("Quit"),
        footer_key: None,
    },
];

pub fn visible_entries(is_todo: bool, is_waiting: bool) -> Vec<&'static HelpEntry> {
    HELP_ENTRIES
        .iter()
        .filter(|e| e.is_visible(is_todo, is_waiting))
        .collect()
}

pub fn footer_entries(
    is_todo: bool,
    is_waiting: bool,
    show_deferred: bool,
) -> Vec<(&'static str, &'static str)> {
    HELP_ENTRIES
        .iter()
        .filter(|e| e.footer.is_some() && e.is_visible(is_todo, is_waiting))
        .map(|e| {
            let label = match e.footer.unwrap() {
                SHOW_LATER if show_deferred => HIDE_LATER,
                label => label,
            };
            (e.footer_key.unwrap_or(e.key), label)
        })
        .collect()
}

pub fn cli_help_text() -> String {
    let max_key_width = HELP_ENTRIES
        .iter()
        .map(|e| e.key.len() + if e.indent { 2 } else { 0 })
        .max()
        .unwrap_or(0);

    let lines: Vec<String> = HELP_ENTRIES
        .iter()
        .map(|e| {
            let prefix = if e.indent { "  " } else { "" };
            let padded_key = format!("{prefix}{}", e.key);
            format!("  {padded_key:<max_key_width$}  {}", e.desc)
        })
        .collect();

    format!("Keyboard Controls:\n{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_help_text_contains_all_keys() {
        let text = cli_help_text();
        for entry in HELP_ENTRIES {
            assert!(
                text.contains(entry.key),
                "Help text should contain key: {}",
                entry.key
            );
        }
    }

    #[test]
    fn test_footer_entries_contains_short_labels() {
        let entries = footer_entries(true, false, false);
        assert!(entries.contains(&("hjkl/Tab/S-Tab", "Nav")));
        assert!(entries.contains(&("x", "Done")));
        assert!(entries.contains(&("o", "URL")));
        assert!(entries.contains(&("s", "Send")));
        assert!(!entries.iter().any(|(_, desc)| *desc == "Claude"));
        assert!(entries.contains(&("t", "Tag")));
        assert!(entries.contains(&("?", "Help")));
        assert!(entries.contains(&("q", "Quit")));
        assert!(!entries.iter().any(|(_, desc)| *desc == "Mode"));
    }

    #[test]
    fn test_footer_entries_excludes_stab_as_separate_entry() {
        let entries = footer_entries(true, false, false);
        // S-Tab should not appear as its own footer entry (merged into Tab/S-Tab)
        assert!(!entries.iter().any(|(k, _)| *k == "S-Tab"));
        // Tab also should not appear as a standalone key since it's merged
        assert!(!entries.iter().any(|(k, _)| *k == "Tab"));
    }

    #[test]
    fn test_help_entries_real_keys_preserved() {
        // Ensure real key names for Tab and S-Tab remain intact for the help overlay
        assert!(HELP_ENTRIES.iter().any(|e| e.key == "Tab"));
        assert!(HELP_ENTRIES.iter().any(|e| e.key == "S-Tab"));
    }

    #[test]
    fn test_footer_entries_nav_merged_into_single_entry() {
        // Nav+Mode are collapsed into a single `hjkl/Tab/S-Tab:Nav` footer entry.
        let entries = footer_entries(true, false, false);
        let nav_count = entries
            .iter()
            .filter(|(k, _)| *k == "hjkl/Tab/S-Tab")
            .count();
        assert_eq!(nav_count, 1);
    }

    #[test]
    fn test_x_entry_visible_in_waiting() {
        // is_todo=false, is_waiting=true
        let entries = visible_entries(false, true);
        assert!(
            entries.iter().any(|e| e.key == "x"),
            "x entry should be visible in Waiting mode"
        );
    }

    #[test]
    fn test_x_entry_hidden_in_inbox() {
        // is_todo=false, is_waiting=false (e.g., Inbox/Ref/Someday)
        let entries = visible_entries(false, false);
        assert!(
            !entries.iter().any(|e| e.key == "x"),
            "x entry should be hidden in non-Todo/Waiting modes"
        );
    }

    #[test]
    fn test_f_entry_visible_in_all_modes() {
        for (is_todo, is_waiting) in [(true, false), (false, true), (false, false)] {
            let entries = visible_entries(is_todo, is_waiting);
            assert!(
                entries.iter().any(|e| e.key == "f"),
                "f (hint jump) should be visible in mode is_todo={is_todo} is_waiting={is_waiting}"
            );
        }
    }

    #[test]
    fn test_f_entry_in_footer() {
        let entries = footer_entries(false, false, false);
        assert!(
            entries.iter().any(|(k, desc)| *k == "f" && *desc == "Jump"),
            "f:Jump should appear in footer"
        );
    }

    #[test]
    fn test_z_footer_offers_to_show_later_items_while_they_are_hidden() {
        let entries = footer_entries(true, false, false);
        assert!(entries.contains(&("z", "Show later")));
    }

    #[test]
    fn test_z_footer_offers_to_hide_later_items_while_they_are_shown() {
        let entries = footer_entries(true, false, true);
        assert!(entries.contains(&("z", "Hide later")));
    }
}
