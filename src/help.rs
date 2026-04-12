pub struct HelpEntry {
    pub key: &'static str,
    pub desc: &'static str,
    pub indent: bool,
    pub todo_only: bool,
    pub requires_claude: bool,
    pub footer: Option<&'static str>,
}

impl HelpEntry {
    const fn is_visible(&self, is_todo: bool, has_claude: bool) -> bool {
        (!self.todo_only || is_todo) && (!self.requires_claude || has_claude)
    }
}

pub const HELP_ENTRIES: &[HelpEntry] = &[
    HelpEntry { key: "hjkl", desc: "Navigate between columns and todos", indent: false, todo_only: false, requires_claude: false, footer: Some("Nav") },
    HelpEntry { key: "x", desc: "Complete selected todo", indent: false, todo_only: true, requires_claude: false, footer: Some("Complete") },
    HelpEntry { key: "o", desc: "Open URLs in selected todo", indent: false, todo_only: false, requires_claude: false, footer: Some("Open URL") },
    HelpEntry { key: "s", desc: "Send to... submenu", indent: false, todo_only: false, requires_claude: false, footer: Some("Send") },
    HelpEntry { key: "st", desc: "Send to todo.txt", indent: true, todo_only: false, requires_claude: false, footer: None },
    HelpEntry { key: "sr", desc: "Send to ref.txt", indent: true, todo_only: false, requires_claude: false, footer: None },
    HelpEntry { key: "si", desc: "Send to inbox.txt", indent: true, todo_only: false, requires_claude: false, footer: None },
    HelpEntry { key: "ss", desc: "Send to someday.txt", indent: true, todo_only: false, requires_claude: false, footer: None },
    HelpEntry { key: "sw", desc: "Send to waiting.txt", indent: true, todo_only: false, requires_claude: false, footer: None },
    HelpEntry { key: "Tab", desc: "Next mode", indent: false, todo_only: false, requires_claude: false, footer: Some("Next mode") },
    HelpEntry { key: "S-Tab", desc: "Previous mode", indent: false, todo_only: false, requires_claude: false, footer: Some("Prev mode") },
    HelpEntry { key: "c", desc: "Claude submenu (requires crmux or claude CLI)", indent: false, todo_only: true, requires_claude: true, footer: Some("Claude") },
    HelpEntry { key: "csp", desc: "Send plan prompt to project's idle crmux session (>= 0.10.0)", indent: true, todo_only: true, requires_claude: true, footer: None },
    HelpEntry { key: "csi", desc: "Send implement prompt to project's idle crmux session (>= 0.10.0)", indent: true, todo_only: true, requires_claude: true, footer: None },
    HelpEntry { key: "cgp", desc: "Get plans and import via crmux (>= 0.11.0)", indent: true, todo_only: true, requires_claude: true, footer: None },
    HelpEntry { key: "clp", desc: "Launch claude plan in tmux window (requires cwd in frontmatter)", indent: true, todo_only: true, requires_claude: true, footer: None },
    HelpEntry { key: "cli", desc: "Launch claude implement in tmux window (requires cwd in frontmatter)", indent: true, todo_only: true, requires_claude: true, footer: None },
    HelpEntry { key: "?", desc: "Toggle help", indent: false, todo_only: false, requires_claude: false, footer: Some("Help") },
    HelpEntry { key: "q", desc: "Quit", indent: false, todo_only: false, requires_claude: false, footer: Some("Quit") },
];

pub fn visible_entries(is_todo: bool, has_claude: bool) -> Vec<&'static HelpEntry> {
    HELP_ENTRIES
        .iter()
        .filter(|e| e.is_visible(is_todo, has_claude))
        .collect()
}

pub fn footer_entries(is_todo: bool, has_claude: bool) -> Vec<(&'static str, &'static str)> {
    HELP_ENTRIES
        .iter()
        .filter(|e| e.footer.is_some() && e.is_visible(is_todo, has_claude))
        .map(|e| (e.key, e.footer.unwrap()))
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
}
