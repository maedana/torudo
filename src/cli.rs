use crate::app_state::ViewMode;
use crate::md_preview;
use crate::recurrence::parse_pattern;
use crate::todo::{self, Item, Recurred};
use crate::url::is_url;
use clap::{Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::json;
use std::error::Error;
use std::path::Path;

/// Actions a GTD mode scopes (`torudo <mode> <action>`). Only these two: the mode
/// is a place here — one to put an item into, one to read out of. Everything else
/// acts on an item, which its `id:` already identifies, so it hangs off the top
/// level instead and finds the mode itself.
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
        /// Add each item's detail md (`todos/{id}.md`) as an `md` field. JSON only
        #[arg(long, requires = "json")]
        with_md: bool,
    },
}

/// The item-scoped commands, the ones `torudo <verb> <id>` reaches. They live
/// together because they share a first step: find the mode holding `id`, which
/// [`ItemAction::run`] does once for all of them.
#[derive(Subcommand)]
pub enum ItemAction {
    /// Set GTD tags on an item and print it as JSON
    Set {
        /// Id of the item to update
        id: String,
        /// Estimated time to finish (`time:` tag). `none` removes the tag.
        #[arg(long, value_enum, ignore_case = true)]
        time: Option<TimeValue>,
        /// Energy the item demands (`energy:` tag). `none` removes the tag.
        #[arg(long, value_enum, ignore_case = true)]
        energy: Option<EnergyValue>,
        /// How often the item repeats once completed (`rec:` tag), e.g. `1w`, `+1m`.
        /// `none` removes the tag.
        #[arg(long, value_parser = parse_rec_value)]
        rec: Option<String>,
        /// Priority letter (`A`-`E`). `none` clears it.
        #[arg(long, value_enum, ignore_case = true)]
        priority: Option<PriorityValue>,
        /// Threshold date (`t:` tag) as `YYYY-MM-DD`: the item stays in the list
        /// but sinks to the bottom until then. `none` removes the tag.
        #[arg(long, value_parser = parse_date_value)]
        threshold: Option<String>,
        /// Due date (`due:` tag) as `YYYY-MM-DD`. `none` removes the tag.
        #[arg(long, value_parser = parse_date_value)]
        due: Option<String>,
        /// Project (`+project` tag), with or without the `+`. Replaces every
        /// project the item has. `none` removes them all.
        #[arg(long, value_parser = parse_project_value)]
        project: Option<String>,
        /// Context (`@context` tag), with or without the `@`. Replaces every
        /// context the item has. `none` removes them all.
        #[arg(long, value_parser = parse_context_value)]
        context: Option<String>,
        /// Title: the item's description. Its priority, dates, tags and URLs stay
        /// as they are. A word that would read as a tag (`+x`, `@x`, `key:value`)
        /// is refused rather than quietly becoming one.
        #[arg(long, value_parser = parse_title_value)]
        title: Option<String>,
    },
    /// Append URLs to an item's line, or drop them, and print it as JSON
    Link {
        /// Id of the item to update
        id: String,
        /// URLs to append. One the item already carries is left as it is.
        #[arg(value_parser = parse_url_value)]
        urls: Vec<String>,
        /// URL to drop from the line. Repeatable, and dropping one the item does
        /// not carry is not an error. Pass it alongside a URL to append to swap
        /// one for the other in a single write.
        #[arg(long, value_parser = parse_url_value)]
        remove: Vec<String>,
    },
    /// Complete an item, moving it to done.txt, and print it as JSON
    Complete {
        /// Id of the item to complete
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

/// The one value that removes a tag instead of setting one, shared by `--rec`,
/// `--threshold`, `--due`, `--project` and `--context`. Spelled in any case on the command line — the
/// parsers normalise to this lowercase form, which `set_or_clear` compares
/// against — matching `--priority`, `--time` and `--energy none`, which clap
/// already accepts either way.
const NONE_VALUE: &str = "none";

/// Reject a bad `rec:` pattern at the argument boundary, the way `--time` and
/// `--energy` are rejected by their `value_enum`. [`NONE_VALUE`] is let through
/// as the one non-pattern value, in any case, normalised to lowercase so
/// `set_or_clear` still recognises it.
fn parse_rec_value(value: &str) -> Result<String, String> {
    if value.eq_ignore_ascii_case(NONE_VALUE) {
        return Ok(NONE_VALUE.to_string());
    }
    parse_pattern(value).map_err(|error| error.to_string())?;
    Ok(value.to_string())
}

/// Reject a bad date at the argument boundary, the way `--rec` rejects a bad
/// pattern. [`NONE_VALUE`] is let through as the one non-date value, in any
/// case, normalised to lowercase so `set_or_clear` still recognises it. Only an
/// absolute date is accepted — "next monday" is the caller's job to resolve —
/// and the parsed date is written back out so `2026-8-1` lands as `2026-08-01`.
fn parse_date_value(value: &str) -> Result<String, String> {
    if value.eq_ignore_ascii_case(NONE_VALUE) {
        return Ok(NONE_VALUE.to_string());
    }
    let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| "expected a date as YYYY-MM-DD, or none to clear it".to_string())?;
    Ok(date.format("%Y-%m-%d").to_string())
}

/// Reject a tag name that would not round-trip as one `{sigil}name` token. A
/// leading `sigil` is accepted and dropped so `foo` and `+foo` mean the same.
/// [`NONE_VALUE`] is let through in any case, normalised to lowercase so the
/// caller can recognise it.
fn parse_tag_value(value: &str, sigil: char, what: &str) -> Result<String, String> {
    if value.eq_ignore_ascii_case(NONE_VALUE) {
        return Ok(NONE_VALUE.to_string());
    }
    let name = value.strip_prefix(sigil).unwrap_or(value);
    if name.is_empty() {
        return Err(format!("expected a {what} name, or none to clear it"));
    }
    if name.split_whitespace().count() != 1 {
        return Err(format!("`{value}` contains whitespace"));
    }
    Ok(name.to_string())
}

fn parse_project_value(value: &str) -> Result<String, String> {
    parse_tag_value(value, '+', "project")
}

fn parse_context_value(value: &str) -> Result<String, String> {
    parse_tag_value(value, '@', "context")
}

/// Accept a title only if `Item::parse` would read all of it back as the
/// description. Anything else — `+x`, `@x`, `key:value`, or a priority, date or
/// `x` at its head — would land on the line as a tag, and the item would change
/// in a way the caller never asked for.
fn parse_title_value(value: &str) -> Result<String, String> {
    let title = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return Err("expected a title".to_string());
    }
    let item = Item::parse(&title, 0);
    if item.description != title {
        return Err(format!(
            "`{title}` would not read back as a title: a word in it parses as a priority, date or tag"
        ));
    }
    Ok(title)
}

