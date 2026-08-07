// A guard over the two-accent palette (v0.9.94).
//
// Both properties checked here are invisible at a glance and silently
// reversible. The gradient rules in `.btn-primary` have existed since long
// before the second accent had a value of its own: when `--accent-secondary`
// equals `--accent`, `linear-gradient` composes a colour with itself and the
// button renders as a flat fill — it looks deliberate, not broken. The same goes
// for the neutrals: nobody notices #f5f5f5 creeping back in place of #f4f2f8.
//
// The check lives on the Rust side for the reason spelled out in
// comments.test.ts: Vite runs stylesheets through its own pipeline, so
// `import.meta.glob` returns an EMPTY string for .css under every form of ?raw.
// A vitest guard would have listed the path and scanned nothing. `include_str!`
// reads the actual file, the same bridge i18n.rs and mock_guard.rs already use.
//
// What this does NOT check: that the two accents look good together, or that any
// component actually uses the second one. Those are taste and wiring; this is
// only the invariant that the palette has two distinct accents and no pure grey.

#[cfg(test)]
mod tests {
    const CSS: &str = include_str!("../../src/app.css");

    // The token block of one theme: `:root { … }` for light, `.dark { … }` for
    // dark. Slicing to the closing brace keeps a later block's tokens from
    // leaking into the search.
    fn theme_block(selector: &str) -> &'static str {
        let head = format!("{selector} {{");
        let at = CSS
            .find(&head)
            .unwrap_or_else(|| panic!("в app.css не найден блок {selector}"));
        let rest = &CSS[at..];
        let end = rest
            .find("\n}")
            .unwrap_or_else(|| panic!("блок {selector} не закрыт"));
        &rest[..end]
    }

    fn token(block: &str, name: &str) -> String {
        let needle = format!("--{name}:");
        let at = block
            .find(&needle)
            .unwrap_or_else(|| panic!("токен --{name} не объявлен"));
        let rest = &block[at + needle.len()..];
        let end = rest
            .find(';')
            .unwrap_or_else(|| panic!("токен --{name} без точки с запятой"));
        rest[..end].trim().to_string()
    }

    #[test]
    fn second_accent_differs_from_the_first_in_both_themes() {
        for selector in [":root", ".dark"] {
            let block = theme_block(selector);
            let accent = token(block, "accent");
            let secondary = token(block, "accent-secondary");
            assert_ne!(
                accent, secondary,
                "{selector}: --accent-secondary равен --accent, \
                 градиент .btn-primary вырождается в плоскую заливку"
            );
        }
    }

    #[test]
    fn neutral_surfaces_are_not_pure_grey() {
        // A pure grey has R == G == B. The violet lean is what separates this
        // palette from the default grey it grew out of, and it is one careless
        // revert away from being lost.
        fn is_pure_grey(hex: &str) -> bool {
            let h = hex.trim_start_matches('#');
            h.len() == 6 && h.is_char_boundary(2) && h[0..2] == h[2..4] && h[2..4] == h[4..6]
        }

        for selector in [":root", ".dark"] {
            let block = theme_block(selector);
            for name in ["bg-secondary", "bg-hover", "border"] {
                let value = token(block, name);
                assert!(
                    !is_pure_grey(&value),
                    "{selector}: --{name} = {value} — чистый серый"
                );
            }
        }
    }
}
