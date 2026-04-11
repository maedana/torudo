pub struct HelpEntry {
    pub key: &'static str,
    pub desc: &'static str,
    pub indent: bool,
}

pub const HELP_ENTRIES: &[HelpEntry] = &[
    HelpEntry { key: "hjkl", desc: "Navigate between columns and todos", indent: false },
    HelpEntry { key: "x", desc: "Complete selected todo", indent: false },
    HelpEntry { key: "r", desc: "Reload todo.txt", indent: false },
    HelpEntry { key: "c", desc: "Claude submenu (requires crmux or claude CLI)", indent: false },
    HelpEntry { key: "csp", desc: "Send plan prompt to project's idle crmux session (>= 0.10.0)", indent: true },
    HelpEntry { key: "csi", desc: "Send implement prompt to project's idle crmux session (>= 0.10.0)", indent: true },
    HelpEntry { key: "cgp", desc: "Get plans and import via crmux (>= 0.11.0)", indent: true },
    HelpEntry { key: "clp", desc: "Launch claude plan in tmux window (requires cwd in frontmatter)", indent: true },
    HelpEntry { key: "cli", desc: "Launch claude implement in tmux window (requires cwd in frontmatter)", indent: true },
    HelpEntry { key: "v", desc: "Hide current project column", indent: false },
    HelpEntry { key: "V", desc: "Show all hidden projects", indent: false },
    HelpEntry { key: "?", desc: "Toggle help", indent: false },
    HelpEntry { key: "q", desc: "Quit", indent: false },
];

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
}
