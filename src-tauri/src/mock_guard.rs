// A guard over the e2e Tauri mock (v0.9.80).
//
// `e2e/tauri-mock.js` reimplements the whole backend in JavaScript so Playwright
// can run against a real UI with no Rust process. That parallel implementation is
// hand-maintained: nothing forced a new `#[tauri::command]` to gain a mock, and an
// unmocked command silently resolves to `undefined` — the screen renders empty and
// the e2e test stays green while testing nothing.
//
// This module is test-only. It uses the same cross-boundary trick as
// error.rs::frontend_knows_the_same_prefixes: `include_str!` reaches into the
// frontend at compile time, so the check runs inside `cargo test` with no test
// runner able to skip it. It catches the stronger case — a command registered in
// Rust and missing from the mock — even when no e2e test calls it, which the
// runtime `__unknownInvokes` check in smoke.spec.ts cannot see.
//
// What it deliberately does NOT check: whether a mocked command *behaves* like the
// Rust one. That is semantics, not presence. A known live example is
// `complete_task`, where the mock advances a recurring deadline by exactly one day
// while `tasks.rs::complete_task_impl` computes a real `next_occurrence`; the name
// is present, so this guard is silent by design. Such divergence is caught only by
// an e2e test that asserts the value.

#[cfg(test)]
mod tests {
    // Commands that are intentionally absent from the mock. Both are read-only
    // probes over the in-memory ActivityTracker rather than the DB, and no screen
    // calls them through the frontend api layer, so the mock has nothing to serve.
    const ALLOWED_UNMOCKED: &[&str] = &["get_activity_state", "get_session_stats"];

    /// Command names registered in `tauri::generate_handler![...]`.
    ///
    /// Parsed rather than hand-listed: a hand-written copy is exactly the kind of
    /// second source of truth this guard exists to prevent.
    fn registered_commands(lib_rs: &str) -> Vec<String> {
        let start = lib_rs
            .find("tauri::generate_handler![")
            .expect("generate_handler! not found in lib.rs");
        let rest = &lib_rs[start..];
        let end = rest.find(']').expect("generate_handler! is not closed");

        rest[..end]
            .lines()
            .skip(1) // the `tauri::generate_handler![` line itself
            .filter_map(|line| {
                let line = line.trim().trim_end_matches(',');
                if line.is_empty() || line.starts_with("//") {
                    return None;
                }
                // `commands::tasks::create_task` -> `create_task`
                line.rsplit("::").next().map(str::to_string)
            })
            .filter(|name| !name.is_empty())
            .collect()
    }

    /// Command keys of the `commands` object in the mock.
    ///
    /// Scoped to that object on purpose: the file also holds a settings object
    /// whose keys sit at the same indentation and would otherwise be counted.
    fn mocked_commands(mock_js: &str) -> Vec<String> {
        let start = mock_js
            .find("const commands = {")
            .expect("`const commands = {` not found in tauri-mock.js");
        let rest = &mock_js[start..];
        let end = rest.find("\n  };").expect("the commands object is not closed");

        rest[..end]
            .lines()
            .filter_map(|line| {
                // Exactly four spaces of indentation = a key of this object, not of
                // something nested inside a handler.
                let key = line.strip_prefix("    ")?;
                if key.starts_with(' ') {
                    return None;
                }
                let name = key.split(':').next()?.trim();
                if name.is_empty() || !name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
                    return None;
                }
                Some(name.to_string())
            })
            .collect()
    }

    // The guard itself: everything registered in Rust must exist in the mock.
    #[test]
    fn every_registered_command_is_mocked() {
        let registered = registered_commands(include_str!("lib.rs"));
        let mocked = mocked_commands(include_str!("../../e2e/tauri-mock.js"));

        // A lower bound on both lists. Without it a parser broken by reformatting
        // would return an empty vec and this test would "pass" forever — the way
        // guards like this die silently.
        assert!(
            registered.len() > 120,
            "parsed only {} commands from generate_handler! — the parser is broken, \
             not the mock",
            registered.len()
        );
        assert!(
            mocked.len() > 120,
            "parsed only {} commands from tauri-mock.js — the parser is broken, \
             not the mock",
            mocked.len()
        );

        let missing: Vec<&String> = registered
            .iter()
            .filter(|name| !mocked.contains(name) && !ALLOWED_UNMOCKED.contains(&name.as_str()))
            .collect();

        assert!(
            missing.is_empty(),
            "commands registered in lib.rs but missing from e2e/tauri-mock.js: {missing:?}\n\
             An unmocked command resolves to `undefined` in e2e and the screen renders \
             empty while the test stays green. Add it to the mock, or to \
             ALLOWED_UNMOCKED with a reason."
        );
    }

    // The inverse direction: a mock entry for a command that no longer exists is
    // dead weight, and usually the leftover of a rename that was done on one side.
    #[test]
    fn mock_has_no_commands_the_backend_dropped() {
        let registered = registered_commands(include_str!("lib.rs"));
        let mocked = mocked_commands(include_str!("../../e2e/tauri-mock.js"));

        let extra: Vec<&String> = mocked
            .iter()
            .filter(|name| !registered.contains(name))
            .collect();

        assert!(
            extra.is_empty(),
            "e2e/tauri-mock.js mocks commands that are not registered in lib.rs: {extra:?}\n\
             Either the command was renamed on the Rust side only, or the mock entry \
             is dead and should go."
        );
    }

    // ALLOWED_UNMOCKED must not outlive its reason: once a command is mocked, its
    // exemption is stale and hides the next real gap.
    #[test]
    fn allowed_unmocked_entries_are_still_needed() {
        let registered = registered_commands(include_str!("lib.rs"));
        let mocked = mocked_commands(include_str!("../../e2e/tauri-mock.js"));

        for name in ALLOWED_UNMOCKED {
            assert!(
                registered.iter().any(|r| r == name),
                "ALLOWED_UNMOCKED lists `{name}`, which is not registered in lib.rs at all"
            );
            assert!(
                !mocked.contains(&name.to_string()),
                "`{name}` is mocked now — remove it from ALLOWED_UNMOCKED"
            );
        }
    }
}