/// Reject anything that is not a URL at the argument boundary, the way `--rec`
/// and `--threshold` reject a bad pattern or date. The rule is [`is_url`], the one
/// the TUI opens links by. Whitespace is refused on top of it: a todo.txt line is
/// whitespace-delimited, so a value carrying a space would land as two tokens and
/// neither would round-trip.
fn parse_url_value(value: &str) -> Result<String, String> {
    if !is_url(value) {
        return Err(format!("`{value}` is not an http:// or https:// URL"));
    }
    if value.split_whitespace().count() != 1 {
        return Err(format!("`{value}` contains whitespace"));
    }
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
    /// Removes the tag instead of setting one.
    // A variant for the same reason as `PriorityValue::None`.
    None,
}

impl TimeValue {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Short => "short",
            Self::Medium => "medium",
            Self::Long => "long",
            Self::None => NONE_VALUE,
        }
    }
}

#[derive(Clone, Copy, ValueEnum)]
pub enum EnergyValue {
    Low,
    High,
    /// Removes the tag instead of setting one.
    // A variant for the same reason as `PriorityValue::None`.
    None,
}

impl EnergyValue {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
            Self::None => NONE_VALUE,
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
    /// Clears the priority instead of setting one.
    // Plays the same role that `NONE_VALUE` does for `--rec`, `--threshold` and
    // `--due`; it is a variant rather than that sentinel because `--priority` is
    // a closed `value_enum`. Kept off the doc comment, which clap prints in
    // `--help`, so the internal name stays out of user-facing text.
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

/// Apply a nullable tag flag: `none` clears the tag, anything else sets it. The
/// value has already been validated at the argument boundary.
fn set_or_clear(file: &str, id: &str, key: &str, value: &str) -> Result<(), Box<dyn Error>> {
    if value == NONE_VALUE {
        todo::clear_key_value(file, id, key)
    } else {
        todo::set_key_value(file, id, key, value)
    }
}

pub fn run(mode: ViewMode, action: &ModeAction, todotxt_dir: &str) -> Result<(), Box<dyn Error>> {
    let file = mode.file_path(todotxt_dir);
    match action {
        ModeAction::Add { text } => {
            let item = todo::add_item(&file, &text.join(" "))?;
            println!("{}", todo::item_to_json(&item, todotxt_dir)?);
        }
        ModeAction::List { json, with_md } => {
            print_items(
                &load_mode_items(&file)?,
                *json,
                with_md.then_some(todotxt_dir),
            )?;
        }
    }
    Ok(())
}

/// The GTD mode whose file holds `id`. This is the step every item-scoped command
/// starts with, and the reason they need no mode on the command line: an `id:` is
/// unique across the files, so naming the mode as well only created a way to name
/// the wrong one.
///
/// `done.txt` is searched last, and only to tell an archived item apart from a
/// typo. The two need different answers: a bad id is retyped, a completed one is
/// reopened. Writing to the archive is refused outright — `done.txt` is a record
/// of what happened, so an item has to come back into a mode before it can change
/// again.
fn locate_item(todotxt_dir: &str, id: &str) -> Result<ViewMode, Box<dyn Error>> {
    for mode in ViewMode::ALL {
        if todo::has_todo_with_id(&mode.file_path(todotxt_dir), id) {
            return Ok(*mode);
        }
    }
    if todo::has_todo_with_id(&done_file(todotxt_dir), id) {
        return Err(format!("item id:{id} is completed; reopen it first").into());
    }
    Err(format!("no item with id:{id} in any mode").into())
}

pub fn run_item(action: &ItemAction, todotxt_dir: &str) -> Result<(), Box<dyn Error>> {
    match action {
        ItemAction::Set {
            id,
            time,
            energy,
            rec,
            priority,
            threshold,
            due,
            project,
            context,
            title,
        } => {
            if time.is_none()
                && energy.is_none()
                && rec.is_none()
                && priority.is_none()
                && threshold.is_none()
                && due.is_none()
                && project.is_none()
                && context.is_none()
                && title.is_none()
            {
                return Err(
                    "specify at least one of --time, --energy, --rec, --priority, --threshold, --due, --project, --context or --title"
                        .into(),
                );
            }
            let file = locate_item(todotxt_dir, id)?.file_path(todotxt_dir);
            if let Some(time) = time {
                set_or_clear(&file, id, "time", time.as_str())?;
            }
            if let Some(energy) = energy {
                set_or_clear(&file, id, "energy", energy.as_str())?;
            }
            if let Some(rec) = rec {
                set_or_clear(&file, id, "rec", rec)?;
            }
            if let Some(priority) = priority {
                todo::set_priority(&file, id, priority.as_char())?;
            }
            if let Some(threshold) = threshold {
                set_or_clear(&file, id, "t", threshold)?;
            }
            if let Some(due) = due {
                set_or_clear(&file, id, "due", due)?;
            }
            if let Some(project) = project {
                let project = (project != NONE_VALUE).then_some(project.as_str());
                todo::set_tag(&file, id, '+', project)?;
            }
            if let Some(context) = context {
                let context = (context != NONE_VALUE).then_some(context.as_str());
                todo::set_tag(&file, id, '@', context)?;
            }
            if let Some(title) = title {
                todo::set_title(&file, id, title)?;
            }
            print_item(&file, id, todotxt_dir)?;
        }
        ItemAction::Link { id, urls, remove } => {
            if urls.is_empty() && remove.is_empty() {
                return Err("specify at least one URL to append, or --remove".into());
            }
            let file = locate_item(todotxt_dir, id)?.file_path(todotxt_dir);
            todo::edit_urls(&file, id, urls, remove)?;
            print_item(&file, id, todotxt_dir)?;
        }
        ItemAction::Complete { id } => {
            let mode = locate_item(todotxt_dir, id)?;
            // Matches the TUI, where `x` only works in Todo and Waiting.
            if !matches!(mode, ViewMode::Todo | ViewMode::Waiting) {
                return Err(format!(
                    "cannot complete from {}; promote it to Todo first",
                    mode.label()
                )
                .into());
            }
            let recurred = todo::mark_complete(&mode.file_path(todotxt_dir), id)?;
            // The item is completed either way, so a bad pattern is a warning on
            // stderr rather than a non-zero exit.
            if let Recurred::Failed { value, error } = &recurred {
                eprintln!("warning: rec:{value} is invalid ({error}); no next occurrence created");
            }
            print_completed_item(&done_file(todotxt_dir), id, todotxt_dir, &recurred)?;
        }
        ItemAction::Promote { id, to } => {
            let target = to.view_mode();
            let mode = locate_item(todotxt_dir, id)?;
            if target == mode {
                return Err(format!("item is already in {}", mode.label()).into());
            }
            let target_file = target.file_path(todotxt_dir);
            todo::move_to_file(&mode.file_path(todotxt_dir), &target_file, id)?;
            print_item(&target_file, id, todotxt_dir)?;
        }
    }
    Ok(())
}

/// `reopen` is the one item command that takes a mode, because here the mode is
/// where the item is going rather than where it is: the item is in `done.txt`, and
/// nothing records which file it left. Todo is the default because that is where a
/// mistaken completion nearly always belongs.
pub fn run_reopen(id: &str, to: ModeName, todotxt_dir: &str) -> Result<(), Box<dyn Error>> {
    let file = to.view_mode().file_path(todotxt_dir);
    if !todo::reopen_item(&done_file(todotxt_dir), &file, id)? {
        return Err(format!("no completed item with id:{id} in done.txt").into());
    }
    print_item(&file, id, todotxt_dir)?;
    Ok(())
}

/// Actions available under the `done` subcommand. `done.txt` is an archive, not a
/// GTD mode, so it is read-only here — items leave it through `torudo reopen`.
#[derive(Subcommand)]
pub enum DoneAction {
    /// List the completed items
    List {
        /// Print the items as a JSON array instead of one todo.txt line each
        #[arg(long)]
        json: bool,
        /// Add each item's detail md (`todos/{id}.md`) as an `md` field. JSON only
        #[arg(long, requires = "json")]
        with_md: bool,
    },
}

pub fn run_done(action: &DoneAction, todotxt_dir: &str) -> Result<(), Box<dyn Error>> {
    match action {
        DoneAction::List { json, with_md } => print_items(
            &load_mode_items(&done_file(todotxt_dir))?,
            *json,
            with_md.then_some(todotxt_dir),
        ),
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
    /// The detail md body, and only under `--with-md`. Absent drops the key
    /// instead of printing null — the opposite of `id` above, deliberately: this is
    /// the shape `item_to_value` already gives `add`, `set`, `show` and `list
    /// --with-md`, so one `md` in the object means one thing everywhere. `id: null`
    /// is itself an answer ("the item exists but cannot be focused"), whereas a null
    /// `md` could not say whether the file was missing or the flag was left off.
    #[serde(skip_serializing_if = "Option::is_none")]
    md: Option<String>,
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
    with_md: bool,
    mode: Option<ModeName>,
    todotxt_dir: &str,
) -> Result<(), Box<dyn Error>> {
    if query.is_empty() {
        return Err("query must not be empty".into());
    }
    let mut hits = Vec::new();
    for mode in search_targets(mode) {
        hits.extend(collect_hits(mode, todotxt_dir, query, with_md)?);
    }
    print_hits(&hits, json)
}

/// A missing mode file simply contributes no hits.
fn collect_hits(
    mode: ViewMode,
    todotxt_dir: &str,
    query: &str,
    with_md: bool,
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
                // The body was read to match against anyway, so `--with-md` costs no
                // extra read — it only decides whether to hand it back.
                md: if with_md { md } else { None },
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

/// The JSON body of a `list`. With `md_dir` set — that is, with `--with-md` — every
/// item goes through `todo::item_to_value`, the same merge `add` and `set` print, so
/// an item whose `todos/{id}.md` is missing or unreadable simply carries no `md` key
/// rather than a null one.
fn items_to_values(
    items: &[Item],
    md_dir: Option<&str>,
) -> Result<Vec<serde_json::Value>, Box<dyn Error>> {
    items
        .iter()
        .map(|item| {
            md_dir.map_or_else(
                || serde_json::to_value(item).map_err(Into::into),
                |dir| todo::item_to_value(item, dir),
            )
        })
        .collect()
}

fn print_items(items: &[Item], json: bool, md_dir: Option<&str>) -> Result<(), Box<dyn Error>> {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&items_to_values(items, md_dir)?)?
        );
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

/// The mode label `show` reports for an item that has already been completed.
/// `done.txt` is an archive rather than a `ViewMode`, so it needs its own name.
const DONE_MODE: &str = "done";

/// Look an item up by id across every mode, the archive included. The GTD modes come
/// first, so an id that somehow sits in both is reported where it can still be acted
/// on. `search` skips `done.txt` because a completed item cannot be focused; `show`
/// only reads, so the archive belongs in its sweep — `reopen` decisions need the md
/// of something already completed.
fn find_item_anywhere(
    todotxt_dir: &str,
    id: &str,
) -> Result<Option<(&'static str, Item)>, Box<dyn Error>> {
    for mode in ViewMode::ALL {
        if let Some(item) = find_in_file(&mode.file_path(todotxt_dir), id)? {
            return Ok(Some((mode.cli_name(), item)));
        }
    }
    Ok(find_in_file(&done_file(todotxt_dir), id)?.map(|item| (DONE_MODE, item)))
}

/// One item by id, md included, with the mode it was found in folded into the same
/// object — the way `complete` folds in `recurrence`. Always JSON: the point of
/// `show` is the md body, which has no useful one-line rendering.
pub fn run_show(id: &str, todotxt_dir: &str) -> Result<(), Box<dyn Error>> {
    let (mode, item) = find_item_anywhere(todotxt_dir, id)?
        .ok_or_else(|| format!("no item with id:{id} in any mode"))?;
    let mut json = todo::item_to_value(&item, todotxt_dir)?;
    json["mode"] = serde_json::Value::String(mode.to_string());
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

/// The id lookup `find_item` and `find_item_anywhere` share: one file in, at most one
/// item out. Keeping the match in one place means a change to what counts as the same
/// id lands everywhere at once.
fn find_in_file(file: &str, id: &str) -> Result<Option<Item>, Box<dyn Error>> {
    Ok(load_mode_items(file)?
        .into_iter()
        .find(|item| item.id.as_deref() == Some(id)))
}

fn find_item(file: &str, id: &str) -> Result<Item, Box<dyn Error>> {
    find_in_file(file, id)?.ok_or_else(|| format!("item id:{id} is gone from {file}").into())
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
        assert_eq!(parse_rec_value(NONE_VALUE), Ok(NONE_VALUE.to_string()));
    }

    #[test]
    fn parse_rec_value_accepts_none_whatever_the_case() {
        // The returned value feeds `set_or_clear`, which compares it against the
        // lowercase `NONE_VALUE`, so it has to come back normalised.
        assert_eq!(parse_rec_value("NONE"), Ok(NONE_VALUE.to_string()));
        assert_eq!(parse_rec_value("None"), Ok(NONE_VALUE.to_string()));
    }

    #[test]
    fn parse_rec_value_still_rejects_a_bad_pattern() {
        assert_eq!(
            parse_rec_value("banana"),
            Err(RecurrenceError::Malformed.to_string())
        );
    }

    #[test]
    fn parse_date_value_accepts_an_iso_date() {
        assert_eq!(parse_date_value("2026-08-24"), Ok("2026-08-24".to_string()));
    }

    #[test]
    fn parse_date_value_accepts_none_as_the_clearing_keyword() {
        assert_eq!(parse_date_value(NONE_VALUE), Ok(NONE_VALUE.to_string()));
    }

    #[test]
    fn parse_date_value_accepts_none_whatever_the_case() {
        assert_eq!(parse_date_value("NONE"), Ok(NONE_VALUE.to_string()));
        assert_eq!(parse_date_value("None"), Ok(NONE_VALUE.to_string()));
    }

    #[test]
    fn parse_date_value_normalises_a_short_date() {
        assert_eq!(parse_date_value("2026-8-1"), Ok("2026-08-01".to_string()));
    }

    #[test]
    fn parse_date_value_rejects_a_relative_date() {
        assert_eq!(
            parse_date_value("next monday"),
            Err("expected a date as YYYY-MM-DD, or none to clear it".to_string())
        );
    }

    #[test]
    fn parse_date_value_rejects_an_impossible_date() {
        assert_eq!(
            parse_date_value("2026-02-30"),
            Err("expected a date as YYYY-MM-DD, or none to clear it".to_string())
        );
    }

    #[test]
    fn parse_project_value_accepts_a_bare_name() {
        assert_eq!(parse_project_value("torudo"), Ok("torudo".to_string()));
    }

    #[test]
    fn parse_project_value_strips_a_leading_plus() {
        assert_eq!(parse_project_value("+torudo"), Ok("torudo".to_string()));
    }

    #[test]
    fn parse_project_value_accepts_none_whatever_the_case() {
        assert_eq!(parse_project_value("None"), Ok(NONE_VALUE.to_string()));
    }

    #[test]
    fn parse_project_value_rejects_an_empty_name() {
        assert!(parse_project_value("+").is_err());
    }

    #[test]
    fn parse_project_value_rejects_whitespace() {
        assert!(parse_project_value("two words").is_err());
    }

    #[test]
    fn parse_context_value_strips_a_leading_at_sign() {
        assert_eq!(parse_context_value("@pc"), Ok("pc".to_string()));
    }

    #[test]
    fn parse_context_value_accepts_none_whatever_the_case() {
        assert_eq!(parse_context_value("NONE"), Ok(NONE_VALUE.to_string()));
    }

    #[test]
    fn parse_title_value_normalises_whitespace() {
        assert_eq!(
            parse_title_value("  Rewrite   the docs "),
            Ok("Rewrite the docs".to_string())
        );
    }

    #[test]
    fn parse_title_value_rejects_an_empty_title() {
        assert!(parse_title_value("   ").is_err());
    }

    #[test]
    fn parse_title_value_rejects_a_word_that_would_parse_as_a_tag() {
        for title in ["Write +docs", "Ask @bob", "Pay due:2026-09-01", "Fix id:t9"] {
            assert!(
                parse_title_value(title).is_err(),
                "{title} should be refused"
            );
        }
    }

    #[test]
    fn parse_title_value_rejects_a_head_that_would_parse_as_priority_or_date() {
        for title in ["(A) Write docs", "2026-08-01 Write docs", "x Write docs"] {
            assert!(
                parse_title_value(title).is_err(),
                "{title} should be refused"
            );
        }
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
            md: None,
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
            md: None,
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
    fn items_to_values_without_an_md_dir_matches_plain_item_serialisation() {
        // Without the flag the objects have to carry exactly what `Serialize` gives —
        // nothing added, nothing dropped. Value equality, not byte equality: routing
        // through `serde_json::Value` sorts the keys, so `list --json` now prints them
        // alphabetically, the way `add` and `set` always have, rather than in the
        // struct's declaration order.
        let items = vec![
            Item::parse("(A) Buy milk +grocery @home id:aaa", 1),
            Item::parse("No id here", 2),
        ];
        assert_eq!(
            serde_json::Value::Array(items_to_values(&items, None).unwrap()),
            serde_json::to_value(&items).unwrap()
        );
    }

    #[test]
    fn items_to_values_leaves_the_md_key_out_when_the_directory_holds_nothing() {
        let items = vec![Item::parse("Buy milk id:aaa", 1)];
        let values = items_to_values(&items, Some("/tmp/torudo-does-not-exist")).unwrap();
        assert_eq!(values.len(), 1);
        assert!(
            values[0].get("md").is_none(),
            "an unreadable detail file drops the key: {}",
            values[0]
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
