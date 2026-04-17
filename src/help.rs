#[allow(clippy::struct_excessive_bools)]
pub struct HelpEntry {
    pub key: &'static str,
    pub desc: &'static str,
    pub indent: bool,
    pub todo_only: bool,
    pub waiting_too: bool,
    pub requires_claude: bool,
    pub footer: Option<&'static str>,
    pub footer_key: Option<&'static str>,
}

impl HelpEntry {
    const fn is_visible(&self, is_todo: bool, is_waiting: bool, has_claude: bool) -> bool {
        let mode_ok = !self.todo_only || is_todo || (self.waiting_too && is_waiting);
        mode_ok && (!self.requires_claude || has_claude)
    }
}

pub const HELP_ENTRIES: &[HelpEntry] = &[
    HelpEntry {
        key: "hjkl",
        desc: "Navigate between columns and todos",
        indent: false,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: Some("Nav"),
        footer_key: None,
    },
    HelpEntry {
        key: "Tab",
        desc: "Next mode",
        indent: false,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: Some("Mode"),
        footer_key: Some("Tab/S-Tab"),
    },
    HelpEntry {
        key: "S-Tab",
        desc: "Previous mode",
        indent: false,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "x",
        desc: "Complete selected todo",
        indent: false,
        todo_only: true,
        waiting_too: true,
        requires_claude: false,
        footer: Some("Done"),
        footer_key: None,
    },
    HelpEntry {
        key: "dd",
        desc: "Delete selected todo (and its detail .md file)",
        indent: false,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: Some("Del"),
        footer_key: None,
    },
    HelpEntry {
        key: "o",
        desc: "Open URLs in selected todo",
        indent: false,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: Some("URL"),
        footer_key: None,
    },
    HelpEntry {
        key: "f",
        desc: "Jump to visible todo by hint label (a-z, aa-zz)",
        indent: false,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: Some("Jump"),
        footer_key: None,
    },
    HelpEntry {
        key: "s",
        desc: "Send to... submenu",
        indent: false,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: Some("Send"),
        footer_key: None,
    },
    HelpEntry {
        key: "st",
        desc: "Send to todo.txt",
        indent: true,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "sr",
        desc: "Send to ref.txt",
        indent: true,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "si",
        desc: "Send to inbox.txt",
        indent: true,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "ss",
        desc: "Send to someday.txt",
        indent: true,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "sw",
        desc: "Send to waiting.txt",
        indent: true,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "p",
        desc: "Set/clear priority submenu",
        indent: false,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: Some("Priority"),
        footer_key: None,
    },
    HelpEntry {
        key: "pa",
        desc: "Set priority (A)",
        indent: true,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "pb",
        desc: "Set priority (B)",
        indent: true,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "pc",
        desc: "Set priority (C)",
        indent: true,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "pd",
        desc: "Set priority (D)",
        indent: true,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "pe",
        desc: "Set priority (E)",
        indent: true,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "px",
        desc: "Clear priority",
        indent: true,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "c",
        desc: "Claude submenu (requires crmux or claude CLI)",
        indent: false,
        todo_only: true,
        waiting_too: false,
        requires_claude: true,
        footer: Some("Claude"),
        footer_key: None,
    },
    HelpEntry {
        key: "csp",
        desc: "Send plan prompt to project's idle crmux session (>= 0.10.0)",
        indent: true,
        todo_only: true,
        waiting_too: false,
        requires_claude: true,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "csi",
        desc: "Send implement prompt to project's idle crmux session (>= 0.10.0)",
        indent: true,
        todo_only: true,
        waiting_too: false,
        requires_claude: true,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "cgp",
        desc: "Get plans and import via crmux (>= 0.11.0)",
        indent: true,
        todo_only: true,
        waiting_too: false,
        requires_claude: true,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "clp",
        desc: "Launch claude plan in tmux window (requires cwd in frontmatter)",
        indent: true,
        todo_only: true,
        waiting_too: false,
        requires_claude: true,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "cli",
        desc: "Launch claude implement in tmux window (requires cwd in frontmatter)",
        indent: true,
        todo_only: true,
        waiting_too: false,
        requires_claude: true,
        footer: None,
        footer_key: None,
    },
    HelpEntry {
        key: "t",
        desc: "Insert template from templates/*.md (j/k move, Enter insert, Esc/q cancel)",
        indent: false,
        todo_only: true,
        waiting_too: true,
        requires_claude: false,
        footer: Some("Tpl"),
        footer_key: None,
    },
    HelpEntry {
        key: "?",
        desc: "Toggle help",
        indent: false,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: Some("Help"),
        footer_key: None,
    },
    HelpEntry {
        key: "q",
        desc: "Quit",
        indent: false,
        todo_only: false,
        waiting_too: false,
        requires_claude: false,
        footer: Some("Quit"),
        footer_key: None,
    },
];

