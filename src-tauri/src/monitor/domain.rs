// Извлечение домена из заголовка окна браузера (v0.9.31).
//
// ПРИВАТНОСТЬ — главное ограничение этого модуля, а не деталь реализации.
// Заголовок окна браузера содержит название вкладки целиком: поисковые
// запросы, названия документов, имена людей в переписке. В БД он не
// попадал никогда (activity.rs берёт только w.app, w.title выбрасывается),
// и эта версия ничего не меняет: домен извлекается в памяти, заголовок
// уходит в мусор сразу же. По умолчанию функция вообще не вызывается —
// нужна явная галочка в Настройках.
//
// Отсюда следует главное правило: **при малейшем сомнении возвращаем None**.
// Ложный None стоит потерянной строчки статистики; ложный Some может
// записать в БД кусок личного заголовка, приняв его за домен.

// Браузеры, чьи заголовки имеет смысл разбирать. Белый список, а не «всё,
// что не в чёрном»: в заголовке редактора или мессенджера тоже бывает
// точка, и без ограничения мы бы писали в БД обрывки чужих текстов.
const BROWSER_CLASSES: &[&str] = &[
    "firefox", "librewolf", "zen", "floorp", "waterfox",
    "chromium", "google-chrome", "brave-browser", "vivaldi", "opera",
    "microsoft-edge", "epiphany", "qutebrowser",
];

pub fn is_browser(app_class: &str) -> bool {
    let lower = app_class.to_ascii_lowercase();
    BROWSER_CLASSES.iter().any(|b| lower.contains(b))
}

// Похоже ли на доменное имя. Намеренно строго: латиница, цифры, дефис,
// точки; минимум одна точка; TLD от двух букв. Отсекает «версия 2.0»,
// «12.5 мм», пути с точками и прочие ложные срабатывания.
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

// Нормализация: нижний регистр и без www — иначе github.com и WWW.GitHub.com
// разъедутся в статистике как разные сайты.
fn normalize(host: &str) -> String {
    let h = host.trim().to_ascii_lowercase();
    h.strip_prefix("www.").unwrap_or(&h).to_string()
}

/// Домен из заголовка окна браузера. None — если заголовок не содержит
/// ничего, уверенно похожего на домен.
///
/// Заголовки браузеров выглядят так:
///   "Название статьи — Википедия — Mozilla Firefox"
///   "GitHub - user/repo: описание — Mozilla Firefox"
///   "https://example.com/path — Mozilla Firefox"
/// Домен в них есть далеко не всегда: браузеры показывают заголовок
/// страницы, а не URL. Поэтому None — обычный, ожидаемый результат, а не
/// ошибка: время такого окна просто останется учтённым как «браузер»
/// без разбивки по сайтам.
pub fn domain_from_title(title: &str) -> Option<String> {
    // Явный URL в заголовке — самый надёжный случай.
    for token in title.split_whitespace() {
        let t = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != ':' && c != '/' && c != '.' && c != '-');
        if let Some(rest) = t.strip_prefix("https://").or_else(|| t.strip_prefix("http://")) {
            let host = rest.split('/').next()?.split(':').next()?;
            if looks_like_domain(host) {
                return Some(normalize(host));
            }
        }
    }

    // Иначе ищем голый домен среди слов заголовка. Разделители заголовка
    // (— - | ·) режем сами: "GitHub - user/repo" не должен дать "repo".
    for raw in title.split(|c: char| c.is_whitespace() || c == '|' || c == '·' || c == '—' || c == '–') {
        let token = raw.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-');
        // Путь/запрос отсекаем: "example.com/a/b" → "example.com"
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
        // порт и путь не попадают в домен
        assert_eq!(domain_from_title("https://localhost.dev:3000/app").as_deref(), Some("localhost.dev"));
    }

    #[test]
    fn extracts_bare_domain_from_title() {
        assert_eq!(domain_from_title("Поиск — google.com — Mozilla Firefox").as_deref(), Some("google.com"));
        assert_eq!(domain_from_title("news.ycombinator.com").as_deref(), Some("news.ycombinator.com"));
    }

    #[test]
    fn normalizes_case_and_www() {
        // иначе один сайт разъедется в статистике на несколько строк
        assert_eq!(domain_from_title("WWW.GitHub.COM").as_deref(), Some("github.com"));
        assert_eq!(domain_from_title("https://WWW.Example.com/x").as_deref(), Some("example.com"));
    }

    // Главный приватностный тест: заголовок без домена НЕ должен превратиться
    // в «домен» из куска личного текста.
    #[test]
    fn returns_none_when_no_domain_present() {
        assert_eq!(domain_from_title("Как уволиться красиво — Google Поиск"), None);
        assert_eq!(domain_from_title("Входящие (12) — Почта"), None);
        assert_eq!(domain_from_title("Договор_финал_v2 — LibreOffice Writer"), None);
        assert_eq!(domain_from_title(""), None);
        assert_eq!(domain_from_title("Mozilla Firefox"), None);
    }

    // Точка в тексте не делает его доменом.
    #[test]
    fn does_not_mistake_numbers_and_versions_for_domains() {
        assert_eq!(domain_from_title("Версия 2.0 вышла"), None);
        assert_eq!(domain_from_title("Цена 12.50 руб"), None);
        // Кириллица в метке не проходит is_ascii_alphanumeric — и хорошо:
        // «файл.txt» формально имеет вид домена, но доменом не является.
        assert_eq!(domain_from_title("файл.txt открыт"), None);
    }

    // Латинское имя файла с расширением-как-TLD — известное ограничение
    // эвристики. Оно осознанное: заголовки разбираются ТОЛЬКО у браузеров
    // (is_browser), где «report.com» в имени файла практически не встречается,
    // а ужесточать проверку до списка реальных TLD означало бы держать этот
    // список в коде и обновлять его.
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
        // не браузеры — их заголовки не разбираются вовсе
        assert!(!is_browser("kitty"));
        assert!(!is_browser("code"));
        assert!(!is_browser("telegram-desktop"));
    }
}
