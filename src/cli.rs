use crate::app_state::ViewMode;
use crate::md_preview;
use crate::recurrence::parse_pattern;
use crate::todo::{self, Item, Recurred};
use clap::{Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::json;
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
        /// How often the item repeats once completed (`rec:` tag), e.g. `1w`, `+1m`.
        /// `none` removes the tag.
        #[arg(long, value_parser = parse_rec_value)]
        rec: Option<String>,
        /// Priority letter (`A`-`E`). `none` clears it.
        #[arg(long, value_enum, ignore_case = true)]
        priority: Option<PriorityValue>,
    },
    /// Complete an item, moving it to done.txt, and print it as JSON
    Complete {
        /// Id of the item to complete
        id: String,
    },
    /// Bring a completed item back from done.txt into this mode
    Reopen {
        /// Id of the completed item to bring back
        id: String,
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

/// The `--rec` value that removes the tag instead of setting one. `--time` and
/// `--energy` have no equivalent yet: a stale `rec:` keeps spawning occurrences,
/// so being able to clear it matters more than for the other two tags.
const REC_NONE: &str = "none";

/// Reject a bad `rec:` pattern at the argument boundary, the way `--time` and
/// `--energy` are rejected by their `value_enum`. [`REC_NONE`] is let through as
/// the one non-pattern value.
fn parse_rec_value(value: &str) -> Result<String, String> {
    if value == REC_NONE {
        return Ok(value.to_string());
    }
    parse_pattern(value).map_err(|error| error.to_string())?;
    Ok(value.to_string())
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

#[derive(Clone, Copy, ValueEnum)]
pub enum PriorityValue {
    A,
    B,
    C,
    D,
    E,
    /// Clears the priority instead of setting one, mirroring `REC_NONE`.
    None,
}

impl PriorityValue {
    const fn as_char(self) -> Option<char> {
        match self {
            Self::A => Some('A'),
            Self::B => Some('B'),
            Self::C => Some('C'),
            Self::D => Some('D'),
            Self::E => Some('E'),
            Self::None => None,
        }
    }
}

pub fn run(mode: ViewMode, action: &ModeAction, todotxt_dir: &str) -> Result<(), Box<dyn Error>> {
    let file = mode.file_path(todotxt_dir);
    match action {
        ModeAction::Add { text } => {
            let item = todo::add_item(&file, &text.join(" "))?;
            println!("{}", todo::item_to_json(&item, todotxt_dir)?);
        }
        ModeAction::List { json } => print_items(&load_mode_items(&file)?, *json)?,
        ModeAction::Set {
            id,
            time,
            energy,
            rec,
            priority,
        } => {
            if time.is_none() && energy.is_none() && rec.is_none() && priority.is_none() {
                return Err("specify at least one of --time, --energy, --rec or --priority".into());
            }
            require_item(&file, mode, id)?;
            if let Some(time) = time {
                todo::set_key_value(&file, id, "time", time.as_str())?;
            }
            if let Some(energy) = energy {
                todo::set_key_value(&file, id, "energy", energy.as_str())?;
            }
            if let Some(rec) = rec {
                if rec == REC_NONE {
                    todo::clear_key_value(&file, id, "rec")?;
                } else {
                    todo::set_key_value(&file, id, "rec", rec)?;
                }
            }
            if let Some(priority) = priority {
                todo::set_priority(&file, id, priority.as_char())?;
            }
            print_item(&file, id, todotxt_dir)?;
        }
        ModeAction::Complete { id } => {
            // Matches the TUI, where `x` only works in Todo and Waiting.
            if !matches!(mode, ViewMode::Todo | ViewMode::Waiting) {
                return Err(format!(
                    "cannot complete from {}; promote it to Todo first",
                    mode.label()
                )
                .into());
            }
            require_item(&file, mode, id)?;
            let recurred = todo::mark_complete(&file, id)?;
            // The item is completed either way, so a bad pattern is a warning on
            // stderr rather than a non-zero exit.
            if let Recurred::Failed { value, error } = &recurred {
                eprintln!("warning: rec:{value} is invalid ({error}); no next occurrence created");
            }
            print_completed_item(&done_file(todotxt_dir), id, todotxt_dir, &recurred)?;
        }
        ModeAction::Reopen { id } => {
            let done = done_file(todotxt_dir);
            if !todo::reopen_item(&done, &file, id)? {
                return Err(format!("no completed item with id:{id} in done.txt").into());
            }
            print_item(&file, id, todotxt_dir)?;
        }
        ModeAction::Promote { id, to } => {
            let target = to.view_mode();
            if target == mode {
                return Err(format!("item is already in {}", mode.label()).into());
            }
            require_item(&file, mode, id)?;
            let target_file = target.file_path(todotxt_dir);
            todo::move_to_file(&file, &target_file, id)?;
            print_item(&target_file, id, todotxt_dir)?;
        }
    }
    Ok(())
}

/// Actions available under the `done` subcommand. `done.txt` is an archive, not a
/// GTD mode, so it is read-only here — items leave it through `<mode> reopen`.
#[derive(Subcommand)]
pub enum DoneAction {
    /// List the completed items
    List {
        /// Print the items as a JSON array instead of one todo.txt line each
        #[arg(long)]
        json: bool,
    },
}

pub fn run_done(action: &DoneAction, todotxt_dir: &str) -> Result<(), Box<dyn Error>> {
    match action {
        DoneAction::List { json } => print_items(&load_mode_items(&done_file(todotxt_dir))?, *json),
    }
}

/// `--mode` narrows the sweep; without it every GTD mode is searched.
/// `done.txt` is never a target because its items cannot be focused.
fn search_targets(mode: Option<ModeName>) -> Vec<ViewMode> {
    mode.map_or_else(|| ViewMode::ALL.to_vec(), |mode| vec![mode.view_mode()])
}

/// Which part of an item the query was found in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum MatchField {
    /// The title line: the description together with its `+project` and
    /// `@context` tags. See `searchable_title`.
    Title,
    Md,
    Both,
}

impl MatchField {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Md => "md",
            Self::Both => "both",
        }
    }
}

