use crate::md_preview::MdMeta;
use crate::recurrence::{NextDates, RecurrenceError, next_dates, parse_pattern, reset_md};
use chrono::NaiveDate;
use log::{debug, error};
use serde::Serialize;
use std::{collections::HashMap, error::Error, fs};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub completed: bool,
    pub priority: Option<char>,
    pub creation_date: Option<NaiveDate>,
    pub completion_date: Option<NaiveDate>,
    #[serde(rename = "title")]
    pub description: String,
    pub projects: Vec<String>,
    pub contexts: Vec<String>,
    pub id: Option<String>,
    pub key_values: HashMap<String, String>,
    #[serde(skip)]
    pub line_number: usize,
    #[serde(skip)]
    pub md_meta: Option<MdMeta>,
}

impl Item {
    pub fn parse(line: &str, line_number: usize) -> Self {
        let mut parts = line.split_whitespace().peekable();
        let mut item = Self {
            completed: false,
            priority: None,
            creation_date: None,
            completion_date: None,
            description: String::new(),
            projects: Vec::new(),
            contexts: Vec::new(),
            id: None,
            key_values: HashMap::new(),
            line_number,
            md_meta: None,
        };
        let mut desc_parts = Vec::new();
        if parts.peek() == Some(&"x") {
            item.completed = true;
            parts.next();
        }
        // Consume leading priority and dates in any order.
        let mut leading_dates: Vec<NaiveDate> = Vec::new();
        while let Some(part) = parts.peek() {
            if item.priority.is_none()
                && let Some(c) = extract_priority_char(part)
            {
                item.priority = Some(c);
                parts.next();
                continue;
            }
            if leading_dates.len() < 2
                && let Ok(date) = NaiveDate::parse_from_str(part, "%Y-%m-%d")
            {
                leading_dates.push(date);
                parts.next();
                continue;
            }
            break;
        }
        if item.completed {
            // Per the todo.txt spec, a completion date implies a creation date follows.
            // A completed line without a creation date is technically invalid, but we
            // accept it leniently and treat the single date as the completion date.
            match leading_dates.len() {
                2 => {
                    item.completion_date = Some(leading_dates[0]);
                    item.creation_date = Some(leading_dates[1]);
                }
                1 => item.completion_date = Some(leading_dates[0]),
                _ => {}
            }
        } else if let Some(d) = leading_dates.first() {
            item.creation_date = Some(*d);
        }
        for part in parts {
            if let Some(stripped) = part.strip_prefix('+') {
                item.projects.push(stripped.to_string());
            } else if let Some(stripped) = part.strip_prefix('@') {
                item.contexts.push(stripped.to_string());
            } else if let Some(stripped) = part.strip_prefix("id:") {
                item.id = Some(stripped.to_string());
            } else if let Some((key, value)) = split_key_value(part) {
                item.key_values.insert(key.to_string(), value.to_string());
            } else {
                desc_parts.push(part);
            }
        }
        item.description = desc_parts.join(" ");
        item
    }

    pub fn threshold_date(&self) -> Option<NaiveDate> {
        self.parse_key_date("t")
    }

    pub fn is_threshold_pending(&self, today: NaiveDate) -> bool {
        self.threshold_date().is_some_and(|d| d > today)
    }

    pub fn due_date(&self) -> Option<NaiveDate> {
        self.parse_key_date("due")
    }

    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        self.due_date().is_some_and(|d| d <= today)
    }

    pub fn time_estimate(&self) -> Option<&str> {
        match self.key_values.get("time").map(String::as_str) {
            Some(v @ ("short" | "medium" | "long")) => Some(v),
            _ => None,
        }
    }

    pub fn energy_estimate(&self) -> Option<&str> {
        match self.key_values.get("energy").map(String::as_str) {
            Some(v @ ("low" | "high")) => Some(v),
            _ => None,
        }
    }

    /// The `rec:` value, but only when it parses. Matches `time_estimate` and
    /// `energy_estimate`: a value that survives this is safe to display as is.
    /// Code that needs the raw text (to report why it is bad) reads `key_values`.
    pub fn recurrence(&self) -> Option<&str> {
        let value = self.key_values.get("rec")?;
        crate::recurrence::parse_pattern(value).ok()?;
        Some(value)
    }

    fn parse_key_date(&self, key: &str) -> Option<NaiveDate> {
        self.key_values
            .get(key)
            .and_then(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
    }
}

fn extract_priority_char(part: &str) -> Option<char> {
    let bytes = part.as_bytes();
    if bytes.len() == 3 && bytes[0] == b'(' && bytes[2] == b')' && bytes[1].is_ascii_uppercase() {
        Some(bytes[1] as char)
    } else {
        None
    }
}

/// Insert `date` after the leading priority (if any) in `text`.
/// Used by both `add_item` (creation date) and `mark_complete` (completion date).
fn insert_date_after_priority(text: &str, date: &str) -> String {
    let (priority, rest) = split_priority_prefix(text);
    priority.map_or_else(
        || format!("{date} {rest}"),
        |pri| format!("{pri} {date} {rest}"),
    )
}

fn split_key_value(part: &str) -> Option<(&str, &str)> {
    if part.contains("://") {
        return None;
    }
    let (key, value) = part.split_once(':')?;
    if key.is_empty() || value.is_empty() {
        return None;
    }
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((key, value))
}

pub fn load_todos(file_path: &str) -> Result<Vec<Item>, Box<dyn Error>> {
    let content = fs::read_to_string(file_path)?;
    let mut todos: Vec<Item> = content
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(line_num, line)| Item::parse(line, line_num + 1))
        .collect();

    let today = chrono::Local::now().date_naive();
    sort_todos(&mut todos, today);

    Ok(todos)
}

/// Order: threshold-reached items first (pending ones at the bottom),
/// then by priority (A < B < C < None), then by line number.
fn sort_todos(todos: &mut [Item], today: NaiveDate) {
    todos.sort_by(|a, b| {
        let a_pending = a.is_threshold_pending(today);
        let b_pending = b.is_threshold_pending(today);
        a_pending
            .cmp(&b_pending)
            .then_with(|| match (a.priority, b.priority) {
                (Some(p1), Some(p2)) => p1.cmp(&p2),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| a.line_number.cmp(&b.line_number))
    });
}

pub fn add_missing_ids(file_path: &str) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(file_path)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut modified = false;
    let mut new_lines = Vec::new();

    for (line_num, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            new_lines.push(line.to_string());
            continue;
        }

        let todo = Item::parse(line, line_num + 1);
        if todo.id.is_none() {
            let new_id = Uuid::new_v4().to_string();
            let new_line = format!("{line} id:{new_id}");
            new_lines.push(new_line);
            modified = true;
        } else {
            new_lines.push(line.to_string());
        }
    }

    if modified {
        let new_content = new_lines.join("\n");
        debug!(
            "Adding missing IDs to {} lines in todo file",
            usize::from(modified)
        );
        fs::write(file_path, new_content)?;
    }

    Ok(())
}

fn split_priority_prefix(line: &str) -> (Option<&str>, &str) {
    if line.starts_with('(') && line.len() >= 4 && line.chars().nth(2) == Some(')') {
        (Some(&line[..3]), line[3..].trim_start())
    } else {
        (None, line)
    }
}

/// What `mark_complete` did about the item's `rec:` tag, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recurred {
    /// No `rec:` tag, or the line was already completed.
    None,
    Created {
        id: String,
        t: Option<NaiveDate>,
        due: Option<NaiveDate>,
    },
    /// The `rec:` value could not be used. Completion still succeeded.
    Failed {
        value: String,
        error: RecurrenceError,
    },
}

/// Build the line for the next occurrence of a recurring item, or report why
/// there is none. Pure: the caller writes the line out and carries the detail
/// file over. A bad `rec:` value is reported, never fatal.
fn next_occurrence(
    original_line: &str,
    item: &Item,
    today: NaiveDate,
) -> (Option<String>, Recurred) {
    let Some(value) = item.key_values.get("rec") else {
        return (None, Recurred::None);
    };
    let failed = |error| {
        (
            None,
            Recurred::Failed {
                value: value.clone(),
                error,
            },
        )
    };
    let pattern = match parse_pattern(value) {
        Ok(pattern) => pattern,
        Err(error) => return failed(error),
    };
    let next = match next_dates(pattern, item.threshold_date(), item.due_date(), today) {
        Ok(next) => next,
        Err(error) => return failed(error),
    };

    let new_id = Uuid::new_v4().to_string();
    let line = next_occurrence_line(original_line, next, &new_id, today);
    debug!("Next occurrence {new_id} of {:?}", item.id);
    (
        Some(line),
        Recurred::Created {
            id: new_id,
            t: next.t,
            due: next.due,
        },
    )
}

/// Copy `{old_id}.md` to `{new_id}.md` with its checkboxes cleared. An item
/// without a detail file simply has nothing to carry over.
fn carry_over_md(todotxt_dir: &str, old_id: &str, new_id: &str) -> std::io::Result<()> {
    let Ok(content) = fs::read_to_string(crate::md_preview::md_path(todotxt_dir, old_id)) else {
        return Ok(());
    };
    fs::write(
        crate::md_preview::md_path(todotxt_dir, new_id),
        reset_md(&content),
    )
}

pub fn mark_complete(todo_file: &str, todo_id: &str) -> Result<Recurred, Box<dyn Error>> {
    let content = fs::read_to_string(todo_file)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::new();
    let mut completed_line = None;
    let mut recurring = None;
    let today = chrono::Local::now().date_naive();
    let today_str = today.format(DATE_FORMAT).to_string();

    for (line_num, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            new_lines.push(line.to_string());
            continue;
        }

        let todo = Item::parse(line, line_num + 1);
        if todo.id.as_deref() == Some(todo_id) {
            // An already completed line just moves across; recurring it again would
            // spawn a duplicate.
            let completed_todo_line = if todo.completed {
                line.to_string()
            } else {
                recurring = Some(((*line).to_string(), todo));
                format!("x {}", insert_date_after_priority(line, &today_str))
            };
            completed_line = Some(completed_todo_line);
            continue;
        }
        new_lines.push(line.to_string());
    }

    let Some(completed_todo) = completed_line else {
        return Ok(Recurred::None);
    };

    let todo_dir = std::path::Path::new(todo_file).parent().unwrap();
    let done_file = todo_dir.join("done.txt");

    debug!("Moving completed todo to done.txt: {completed_todo}");

    let mut done_content = if done_file.exists() {
        fs::read_to_string(&done_file)?
    } else {
        String::new()
    };

    if !done_content.is_empty() && !done_content.ends_with('\n') {
        done_content.push('\n');
    }
    done_content.push_str(&completed_todo);
    done_content.push('\n');

    fs::write(&done_file, done_content)?;

    // The next occurrence goes out with the same write, so completing and
    // recurring never leave the file half updated.
    let (next_line, recurred) = recurring
        .as_ref()
        .map_or((None, Recurred::None), |(line, item)| {
            next_occurrence(line, item, today)
        });
    if let Some(line) = next_line {
        new_lines.push(line);
    }
    // Trailing newline: `append_todo` used to add it on the way past, and a file
    // that ends mid-line trips up anything reading it outside torudo.
    let mut new_todo_content = new_lines.join("\n");
    if !new_todo_content.is_empty() {
        new_todo_content.push('\n');
    }
    fs::write(todo_file, new_todo_content)?;

    debug!("Successfully moved todo to done.txt and updated todo.txt");

    if let Recurred::Created { id: new_id, .. } = &recurred
        && let Some((_, item)) = &recurring
        && let Some(old_id) = &item.id
        && let Some(todotxt_dir) = todo_dir.to_str()
    {
        // Best effort: the completion and the next occurrence are already on
        // disk, so a detail file we cannot copy is reported, never fatal.
        if let Err(e) = carry_over_md(todotxt_dir, old_id, new_id) {
            error!("Could not carry {old_id}.md over to {new_id}.md: {e}");
        }
    }

    Ok(recurred)
}

