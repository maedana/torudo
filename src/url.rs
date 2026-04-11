/// Extract all URLs (http:// or https://) from text.
pub fn extract_urls(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter(|word| word.starts_with("https://") || word.starts_with("http://"))
        .map(String::from)
        .collect()
}

/// Remove all URLs from text and clean up extra whitespace.
pub fn strip_urls(text: &str) -> String {
    text.split_whitespace()
        .filter(|word| !word.starts_with("https://") && !word.starts_with("http://"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Open all URLs in the default browser.
pub fn open_urls(urls: &[String]) {
    for url in urls {
        let _ = opener::open(url);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_urls_no_url() {
        assert_eq!(extract_urls("just plain text"), Vec::<String>::new());
    }

    #[test]
    fn extract_urls_single_https() {
        assert_eq!(
            extract_urls("check https://example.com/path please"),
            vec!["https://example.com/path"]
        );
    }

    #[test]
    fn extract_urls_single_http() {
        assert_eq!(
            extract_urls("visit http://example.com"),
            vec!["http://example.com"]
        );
    }

    #[test]
    fn extract_urls_multiple() {
        assert_eq!(
            extract_urls("see https://a.com and https://b.com/path"),
            vec!["https://a.com", "https://b.com/path"]
        );
    }

    #[test]
    fn extract_urls_at_start() {
        assert_eq!(
            extract_urls("https://example.com is the link"),
            vec!["https://example.com"]
        );
    }

    #[test]
    fn extract_urls_at_end() {
        assert_eq!(
            extract_urls("link is https://example.com"),
            vec!["https://example.com"]
        );
    }

    #[test]
    fn strip_urls_no_url() {
        assert_eq!(strip_urls("just plain text"), "just plain text");
    }

    #[test]
    fn strip_urls_single() {
        assert_eq!(
            strip_urls("check https://example.com/path please"),
            "check please"
        );
    }

    #[test]
    fn strip_urls_at_end() {
        assert_eq!(
            strip_urls("link is https://example.com"),
            "link is"
        );
    }

    #[test]
    fn strip_urls_at_start() {
        assert_eq!(
            strip_urls("https://example.com is the link"),
            "is the link"
        );
    }

    #[test]
    fn strip_urls_multiple() {
        assert_eq!(
            strip_urls("see https://a.com and https://b.com/path end"),
            "see and end"
        );
    }

    #[test]
    fn strip_urls_only_url() {
        assert_eq!(strip_urls("https://example.com"), "");
    }
}