pub fn visible_entries(
    is_todo: bool,
    is_waiting: bool,
    has_claude: bool,
) -> Vec<&'static HelpEntry> {
    HELP_ENTRIES
        .iter()
        .filter(|e| e.is_visible(is_todo, is_waiting, has_claude))
        .collect()
}

pub fn footer_entries(
    is_todo: bool,
    is_waiting: bool,
    has_claude: bool,
) -> Vec<(&'static str, &'static str)> {
    HELP_ENTRIES
        .iter()
        .filter(|e| e.footer.is_some() && e.is_visible(is_todo, is_waiting, has_claude))
        .map(|e| (e.footer_key.unwrap_or(e.key), e.footer.unwrap()))
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
        let entries = footer_entries(true, false, true);
        assert!(entries.contains(&("hjkl", "Nav")));
        assert!(entries.contains(&("x", "Done")));
        assert!(entries.contains(&("o", "URL")));
        assert!(entries.contains(&("s", "Send")));
        assert!(entries.contains(&("Tab/S-Tab", "Mode")));
        assert!(entries.contains(&("c", "Claude")));
        assert!(entries.contains(&("?", "Help")));
        assert!(entries.contains(&("q", "Quit")));
    }

    #[test]
    fn test_footer_entries_excludes_stab_as_separate_entry() {
        let entries = footer_entries(true, false, true);
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
    fn test_footer_entries_tab_mode_placed_right_after_hjkl() {
        // Tab/S-Tab is also a navigation key, so it should sit adjacent to hjkl
        let entries = footer_entries(true, false, true);
        let keys: Vec<&str> = entries.iter().map(|(k, _)| *k).collect();
        let hjkl_pos = keys.iter().position(|k| *k == "hjkl").unwrap();
        let tab_pos = keys.iter().position(|k| *k == "Tab/S-Tab").unwrap();
        assert_eq!(
            tab_pos,
            hjkl_pos + 1,
            "Tab/S-Tab should come right after hjkl, got keys: {keys:?}"
        );
    }

    #[test]
    fn test_x_entry_visible_in_waiting() {
        // is_todo=false, is_waiting=true, has_claude=false
        let entries = visible_entries(false, true, false);
        assert!(
            entries.iter().any(|e| e.key == "x"),
            "x entry should be visible in Waiting mode"
        );
    }

    #[test]
    fn test_x_entry_hidden_in_inbox() {
        // is_todo=false, is_waiting=false (e.g., Inbox/Ref/Someday)
        let entries = visible_entries(false, false, false);
        assert!(
            !entries.iter().any(|e| e.key == "x"),
            "x entry should be hidden in non-Todo/Waiting modes"
        );
    }

    #[test]
    fn test_c_entry_still_todo_only() {
        // c must NOT widen into Waiting even with has_claude=true
        let entries = visible_entries(false, true, true);
        assert!(
            !entries.iter().any(|e| e.key == "c"),
            "c entry must stay Todo-only"
        );
    }

    #[test]
    fn test_f_entry_visible_in_all_modes() {
        for (is_todo, is_waiting) in [(true, false), (false, true), (false, false)] {
            let entries = visible_entries(is_todo, is_waiting, false);
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
}
