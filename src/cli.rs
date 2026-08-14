use crate::app_state::ViewMode;
use crate::todo::{self, Item};
use clap::{Subcommand, ValueEnum};
use std::error::Error;
use std::path::Path;

/// Actions available under every GTD mode subcommand (`torudo <mode> <action>`).
#[derive(Subcommand)]
pub enum ModeAction {
    /// Add a new item to this mode's file and print it as JSON
    Add {
        /// Todo text (priority, projects, contexts, id, key:value are all supported)
        #[arg(trailing_var_arg = true, num_args = 1..)]
        text: Vec<String>,
    },
    /// List the items held by this mode
    List {
        /// Print the items as a JSON array instead of one todo.txt line each
        #[arg(long)]
        json: bool,
    },
    /// Set GTD tags on an item and print it as JSON
    Set {
        /// Id of the item to update
        id: String,
        /// Estimated time to finish (`time:` tag)
        #[arg(long, value_enum)]
        time: Option<TimeValue>,
        /// Energy the item demands (`energy:` tag)
        #[arg(long, value_enum)]
        energy: Option<EnergyValue>,
    },
    /// Move an item to another mode and print it as JSON
    Promote {
        /// Id of the item to move
        id: String,
        /// Mode to move the item into
        #[arg(long, value_enum)]
        to: ModeName,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum ModeName {
    Inbox,
    Todo,
    Waiting,
    Ref,
    Someday,
}

impl ModeName {
    const fn view_mode(self) -> ViewMode {
        match self {
            Self::Inbox => ViewMode::Inbox,
            Self::Todo => ViewMode::Todo,
            Self::Waiting => ViewMode::Waiting,
            Self::Ref => ViewMode::Ref,
            Self::Someday => ViewMode::Someday,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum TimeValue {
    Short,
    Medium,
    Long,
}

impl TimeValue {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::Medium => "medium",
            Self::Long => "long",
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum EnergyValue {
    Low,
    High,
}

impl EnergyValue {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
        }
    }
}

pub fn run(mode: ViewMode, action: &ModeAction, todotxt_dir: &str) -> Result<(), Box<dyn Error>> {
    let file = mode_file(mode, todotxt_dir);
    match action {
        ModeAction::Add { text } => {
            let item = todo::add_item(&file, &text.join(" "))?;
            println!("{}", todo::item_to_json(&item, todotxt_dir)?);
        }
        ModeAction::List { json } => {
            let items = load_mode_items(&file)?;
            if *json {
                println!("{}", serde_json::to_string_pretty(&items)?);
            } else {
                for item in &items {
                    println!("{}", format_item(item));
                }
            }
        }
        ModeAction::Set { id, time, energy } => {
            if time.is_none() && energy.is_none() {
                return Err("specify at least one of --time or --energy".into());
            }
            require_item(&file, mode, id)?;
            if let Some(time) = time {
                todo::set_key_value(&file, id, "time", time.as_str())?;
            }
            if let Some(energy) = energy {
                todo::set_key_value(&file, id, "energy", energy.as_str())?;
            }
            print_item(&file, id, todotxt_dir)?;
        }
        ModeAction::Promote { id, to } => {
            let target = to.view_mode();
            if target == mode {
                return Err(format!("item is already in {}", mode.label()).into());
            }
            require_item(&file, mode, id)?;
            let target_file = mode_file(target, todotxt_dir);
            todo::move_to_file(&file, &target_file, id)?;
            print_item(&target_file, id, todotxt_dir)?;
        }
    }
    Ok(())
}

fn require_item(file: &str, mode: ViewMode, id: &str) -> Result<(), Box<dyn Error>> {
    if todo::has_todo_with_id(file, id) {
        Ok(())
    } else {
        Err(format!("no item with id:{id} in {}", mode.filename()).into())
    }
}

fn print_item(file: &str, id: &str, todotxt_dir: &str) -> Result<(), Box<dyn Error>> {
    let items = load_mode_items(file)?;
    let item = items
        .iter()
        .find(|item| item.id.as_deref() == Some(id))
        .ok_or_else(|| format!("item id:{id} is gone from {file}"))?;
    println!("{}", todo::item_to_json(item, todotxt_dir)?);
    Ok(())
}

fn mode_file(mode: ViewMode, todotxt_dir: &str) -> String {
    format!("{todotxt_dir}/{}", mode.filename())
}

/// A mode file is created lazily, so a missing one simply holds no items.
fn load_mode_items(file: &str) -> Result<Vec<Item>, Box<dyn Error>> {
    if Path::new(file).exists() {
        todo::load_todos(file)
    } else {
        Ok(Vec::new())
    }
}

/// Render an item back as a todo.txt line for human-readable output.
fn format_item(item: &Item) -> String {
    let mut parts = Vec::new();
    if item.completed {
        parts.push("x".to_string());
    }
    if let Some(priority) = item.priority {
        parts.push(format!("({priority})"));
    }
    if let Some(date) = item.completion_date {
        parts.push(date.to_string());
    }
    if let Some(date) = item.creation_date {
        parts.push(date.to_string());
    }
    if !item.description.is_empty() {
        parts.push(item.description.clone());
    }
    for project in &item.projects {
        parts.push(format!("+{project}"));
    }
    for context in &item.contexts {
        parts.push(format!("@{context}"));
    }
    let mut key_values: Vec<(&String, &String)> = item.key_values.iter().collect();
    key_values.sort_by(|a, b| a.0.cmp(b.0));
    for (key, value) in key_values {
        parts.push(format!("{key}:{value}"));
    }
    if let Some(id) = &item.id {
        parts.push(format!("id:{id}"));
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_file_joins_dir_and_mode_filename() {
        assert_eq!(
            mode_file(ViewMode::Waiting, "/tmp/todotxt"),
            "/tmp/todotxt/waiting.txt"
        );
    }

    #[test]
    fn load_mode_items_on_missing_file_is_empty() {
        let items = load_mode_items("/tmp/torudo-does-not-exist/inbox.txt").unwrap();
        assert!(items.is_empty());
    }

    #[test]
    fn format_item_renders_a_todo_txt_line() {
        let item = Item::parse("(A) Buy milk +grocery @home time:short id:abc", 1);
        assert_eq!(
            format_item(&item),
            "(A) Buy milk +grocery @home time:short id:abc"
        );
    }

    #[test]
    fn format_item_keeps_the_creation_date() {
        let item = Item::parse("(A) 2026-08-15 Buy milk id:abc", 1);
        assert_eq!(format_item(&item), "(A) 2026-08-15 Buy milk id:abc");
    }

    #[test]
    fn format_item_renders_a_completed_line() {
        let item = Item::parse("x 2026-08-15 2026-08-01 Buy milk id:abc", 1);
        assert_eq!(
            format_item(&item),
            "x 2026-08-15 2026-08-01 Buy milk id:abc"
        );
    }

    #[test]
    fn format_item_sorts_key_values_for_stable_output() {
        let item = Item::parse("Task time:short energy:low due:2026-01-01 id:abc", 1);
        assert_eq!(
            format_item(&item),
            "Task due:2026-01-01 energy:low time:short id:abc"
        );
    }
}