/// A single search result. `Item` carries no mode, so hits need this wrapper.
#[derive(Serialize)]
struct SearchHit {
    /// Items without an `id:` tag are reported as null rather than dropped.
    id: Option<String>,
    /// Lowercase mode name, spelled the way `--mode` accepts it.
    mode: &'static str,
    /// The bare description: tags are searched, but not echoed back here.
    title: String,
    matched: MatchField,
}

/// One line per hit, id first so `awk '{print $1}'` feeds `torudo focus`.
fn format_hit(hit: &SearchHit) -> String {
    let id = hit.id.as_deref().unwrap_or("-");
    format!(
        "{id} [{}] {} ({})",
        hit.mode,
        hit.title,
        hit.matched.as_str()
    )
}

/// The text a `MatchField::Title` hit covers. `Item::parse` lifts the
/// `+project` and `@context` tags out of the description, but torudo organises
/// items by project, so they are exactly what a user searching for "keihi"
/// means. The `key:value` tags stay out: `id:` and `time:short` are metadata
/// and would only produce noise hits.
fn searchable_title(item: &Item) -> String {
    let mut text = item.description.clone();
    for project in &item.projects {
        text.push_str(" +");
        text.push_str(project);
    }
    for context in &item.contexts {
        text.push_str(" @");
        text.push_str(context);
    }
    text
}

/// Case-insensitive substring match. The query is never treated as a regex.
/// `title` is a `searchable_title`, not the bare description.
fn matched_field(title: &str, md: Option<&str>, query: &str) -> Option<MatchField> {
    let needle = query.to_lowercase();
    let in_title = title.to_lowercase().contains(&needle);
    let in_md = md.is_some_and(|md| md.to_lowercase().contains(&needle));
    match (in_title, in_md) {
        (true, true) => Some(MatchField::Both),
        (true, false) => Some(MatchField::Title),
        (false, true) => Some(MatchField::Md),
        (false, false) => None,
    }
}

pub fn run_search(
    query: &str,
    json: bool,
    mode: Option<ModeName>,
    todotxt_dir: &str,
) -> Result<(), Box<dyn Error>> {
    if query.is_empty() {
        return Err("query must not be empty".into());
    }
    let mut hits = Vec::new();
    for mode in search_targets(mode) {
        hits.extend(collect_hits(mode, todotxt_dir, query)?);
    }
    print_hits(&hits, json)
}

/// A missing mode file simply contributes no hits.
fn collect_hits(
    mode: ViewMode,
    todotxt_dir: &str,
    query: &str,
) -> Result<Vec<SearchHit>, Box<dyn Error>> {
    let mut hits = Vec::new();
    for item in load_mode_items(&mode.file_path(todotxt_dir))? {
        let md = item.id.as_deref().and_then(|id| read_md(todotxt_dir, id));
        if let Some(matched) = matched_field(&searchable_title(&item), md.as_deref(), query) {
            hits.push(SearchHit {
                id: item.id,
                mode: mode.cli_name(),
                title: item.description,
                matched,
            });
        }
    }
    Ok(hits)
}

/// The detail file is optional, so an unreadable one is the same as an absent one.
fn read_md(todotxt_dir: &str, id: &str) -> Option<String> {
    std::fs::read_to_string(md_preview::md_path(todotxt_dir, id)).ok()
}