/// Undo what `mark_complete` wrote: drop the leading `x` and the completion date
/// it inserted, leaving any priority and creation date in place.
fn strip_completion_marker(line: &str) -> String {
    let mut tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.first() != Some(&"x") {
        return line.to_string();
    }
    tokens.remove(0);
    if let Some(pos) = tokens
        .iter()
        .position(|token| NaiveDate::parse_from_str(token, "%Y-%m-%d").is_ok())
    {
        tokens.remove(pos);
    }
    tokens.join(" ")
}

/// Move a completed item out of `done.txt` and back into `dest_file`, clearing its
/// completion marker. Returns whether the id was found.
pub fn reopen_item(
    done_file: &str,
    dest_file: &str,
    todo_id: &str,
) -> Result<bool, Box<dyn Error>> {
    let content = fs::read_to_string(done_file)?;
    let mut kept = Vec::new();
    let mut reopened = None;

    for (line_num, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            kept.push(line.to_string());
            continue;
        }
        let item = Item::parse(line, line_num + 1);
        if reopened.is_none() && item.id.as_deref() == Some(todo_id) {
            reopened = Some(strip_completion_marker(line));
            continue;
        }
        kept.push(line.to_string());
    }

    let Some(line) = reopened else {
        return Ok(false);
    };

    append_todo(dest_file, &line)?;
    let mut out = kept.join("\n");
    if !out.is_empty() && content.ends_with('\n') {
        out.push('\n');
    }
    fs::write(done_file, out)?;
    debug!("Reopened {todo_id} from done.txt into {dest_file}");
    Ok(true)
}

pub fn delete_todo(todo_file: &str, todo_id: &str) -> Result<bool, Box<dyn Error>> {
    let content = fs::read_to_string(todo_file)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::with_capacity(lines.len());
    let mut removed = false;

    for (line_num, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            new_lines.push((*line).to_string());
            continue;
        }

        let todo = Item::parse(line, line_num + 1);
        if let Some(id) = &todo.id
            && id == todo_id
        {
            removed = true;
            continue;
        }
        new_lines.push((*line).to_string());
    }

    if removed {
        let mut out = new_lines.join("\n");
        if content.ends_with('\n') {
            out.push('\n');
        }
        fs::write(todo_file, out)?;
        debug!("Deleted todo {todo_id} from {todo_file}");
    }

    Ok(removed)
}

pub fn set_priority(
    todo_file: &str,
    todo_id: &str,
    priority: Option<char>,
) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(todo_file)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::with_capacity(lines.len());
    let mut changed = false;

    for (line_num, line) in lines.iter().enumerate() {
        if line.trim().is_empty() || line.starts_with("x ") {
            new_lines.push((*line).to_string());
            continue;
        }
        let todo = Item::parse(line, line_num + 1);
        if todo.id.as_deref() != Some(todo_id) {
            new_lines.push((*line).to_string());
            continue;
        }
        let (_, rest) = split_priority_prefix(line);
        let new_line = priority.map_or_else(|| rest.to_string(), |c| format!("({c}) {rest}"));
        new_lines.push(new_line);
        changed = true;
    }

    if changed {
        let mut out = new_lines.join("\n");
        if content.ends_with('\n') {
            out.push('\n');
        }
        fs::write(todo_file, out)?;
        debug!("Set priority {priority:?} on {todo_id} in {todo_file}");
    }
    Ok(())
}

const DATE_FORMAT: &str = "%Y-%m-%d";

/// Build the line for the next occurrence by copying the original and swapping the
/// tokens that must not carry over. Everything else — priority, description,
/// projects, contexts, `rec:`, `time:`, `energy:` — is kept as written.
fn next_occurrence_line(original: &str, next: NextDates, new_id: &str, today: NaiveDate) -> String {
    let mut line = replace_creation_date(original, &today.format(DATE_FORMAT).to_string());
    for (key, date) in [("t", next.t), ("due", next.due)] {
        line = date.map_or_else(
            || remove_key_value(&line, key),
            |d| replace_key_value(&line, key, &d.format(DATE_FORMAT).to_string()),
        );
    }
    replace_key_value(&line, "id", new_id)
}

/// Replace the creation date of an open line, inserting one when it has none.
fn replace_creation_date(line: &str, date: &str) -> String {
    let (priority, rest) = split_priority_prefix(line);
    let mut tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens
        .first()
        .is_some_and(|tok| NaiveDate::parse_from_str(tok, DATE_FORMAT).is_ok())
    {
        tokens.remove(0);
    }
    // Put the priority back so the shape of the line's head stays owned by
    // `insert_date_after_priority`.
    let without_date = priority.map_or_else(
        || tokens.join(" "),
        |pri| format!("{pri} {}", tokens.join(" ")),
    );
    insert_date_after_priority(&without_date, date)
}

/// Drop every `key:value` token for `key` from `line`.
fn remove_key_value(line: &str, key: &str) -> String {
    let key_prefix = format!("{key}:");
    let kept: Vec<&str> = line
        .split_whitespace()
        .filter(|tok| !tok.starts_with(&key_prefix))
        .collect();
    kept.join(" ")
}

/// Drop any `key:value` token from `line` and append the new one at the end.
fn replace_key_value(line: &str, key: &str, value: &str) -> String {
    format!("{} {key}:{value}", remove_key_value(line, key))
}

pub fn set_key_value(
    todo_file: &str,
    todo_id: &str,
    key: &str,
    value: &str,
) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(todo_file)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::with_capacity(lines.len());
    let mut changed = false;

    for (line_num, line) in lines.iter().enumerate() {
        if line.trim().is_empty() || line.starts_with("x ") {
            new_lines.push((*line).to_string());
            continue;
        }
        let todo = Item::parse(line, line_num + 1);
        if todo.id.as_deref() != Some(todo_id) {
            new_lines.push((*line).to_string());
            continue;
        }
        new_lines.push(replace_key_value(line, key, value));
        changed = true;
    }

    if changed {
        let mut out = new_lines.join("\n");
        if content.ends_with('\n') {
            out.push('\n');
        }
        fs::write(todo_file, out)?;
        debug!("Set {key}:{value} on {todo_id} in {todo_file}");
    }
    Ok(())
}

/// Drop every `key:value` token for `key` from the item with `todo_id`. Absent is
/// not an error: nothing matches, nothing is written.
pub fn clear_key_value(todo_file: &str, todo_id: &str, key: &str) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(todo_file)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::with_capacity(lines.len());
    let mut changed = false;

    for (line_num, line) in lines.iter().enumerate() {
        if line.trim().is_empty() || line.starts_with("x ") {
            new_lines.push((*line).to_string());
            continue;
        }
        let todo = Item::parse(line, line_num + 1);
        if todo.id.as_deref() != Some(todo_id) {
            new_lines.push((*line).to_string());
            continue;
        }
        let new_line = remove_key_value(line, key);
        changed |= new_line != *line;
        new_lines.push(new_line);
    }

    if changed {
        let mut out = new_lines.join("\n");
        if content.ends_with('\n') {
            out.push('\n');
        }
        fs::write(todo_file, out)?;
        debug!("Cleared {key}: on {todo_id} in {todo_file}");
    }
    Ok(())
}

/// Rewrite the line of the item with `todo_id` in place, handing it to `transform`
/// and leaving every other line — blank ones and completed ones included —
/// byte-identical. Nothing is written when the transform changes nothing, so a
/// no-op edit does not touch the file's mtime and wake the watcher. Reports
/// whether it wrote.
fn rewrite_item_line(
    todo_file: &str,
    todo_id: &str,
    transform: impl Fn(&str) -> String,
) -> Result<bool, Box<dyn Error>> {
    let content = fs::read_to_string(todo_file)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::with_capacity(lines.len());
    let mut changed = false;

    for (line_num, line) in lines.iter().enumerate() {
        if line.trim().is_empty() || line.starts_with("x ") {
            new_lines.push((*line).to_string());
            continue;
        }
        let todo = Item::parse(line, line_num + 1);
        if todo.id.as_deref() != Some(todo_id) {
            new_lines.push((*line).to_string());
            continue;
        }
        let new_line = transform(line);
        changed |= new_line != *line;
        new_lines.push(new_line);
    }

    if changed {
        let mut out = new_lines.join("\n");
        if content.ends_with('\n') {
            out.push('\n');
        }
        fs::write(todo_file, out)?;
    }
    Ok(changed)
}

/// Drop the URLs in `remove` from `line`, then append the ones in `add` that are
/// not on it already. URLs live in the description rather than in `key_values` —
/// `split_key_value` deliberately lets `://` tokens through — so they are matched
/// as whole whitespace-separated tokens, and only tokens that are themselves URLs
/// can be removed. Removal runs first so one call can swap one URL for another;
/// a URL handed to both therefore survives.
fn apply_url_edits(line: &str, add: &[String], remove: &[String]) -> String {
    let mut tokens: Vec<&str> = line
        .split_whitespace()
        .filter(|tok| !(crate::url::is_url(tok) && remove.iter().any(|url| url == tok)))
        .collect();
    for url in add {
        if !tokens.contains(&url.as_str()) {
            tokens.push(url);
        }
    }
    tokens.join(" ")
}

/// Append URLs to the item with `todo_id`, and drop others from it. The new ones
/// go at the end of the line, where `replace_key_value` already puts the tags it
/// writes. A URL the item already carries is not appended twice, and one it never
/// had is not an error to remove — the same "absent is not an error" stance as
/// [`clear_key_value`].
pub fn edit_urls(
    todo_file: &str,
    todo_id: &str,
    add: &[String],
    remove: &[String],
) -> Result<(), Box<dyn Error>> {
    if rewrite_item_line(todo_file, todo_id, |line| {
        apply_url_edits(line, add, remove)
    })? {
        debug!("Edited urls on {todo_id} in {todo_file}");
    }
    Ok(())
}

pub fn move_to_file(
    source_file: &str,
    dest_file: &str,
    todo_id: &str,
) -> Result<(), Box<dyn Error>> {
    let content = fs::read_to_string(source_file)?;
    let lines: Vec<&str> = content.lines().collect();
    let mut new_lines = Vec::new();
    let mut moved_line = None;

    for (line_num, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            new_lines.push(line.to_string());
            continue;
        }

        let todo = Item::parse(line, line_num + 1);
        if let Some(id) = &todo.id
            && id == todo_id
        {
            moved_line = Some(line.to_string());
            continue;
        }
        new_lines.push(line.to_string());
    }

    if let Some(line) = moved_line {
        append_todo(dest_file, &line)?;

        let new_source_content = new_lines.join("\n");
        fs::write(source_file, new_source_content)?;

        debug!("Moved todo to {dest_file}: {line}");
    }

    Ok(())
}

