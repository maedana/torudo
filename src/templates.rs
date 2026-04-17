use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateEntry {
    pub name: String,
    pub path: String,
    pub content: String,
}

pub fn load_templates(dir: &Path) -> io::Result<Vec<TemplateEntry>> {
    let mut entries: Vec<TemplateEntry> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("md"))
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_stem()?.to_str()?.to_string();
            let content = fs::read_to_string(&path).ok()?;
            Some(TemplateEntry {
                name,
                path: path.to_string_lossy().into_owned(),
                content,
            })
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

pub fn compute_appended_content(existing: &str, content: &str) -> String {
    let separator: &str = if existing.is_empty() || existing.ends_with("\n\n") {
        ""
    } else if existing.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let mut new_content = format!("{existing}{separator}{content}");
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content
}

pub fn insert_template(md_path: &Path, content: &str) -> io::Result<()> {
    let existing = fs::read_to_string(md_path).unwrap_or_default();
    fs::write(md_path, compute_appended_content(&existing, content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn write_file(dir: &Path, name: &str, content: &str) {
        let mut f = File::create(dir.join(name)).unwrap();
        f.write_all(content.as_bytes()).unwrap();
    }

    #[test]
    fn load_templates_missing_dir_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("no-such-dir");
        assert!(load_templates(&missing).is_err());
    }

    #[test]
    fn load_templates_empty_dir_returns_empty_vec() {
        let tmp = tempfile::tempdir().unwrap();
        let result = load_templates(tmp.path()).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn load_templates_sorted_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(tmp.path(), "zeta.md", "z");
        write_file(tmp.path(), "alpha.md", "a");
        write_file(tmp.path(), "mu.md", "m");
        let result = load_templates(tmp.path()).unwrap();
        let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "mu", "zeta"]);
    }

    #[test]
    fn load_templates_ignores_non_md() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(tmp.path(), "good.md", "ok");
        write_file(tmp.path(), "skip.txt", "no");
        write_file(tmp.path(), "noext", "no");
        let result = load_templates(tmp.path()).unwrap();
        let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["good"]);
    }

    #[test]
    fn load_templates_reads_content() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(tmp.path(), "t.md", "hello\n- [ ] a\n");
        let result = load_templates(tmp.path()).unwrap();
        assert_eq!(result[0].content, "hello\n- [ ] a\n");
    }

    #[test]
    fn insert_template_creates_new_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("new.md");
        insert_template(&path, "## Design\n- [ ] spec\n").unwrap();
        let got = fs::read_to_string(&path).unwrap();
        assert_eq!(got, "## Design\n- [ ] spec\n");
    }

    #[test]
    fn insert_template_appends_with_blank_line_when_single_newline() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("existing.md");
        fs::write(&path, "# Todo\n").unwrap();
        insert_template(&path, "## Design\n- [ ] a\n").unwrap();
        let got = fs::read_to_string(&path).unwrap();
        assert_eq!(got, "# Todo\n\n## Design\n- [ ] a\n");
    }

    #[test]
    fn insert_template_appends_double_newline_when_no_trailing_newline() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("existing.md");
        fs::write(&path, "# Todo").unwrap();
        insert_template(&path, "## Design\n").unwrap();
        let got = fs::read_to_string(&path).unwrap();
        assert_eq!(got, "# Todo\n\n## Design\n");
    }

    #[test]
    fn insert_template_appends_directly_when_already_double_newline() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("existing.md");
        fs::write(&path, "# A\n\n").unwrap();
        insert_template(&path, "## B\n").unwrap();
        let got = fs::read_to_string(&path).unwrap();
        assert_eq!(got, "# A\n\n## B\n");
    }

    #[test]
    fn insert_template_ensures_trailing_newline() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("new.md");
        insert_template(&path, "no newline").unwrap();
        let got = fs::read_to_string(&path).unwrap();
        assert_eq!(got, "no newline\n");
    }

    #[test]
    fn compute_appended_content_empty_existing() {
        assert_eq!(compute_appended_content("", "## A\n"), "## A\n");
    }

    #[test]
    fn compute_appended_content_single_newline_adds_blank_line() {
        assert_eq!(compute_appended_content("# H\n", "## A\n"), "# H\n\n## A\n");
    }

    #[test]
    fn compute_appended_content_double_newline_keeps_separation() {
        assert_eq!(
            compute_appended_content("# H\n\n", "## A\n"),
            "# H\n\n## A\n"
        );
    }

    #[test]
    fn compute_appended_content_no_trailing_newline() {
        assert_eq!(compute_appended_content("# H", "## A\n"), "# H\n\n## A\n");
    }
}