/// The counterpart of `print_items`, which only accepts `&[Item]`.
fn print_hits(hits: &[SearchHit], json: bool) -> Result<(), Box<dyn Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(hits)?);
    } else {
        for hit in hits {
            println!("{}", format_hit(hit));
        }
    }
    Ok(())
}

fn print_items(items: &[Item], json: bool) -> Result<(), Box<dyn Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(items)?);
    } else {
        for item in items {
            println!("{}", format_item(item));
        }
    }
    Ok(())
}

fn done_file(todotxt_dir: &str) -> String {
    format!("{todotxt_dir}/done.txt")
}

fn require_item(file: &str, mode: ViewMode, id: &str) -> Result<(), Box<dyn Error>> {
    if todo::has_todo_with_id(file, id) {
        Ok(())
    } else {
        Err(format!("no item with id:{id} in {}", mode.filename()).into())
    }
}

fn find_item(file: &str, id: &str) -> Result<Item, Box<dyn Error>> {
    load_mode_items(file)?
        .into_iter()
        .find(|item| item.id.as_deref() == Some(id))
        .ok_or_else(|| format!("item id:{id} is gone from {file}").into())
}

fn print_item(file: &str, id: &str, todotxt_dir: &str) -> Result<(), Box<dyn Error>> {
    println!(
        "{}",
        todo::item_to_json(&find_item(file, id)?, todotxt_dir)?
    );
    Ok(())
}

/// The `recurrence` field of `complete`'s output. `None` when the item had no
/// `rec:` tag, in which case the field is left out of the JSON entirely.
fn recurrence_value(recurred: &Recurred) -> Option<serde_json::Value> {
    match recurred {
        Recurred::None => None,
        Recurred::Created { id, t, due } => Some(json!({
            "id": id,
            "t": t.map(|d| d.to_string()),
            "due": due.map(|d| d.to_string()),
        })),
        Recurred::Failed { value, error } => Some(json!({
            "value": value,
            "error": error.to_string(),
        })),
    }
}