pub fn has_todo_with_id(file_path: &str, id: &str) -> bool {
    let Ok(content) = fs::read_to_string(file_path) else {
        return false;
    };
    let id_tag = format!("id:{id}");
    content
        .lines()
        .any(|line| line.split_whitespace().any(|word| word == id_tag))
}

/// Render an `Item` as a JSON object, merging in the contents of
/// `{todotxt_dir}/todos/{id}.md` as the `md` field when present. Callers that
/// need to add fields of their own work on this before printing.
pub fn item_to_value(item: &Item, todotxt_dir: &str) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut json = serde_json::to_value(item)?;
    if let Some(todo_id) = &item.id {
        let path = crate::md_preview::md_path(todotxt_dir, todo_id);
        if let Ok(content) = fs::read_to_string(&path) {
            json["md"] = serde_json::Value::String(content);
        }
    }
    Ok(json)
}

/// `item_to_value` rendered as pretty JSON.
pub fn item_to_json(item: &Item, todotxt_dir: &str) -> Result<String, Box<dyn Error>> {
    Ok(serde_json::to_string_pretty(&item_to_value(
        item,
        todotxt_dir,
    )?)?)
}

pub fn add_item(file_path: &str, text: &str) -> Result<Item, Box<dyn Error>> {
    let parsed = Item::parse(text, 0);
    let text_with_date = if parsed.creation_date.is_some() {
        text.to_string()
    } else {
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        insert_date_after_priority(text, &today)
    };
    let mut item = Item::parse(&text_with_date, 0);
    let final_line = if item.id.is_some() {
        text_with_date
    } else {
        let uuid = Uuid::new_v4().to_string();
        let line = format!("{text_with_date} id:{uuid}");
        item.id = Some(uuid);
        line
    };
    append_todo(file_path, &final_line)?;
    Ok(item)
}

pub fn append_todo(file_path: &str, line: &str) -> Result<(), Box<dyn Error>> {
    let mut content = if std::path::Path::new(file_path).exists() {
        fs::read_to_string(file_path)?
    } else {
        String::new()
    };
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(line);
    content.push('\n');
    fs::write(file_path, content)?;
    Ok(())
}

pub const PICK_A_COLUMN: &str = "(A)";
pub const PICK_HIGH_COLUMN: &str = "High";
pub const PICK_LOW_COLUMN: &str = "Low";
pub const PICK_UNSET_COLUMN: &str = "No Energy";

// (column name, whether shorter time estimates should sort first).
// `None` means no time-based ordering — only threshold-pending demotion applies.
const PICK_COLUMN_ORDER: &[(&str, Option<bool>)] = &[
    (PICK_A_COLUMN, None),
    (PICK_HIGH_COLUMN, Some(false)),
    (PICK_LOW_COLUMN, Some(true)),
    (PICK_UNSET_COLUMN, Some(false)),
];

fn time_rank(t: Option<&str>, short_first: bool) -> u8 {
    match t {
        Some("long") => {
            if short_first {
                2
            } else {
                0
            }
        }
        Some("medium") => 1,
        Some("short") => {
            if short_first {
                0
            } else {
                2
            }
        }
        _ => 3,
    }
}

pub fn group_todos_for_pick(
    todos: &[Item],
    today: NaiveDate,
) -> (Vec<String>, HashMap<String, Vec<Item>>) {
    let mut grouped: HashMap<String, Vec<Item>> = HashMap::new();

    for todo in todos {
        if todo.priority == Some('A') {
            grouped
                .entry(PICK_A_COLUMN.to_string())
                .or_default()
                .push(todo.clone());
        }
        let energy_col = match todo.energy_estimate() {
            Some("high") => PICK_HIGH_COLUMN,
            Some("low") => PICK_LOW_COLUMN,
            _ => PICK_UNSET_COLUMN,
        };
        grouped
            .entry(energy_col.to_string())
            .or_default()
            .push(todo.clone());
    }

    // Threshold-pending items sink below active ones in every column so that
    // `t:` dates still in the future don't crowd the top of the Pick list.
    for (col_name, short_first) in PICK_COLUMN_ORDER {
        if let Some(col) = grouped.get_mut(*col_name) {
            col.sort_by_cached_key(|t| {
                let time_key = short_first.map_or(0, |sf| time_rank(t.time_estimate(), sf));
                (t.is_threshold_pending(today), time_key)
            });
        }
    }

    let names: Vec<String> = PICK_COLUMN_ORDER
        .iter()
        .filter(|(c, _)| grouped.contains_key(*c))
        .map(|(s, _)| (*s).to_string())
        .collect();

    (names, grouped)
}

