use std::fs;
use std::path::Path;
use std::time::SystemTime;

const PREVIEW_MAX: usize = 3;

#[derive(Debug, Clone)]
pub struct MdMeta {
    pub mtime: SystemTime,
    pub preview: Vec<String>,
    pub stats: Option<(usize, usize)>,
}

pub fn md_path(todotxt_dir: &str, id: &str) -> String {
    format!("{todotxt_dir}/todos/{id}.md")
}

fn read_mtime<P: AsRef<Path>>(path: P) -> Option<SystemTime> {
    fs::metadata(path).ok()?.modified().ok()
}

pub fn format_elapsed(mtime: SystemTime, now: SystemTime) -> String {
    let secs = now.duration_since(mtime).map(|d| d.as_secs()).unwrap_or(0);
    if secs < 60 {
        format!("{secs:2}s")
    } else if secs < 3600 {
        format!("{:2}m", secs / 60)
    } else {
        let hours = (secs / 3600).min(99);
        format!("{hours:2}h")
    }
}

fn scan_md(content: &str, max: usize) -> (Vec<String>, Option<(usize, usize)>) {
    let mut preview = Vec::new();
    let mut done = 0usize;
    let mut total = 0usize;
    for line in content.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("- [ ] ") {
            total += 1;
            if preview.len() < max {
                preview.push(rest.to_string());
            }
        } else if t.starts_with("- [x] ") || t.starts_with("- [X] ") {
            done += 1;
            total += 1;
        }
    }
    let stats = (total > 0).then_some((done, total));
    (preview, stats)
}

pub fn compute_meta(todotxt_dir: &str, id: &str) -> Option<MdMeta> {
    let path = md_path(todotxt_dir, id);
    let mtime = read_mtime(&path)?;
    let (preview, stats) = fs::read_to_string(&path).map_or_else(
        |_| (Vec::new(), None),
        |content| scan_md(&content, PREVIEW_MAX),
    );
    Some(MdMeta {
        mtime,
        preview,
        stats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Duration;

    fn st(now: SystemTime, secs_ago: u64) -> SystemTime {
        now - Duration::from_secs(secs_ago)
    }

    #[test]
    fn format_elapsed_seconds() {
        let now = SystemTime::now();
        assert_eq!(format_elapsed(st(now, 0), now), " 0s");
        assert_eq!(format_elapsed(st(now, 59), now), "59s");
    }

    #[test]
    fn format_elapsed_minutes() {
        let now = SystemTime::now();
        assert_eq!(format_elapsed(st(now, 60), now), " 1m");
        assert_eq!(format_elapsed(st(now, 3599), now), "59m");
    }

    #[test]
    fn format_elapsed_hours() {
        let now = SystemTime::now();
        assert_eq!(format_elapsed(st(now, 3600), now), " 1h");
        assert_eq!(format_elapsed(st(now, 99 * 3600), now), "99h");
    }

    #[test]
    fn format_elapsed_capped_at_99h() {
        let now = SystemTime::now();
        assert_eq!(format_elapsed(st(now, 200 * 3600), now), "99h");
    }

    #[test]
    fn format_elapsed_future_mtime_returns_zero() {
        let now = SystemTime::now();
        let future = now + Duration::from_secs(60);
        assert_eq!(format_elapsed(future, now), " 0s");
    }

    #[test]
    fn scan_md_no_boxes() {
        assert_eq!(scan_md("just text\n", 3), (vec![], None));
    }

    #[test]
    fn scan_md_all_checked() {
        let content = "- [x] done one\n- [X] done two\n";
        assert_eq!(scan_md(content, 3), (vec![], Some((2, 2))));
    }

    #[test]
    fn scan_md_unchecked_only() {
        let content = "- [ ] first\n- [ ] second\n";
        assert_eq!(
            scan_md(content, 3),
            (vec!["first".into(), "second".into()], Some((0, 2)))
        );
    }

    #[test]
    fn scan_md_caps_preview_but_keeps_counting() {
        let content = "- [ ] a\n- [ ] b\n- [ ] c\n- [ ] d\n";
        let (preview, stats) = scan_md(content, 3);
        assert_eq!(
            preview,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert_eq!(stats, Some((0, 4)));
    }

    #[test]
    fn scan_md_mixed_with_indent_and_tabs() {
        let content = "  - [ ] indented\n\t- [x] tabbed\n- [X] capital\n- [ ] last\n";
        let (preview, stats) = scan_md(content, 3);
        assert_eq!(preview, vec!["indented".to_string(), "last".to_string()]);
        assert_eq!(stats, Some((2, 4)));
    }

    fn write_md(dir: &Path, id: &str, content: &str) {
        let todos = dir.join("todos");
        fs::create_dir_all(&todos).unwrap();
        let mut f = fs::File::create(todos.join(format!("{id}.md"))).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn compute_meta_missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        assert!(compute_meta(dir.path().to_str().unwrap(), "ghost").is_none());
    }

    #[test]
    fn compute_meta_no_checkboxes_returns_no_stats() {
        let dir = tempfile::tempdir().unwrap();
        write_md(dir.path(), "id1", "just text\n");
        let meta = compute_meta(dir.path().to_str().unwrap(), "id1").unwrap();
        assert!(meta.preview.is_empty());
        assert!(meta.stats.is_none());
    }

    #[test]
    fn compute_meta_with_mixed_checkboxes() {
        let dir = tempfile::tempdir().unwrap();
        write_md(
            dir.path(),
            "id1",
            "- [x] done one\n- [x] done two\n- [ ] one\n- [ ] two\n- [ ] three\n- [ ] four\n- [ ] five\n",
        );
        let meta = compute_meta(dir.path().to_str().unwrap(), "id1").unwrap();
        assert_eq!(meta.preview.len(), 3);
        assert_eq!(meta.stats, Some((2, 7)));
    }

    #[test]
    fn compute_meta_only_completed_returns_empty_preview_with_stats() {
        let dir = tempfile::tempdir().unwrap();
        write_md(dir.path(), "id1", "- [x] only one\n");
        let meta = compute_meta(dir.path().to_str().unwrap(), "id1").unwrap();
        assert!(meta.preview.is_empty());
        assert_eq!(meta.stats, Some((1, 1)));
    }
}