/// Like `print_item`, but folds the recurrence outcome into the same object so
/// stdout stays a single JSON document.
fn print_completed_item(
    file: &str,
    id: &str,
    todotxt_dir: &str,
    recurred: &Recurred,
) -> Result<(), Box<dyn Error>> {
    let mut json = todo::item_to_value(&find_item(file, id)?, todotxt_dir)?;
    if let Some(recurrence) = recurrence_value(recurred) {
        json["recurrence"] = recurrence;
    }
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
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
    use crate::recurrence::RecurrenceError;

    #[test]
    fn parse_rec_value_accepts_a_valid_pattern() {
        assert_eq!(parse_rec_value("+1m"), Ok("+1m".to_string()));
        assert_eq!(parse_rec_value("2w"), Ok("2w".to_string()));
    }

    #[test]
    fn parse_rec_value_accepts_none_as_the_clearing_keyword() {
        assert_eq!(parse_rec_value(REC_NONE), Ok(REC_NONE.to_string()));
    }

    #[test]
    fn parse_rec_value_still_rejects_a_bad_pattern() {
        assert_eq!(
            parse_rec_value("banana"),
            Err(RecurrenceError::Malformed.to_string())
        );
    }

    #[test]
    fn matched_field_is_none_when_nothing_contains_the_query() {
        assert_eq!(matched_field("Buy milk", None, "boat"), None);
    }

    #[test]
    fn matched_field_is_title_when_the_title_contains_the_query() {
        assert_eq!(
            matched_field("Buy milk", None, "milk"),
            Some(MatchField::Title)
        );
    }

    #[test]
    fn matched_field_ignores_case_on_both_sides() {
        assert_eq!(
            matched_field("Buy MILK", None, "milk"),
            Some(MatchField::Title)
        );
        assert_eq!(
            matched_field("Buy milk", None, "MILK"),
            Some(MatchField::Title)
        );
    }

    #[test]
    fn matched_field_is_md_when_only_the_md_body_contains_the_query() {
        assert_eq!(
            matched_field("Buy milk", Some("remember the Boat"), "boat"),
            Some(MatchField::Md)
        );
    }

    #[test]
    fn matched_field_is_both_when_title_and_md_contain_the_query() {
        assert_eq!(
            matched_field("Buy milk", Some("2 litres of milk"), "milk"),
            Some(MatchField::Both)
        );
    }

    #[test]
    fn matched_field_searches_the_title_alone_when_there_is_no_md_file() {
        assert_eq!(matched_field("Buy milk", None, "litres"), None);
        assert_eq!(
            matched_field("Buy milk", Some("2 litres"), "litres"),
            Some(MatchField::Md)
        );
    }

    #[test]
    fn matched_field_does_not_treat_the_query_as_a_regex() {
        assert_eq!(matched_field("Buy milk", None, ".*"), None);
        assert_eq!(matched_field("Buy milk", None, "^Buy"), None);
    }

    #[test]
    fn searchable_title_appends_the_project_and_context_tags() {
        let item = Item::parse("Fix the flaky spec +keihi @office time:short id:aaa", 1);
        assert_eq!(searchable_title(&item), "Fix the flaky spec +keihi @office");
    }

    #[test]
    fn searchable_title_leaves_out_the_key_value_tags() {
        let item = Item::parse("Fix it time:short energy:low id:aaa", 1);
        assert_eq!(searchable_title(&item), "Fix it");
    }

    #[test]
    fn searchable_title_of_an_untagged_item_is_its_description() {
        let item = Item::parse("(A) 2026-08-15 Buy milk", 1);
        assert_eq!(searchable_title(&item), "Buy milk");
    }

    #[test]
    fn matched_field_finds_a_tag_in_the_searchable_title() {
        let item = Item::parse("Fix the flaky spec +keihi @office id:aaa", 1);
        let title = searchable_title(&item);
        assert_eq!(
            matched_field(&title, None, "keihi"),
            Some(MatchField::Title)
        );
        assert_eq!(
            matched_field(&title, None, "office"),
            Some(MatchField::Title)
        );
        assert_eq!(matched_field(&title, None, "aaa"), None);
    }

    #[test]
    fn search_targets_without_a_mode_covers_every_mode() {
        assert_eq!(search_targets(None), ViewMode::ALL.to_vec());
    }

    #[test]
    fn search_targets_with_a_mode_covers_only_that_mode() {
        assert_eq!(search_targets(Some(ModeName::Ref)), vec![ViewMode::Ref]);
    }

    #[test]
    fn format_hit_starts_the_line_with_the_id() {
        let hit = SearchHit {
            id: Some("aaa-111".to_string()),
            mode: "inbox",
            title: "Buy milk".to_string(),
            matched: MatchField::Title,
        };
        assert_eq!(format_hit(&hit), "aaa-111 [inbox] Buy milk (title)");
    }

    #[test]
    fn format_hit_marks_a_missing_id_with_a_dash() {
        let hit = SearchHit {
            id: None,
            mode: "ref",
            title: "Untagged line".to_string(),
            matched: MatchField::Both,
        };
        assert_eq!(format_hit(&hit), "- [ref] Untagged line (both)");
    }

    #[test]
    fn every_cli_name_round_trips_through_the_mode_argument() {
        // `search` prints `cli_name()` and the docs promise it can be fed
        // straight back to `--mode`, so clap has to accept every one of them.
        for mode in ViewMode::ALL {
            let parsed = ModeName::from_str(mode.cli_name(), false)
                .unwrap_or_else(|_| panic!("--mode rejects {}", mode.cli_name()));
            assert_eq!(parsed.view_mode(), *mode);
        }
    }

    #[test]
    fn file_path_joins_dir_and_mode_filename() {
        assert_eq!(
            ViewMode::Waiting.file_path("/tmp/todotxt"),
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
    fn recurrence_value_is_left_out_when_nothing_recurred() {
        assert_eq!(recurrence_value(&todo::Recurred::None), None);
    }

    #[test]
    fn recurrence_value_reports_the_new_id_and_its_dates() {
        let value = recurrence_value(&todo::Recurred::Created {
            id: "new-id".to_string(),
            t: None,
            due: chrono::NaiveDate::from_ymd_opt(2026, 9, 15),
        })
        .expect("a created occurrence carries a recurrence field");

        assert_eq!(value["id"], "new-id");
        assert_eq!(value["due"], "2026-09-15");
        assert!(value["t"].is_null(), "an unset date stays null: {value}");
    }

    #[test]
    fn recurrence_value_reports_the_pattern_that_could_not_be_used() {
        let value = recurrence_value(&todo::Recurred::Failed {
            value: "1b".to_string(),
            error: RecurrenceError::UnsupportedUnit('b'),
        })
        .expect("a failed recurrence carries a recurrence field");

        assert_eq!(value["value"], "1b");
        assert_eq!(value["error"], "unsupported unit 'b'");
        assert!(value.get("id").is_none(), "nothing was created: {value}");
    }

    #[test]
    fn priority_value_as_char_maps_the_letters() {
        assert_eq!(PriorityValue::A.as_char(), Some('A'));
        assert_eq!(PriorityValue::B.as_char(), Some('B'));
        assert_eq!(PriorityValue::C.as_char(), Some('C'));
        assert_eq!(PriorityValue::D.as_char(), Some('D'));
        assert_eq!(PriorityValue::E.as_char(), Some('E'));
    }

    #[test]
    fn priority_value_none_clears_the_priority() {
        assert_eq!(PriorityValue::None.as_char(), None);
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