pub fn group_todos_by_project_owned(todos: &[Item]) -> HashMap<String, Vec<Item>> {
    let mut grouped = HashMap::new();
    for todo in todos {
        if todo.projects.is_empty() {
            grouped
                .entry("No Project".to_string())
                .or_insert_with(Vec::new)
                .push(todo.clone());
        } else {
            for project in &todo.projects {
                grouped
                    .entry(project.clone())
                    .or_insert_with(Vec::new)
                    .push(todo.clone());
            }
        }
    }
    grouped
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::fs;

    #[test]
    fn test_item_parse_simple_todo() {
        let line = "Buy groceries";
        let item = Item::parse(line, 1);

        assert!(!item.completed);
        assert_eq!(item.priority, None);
        assert_eq!(item.creation_date, None);
        assert_eq!(item.completion_date, None);
        assert_eq!(item.description, "Buy groceries");
        assert!(item.projects.is_empty());
        assert!(item.contexts.is_empty());
        assert_eq!(item.id, None);
    }

    #[test]
    fn test_item_parse_with_priority() {
        let line = "(A) Call Mom";
        let item = Item::parse(line, 1);

        assert!(!item.completed);
        assert_eq!(item.priority, Some('A'));
        assert_eq!(item.description, "Call Mom");
    }

    #[test]
    fn test_item_parse_with_creation_date() {
        let line = "2024-01-15 Review quarterly report";
        let item = Item::parse(line, 1);

        assert!(!item.completed);
        assert_eq!(
            item.creation_date,
            Some(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap())
        );
        assert_eq!(item.description, "Review quarterly report");
    }

    #[test]
    fn test_item_parse_completed_todo() {
        let line = "x 2024-01-20 2024-01-15 Complete project report";
        let item = Item::parse(line, 1);

        assert!(item.completed);
        assert_eq!(
            item.completion_date,
            Some(NaiveDate::from_ymd_opt(2024, 1, 20).unwrap())
        );
        assert_eq!(
            item.creation_date,
            Some(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap())
        );
        assert_eq!(item.priority, None);
        assert_eq!(item.description, "Complete project report");
    }

    #[test]
    fn test_item_parse_with_projects_and_contexts() {
        let line = "(C) Buy groceries +personal @errands @shopping";
        let item = Item::parse(line, 1);

        assert_eq!(item.priority, Some('C'));
        assert_eq!(item.description, "Buy groceries");
        assert_eq!(item.projects, vec!["personal"]);
        assert_eq!(item.contexts, vec!["errands", "shopping"]);
    }

    #[test]
    fn test_item_parse_with_id() {
        let line = "Learn Rust programming +learning @coding id:abc123";
        let item = Item::parse(line, 1);

        assert_eq!(item.description, "Learn Rust programming");
        assert_eq!(item.projects, vec!["learning"]);
        assert_eq!(item.contexts, vec!["coding"]);
        assert_eq!(item.id, Some("abc123".to_string()));
    }

    #[test]
    fn test_item_parse_complex_todo() {
        let line = "(A) 2024-01-10 Fix critical bug +work @urgent @coding id:bug-001";
        let item = Item::parse(line, 1);

        assert!(!item.completed);
        assert_eq!(item.priority, Some('A'));
        assert_eq!(
            item.creation_date,
            Some(NaiveDate::from_ymd_opt(2024, 1, 10).unwrap())
        );
        assert_eq!(item.description, "Fix critical bug");
        assert_eq!(item.projects, vec!["work"]);
        assert_eq!(item.contexts, vec!["urgent", "coding"]);
        assert_eq!(item.id, Some("bug-001".to_string()));
    }

    #[test]
    fn test_item_parse_image_canonical_format() {
        // Canonical todo.txt format from the official diagram:
        // x (A) <completion> <creation> description +project @context key:value
        let line =
            "x (A) 2016-05-20 2016-04-30 measure space for +chapelShelving @chapel due:2016-05-30";
        let item = Item::parse(line, 1);

        assert!(item.completed);
        assert_eq!(item.priority, Some('A'));
        assert_eq!(
            item.completion_date,
            Some(NaiveDate::from_ymd_opt(2016, 5, 20).unwrap())
        );
        assert_eq!(
            item.creation_date,
            Some(NaiveDate::from_ymd_opt(2016, 4, 30).unwrap())
        );
        assert_eq!(item.description, "measure space for");
        assert_eq!(item.projects, vec!["chapelShelving"]);
        assert_eq!(item.contexts, vec!["chapel"]);
        assert_eq!(
            item.key_values.get("due").map(String::as_str),
            Some("2016-05-30")
        );
    }

    #[test]
    fn test_item_parse_url_not_extracted_as_key_value() {
        let line = "Read article https://example.com/path due:2026-01-01";
        let item = Item::parse(line, 1);

        assert_eq!(item.description, "Read article https://example.com/path");
        assert_eq!(
            item.key_values.get("due").map(String::as_str),
            Some("2026-01-01")
        );
        assert!(!item.key_values.contains_key("https"));
    }

    #[test]
    fn test_item_parse_multiple_key_values_and_id_excluded() {
        let line = "Plan meeting due:2026-05-30 t:14:00 id:meet-1";
        let item = Item::parse(line, 1);

        assert_eq!(item.description, "Plan meeting");
        assert_eq!(item.id.as_deref(), Some("meet-1"));
        assert_eq!(
            item.key_values.get("due").map(String::as_str),
            Some("2026-05-30")
        );
        assert_eq!(item.key_values.get("t").map(String::as_str), Some("14:00"));
        assert!(!item.key_values.contains_key("id"));
    }

    #[test]
    fn test_load_todos_from_content() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_todo.txt");

        let content = r"(A) Call Mom +family @phone
Buy groceries +personal @errands
x 2024-01-15 (B) Review report +work @office
Learn Rust +learning @coding id:rust-001";

        fs::write(&test_file, content).unwrap();

        let todos = load_todos(test_file.to_str().unwrap()).unwrap();

        assert_eq!(todos.len(), 4);

        // Test first todo
        assert_eq!(todos[0].priority, Some('A'));
        assert_eq!(todos[0].description, "Call Mom");
        assert_eq!(todos[0].projects, vec!["family"]);
        assert_eq!(todos[0].contexts, vec!["phone"]);

        // Test completed todo (should be at index 1 after sorting: A, B, then no priority)
        assert!(todos[1].completed);
        assert_eq!(todos[1].priority, Some('B'));
        assert_eq!(todos[1].description, "Review report");

        // Test todo with ID (should be at index 3 after sorting)
        assert_eq!(todos[3].id, Some("rust-001".to_string()));

        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_group_todos_by_project() {
        let todos = vec![
            Item {
                completed: false,
                priority: Some('A'),
                creation_date: None,
                completion_date: None,
                description: "Task 1".to_string(),
                projects: vec!["work".to_string()],
                contexts: vec![],
                id: Some("1".to_string()),
                key_values: HashMap::new(),
                line_number: 1,
                md_meta: None,
            },
            Item {
                completed: false,
                priority: Some('B'),
                creation_date: None,
                completion_date: None,
                description: "Task 2".to_string(),
                projects: vec!["personal".to_string()],
                contexts: vec![],
                id: Some("2".to_string()),
                key_values: HashMap::new(),
                line_number: 2,
                md_meta: None,
            },
            Item {
                completed: false,
                priority: None,
                creation_date: None,
                completion_date: None,
                description: "Task 3".to_string(),
                projects: vec![],
                contexts: vec![],
                id: Some("3".to_string()),
                key_values: HashMap::new(),
                line_number: 3,
                md_meta: None,
            },
            Item {
                completed: false,
                priority: None,
                creation_date: None,
                completion_date: None,
                description: "Task 4".to_string(),
                projects: vec!["work".to_string(), "urgent".to_string()],
                contexts: vec![],
                id: Some("4".to_string()),
                key_values: HashMap::new(),
                line_number: 4,
                md_meta: None,
            },
        ];

        let grouped = group_todos_by_project_owned(&todos);

        assert_eq!(grouped.len(), 4); // work, personal, No Project, urgent
        assert_eq!(grouped.get("work").unwrap().len(), 2); // Task 1 and Task 4
        assert_eq!(grouped.get("personal").unwrap().len(), 1); // Task 2
        assert_eq!(grouped.get("No Project").unwrap().len(), 1); // Task 3
        assert_eq!(grouped.get("urgent").unwrap().len(), 1); // Task 4
    }

    fn pick_test_today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 4, 21).unwrap()
    }

    fn make_pick_item(
        desc: &str,
        priority: Option<char>,
        energy: Option<&str>,
        time: Option<&str>,
    ) -> Item {
        let mut kv = HashMap::new();
        if let Some(e) = energy {
            kv.insert("energy".to_string(), e.to_string());
        }
        if let Some(t) = time {
            kv.insert("time".to_string(), t.to_string());
        }
        Item {
            completed: false,
            priority,
            creation_date: None,
            completion_date: None,
            description: desc.to_string(),
            projects: vec![],
            contexts: vec![],
            id: Some(desc.to_string()),
            key_values: kv,
            line_number: 0,
            md_meta: None,
        }
    }

    #[test]
    fn group_todos_for_pick_puts_priority_a_in_a_column() {
        let today = pick_test_today();
        let todos = vec![
            make_pick_item("t1", Some('A'), None, None),
            make_pick_item("t2", Some('B'), None, None),
            make_pick_item("t3", None, None, None),
        ];
        let (_names, grouped) = group_todos_for_pick(&todos, today);
        let a_col = grouped.get("(A)").expect("(A) column exists");
        assert_eq!(a_col.len(), 1);
        assert_eq!(a_col[0].description, "t1");
    }

    #[test]
    fn group_todos_for_pick_groups_by_energy() {
        let today = pick_test_today();
        let todos = vec![
            make_pick_item("h1", None, Some("high"), None),
            make_pick_item("l1", None, Some("low"), None),
            make_pick_item("u1", None, None, None),
        ];
        let (_names, grouped) = group_todos_for_pick(&todos, today);
        assert_eq!(grouped.get("High").unwrap().len(), 1);
        assert_eq!(grouped.get("High").unwrap()[0].description, "h1");
        assert_eq!(grouped.get("Low").unwrap().len(), 1);
        assert_eq!(grouped.get("Low").unwrap()[0].description, "l1");
        assert_eq!(grouped.get("No Energy").unwrap().len(), 1);
        assert_eq!(grouped.get("No Energy").unwrap()[0].description, "u1");
    }

    #[test]
    fn group_todos_for_pick_duplicates_priority_a_into_energy_columns() {
        // A priority item with energy=high should show up in BOTH (A) and High columns
        let today = pick_test_today();
        let todos = vec![make_pick_item("ah", Some('A'), Some("high"), Some("long"))];
        let (_names, grouped) = group_todos_for_pick(&todos, today);
        assert_eq!(grouped.get("(A)").unwrap().len(), 1);
        assert_eq!(grouped.get("High").unwrap().len(), 1);
        assert_eq!(grouped.get("(A)").unwrap()[0].description, "ah");
        assert_eq!(grouped.get("High").unwrap()[0].description, "ah");
    }

    #[test]
    fn group_todos_for_pick_sorts_high_column_by_time_long_first() {
        let today = pick_test_today();
        let todos = vec![
            make_pick_item("hs", None, Some("high"), Some("short")),
            make_pick_item("hl", None, Some("high"), Some("long")),
            make_pick_item("hm", None, Some("high"), Some("medium")),
            make_pick_item("hu", None, Some("high"), None),
        ];
        let (_names, grouped) = group_todos_for_pick(&todos, today);
        let high = grouped.get("High").unwrap();
        let descs: Vec<&str> = high.iter().map(|t| t.description.as_str()).collect();
        assert_eq!(descs, vec!["hl", "hm", "hs", "hu"]);
    }

    #[test]
    fn group_todos_for_pick_sorts_low_column_by_time_short_first() {
        let today = pick_test_today();
        let todos = vec![
            make_pick_item("ll", None, Some("low"), Some("long")),
            make_pick_item("ls", None, Some("low"), Some("short")),
            make_pick_item("lm", None, Some("low"), Some("medium")),
            make_pick_item("lu", None, Some("low"), None),
        ];
        let (_names, grouped) = group_todos_for_pick(&todos, today);
        let low = grouped.get("Low").unwrap();
        let descs: Vec<&str> = low.iter().map(|t| t.description.as_str()).collect();
        assert_eq!(descs, vec!["ls", "lm", "ll", "lu"]);
    }

    #[test]
    fn group_todos_for_pick_puts_energy_missing_into_unset_column() {
        let today = pick_test_today();
        let todos = vec![
            make_pick_item("u1", None, None, Some("short")),
            make_pick_item("u2", Some('B'), None, None),
        ];
        let (_names, grouped) = group_todos_for_pick(&todos, today);
        let unset = grouped.get("No Energy").unwrap();
        assert_eq!(unset.len(), 2);
    }

    #[test]
    fn group_todos_for_pick_excludes_empty_columns_from_column_list() {
        // Only high-energy items -> only High column in names
        let today = pick_test_today();
        let todos = vec![make_pick_item("h1", None, Some("high"), Some("long"))];
        let (names, grouped) = group_todos_for_pick(&todos, today);
        assert_eq!(names, vec!["High".to_string()]);
        assert!(!grouped.contains_key("(A)"));
        assert!(!grouped.contains_key("Low"));
        assert!(!grouped.contains_key("No Energy"));
    }

    #[test]
    fn group_todos_for_pick_pending_threshold_items_sink_to_bottom_of_column() {
        let today = pick_test_today();
        let mut pending = make_pick_item("pending", None, Some("high"), Some("long"));
        pending
            .key_values
            .insert("t".to_string(), "2099-01-01".to_string());
        let active = make_pick_item("active", None, Some("high"), Some("short"));
        let todos = vec![pending, active];
        let (_names, grouped) = group_todos_for_pick(&todos, today);
        let high = grouped.get("High").unwrap();
        let descs: Vec<&str> = high.iter().map(|t| t.description.as_str()).collect();
        // Normally long > short, but pending sinks below active
        assert_eq!(descs, vec!["active", "pending"]);
    }

    #[test]
    fn group_todos_for_pick_pending_threshold_in_a_column_goes_bottom() {
        let today = pick_test_today();
        let mut pending = make_pick_item("p-a", Some('A'), None, None);
        pending
            .key_values
            .insert("t".to_string(), "2099-01-01".to_string());
        let active = make_pick_item("a-a", Some('A'), None, None);
        let todos = vec![pending, active];
        let (_names, grouped) = group_todos_for_pick(&todos, today);
        let a_col = grouped.get("(A)").unwrap();
        let descs: Vec<&str> = a_col.iter().map(|t| t.description.as_str()).collect();
        assert_eq!(descs, vec!["a-a", "p-a"]);
    }

    #[test]
    fn group_todos_for_pick_column_order_is_a_high_low_unset() {
        let today = pick_test_today();
        let todos = vec![
            make_pick_item("u", None, None, None),
            make_pick_item("l", None, Some("low"), Some("short")),
            make_pick_item("h", None, Some("high"), Some("long")),
            make_pick_item("a", Some('A'), None, None),
        ];
        let (names, _grouped) = group_todos_for_pick(&todos, today);
        assert_eq!(
            names,
            vec![
                "(A)".to_string(),
                "High".to_string(),
                "Low".to_string(),
                "No Energy".to_string()
            ]
        );
    }

    #[test]
    fn test_add_missing_ids() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_add_ids.txt");

        let content = r"(A) Call Mom +family @phone
