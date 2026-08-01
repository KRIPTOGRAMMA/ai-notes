// Extracting a domain from a browser window title.
//
// PRIVACY is this module's governing constraint, not an implementation detail.
// A browser window title contains the whole tab name: search queries, document
// names, the names of people in a conversation. It has never reached the DB
// (activity.rs takes only w.app and throws w.title away), and nothing here
// changes that: the domain is extracted in memory and the title is discarded
// immediately. By default this function is not called at all — it requires an
// explicit checkbox in Settings.
//
// Hence the governing rule: **at the slightest doubt, return None**. A false
// None costs one row of statistics; a false Some could write a fragment of a
// private title into the DB, having mistaken it for a domain.

// The browsers whose titles are worth parsing. An allowlist rather than
// "anything not on a blocklist": an editor's or messenger's title can contain a
// dot too, and without the restriction we would be writing fragments of other
// people's text into the DB.
const BROWSER_CLASSES: &[&str] = &[
    "firefox", "librewolf", "zen", "floorp", "waterfox",
    "chromium", "google-chrome", "brave-browser", "vivaldi", "opera",
    "microsoft-edge", "epiphany", "qutebrowser",
];

pub fn is_browser(app_class: &str) -> bool {
    let lower = app_class.to_ascii_lowercase();
    BROWSER_CLASSES.iter().any(|b| lower.contains(b))
}

// Whether something looks like a domain name. Deliberately strict: Latin
// letters, digits, hyphens and dots; at least one dot; a TLD of two letters or
// more. This rejects "version 2.0", "12.5 mm", dotted paths and other false
// positives.
fn looks_like_domain(s: &str) -> bool {
    let labels: Vec<&str> = s.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    let tld = labels[labels.len() - 1];
    if tld.len() < 2 || !tld.chars().all(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    labels.iter().all(|l| {
        !l.is_empty()
            && !l.starts_with('-')
            && !l.ends_with('-')
            && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

// Normalization: lowercase and without www, or github.com and WWW.GitHub.com
// would drift apart in the statistics as two different sites.
fn normalize(host: &str) -> String {
    let h = host.trim().to_ascii_lowercase();
    h.strip_prefix("www.").unwrap_or(&h).to_string()
}

/// The domain from a browser window title. None if the title contains nothing
/// confidently domain-like.
///
/// Browser titles look like this:
///   "Article title — Wikipedia — Mozilla Firefox"
///   "GitHub - user/repo: description — Mozilla Firefox"
///   "https://example.com/path — Mozilla Firefox"
/// They very often contain no domain at all: browsers show the page title, not
/// the URL. So None is the ordinary, expected result rather than a failure —
/// that window's time simply stays counted as "browser" with no per-site
/// breakdown.
pub fn domain_from_title(title: &str) -> Option<String> {
    // An explicit URL in the title is the most reliable case.
    for token in title.split_whitespace() {
        let t = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != ':' && c != '/' && c != '.' && c != '-');
        if let Some(rest) = t.strip_prefix("https://").or_else(|| t.strip_prefix("http://")) {
            let host = rest.split('/').next()?.split(':').next()?;
            if looks_like_domain(host) {
                return Some(normalize(host));
            }
        }
    }

    // Otherwise look for a bare domain among the title's words. We split on the
    // title separators (— - | ·) ourselves: "GitHub - user/repo" must not yield
    // "repo".
    for raw in title.split(|c: char| c.is_whitespace() || c == '|' || c == '·' || c == '—' || c == '–') {
        let token = raw.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-');
        // Strip the path and query: "example.com/a/b" -> "example.com"
        let host = token.split('/').next().unwrap_or(token);
        if looks_like_domain(host) {
            return Some(normalize(host));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_from_explicit_url() {
        assert_eq!(domain_from_title("https://github.com/user/repo — Firefox").as_deref(), Some("github.com"));
        assert_eq!(domain_from_title("http://example.com").as_deref(), Some("example.com"));
        // the port and path do not become part of the domain
        assert_eq!(domain_from_title("https://localhost.dev:3000/app").as_deref(), Some("localhost.dev"));
    }

    #[test]
    fn extracts_bare_domain_from_title() {
        assert_eq!(domain_from_title("Поиск — google.com — Mozilla Firefox").as_deref(), Some("google.com"));
        assert_eq!(domain_from_title("news.ycombinator.com").as_deref(), Some("news.ycombinator.com"));
    }

    #[test]
    fn normalizes_case_and_www() {
        // otherwise one site would split into several rows of statistics
        assert_eq!(domain_from_title("WWW.GitHub.COM").as_deref(), Some("github.com"));
        assert_eq!(domain_from_title("https://WWW.Example.com/x").as_deref(), Some("example.com"));
    }

    // The central privacy test: a title with no domain must NOT be turned into a
    // "domain" made from a fragment of private text.
    #[test]
    fn returns_none_when_no_domain_present() {
        assert_eq!(domain_from_title("Как уволиться красиво — Google Поиск"), None);
        assert_eq!(domain_from_title("Входящие (12) — Почта"), None);
        assert_eq!(domain_from_title("Договор_финал_v2 — LibreOffice Writer"), None);
        assert_eq!(domain_from_title(""), None);
        assert_eq!(domain_from_title("Mozilla Firefox"), None);
    }

    // A dot in some text does not make it a domain.
    #[test]
    fn does_not_mistake_numbers_and_versions_for_domains() {
        assert_eq!(domain_from_title("Версия 2.0 вышла"), None);
        assert_eq!(domain_from_title("Цена 12.50 руб"), None);
        // Cyrillic in a label fails is_ascii_alphanumeric, which is what we want:
        // "файл.txt" has the shape of a domain but is not one.
        assert_eq!(domain_from_title("файл.txt открыт"), None);
    }

    // A Latin filename with a TLD-like extension is a known limitation of the
    // heuristic, and a deliberate one: titles are parsed ONLY for browsers
    // (is_browser), where a filename like "report.com" is virtually unheard of,
    // and tightening the check to a list of real TLDs would mean keeping that
    // list in the code and updating it.
    #[test]
    fn known_limitation_file_like_names_in_browser_titles() {
        assert_eq!(domain_from_title("report.com открыт").as_deref(), Some("report.com"));
    }

    #[test]
    fn browser_whitelist_is_case_insensitive_and_matches_variants() {
        assert!(is_browser("firefox"));
        assert!(is_browser("Firefox"));
        assert!(is_browser("google-chrome"));
        assert!(is_browser("Brave-Browser"));
        // not browsers — their titles are not parsed at all
        assert!(!is_browser("kitty"));
        assert!(!is_browser("code"));
        assert!(!is_browser("telegram-desktop"));
    }
}
