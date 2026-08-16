use chrono::{Days, Months, NaiveDate};

use crate::md_preview;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Day,
    Week,
    Month,
    Year,
}

/// A parsed `rec:` value. Opaque outside this module: callers hand it back to
/// `next_dates` rather than inspecting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pattern {
    strict: bool,
    count: u32,
    unit: Unit,
}

impl Pattern {
    /// Move `base` forward by one period. Month and year arithmetic clamps to the
    /// end of the target month, so 1/31 + 1m lands on 2/28 (2/29 in a leap year).
    fn advance(self, base: NaiveDate) -> Result<NaiveDate, RecurrenceError> {
        let advanced = match self.unit {
            Unit::Day => base.checked_add_days(Days::new(u64::from(self.count))),
            Unit::Week => base.checked_add_days(Days::new(u64::from(self.count) * 7)),
            Unit::Month => base.checked_add_months(Months::new(self.count)),
            Unit::Year => self
                .count
                .checked_mul(12)
                .and_then(|months| base.checked_add_months(Months::new(months))),
        };
        advanced.ok_or(RecurrenceError::OutOfRange)
    }

    /// Apply the pattern to `base` repeatedly until the result is strictly past
    /// `floor`, so a long-overdue strict item catches up in one completion instead
    /// of needing one `x` per missed period. A date landing exactly on `floor`
    /// takes one more period. This is a deliberate departure from topydo, which
    /// only ever advances once.
    ///
    /// A loose pattern passes `base == floor`, and one period of at least a day
    /// always clears it, so the loop runs exactly once and the old behaviour holds.
    fn advance_past(self, base: NaiveDate, floor: NaiveDate) -> Result<NaiveDate, RecurrenceError> {
        let mut next = self.advance(base)?;
        while next <= floor {
            next = self.advance(next)?;
        }
        Ok(next)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecurrenceError {
    Malformed,
    ZeroCount,
    UnsupportedUnit(char),
    OutOfRange,
}

/// Parse the value of a `rec:` tag, e.g. `1d`, `2w`, `+1m`.
/// A leading `+` marks the pattern as strict (relative to the item's own dates
/// rather than to the completion date).
pub fn parse_pattern(value: &str) -> Result<Pattern, RecurrenceError> {
    let (strict, rest) = value
        .strip_prefix('+')
        .map_or((false, value), |rest| (true, rest));

    let mut chars = rest.chars();
    let unit_char = chars.next_back().ok_or(RecurrenceError::Malformed)?;
    let digits = chars.as_str();

    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(RecurrenceError::Malformed);
    }
    let count: u32 = digits.parse().map_err(|_| RecurrenceError::Malformed)?;
    if count == 0 {
        return Err(RecurrenceError::ZeroCount);
    }

    let unit = match unit_char {
        'd' => Unit::Day,
        'w' => Unit::Week,
        'm' => Unit::Month,
        'y' => Unit::Year,
        c if c.is_ascii_lowercase() => return Err(RecurrenceError::UnsupportedUnit(c)),
        _ => return Err(RecurrenceError::Malformed),
    };

    Ok(Pattern {
        strict,
        count,
        unit,
    })
}

impl std::fmt::Display for RecurrenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed => write!(f, "malformed recurrence pattern"),
            Self::ZeroCount => write!(f, "recurrence count must be at least 1"),
            Self::UnsupportedUnit(unit) => write!(f, "unsupported unit '{unit}'"),
            Self::OutOfRange => write!(f, "the next date falls outside the supported range"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NextDates {
    pub t: Option<NaiveDate>,
    pub due: Option<NaiveDate>,
}

/// Where the next occurrence counts from: the item's own date under a strict
/// pattern, the completion date otherwise. A strict pattern with no date of its
/// own falls back to the completion date too.
fn offset_for(pattern: Pattern, own: Option<NaiveDate>, completion: NaiveDate) -> NaiveDate {
    if pattern.strict {
        own.unwrap_or(completion)
    } else {
        completion
    }
}

/// Compute the dates of the next occurrence, following topydo's rules except that
/// a strict pattern catches up rather than advancing a single period; see
/// [`Pattern::advance_past`].
pub fn next_dates(
    pattern: Pattern,
    t: Option<NaiveDate>,
    due: Option<NaiveDate>,
    completion: NaiveDate,
) -> Result<NextDates, RecurrenceError> {
    // An item carrying only a threshold date moves that date; every other shape is
    // driven by the due date.
    if let (Some(old_t), None) = (t, due) {
        return Ok(NextDates {
            t: Some(
                pattern.advance_past(offset_for(pattern, Some(old_t), completion), completion)?,
            ),
            due: None,
        });
    }

    let new_due = pattern.advance_past(offset_for(pattern, due, completion), completion)?;
    // Keep the original gap between the threshold and the due date.
    let new_t = match (t, due) {
        (Some(old_t), Some(old_due)) => Some(
            new_due
                .checked_sub_signed(old_due - old_t)
                .ok_or(RecurrenceError::OutOfRange)?,
        ),
        _ => None,
    };
    Ok(NextDates {
        t: new_t,
        due: Some(new_due),
    })
}

/// Prepare an existing detail file for the next occurrence: clear every checked
/// box and drop the frontmatter's `branch:` line, which points at the worktree of
/// the occurrence that just finished. Checkbox detection matches
/// `md_preview::scan_md`, so a `- [x]` appearing mid-line is left alone.
pub fn reset_md(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut in_frontmatter = content.starts_with("---\n") || content == "---";
    for (index, line) in content.lines().enumerate() {
        if in_frontmatter {
            if index > 0 && line.trim_end() == "---" {
                in_frontmatter = false;
            } else if line.trim_start().starts_with("branch:") {
                continue;
            }
        }
        push_with_box_reset(&mut out, line);
        out.push('\n');
    }
    if !content.ends_with('\n') {
        out.pop();
    }
    out
}

/// Append `line` to `out`, turning a leading checked box into an empty one.
fn push_with_box_reset(out: &mut String, line: &str) {
    let trimmed = line.trim_start();
    let Some(rest) = md_preview::CHECKED_BOXES
        .iter()
        .find_map(|box_| trimmed.strip_prefix(box_))
    else {
        out.push_str(line);
        return;
    };
    let indent = line.len() - trimmed.len();
    out.push_str(&line[..indent]);
    out.push_str(md_preview::UNCHECKED_BOX);
    out.push_str(rest);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pattern_reads_a_simple_day_pattern() {
        assert_eq!(
            parse_pattern("1d"),
            Ok(Pattern {
                strict: false,
                count: 1,
                unit: Unit::Day,
            })
        );
    }

    fn loose(count: u32, unit: Unit) -> Pattern {
        Pattern {
            strict: false,
            count,
            unit,
        }
    }

    #[test]
    fn parse_pattern_reads_every_unit() {
        assert_eq!(parse_pattern("3d"), Ok(loose(3, Unit::Day)));
        assert_eq!(parse_pattern("1w"), Ok(loose(1, Unit::Week)));
        assert_eq!(parse_pattern("2m"), Ok(loose(2, Unit::Month)));
        assert_eq!(parse_pattern("1y"), Ok(loose(1, Unit::Year)));
    }

    #[test]
    fn parse_pattern_marks_a_leading_plus_as_strict() {
        assert_eq!(
            parse_pattern("+1w"),
            Ok(Pattern {
                strict: true,
                count: 1,
                unit: Unit::Week,
            })
        );
    }

    #[test]
    fn parse_pattern_rejects_malformed_values() {
        for value in ["", "banana", "-1d", "w", "1", "1dd", "++1d", "1D", "+"] {
            assert_eq!(
                parse_pattern(value),
                Err(RecurrenceError::Malformed),
                "{value} should be malformed"
            );
        }
    }

    #[test]
    fn parse_pattern_rejects_a_zero_count() {
        assert_eq!(parse_pattern("0w"), Err(RecurrenceError::ZeroCount));
        assert_eq!(parse_pattern("+0d"), Err(RecurrenceError::ZeroCount));
    }

    #[test]
    fn parse_pattern_reports_the_unsupported_unit() {
        assert_eq!(
            parse_pattern("1b"),
            Err(RecurrenceError::UnsupportedUnit('b'))
        );
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn advanced(pattern: &str, base: NaiveDate) -> Result<NaiveDate, RecurrenceError> {
        parse_pattern(pattern)?.advance(base)
    }

    #[test]
    fn advance_adds_days_and_weeks() {
        assert_eq!(advanced("3d", date(2026, 8, 15)), Ok(date(2026, 8, 18)));
        assert_eq!(advanced("1w", date(2026, 8, 15)), Ok(date(2026, 8, 22)));
    }

    #[test]
    fn advance_clamps_to_the_end_of_the_target_month() {
        assert_eq!(advanced("1m", date(2026, 1, 31)), Ok(date(2026, 2, 28)));
        assert_eq!(advanced("1m", date(2024, 1, 31)), Ok(date(2024, 2, 29)));
        assert_eq!(advanced("2m", date(2026, 12, 31)), Ok(date(2027, 2, 28)));
        // 4/30 + 1m is 5/30, not the end of May.
        assert_eq!(advanced("1m", date(2026, 4, 30)), Ok(date(2026, 5, 30)));
    }

    #[test]
    fn advance_crosses_the_year_boundary() {
        assert_eq!(advanced("1m", date(2026, 12, 15)), Ok(date(2027, 1, 15)));
        assert_eq!(advanced("1y", date(2024, 2, 29)), Ok(date(2025, 2, 28)));
        assert_eq!(advanced("1y", date(2026, 8, 15)), Ok(date(2027, 8, 15)));
    }

    #[test]
    fn advance_reports_out_of_range() {
        // chrono dates run to +262143-12-31, well past year 9999, so only the very
        // end of the calendar overflows.
        assert_eq!(
            advanced("1d", NaiveDate::MAX),
            Err(RecurrenceError::OutOfRange)
        );
        assert_eq!(
            advanced("1y", NaiveDate::MAX),
            Err(RecurrenceError::OutOfRange)
        );
        assert_eq!(advanced("1y", date(9999, 12, 31)), Ok(date(10000, 12, 31)));
    }

    fn next(
        pattern: &str,
        t: Option<NaiveDate>,
        due: Option<NaiveDate>,
        completion: NaiveDate,
    ) -> NextDates {
        next_dates(parse_pattern(pattern).unwrap(), t, due, completion).unwrap()
    }

    #[test]
    fn next_dates_with_only_due_is_relative_to_the_completion_date() {
        let completion = date(2026, 8, 15);
        assert_eq!(
            next("1w", None, Some(date(2026, 8, 10)), completion),
            NextDates {
                t: None,
                due: Some(date(2026, 8, 22)),
            }
        );
    }

    #[test]
    fn next_dates_with_only_due_and_strict_is_relative_to_the_old_due() {
        let completion = date(2026, 8, 15);
        assert_eq!(
            next("+1w", None, Some(date(2026, 8, 10)), completion),
            NextDates {
                t: None,
                due: Some(date(2026, 8, 17)),
            }
        );
    }

    #[test]
    fn next_dates_with_only_t_moves_t_and_leaves_due_unset() {
        let completion = date(2026, 8, 15);
        assert_eq!(
            next("1w", Some(date(2026, 8, 10)), None, completion),
            NextDates {
                t: Some(date(2026, 8, 22)),
                due: None,
            }
        );
    }

    #[test]
    fn next_dates_with_only_t_and_strict_is_relative_to_the_old_t() {
        let completion = date(2026, 8, 15);
        assert_eq!(
            next("+1w", Some(date(2026, 8, 10)), None, completion),
            NextDates {
                t: Some(date(2026, 8, 17)),
                due: None,
            }
        );
    }

    #[test]
    fn next_dates_with_both_dates_keeps_the_span_between_them() {
        let completion = date(2026, 8, 15);
        assert_eq!(
            next(
                "1w",
                Some(date(2026, 8, 8)),
                Some(date(2026, 8, 10)),
                completion
            ),
            NextDates {
                t: Some(date(2026, 8, 20)),
                due: Some(date(2026, 8, 22)),
            }
        );
    }

    #[test]
    fn next_dates_with_both_dates_and_strict_keeps_the_span_across_a_short_month() {
        // t:2026-01-25 due:2026-01-31 rec:+1m -> due 2026-02-28, t six days earlier.
        assert_eq!(
            next(
                "+1m",
                Some(date(2026, 1, 25)),
                Some(date(2026, 1, 31)),
                date(2026, 2, 3)
            ),
            NextDates {
                t: Some(date(2026, 2, 22)),
                due: Some(date(2026, 2, 28)),
            }
        );
    }

    #[test]
    fn next_dates_without_any_date_gives_the_next_occurrence_a_due_date() {
        let completion = date(2026, 8, 15);
        let expected = NextDates {
            t: None,
            due: Some(date(2026, 8, 22)),
        };
        assert_eq!(next("1w", None, None, completion), expected);
        // Strict has nothing of its own to anchor to, so it falls back to completion.
        assert_eq!(next("+1w", None, None, completion), expected);
    }

    #[test]
    fn next_dates_strict_catches_up_past_a_long_overdue_due_date() {
        // due:2025-01-01 rec:+1m completed 2026-08-15: repeating the pattern until it
        // clears the completion date lands on 2026-09-01, not 2025-02-01.
        assert_eq!(
            next("+1m", None, Some(date(2025, 1, 1)), date(2026, 8, 15)),
            NextDates {
                t: None,
                due: Some(date(2026, 9, 1)),
            }
        );
    }

    #[test]
    fn next_dates_strict_stops_after_one_period_when_that_already_clears_completion() {
        assert_eq!(
            next("+1w", None, Some(date(2026, 8, 10)), date(2026, 8, 15)),
            NextDates {
                t: None,
                due: Some(date(2026, 8, 17)),
            }
        );
    }

    #[test]
    fn next_dates_strict_week_from_three_days_back_always_lands_four_days_out() {
        // The invariant the completion tests in todo.rs, app_state.rs and
        // tests/mode_cli.rs seed their fixtures from, so that they assert the real
        // behaviour on whichever day they happen to run. Day arithmetic never
        // clamps, so it holds across month and year boundaries alike.
        let mut completion = date(2026, 1, 1);
        for _ in 0..800 {
            let due = completion.checked_sub_days(Days::new(3)).unwrap();
            assert_eq!(
                next("+1w", None, Some(due), completion),
                NextDates {
                    t: None,
                    due: completion.checked_add_days(Days::new(4)),
                },
                "completed on {completion}"
            );
            completion = completion.checked_add_days(Days::new(1)).unwrap();
        }
    }

    #[test]
    fn next_dates_strict_advances_again_when_it_lands_on_the_completion_date() {
        // 2026-08-08 + 1w is exactly the completion date, so it takes one more period.
        assert_eq!(
            next("+1w", None, Some(date(2026, 8, 8)), date(2026, 8, 15)),
            NextDates {
                t: None,
                due: Some(date(2026, 8, 22)),
            }
        );
    }

    #[test]
    fn next_dates_non_strict_still_advances_exactly_one_period() {
        // A loose pattern counts from the completion date, so it never catches up.
        assert_eq!(
            next("1m", None, Some(date(2025, 1, 1)), date(2026, 8, 15)),
            NextDates {
                t: None,
                due: Some(date(2026, 9, 15)),
            }
        );
        assert_eq!(
            next("1w", None, Some(date(2025, 1, 1)), date(2026, 8, 15)),
            NextDates {
                t: None,
                due: Some(date(2026, 8, 22)),
            }
        );
    }

    #[test]
    fn next_dates_strict_with_only_t_catches_up_past_the_completion_date() {
        assert_eq!(
            next("+1m", Some(date(2025, 1, 1)), None, date(2026, 8, 15)),
            NextDates {
                t: Some(date(2026, 9, 1)),
                due: None,
            }
        );
    }

    #[test]
    fn next_dates_keeps_the_span_between_the_dates_after_catching_up() {
        // t sits four days before due; that gap survives the catch-up.
        assert_eq!(
            next(
                "+1m",
                Some(date(2025, 1, 1)),
                Some(date(2025, 1, 5)),
                date(2026, 8, 15)
            ),
            NextDates {
                t: Some(date(2026, 9, 1)),
                due: Some(date(2026, 9, 5)),
            }
        );
    }

    #[test]
    fn next_dates_propagates_out_of_range_raised_while_catching_up() {
        let pattern = parse_pattern("+1d").unwrap();
        let base = NaiveDate::MAX.checked_sub_days(Days::new(3)).unwrap();
        assert_eq!(
            next_dates(pattern, None, Some(base), NaiveDate::MAX),
            Err(RecurrenceError::OutOfRange)
        );
    }

    #[test]
    fn next_dates_propagates_an_out_of_range_date() {
        let pattern = parse_pattern("1y").unwrap();
        assert_eq!(
            next_dates(pattern, None, None, NaiveDate::MAX),
            Err(RecurrenceError::OutOfRange)
        );
        assert_eq!(
            next_dates(pattern, Some(NaiveDate::MAX), None, NaiveDate::MAX),
            Err(RecurrenceError::OutOfRange)
        );
    }

    #[test]
    fn recurrence_error_describes_itself() {
        assert_eq!(
            RecurrenceError::UnsupportedUnit('b').to_string(),
            "unsupported unit 'b'"
        );
        assert_eq!(
            RecurrenceError::Malformed.to_string(),
            "malformed recurrence pattern"
        );
        assert_eq!(
            RecurrenceError::ZeroCount.to_string(),
            "recurrence count must be at least 1"
        );
        assert_eq!(
            RecurrenceError::OutOfRange.to_string(),
            "the next date falls outside the supported range"
        );
    }

    #[test]
    fn reset_md_clears_checked_boxes() {
        assert_eq!(
            reset_md("- [x] done\n- [X] also done\n- [ ] open\n"),
            "- [ ] done\n- [ ] also done\n- [ ] open\n"
        );
    }

    #[test]
    fn reset_md_keeps_indentation() {
        assert_eq!(
            reset_md("  - [x] nested\n\t- [X] tabbed\n"),
            "  - [ ] nested\n\t- [ ] tabbed\n"
        );
    }

    #[test]
    fn reset_md_leaves_checkbox_like_text_inside_a_line_alone() {
        let content = "type - [x] to check it off\n`- [x]` marks a done item\n";
        assert_eq!(reset_md(content), content);
    }

    #[test]
    fn reset_md_preserves_a_missing_trailing_newline() {
        assert_eq!(reset_md("- [x] done"), "- [ ] done");
        assert_eq!(reset_md(""), "");
    }

    #[test]
    fn reset_md_drops_branch_from_the_frontmatter_but_keeps_cwd() {
        let content = "---\ncwd: /home/me/src\nbranch: old-branch\n---\n\n- [x] done\n";
        assert_eq!(
            reset_md(content),
            "---\ncwd: /home/me/src\n---\n\n- [x] done\n".replace("- [x]", "- [ ]")
        );
    }

    #[test]
    fn reset_md_leaves_a_branch_line_outside_the_frontmatter_alone() {
        let content = "---\ncwd: /home/me/src\n---\n\nbranch: mentioned in the body\n";
        assert_eq!(reset_md(content), content);
    }

    #[test]
    fn reset_md_without_frontmatter_is_unchanged() {
        let content = "# Notes\n\nbranch: not frontmatter\n- [ ] open\n";
        assert_eq!(reset_md(content), content);
    }
}