Buy groceries +personal @errands id:existing-001
Learn Rust +learning @coding";

        fs::write(&test_file, content).unwrap();

        // Add missing IDs
        add_missing_ids(test_file.to_str().unwrap()).unwrap();

        // Read back and verify
        let new_content = fs::read_to_string(&test_file).unwrap();
        let lines: Vec<&str> = new_content.lines().collect();

        assert_eq!(lines.len(), 3);

        // First line should have ID added
        assert!(lines[0].contains("id:"));
        assert!(lines[0].starts_with("(A) Call Mom +family @phone"));

        // Second line should keep existing ID
        assert!(lines[1].contains("id:existing-001"));

        // Third line should have ID added
        assert!(lines[2].contains("id:"));
        assert!(lines[2].starts_with("Learn Rust +learning @coding"));

        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_mark_complete() {
        let temp_dir = std::env::temp_dir();
        let todo_file = temp_dir.join("test_complete_todo.txt");

        let content = r"(A) Call Mom +family @phone id:task-001
Buy groceries +personal @errands id:task-002
Learn Rust +learning @coding id:task-003";

        fs::write(&todo_file, content).unwrap();

        // Mark task-002 as complete
        mark_complete(todo_file.to_str().unwrap(), "task-002").unwrap();

        // Check todo.txt - should have 2 remaining tasks
        let remaining_content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(
            remaining_content
                .lines()
                .filter(|l| !l.trim().is_empty())
                .count(),
            2
        );
        assert!(!remaining_content.contains("task-002"));

        // Check done.txt - should have 1 completed task
        let done_file = temp_dir.join("done.txt");
        assert!(done_file.exists(), "done.txt should be created");
        let done_content = fs::read_to_string(&done_file).unwrap();
        assert!(done_content.contains('x'));
        assert!(done_content.contains("Buy groceries +personal @errands id:task-002"));
        assert!(done_content.contains(&chrono::Local::now().format("%Y-%m-%d").to_string()));

        fs::remove_file(&todo_file).ok();
        fs::remove_file(&done_file).ok();
    }

    #[test]
    fn test_delete_todo_removes_matching_line() {
        let temp_dir = std::env::temp_dir();
        let todo_file = temp_dir.join("test_delete_todo_removes.txt");

        let content = "(A) Call Mom +family @phone id:del-001\nBuy groceries +personal @errands id:del-002\nLearn Rust +learning @coding id:del-003\n";
        fs::write(&todo_file, content).unwrap();

        let removed = delete_todo(todo_file.to_str().unwrap(), "del-002").unwrap();
        assert!(removed, "delete_todo should return true when line removed");

        let remaining = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(
            remaining.lines().filter(|l| !l.trim().is_empty()).count(),
            2
        );
        assert!(!remaining.contains("del-002"));
        assert!(remaining.contains("del-001"));
        assert!(remaining.contains("del-003"));

        fs::remove_file(&todo_file).ok();
    }

    #[test]
    fn test_delete_todo_returns_false_when_id_missing() {
        let temp_dir = std::env::temp_dir();
        let todo_file = temp_dir.join("test_delete_todo_missing.txt");

        let content = "(A) Task one +work id:keep-001\nTask two +personal id:keep-002\n";
        fs::write(&todo_file, content).unwrap();
        let before = fs::read_to_string(&todo_file).unwrap();

        let removed = delete_todo(todo_file.to_str().unwrap(), "no-such-id").unwrap();
        assert!(!removed, "delete_todo should return false when id absent");

        let after = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(before, after, "file must not change when id not found");

        fs::remove_file(&todo_file).ok();
    }

    #[test]
    fn test_delete_todo_preserves_trailing_newline() {
        let temp_dir = std::env::temp_dir();
        let with_nl = temp_dir.join("test_delete_todo_with_nl.txt");
        let without_nl = temp_dir.join("test_delete_todo_without_nl.txt");

        fs::write(&with_nl, "A +p id:a1\nB +p id:a2\n").unwrap();
        delete_todo(with_nl.to_str().unwrap(), "a1").unwrap();
        let result_with = fs::read_to_string(&with_nl).unwrap();
        assert!(
            result_with.ends_with('\n'),
            "trailing newline should be preserved: {result_with:?}"
        );

        fs::write(&without_nl, "A +p id:b1\nB +p id:b2").unwrap();
        delete_todo(without_nl.to_str().unwrap(), "b1").unwrap();
        let result_without = fs::read_to_string(&without_nl).unwrap();
        assert!(
            !result_without.ends_with('\n'),
            "no trailing newline should be preserved: {result_without:?}"
        );

        fs::remove_file(&with_nl).ok();
        fs::remove_file(&without_nl).ok();
    }

    #[test]
    fn test_append_todo() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_append_todo.txt");

        let content = "(A) Existing task +work id:existing-1";
        fs::write(&test_file, content).unwrap();

        append_todo(
            test_file.to_str().unwrap(),
            "New task +myproject id:new-slug",
        )
        .unwrap();

        let result = fs::read_to_string(&test_file).unwrap();
        assert!(result.contains("Existing task"));
        assert!(result.contains("New task +myproject id:new-slug"));

        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_append_todo_to_nonexistent_file() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_append_todo_new.txt");
        fs::remove_file(&test_file).ok();

        append_todo(test_file.to_str().unwrap(), "First task +project id:first").unwrap();

        let result = fs::read_to_string(&test_file).unwrap();
        assert!(result.contains("First task +project id:first"));

        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_has_todo_with_id() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("test_has_todo_id.txt");

        let content = "(A) Task one +work id:task-001\nTask two +personal id:task-002";
        fs::write(&test_file, content).unwrap();

        assert!(has_todo_with_id(test_file.to_str().unwrap(), "task-001"));
        assert!(has_todo_with_id(test_file.to_str().unwrap(), "task-002"));
        assert!(!has_todo_with_id(test_file.to_str().unwrap(), "task-003"));
        assert!(!has_todo_with_id(test_file.to_str().unwrap(), "task-00")); // partial match should not work

        fs::remove_file(&test_file).ok();
    }

    #[test]
    fn test_has_todo_with_id_nonexistent_file() {
        assert!(!has_todo_with_id("/nonexistent/path/todo.txt", "any-id"));
    }

    #[test]
    fn test_move_to_file() {
        let temp_dir = std::env::temp_dir().join("torudo_test_move_to_file");
        fs::create_dir_all(&temp_dir).unwrap();

        let source_file = temp_dir.join("todo.txt");
        let dest_file = temp_dir.join("ref.txt");

        // Clean up before test
        fs::remove_file(&dest_file).ok();

        let content = "(A) Call Mom +family @phone id:task-001\nBuy groceries +personal @errands id:task-002\nLearn Rust +learning @coding id:task-003";
        fs::write(&source_file, content).unwrap();

        move_to_file(
            source_file.to_str().unwrap(),
            dest_file.to_str().unwrap(),
            "task-002",
        )
        .unwrap();

        // source should have 2 remaining lines
        let remaining = fs::read_to_string(&source_file).unwrap();
        assert_eq!(
            remaining.lines().filter(|l| !l.trim().is_empty()).count(),
            2
        );
        assert!(!remaining.contains("task-002"));
        assert!(remaining.contains("task-001"));
        assert!(remaining.contains("task-003"));

        // dest should have the moved line as-is
        let dest_content = fs::read_to_string(&dest_file).unwrap();
        assert!(dest_content.contains("Buy groceries +personal @errands id:task-002"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_move_to_file_dest_not_exists() {
        let temp_dir = std::env::temp_dir().join("torudo_test_move_dest_new");
        fs::create_dir_all(&temp_dir).unwrap();

        let source_file = temp_dir.join("todo.txt");
        let dest_file = temp_dir.join("ref.txt");
        fs::remove_file(&dest_file).ok();

        let content = "Task one id:task-001";
        fs::write(&source_file, content).unwrap();

        move_to_file(
            source_file.to_str().unwrap(),
            dest_file.to_str().unwrap(),
            "task-001",
        )
        .unwrap();

        assert!(dest_file.exists());
        let dest_content = fs::read_to_string(&dest_file).unwrap();
        assert!(dest_content.contains("Task one id:task-001"));

        // source should be empty (no non-empty lines)
        let remaining = fs::read_to_string(&source_file).unwrap();
        assert_eq!(
            remaining.lines().filter(|l| !l.trim().is_empty()).count(),
            0
        );

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_move_to_file_appends_to_existing_dest() {
        let temp_dir = std::env::temp_dir().join("torudo_test_move_append");
        fs::create_dir_all(&temp_dir).unwrap();

        let source_file = temp_dir.join("todo.txt");
        let dest_file = temp_dir.join("ref.txt");

        fs::write(&source_file, "New item id:task-002").unwrap();
        fs::write(&dest_file, "Existing item id:task-001\n").unwrap();

        move_to_file(
            source_file.to_str().unwrap(),
            dest_file.to_str().unwrap(),
            "task-002",
        )
        .unwrap();

        let dest_content = fs::read_to_string(&dest_file).unwrap();
        assert!(dest_content.contains("Existing item id:task-001"));
        assert!(dest_content.contains("New item id:task-002"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_mark_complete_with_priority() {
        let temp_dir = std::env::temp_dir().join("torudo_test_priority");
        fs::create_dir_all(&temp_dir).unwrap();

        let todo_file = temp_dir.join("todo.txt");
        let done_file = temp_dir.join("done.txt");

        // Clean up before test
        fs::remove_file(&done_file).ok();

        let content = "(A) 2024-01-10 Call Mom +family @phone id:task-001";
        fs::write(&todo_file, content).unwrap();

        mark_complete(todo_file.to_str().unwrap(), "task-001").unwrap();

        let done_content = fs::read_to_string(&done_file).unwrap();

        // Correct order: x (A) completion-date creation-date description
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert!(
            done_content.starts_with(&format!("x (A) {today} 2024-01-10")),
            "Expected format: 'x (A) {today} 2024-01-10...', but got: {done_content}"
        );

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_strip_completion_marker_removes_x_and_completion_date() {
        assert_eq!(
            strip_completion_marker("x 2026-08-15 2024-01-10 Call Mom +family id:t1"),
            "2024-01-10 Call Mom +family id:t1"
        );
    }

    #[test]
    fn test_strip_completion_marker_keeps_priority() {
        assert_eq!(
            strip_completion_marker("x (A) 2026-08-15 2024-01-10 Call Mom id:t1"),
            "(A) 2024-01-10 Call Mom id:t1"
        );
    }

    #[test]
    fn test_strip_completion_marker_without_creation_date() {
        assert_eq!(
            strip_completion_marker("x 2026-08-15 Call Mom id:t1"),
            "Call Mom id:t1"
        );
    }

    #[test]
    fn test_strip_completion_marker_leaves_incomplete_line_alone() {
        assert_eq!(
            strip_completion_marker("2024-01-10 Call Mom id:t1"),
            "2024-01-10 Call Mom id:t1"
        );
    }

    #[test]
    fn test_reopen_item_moves_the_line_back() {
        let temp_dir = std::env::temp_dir().join("torudo_test_reopen");
        fs::create_dir_all(&temp_dir).unwrap();
        let done_file = temp_dir.join("done.txt");
        let todo_file = temp_dir.join("todo.txt");
        fs::write(
            &done_file,
            "x 2026-08-15 2024-01-10 Call Mom id:t1\nx 2026-08-14 Other id:t2\n",
        )
        .unwrap();
        fs::write(&todo_file, "Existing id:t9\n").unwrap();

        let moved = reopen_item(
            done_file.to_str().unwrap(),
            todo_file.to_str().unwrap(),
            "t1",
        )
        .unwrap();

        assert!(moved);
        let todo = fs::read_to_string(&todo_file).unwrap();
        assert!(todo.contains("2024-01-10 Call Mom id:t1"));
        assert!(!todo.contains(" x "));
        assert!(todo.contains("Existing id:t9"));
        let done = fs::read_to_string(&done_file).unwrap();
        assert!(!done.contains("id:t1"));
        assert!(done.contains("id:t2"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_reopen_item_unknown_id_returns_false() {
        let temp_dir = std::env::temp_dir().join("torudo_test_reopen_unknown");
        fs::create_dir_all(&temp_dir).unwrap();
        let done_file = temp_dir.join("done.txt");
        let todo_file = temp_dir.join("todo.txt");
        fs::write(&done_file, "x 2026-08-15 Call Mom id:t1\n").unwrap();

        let moved = reopen_item(
            done_file.to_str().unwrap(),
            todo_file.to_str().unwrap(),
            "nope",
        )
        .unwrap();

        assert!(!moved);
        assert!(!todo_file.exists(), "nothing should be written");
        let done = fs::read_to_string(&done_file).unwrap();
        assert!(done.contains("id:t1"), "done.txt is untouched");

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_priority_adds_to_unprioritized() {
        let temp_dir = std::env::temp_dir().join("torudo_test_set_priority_add");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        fs::write(&todo_file, "2024-01-10 Task one +proj id:t1\n").unwrap();

        set_priority(todo_file.to_str().unwrap(), "t1", Some('A')).unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(content, "(A) 2024-01-10 Task one +proj id:t1\n");
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_priority_replaces_existing() {
        let temp_dir = std::env::temp_dir().join("torudo_test_set_priority_replace");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        fs::write(&todo_file, "(B) Task id:t1\n").unwrap();

        set_priority(todo_file.to_str().unwrap(), "t1", Some('A')).unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(content, "(A) Task id:t1\n");
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_priority_clears() {
        let temp_dir = std::env::temp_dir().join("torudo_test_set_priority_clear");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        fs::write(&todo_file, "(A) Task id:t1\n").unwrap();

        set_priority(todo_file.to_str().unwrap(), "t1", None).unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(content, "Task id:t1\n");
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_priority_preserves_other_lines() {
        let temp_dir = std::env::temp_dir().join("torudo_test_set_priority_preserve");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        let initial = "Task one id:t1\n(B) Task two id:t2\n2024-01-10 Task three id:t3\n";
        fs::write(&todo_file, initial).unwrap();

        set_priority(todo_file.to_str().unwrap(), "t2", Some('A')).unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(
            content,
            "Task one id:t1\n(A) Task two id:t2\n2024-01-10 Task three id:t3\n"
        );
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_priority_nonexistent_id() {
        let temp_dir = std::env::temp_dir().join("torudo_test_set_priority_nonexistent");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        let initial = "(A) Task id:t1\n";
        fs::write(&todo_file, initial).unwrap();

        set_priority(todo_file.to_str().unwrap(), "nonexistent", Some('C')).unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(content, initial);
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_priority_skips_completed_line() {
        let temp_dir = std::env::temp_dir().join("torudo_test_set_priority_completed");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        let initial = "x (A) 2024-01-20 2024-01-10 Done task id:t1\n";
        fs::write(&todo_file, initial).unwrap();

        set_priority(todo_file.to_str().unwrap(), "t1", Some('C')).unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(content, initial);
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_key_value_adds_to_plain_item() {
        let temp_dir = std::env::temp_dir().join("torudo_test_set_kv_add");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        fs::write(&todo_file, "2024-01-10 Task one +proj id:t1\n").unwrap();

        set_key_value(todo_file.to_str().unwrap(), "t1", "time", "short").unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(content, "2024-01-10 Task one +proj id:t1 time:short\n");
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_key_value_replaces_existing() {
        let temp_dir = std::env::temp_dir().join("torudo_test_set_kv_replace");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        fs::write(&todo_file, "Task id:t1 time:short\n").unwrap();

        set_key_value(todo_file.to_str().unwrap(), "t1", "time", "medium").unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(content, "Task id:t1 time:medium\n");
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_key_value_preserves_other_keys() {
        let temp_dir = std::env::temp_dir().join("torudo_test_set_kv_other_keys");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        fs::write(&todo_file, "Task due:2026-01-01 id:t1\n").unwrap();

        set_key_value(todo_file.to_str().unwrap(), "t1", "time", "long").unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(content, "Task due:2026-01-01 id:t1 time:long\n");
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_key_value_preserves_other_lines() {
        let temp_dir = std::env::temp_dir().join("torudo_test_set_kv_preserve_lines");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        let initial = "Task one id:t1\nTask two id:t2\nTask three id:t3\n";
        fs::write(&todo_file, initial).unwrap();

        set_key_value(todo_file.to_str().unwrap(), "t2", "time", "short").unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(
            content,
            "Task one id:t1\nTask two id:t2 time:short\nTask three id:t3\n"
        );
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_key_value_nonexistent_id() {
        let temp_dir = std::env::temp_dir().join("torudo_test_set_kv_nonexistent");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        let initial = "Task id:t1\n";
        fs::write(&todo_file, initial).unwrap();

        set_key_value(todo_file.to_str().unwrap(), "nonexistent", "time", "short").unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(content, initial);
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_set_key_value_skips_completed_line() {
        let temp_dir = std::env::temp_dir().join("torudo_test_set_kv_completed");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        let initial = "x 2024-01-20 2024-01-10 Done task id:t1\n";
        fs::write(&todo_file, initial).unwrap();

        set_key_value(todo_file.to_str().unwrap(), "t1", "time", "short").unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(content, initial);
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_clear_key_value_drops_the_tag_and_keeps_the_others() {
        let temp_dir = std::env::temp_dir().join("torudo_test_clear_kv_drops");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        fs::write(
            &todo_file,
            "2024-01-10 Pay rent +home due:2026-01-01 rec:+1m id:t1\n",
        )
        .unwrap();

        clear_key_value(todo_file.to_str().unwrap(), "t1", "rec").unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(content, "2024-01-10 Pay rent +home due:2026-01-01 id:t1\n");
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_clear_key_value_is_a_no_op_when_the_tag_is_absent() {
        let temp_dir = std::env::temp_dir().join("torudo_test_clear_kv_absent");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        let initial = "Pay rent +home id:t1\n";
        fs::write(&todo_file, initial).unwrap();

        clear_key_value(todo_file.to_str().unwrap(), "t1", "rec").unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(content, initial);
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_clear_key_value_preserves_other_lines_and_completed_ones() {
        let temp_dir = std::env::temp_dir().join("torudo_test_clear_kv_other_lines");
        fs::create_dir_all(&temp_dir).unwrap();
        let todo_file = temp_dir.join("todo.txt");
        fs::write(
            &todo_file,
            "Task one rec:1d id:t1\nTask two rec:1w id:t2\nx 2024-01-20 Done rec:1d id:t3\n",
        )
        .unwrap();

        clear_key_value(todo_file.to_str().unwrap(), "t2", "rec").unwrap();

        let content = fs::read_to_string(&todo_file).unwrap();
        assert_eq!(
            content,
            "Task one rec:1d id:t1\nTask two id:t2\nx 2024-01-20 Done rec:1d id:t3\n"
        );
        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_item_to_json_without_md() {
        let temp_dir = std::env::temp_dir().join("torudo_test_item_to_json_no_md");
        fs::create_dir_all(temp_dir.join("todos")).unwrap();

        let item = Item::parse("(B) Simple task +proj @ctx id:xyz-789", 0);
        let json_str = item_to_json(&item, temp_dir.to_str().unwrap()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(json["title"], "Simple task");
        assert_eq!(json["priority"], "B");
        assert_eq!(json["id"], "xyz-789");
        assert_eq!(json["projects"], serde_json::json!(["proj"]));
        assert_eq!(json["contexts"], serde_json::json!(["ctx"]));
        assert!(json.get("md").is_none());

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_item_to_json_with_md() {
        let temp_dir = std::env::temp_dir().join("torudo_test_item_to_json_with_md");
        let todos_dir = temp_dir.join("todos");
        fs::create_dir_all(&todos_dir).unwrap();
        fs::write(todos_dir.join("abc-123.md"), "# Details").unwrap();

        let item = Item::parse("(A) My task +project @home id:abc-123", 0);
        let json_str = item_to_json(&item, temp_dir.to_str().unwrap()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(json["md"], "# Details");
        assert_eq!(json["id"], "abc-123");

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_item_to_json_includes_key_values() {
        let temp_dir = std::env::temp_dir().join("torudo_test_item_to_json_kv");
        fs::create_dir_all(temp_dir.join("todos")).unwrap();

        let item = Item::parse("Plan trip due:2026-05-30 t:14:00 +travel id:trip-1", 0);
        let json_str = item_to_json(&item, temp_dir.to_str().unwrap()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(json["key_values"]["due"], "2026-05-30");
        assert_eq!(json["key_values"]["t"], "14:00");
        assert_eq!(json["title"], "Plan trip");

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_item_to_json_without_id_skips_md_lookup() {
        let temp_dir = std::env::temp_dir().join("torudo_test_item_to_json_no_id");
        fs::create_dir_all(&temp_dir).unwrap();

        let item = Item::parse("No id todo", 0);
        let json_str = item_to_json(&item, temp_dir.to_str().unwrap()).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert_eq!(json["title"], "No id todo");
        assert!(json.get("md").is_none());

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_add_item_inserts_creation_date_when_missing() {
        let temp_dir = std::env::temp_dir().join("torudo_test_add_item_cdate_missing");
        fs::create_dir_all(&temp_dir).unwrap();
        let inbox = temp_dir.join("inbox.txt");
        fs::remove_file(&inbox).ok();

        let item = add_item(inbox.to_str().unwrap(), "Buy milk").unwrap();
        let today = chrono::Local::now().date_naive();

        assert_eq!(item.creation_date, Some(today));
        assert_eq!(item.description, "Buy milk");

        let content = fs::read_to_string(&inbox).unwrap();
        let today_str = today.format("%Y-%m-%d").to_string();
        assert!(
            content.starts_with(&format!("{today_str} Buy milk ")),
            "expected line starting with '{today_str} Buy milk', got: {content}"
        );

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_add_item_inserts_creation_date_after_priority() {
        let temp_dir = std::env::temp_dir().join("torudo_test_add_item_cdate_priority");
        fs::create_dir_all(&temp_dir).unwrap();
        let inbox = temp_dir.join("inbox.txt");
        fs::remove_file(&inbox).ok();

        let item = add_item(inbox.to_str().unwrap(), "(A) Buy milk +shop @home").unwrap();
        let today = chrono::Local::now().date_naive();

        assert_eq!(item.priority, Some('A'));
        assert_eq!(item.creation_date, Some(today));
        assert_eq!(item.description, "Buy milk");
        assert_eq!(item.projects, vec!["shop"]);
        assert_eq!(item.contexts, vec!["home"]);

        let content = fs::read_to_string(&inbox).unwrap();
        let today_str = today.format("%Y-%m-%d").to_string();
        assert!(
            content.starts_with(&format!("(A) {today_str} Buy milk +shop @home ")),
            "expected line starting with '(A) {today_str} Buy milk +shop @home', got: {content}"
        );

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_add_item_preserves_existing_creation_date() {
        let temp_dir = std::env::temp_dir().join("torudo_test_add_item_cdate_preserve");
        fs::create_dir_all(&temp_dir).unwrap();
        let inbox = temp_dir.join("inbox.txt");
        fs::remove_file(&inbox).ok();

        let item = add_item(inbox.to_str().unwrap(), "(A) 2024-01-01 Old task").unwrap();

        assert_eq!(
            item.creation_date,
            Some(NaiveDate::from_ymd_opt(2024, 1, 1).unwrap())
        );
        assert_eq!(item.description, "Old task");

        let content = fs::read_to_string(&inbox).unwrap();
        assert!(
            content.starts_with("(A) 2024-01-01 Old task "),
            "expected preserved date, got: {content}"
        );
        let today_str = chrono::Local::now().format("%Y-%m-%d").to_string();
        assert!(
            !content.contains(&format!("(A) {today_str} 2024-01-01")),
            "today's date should not be inserted before the existing creation date"
        );

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_add_item_generates_uuid_when_missing() {
        let temp_dir = std::env::temp_dir().join("torudo_test_add_item_uuid");
        fs::create_dir_all(&temp_dir).unwrap();
        let inbox = temp_dir.join("inbox.txt");
        fs::remove_file(&inbox).ok();

        let item = add_item(inbox.to_str().unwrap(), "Buy milk").unwrap();

        assert_eq!(item.description, "Buy milk");
        let id = item.id.expect("id should be auto-generated");
        // UUID v4 is 36 chars (including hyphens)
        assert_eq!(id.len(), 36);

        let content = fs::read_to_string(&inbox).unwrap();
        assert!(content.contains("Buy milk"));
        assert!(content.contains(&format!("id:{id}")));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_add_item_preserves_existing_id() {
        let temp_dir = std::env::temp_dir().join("torudo_test_add_item_keep_id");
        fs::create_dir_all(&temp_dir).unwrap();
        let inbox = temp_dir.join("inbox.txt");
        fs::remove_file(&inbox).ok();

        let item = add_item(inbox.to_str().unwrap(), "Buy milk id:fixed-123").unwrap();

        assert_eq!(item.id.as_deref(), Some("fixed-123"));

        let content = fs::read_to_string(&inbox).unwrap();
        // id:fixed-123 should appear exactly once (not overwritten or duplicated with a new UUID)
        assert_eq!(content.matches("id:fixed-123").count(), 1);
        assert_eq!(content.matches("id:").count(), 1);

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_add_item_creates_file_if_missing() {
        let temp_dir = std::env::temp_dir().join("torudo_test_add_item_new_file");
        fs::create_dir_all(&temp_dir).unwrap();
        let inbox = temp_dir.join("inbox.txt");
        fs::remove_file(&inbox).ok();
        assert!(!inbox.exists());

        add_item(inbox.to_str().unwrap(), "First item").unwrap();

        assert!(inbox.exists());
        let content = fs::read_to_string(&inbox).unwrap();
        assert!(content.contains("First item"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_add_item_parses_priority_projects_contexts() {
        let temp_dir = std::env::temp_dir().join("torudo_test_add_item_parse");
        fs::create_dir_all(&temp_dir).unwrap();
        let inbox = temp_dir.join("inbox.txt");
        fs::remove_file(&inbox).ok();

        let item = add_item(inbox.to_str().unwrap(), "(A) Buy milk +grocery @home").unwrap();

        assert_eq!(item.priority, Some('A'));
        assert_eq!(item.projects, vec!["grocery".to_string()]);
        assert_eq!(item.contexts, vec!["home".to_string()]);
        assert!(item.id.is_some());

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_add_item_appends_to_existing_content() {
        let temp_dir = std::env::temp_dir().join("torudo_test_add_item_append");
        fs::create_dir_all(&temp_dir).unwrap();
        let inbox = temp_dir.join("inbox.txt");

        // Existing content without trailing newline
        fs::write(&inbox, "Existing line id:old-1").unwrap();

        add_item(inbox.to_str().unwrap(), "New line").unwrap();

        let content = fs::read_to_string(&inbox).unwrap();
        let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Existing line"));
        assert!(lines[1].contains("New line"));

        fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_item_threshold_date_from_key_value() {
        let item = Item::parse("Write report t:2026-04-20 +work", 1);
        assert_eq!(
            item.threshold_date(),
            Some(NaiveDate::from_ymd_opt(2026, 4, 20).unwrap())
        );
    }

    #[test]
    fn test_item_threshold_date_none_when_missing() {
        let item = Item::parse("Write report +work", 1);
        assert_eq!(item.threshold_date(), None);
    }

    #[test]
    fn test_item_threshold_date_none_when_invalid_format() {
        let item = Item::parse("Write report t:tomorrow +work", 1);
        assert_eq!(item.threshold_date(), None);
    }

    #[test]
    fn test_is_threshold_pending_future() {
        let item = Item::parse("Task t:2026-04-20", 1);
        let today = NaiveDate::from_ymd_opt(2026, 4, 14).unwrap();
        assert!(item.is_threshold_pending(today));
    }

    #[test]
    fn test_is_threshold_pending_today_is_not_pending() {
        let item = Item::parse("Task t:2026-04-14", 1);
        let today = NaiveDate::from_ymd_opt(2026, 4, 14).unwrap();
        assert!(!item.is_threshold_pending(today));
    }

    #[test]
    fn test_is_threshold_pending_past() {
        let item = Item::parse("Task t:2026-04-01", 1);
        let today = NaiveDate::from_ymd_opt(2026, 4, 14).unwrap();
        assert!(!item.is_threshold_pending(today));
    }

    #[test]
    fn test_sort_todos_places_pending_after_reached() {
        // Pending (A) with future threshold must sort after reached (C) with no threshold
        let mut todos = vec![
            Item::parse("(A) future task t:2026-04-20", 1),
            Item::parse("(C) normal task", 2),
        ];
        let today = NaiveDate::from_ymd_opt(2026, 4, 14).unwrap();
        sort_todos(&mut todos, today);
        assert_eq!(todos[0].priority, Some('C'));
        assert_eq!(todos[1].priority, Some('A'));
    }

    #[test]
    fn test_sort_todos_preserves_priority_within_reached_group() {
        let mut todos = vec![
            Item::parse("(C) c task", 1),
            Item::parse("(A) a task", 2),
            Item::parse("(B) b task", 3),
        ];
        let today = NaiveDate::from_ymd_opt(2026, 4, 14).unwrap();
        sort_todos(&mut todos, today);
        assert_eq!(todos[0].priority, Some('A'));
        assert_eq!(todos[1].priority, Some('B'));
        assert_eq!(todos[2].priority, Some('C'));
    }

    #[test]
    fn test_item_due_date_from_key_value() {
        let item = Item::parse("Write report due:2026-04-20 +work", 1);
        assert_eq!(
            item.due_date(),
            Some(NaiveDate::from_ymd_opt(2026, 4, 20).unwrap())
        );
    }

    #[test]
    fn test_item_due_date_none_when_missing() {
        let item = Item::parse("Write report +work", 1);
        assert_eq!(item.due_date(), None);
    }

    #[test]
    fn test_item_due_date_none_when_invalid_format() {
        let item = Item::parse("Write report due:tomorrow +work", 1);
        assert_eq!(item.due_date(), None);
    }

    #[test]
    fn test_is_overdue_before_due() {
        let item = Item::parse("Task due:2026-04-20", 1);
        let today = NaiveDate::from_ymd_opt(2026, 4, 14).unwrap();
        assert!(!item.is_overdue(today));
    }

    #[test]
    fn test_is_overdue_on_due_date() {
        let item = Item::parse("Task due:2026-04-14", 1);
        let today = NaiveDate::from_ymd_opt(2026, 4, 14).unwrap();
        assert!(item.is_overdue(today));
    }

    #[test]
    fn test_is_overdue_after_due() {
        let item = Item::parse("Task due:2026-04-01", 1);
        let today = NaiveDate::from_ymd_opt(2026, 4, 14).unwrap();
        assert!(item.is_overdue(today));
    }

    #[test]
    fn test_is_overdue_when_no_due() {
        let item = Item::parse("Task without due", 1);
        let today = NaiveDate::from_ymd_opt(2026, 4, 14).unwrap();
        assert!(!item.is_overdue(today));
    }

    #[test]
    fn test_time_estimate_short() {
        let item = Item::parse("Task time:short", 1);
        assert_eq!(item.time_estimate(), Some("short"));
    }

    #[test]
    fn test_time_estimate_medium() {
        let item = Item::parse("Task time:medium", 1);
        assert_eq!(item.time_estimate(), Some("medium"));
    }

    #[test]
    fn test_time_estimate_long() {
        let item = Item::parse("Task time:long", 1);
        assert_eq!(item.time_estimate(), Some("long"));
    }

    #[test]
    fn test_time_estimate_none_when_missing() {
        let item = Item::parse("Task without time", 1);
        assert_eq!(item.time_estimate(), None);
    }

    #[test]
    fn test_time_estimate_none_when_invalid() {
        let item = Item::parse("Task time:xl", 1);
        assert_eq!(item.time_estimate(), None);
    }

    #[test]
    fn test_energy_estimate_low() {
        let item = Item::parse("Task energy:low", 1);
        assert_eq!(item.energy_estimate(), Some("low"));
    }

    #[test]
    fn test_energy_estimate_high() {
        let item = Item::parse("Task energy:high", 1);
        assert_eq!(item.energy_estimate(), Some("high"));
    }

    #[test]
    fn test_energy_estimate_none_when_missing() {
        let item = Item::parse("Task without energy", 1);
        assert_eq!(item.energy_estimate(), None);
    }

    #[test]
    fn test_energy_estimate_none_when_invalid() {
        let item = Item::parse("Task energy:medium", 1);
        assert_eq!(item.energy_estimate(), None);
    }

    #[test]
    fn recurrence_returns_the_raw_rec_value() {
        assert_eq!(Item::parse("Task rec:1w", 1).recurrence(), Some("1w"));
        assert_eq!(Item::parse("Task rec:+1m", 1).recurrence(), Some("+1m"));
    }

    #[test]
    fn recurrence_is_none_without_the_tag() {
        assert_eq!(Item::parse("Task without rec", 1).recurrence(), None);
    }

    #[test]
    fn recurrence_reads_a_valid_pattern() {
        assert_eq!(Item::parse("Task rec:+1w", 1).recurrence(), Some("+1w"));
    }

    #[test]
    fn recurrence_is_none_for_a_pattern_that_does_not_parse() {
        // Same contract as `time_estimate`: a value that comes back is safe to
        // display. Reporting a typo needs the raw text from `key_values`.
        for line in ["Task rec:banana", "Task rec:0w", "Task rec:1b"] {
            assert_eq!(Item::parse(line, 1).recurrence(), None, "{line}");
        }
    }

    #[test]
    fn replace_key_value_moves_the_key_to_the_end() {
        assert_eq!(
            replace_key_value("Pay rent due:2026-01-01 rec:1m", "due", "2026-02-01"),
            "Pay rent rec:1m due:2026-02-01"
        );
    }

    #[test]
    fn replace_key_value_appends_a_key_that_is_missing() {
        assert_eq!(
            replace_key_value("Pay rent rec:1m", "due", "2026-02-01"),
            "Pay rent rec:1m due:2026-02-01"
        );
    }

    #[test]
    fn replace_key_value_leaves_a_similarly_named_key_alone() {
        assert_eq!(
            replace_key_value("Task time:short t:2026-01-01", "t", "2026-02-01"),
            "Task time:short t:2026-02-01"
        );
    }

    #[test]
    fn remove_key_value_drops_the_token() {
        assert_eq!(
            remove_key_value("Task t:2026-01-01 time:short id:t1", "t"),
            "Task time:short id:t1"
        );
    }

    #[test]
    fn remove_key_value_is_a_no_op_when_the_key_is_absent() {
        assert_eq!(remove_key_value("Task id:t1", "due"), "Task id:t1");
    }

    #[test]
    fn replace_creation_date_swaps_an_existing_date() {
        assert_eq!(
            replace_creation_date("2024-01-10 Call Mom +family id:t1", "2026-08-15"),
            "2026-08-15 Call Mom +family id:t1"
        );
    }

    #[test]
    fn replace_creation_date_keeps_the_priority_in_front() {
        assert_eq!(
            replace_creation_date("(A) 2024-01-10 Call Mom id:t1", "2026-08-15"),
            "(A) 2026-08-15 Call Mom id:t1"
        );
    }

    #[test]
    fn replace_creation_date_inserts_a_date_that_was_missing() {
        assert_eq!(
            replace_creation_date("(B) Call Mom id:t1", "2026-08-15"),
            "(B) 2026-08-15 Call Mom id:t1"
        );
        assert_eq!(
            replace_creation_date("Call Mom id:t1", "2026-08-15"),
            "2026-08-15 Call Mom id:t1"
        );
    }

    #[test]
    fn replace_creation_date_does_not_mistake_a_due_date_for_a_creation_date() {
        assert_eq!(
            replace_creation_date("Call Mom due:2024-01-10 id:t1", "2026-08-15"),
            "2026-08-15 Call Mom due:2024-01-10 id:t1"
        );
    }

    fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// A date `offset` days away from today. `mark_complete` reads the clock
    /// itself, so tests that go through it seed their dates from today rather
    /// than writing a fixed calendar date that would rot.
    fn days_from_today(offset: i64) -> NaiveDate {
        chrono::Local::now().date_naive() + chrono::Duration::days(offset)
    }

    #[test]
    fn next_occurrence_line_carries_everything_but_the_dates_and_the_id() {
        let original = "(A) 2026-08-01 Take out the trash +home @errands t:2026-08-08 due:2026-08-10 \
             rec:+1w time:short energy:low id:old-id";
        let next = NextDates {
            t: Some(ymd(2026, 8, 15)),
            due: Some(ymd(2026, 8, 17)),
        };
        assert_eq!(
            next_occurrence_line(original, next, "new-id", ymd(2026, 8, 15)),
            "(A) 2026-08-15 Take out the trash +home @errands rec:+1w time:short energy:low \
             t:2026-08-15 due:2026-08-17 id:new-id"
        );
    }

    #[test]
    fn next_occurrence_line_drops_dates_the_next_occurrence_does_not_have() {
        let original = "Pay rent t:2026-08-08 due:2026-08-10 rec:1m id:old-id";
        let next = NextDates {
            t: None,
            due: Some(ymd(2026, 9, 15)),
        };
        assert_eq!(
            next_occurrence_line(original, next, "new-id", ymd(2026, 8, 15)),
            "2026-08-15 Pay rent rec:1m due:2026-09-15 id:new-id"
        );
    }

    #[test]
    fn next_occurrence_line_keeps_a_threshold_only_item_without_a_due_date() {
        let original = "2026-07-01 Water the plants t:2026-08-08 rec:1w id:old-id";
        let next = NextDates {
            t: Some(ymd(2026, 8, 22)),
            due: None,
        };
        assert_eq!(
            next_occurrence_line(original, next, "new-id", ymd(2026, 8, 15)),
            "2026-08-15 Water the plants rec:1w t:2026-08-22 id:new-id"
        );
    }

    #[test]
    fn mark_complete_spawns_the_next_occurrence() {
        let dir = tempfile::tempdir().unwrap();
        let todo_file = dir.path().join("todo.txt");
        // A strict `+1w` counts from the item's own due date, three days back, so
        // the next occurrence lands four days out. A pattern read as loose would
        // count from the completion date and land seven days out instead.
        let seeded_due = days_from_today(-3);
        let expected_due = days_from_today(4);
        fs::write(
            &todo_file,
            format!("2026-08-01 Take out the trash +home due:{seeded_due} rec:+1w id:r1\n"),
        )
        .unwrap();

        let result = mark_complete(todo_file.to_str().unwrap(), "r1").unwrap();
        let Recurred::Created { id, t, due } = result else {
            panic!("expected a next occurrence, got {result:?}");
        };
        assert_ne!(id, "r1");
        assert_eq!(t, None);
        assert_eq!(due, Some(expected_due));

        let remaining = fs::read_to_string(&todo_file).unwrap();
        let lines: Vec<&str> = remaining.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(
            lines.len(),
            1,
            "todo.txt should hold only the next occurrence"
        );
        let spawned = lines[0];
        assert!(!spawned.starts_with("x "), "spawned line must be open");
        assert!(spawned.contains("rec:+1w"), "rec: should carry over");
        assert!(spawned.contains("+home"), "project should carry over");
        assert!(
            spawned.contains(&format!("due:{expected_due}")),
            "got {spawned}"
        );
        assert!(spawned.contains(&format!("id:{id}")));

        let done = fs::read_to_string(dir.path().join("done.txt")).unwrap();
        assert!(done.starts_with("x "));
        assert!(done.contains("id:r1"));
    }

    fn spawned_id(result: &Recurred) -> &str {
        match result {
            Recurred::Created { id, .. } => id,
            other => panic!("expected a next occurrence, got {other:?}"),
        }
    }

    #[test]
    fn mark_complete_carries_the_detail_file_over_with_empty_checkboxes() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("todos")).unwrap();
        let todo_file = dir.path().join("todo.txt");
        fs::write(&todo_file, "Take out the trash rec:1w id:r1\n").unwrap();
        let old_md =
            "---\ncwd: /home/me/src\nbranch: old-branch\n---\n\n- [x] step one\n- [ ] step two\n";
        fs::write(dir.path().join("todos/r1.md"), old_md).unwrap();

        let result = mark_complete(todo_file.to_str().unwrap(), "r1").unwrap();
        let new_id = spawned_id(&result);

        let carried = fs::read_to_string(dir.path().join(format!("todos/{new_id}.md"))).unwrap();
        assert_eq!(
            carried,
            "---\ncwd: /home/me/src\n---\n\n- [ ] step one\n- [ ] step two\n"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("todos/r1.md")).unwrap(),
            old_md,
            "the completed item keeps its own detail file"
        );
    }

    #[test]
    fn mark_complete_succeeds_when_the_detail_file_cannot_be_carried_over() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let todos_dir = dir.path().join("todos");
        fs::create_dir_all(&todos_dir).unwrap();
        let todo_file = dir.path().join("todo.txt");
        fs::write(&todo_file, "Take out the trash rec:1w id:r1\n").unwrap();
        fs::write(todos_dir.join("r1.md"), "- [ ] step one\n").unwrap();
        // Readable so the copy gets as far as writing, but not writable.
        fs::set_permissions(&todos_dir, fs::Permissions::from_mode(0o555)).unwrap();

        let result = mark_complete(todo_file.to_str().unwrap(), "r1");

        fs::set_permissions(&todos_dir, fs::Permissions::from_mode(0o755)).unwrap();

        let result = result.expect("an unwritable todos/ must not fail the completion");
        let new_id = spawned_id(&result).to_string();
        assert!(
            fs::read_to_string(&todo_file).unwrap().contains(&new_id),
            "the next occurrence is still written"
        );
        assert!(
            fs::read_to_string(dir.path().join("done.txt"))
                .unwrap()
                .contains("id:r1")
        );
    }

    #[test]
    fn mark_complete_recurs_even_without_a_detail_file() {
        let dir = tempfile::tempdir().unwrap();
        let todo_file = dir.path().join("todo.txt");
        fs::write(&todo_file, "Take out the trash rec:1w id:r1\n").unwrap();

        let result = mark_complete(todo_file.to_str().unwrap(), "r1").unwrap();
        let new_id = spawned_id(&result).to_string();
        assert!(!dir.path().join(format!("todos/{new_id}.md")).exists());
        assert!(fs::read_to_string(&todo_file).unwrap().contains(&new_id));
    }

    #[test]
    fn mark_complete_reports_an_invalid_rec_and_still_completes() {
        let dir = tempfile::tempdir().unwrap();
        let todo_file = dir.path().join("todo.txt");
        fs::write(&todo_file, "Take out the trash rec:banana id:r1\n").unwrap();

        let result = mark_complete(todo_file.to_str().unwrap(), "r1").unwrap();
        assert_eq!(
            result,
            Recurred::Failed {
                value: "banana".to_string(),
                error: RecurrenceError::Malformed,
            }
        );
        assert!(
            fs::read_to_string(&todo_file).unwrap().trim().is_empty(),
            "no next occurrence should be written"
        );
        assert!(
            fs::read_to_string(dir.path().join("done.txt"))
                .unwrap()
                .contains("id:r1")
        );
    }

    #[test]
    fn mark_complete_does_not_recur_an_already_completed_line() {
        let dir = tempfile::tempdir().unwrap();
        let todo_file = dir.path().join("todo.txt");
        fs::write(
            &todo_file,
            "x 2026-08-14 2026-08-01 Take out the trash rec:1w id:r1\n",
        )
        .unwrap();

        let result = mark_complete(todo_file.to_str().unwrap(), "r1").unwrap();
        assert_eq!(result, Recurred::None);
        assert!(fs::read_to_string(&todo_file).unwrap().trim().is_empty());
    }

    #[test]
    fn mark_complete_without_a_rec_tag_reports_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let todo_file = dir.path().join("todo.txt");
        fs::write(&todo_file, "Take out the trash id:r1\n").unwrap();

        assert_eq!(
            mark_complete(todo_file.to_str().unwrap(), "r1").unwrap(),
            Recurred::None
        );
        assert_eq!(
            mark_complete(todo_file.to_str().unwrap(), "ghost").unwrap(),
            Recurred::None
        );
    }

    #[test]
    fn mark_complete_spawns_into_the_file_it_completed_from() {
        let dir = tempfile::tempdir().unwrap();
        let waiting_file = dir.path().join("waiting.txt");
        // Seeded relative to today for the same reason as
        // `mark_complete_spawns_the_next_occurrence`: strict `+1w` from a threshold
        // three days back lands four days out, whatever day the test runs on.
        let seeded_t = days_from_today(-3);
        let expected_t = days_from_today(4);
        fs::write(
            &waiting_file,
            format!("Chase the invoice t:{seeded_t} rec:+1w id:r1\n"),
        )
        .unwrap();

        let result = mark_complete(waiting_file.to_str().unwrap(), "r1").unwrap();
        assert!(matches!(
            result,
            Recurred::Created {
                t: Some(_),
                due: None,
                ..
            }
        ));
        let spawned = fs::read_to_string(&waiting_file).unwrap();
        assert!(
            spawned.contains(&format!("t:{expected_t}")),
            "got {spawned}"
        );
        assert!(!spawned.contains("due:"));
    }

    fn urls(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).to_string()).collect()
    }

    #[test]
    fn apply_url_edits_appends_at_the_end_of_the_line() {
        assert_eq!(
            apply_url_edits(
                "Write docs +torudo id:t1",
                &urls(&["https://example.com/a"]),
                &[]
            ),
            "Write docs +torudo id:t1 https://example.com/a"
        );
    }

    #[test]
    fn apply_url_edits_keeps_a_url_the_line_already_has() {
        let line = "Write docs https://example.com/a id:t1";
        assert_eq!(
            apply_url_edits(line, &urls(&["https://example.com/a"]), &[]),
            line
        );
    }

    #[test]
    fn apply_url_edits_drops_only_the_url_asked_for() {
        assert_eq!(
            apply_url_edits(
                "Write docs https://example.com/a https://example.com/b id:t1",
                &[],
                &urls(&["https://example.com/a"])
            ),
            "Write docs https://example.com/b id:t1"
        );
    }

    #[test]
    fn apply_url_edits_removes_before_it_appends() {
        assert_eq!(
            apply_url_edits(
                "Write docs https://example.com/old id:t1",
                &urls(&["https://example.com/new"]),
                &urls(&["https://example.com/old"])
            ),
            "Write docs id:t1 https://example.com/new"
        );
    }

    #[test]
    fn apply_url_edits_will_not_remove_a_token_that_is_not_a_url() {
        // `remove` reaches here validated, but the guard means a caller that hands
        // it `id:t1` cannot strip the id off the line.
        let line = "Write docs id:t1";
        assert_eq!(apply_url_edits(line, &[], &urls(&["id:t1"])), line);
    }

    #[test]
    fn apply_url_edits_leaves_a_line_it_changes_nothing_on_alone() {
        let line = "Write docs id:t1";
        assert_eq!(apply_url_edits(line, &[], &urls(&["https://absent"])), line);
    }

    #[test]
    fn edit_urls_touches_only_the_item_with_the_id() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("todo.txt");
        fs::write(&file, "Write docs id:t1\nShip it id:t2\n").unwrap();

        edit_urls(
            file.to_str().unwrap(),
            "t1",
            &urls(&["https://example.com/a"]),
            &[],
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "Write docs id:t1 https://example.com/a\nShip it id:t2\n"
        );
    }
}
