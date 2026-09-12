//! Localization via `i18n-embed` + Fluent (S8/T15), same pattern as the other
//! COSMIC apps in this family.
//!
//! This is a *crate-local* loader, deliberately not libcosmic's. libcosmic
//! exports its own `fl!` over its own `LANGUAGE_LOADER`, which embeds
//! libcosmic's `i18n/` — using it would resolve only libcosmic's own message
//! ids and silently fail on ours. So `app/` embeds its own catalogue and
//! defines its own `fl!`, shadowing anything re-exported.
//!
//! `#[macro_export]` puts `fl!` at the crate root, which is why `main.rs` (the
//! binary root) can call it bare while every submodule needs `use crate::fl;`.

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::{DefaultLocalizer, DesktopLanguageRequester, LanguageLoader, Localizer};
use rust_embed::RustEmbed;
use std::sync::LazyLock;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("failed to load fallback language (is app/i18n/en/*.ftl present?)");
    loader
});

/// Apply the system-requested languages. Call once before `app::run`.
pub fn init() {
    let localizer = DefaultLocalizer::new(&*LANGUAGE_LOADER, &Localizations);
    let requested = DesktopLanguageRequester::requested_languages();
    if let Err(e) = localizer.select(&requested) {
        tracing::warn!("failed to select requested languages: {e}");
    }
}

#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id)
    }};
    ($message_id:literal, $($args:expr),*) => {{
        i18n_embed_fl::fl!($crate::i18n::LANGUAGE_LOADER, $message_id, $($args),*)
    }};
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use fluent_bundle::types::FluentValue;

    /// Fluent wraps every interpolated placeable in bidi isolates (FSI `U+2068`
    /// / PDI `U+2069`) so an RTL value cannot reorder the text around it. That is
    /// correct and wanted, but it makes an equality assertion on message text
    /// confusing to read, so the tests below compare with the marks stripped and
    /// assert their presence separately.
    fn strip_isolates(s: &str) -> String {
        s.chars()
            .filter(|c| !matches!(*c, '\u{2066}'..='\u{2069}'))
            .collect()
    }

    fn render(id: &str, count: FluentValue<'static>) -> String {
        let mut args: HashMap<&str, FluentValue<'_>> = HashMap::new();
        args.insert("count", count);
        crate::i18n::LANGUAGE_LOADER.get_args_concrete(id, args)
    }

    fn render_int(id: &str, n: u64) -> String {
        render(id, FluentValue::from(n))
    }

    /// The trap these tests exist to keep out of the tree, pinned.
    ///
    /// `fl!(.., count = n)` expands to `args.insert("count", n.into())`, and
    /// `FluentValue`'s `From<&str>` yields `FluentValue::String` — it does NOT
    /// parse. A Fluent plural selector only matches `[one]`/`[few]`/… against a
    /// **Number**, so a stringified count falls through to `*[other]` and the app
    /// says "1 Containers Available" forever. An integer count routes through
    /// `From<u64>` → `FluentValue::Number`, which is the only path that pluralises.
    ///
    /// Both halves are asserted: integers pluralise, strings do not. So a future
    /// refactor that "helpfully" stringifies a count — `.to_string()`, `.as_str()`,
    /// a `format!` — fails here rather than in a locale nobody tested.
    #[test]
    fn count_args_must_stay_numeric_for_plurals_to_match() {
        assert_eq!(
            strip_isolates(&render_int("misc-updates-n-containers", 1)),
            "One Container Available"
        );
        assert_eq!(
            strip_isolates(&render_int("misc-updates-n-containers", 2)),
            "2 Containers Available"
        );
        assert_eq!(
            strip_isolates(&render_int("backup-n-snapshots", 1)),
            "One snapshot"
        );
        assert_eq!(
            strip_isolates(&render_int("backup-n-snapshots", 3)),
            "3 snapshots"
        );
        assert_eq!(
            strip_isolates(&render_int("app-all-containers-deleted", 1)),
            "All 1 container deleted"
        );

        // The negative control: a `&str` count is a well-typed, compiling call
        // that renders — wrongly. If this ever starts printing "One Container
        // Available", `From<&str>` changed and the doc comment above is stale.
        assert_eq!(
            strip_isolates(&render("misc-updates-n-containers", FluentValue::from("1"))),
            "1 Containers Available",
            "a string count must not pluralise"
        );

        // And the isolates are really there (guarding the stripper above, which
        // would otherwise silently make every comparison below weaker).
        assert!(
            render_int("misc-updates-n-containers", 2).contains('\u{2068}'),
            "Fluent should isolate the interpolated count"
        );
    }

    /// Every plural message in the shipped catalogue, found by reading the FTL
    /// rather than a hand-kept list, so a new `{ $count -> … }` cannot land
    /// outside this guard's coverage.
    ///
    /// `fl!` validates message *ids* at compile time, so an id typo cannot
    /// survive — but nothing compile-checks a *selector*. A message that only ever
    /// reaches `*[other]` compiles and renders perfectly. This is the runtime half.
    #[test]
    fn every_plural_message_reaches_its_one_branch() {
        let ftl = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/i18n/en/gosh_distrobox_manager.ftl"
        ))
        .expect("the catalogue must be readable at its embedding path");

        // Parse `id = { $count ->` … `}` into (id, one_branch, other_branch).
        let mut plurals: Vec<(String, String, String)> = Vec::new();
        let mut cur: Option<(String, String, String)> = None;
        for line in ftl.lines() {
            if let Some((id, rest)) = line.split_once(" = ")
                && rest.trim_start().starts_with("{ $count ->")
            {
                cur = Some((id.trim().to_string(), String::new(), String::new()));
                continue;
            }
            let Some((_, one, other)) = cur.as_mut() else {
                continue;
            };
            let t = line.trim();
            if t == "}" {
                plurals.push(cur.take().expect("just matched"));
            } else if let Some(body) = t.strip_prefix("*[other]") {
                *other = body.trim().to_string();
            } else if let Some(body) = t.strip_prefix("[one]") {
                *one = body.trim().to_string();
            }
        }

        assert_eq!(
            plurals.len(),
            16,
            "the catalogue ships 16 plural messages; found {}: {:?}",
            plurals.len(),
            plurals.iter().map(|p| &p.0).collect::<Vec<_>>()
        );

        for (id, one, other) in &plurals {
            // Only messages whose two branches actually differ can demonstrate
            // anything. `activity-time-*` is deliberately excluded by this: both
            // its branches are textually identical, so no count can distinguish
            // them and there is no `[one]` behaviour to lose.
            if one == other {
                continue;
            }
            let numeric = strip_isolates(&render_int(id, 1));
            let stringy = strip_isolates(&render(id, FluentValue::from("1")));
            assert_ne!(
                numeric, stringy,
                "{id}: a numeric 1 and a string \"1\" rendered identically ({numeric:?}), \
                 so the `[one]` branch is unreachable — something stringified the count"
            );
        }
    }

    /// The previous two tests prove *Fluent* can pluralise. This one guards the
    /// thing that actually regresses: a call site handing `fl!` a stringified
    /// count. Nothing else catches that — the call still compiles, still renders,
    /// and still reads correctly in English at every count except 1.
    ///
    /// Scans the crate's own sources for a `count = <expr>` argument *inside an
    /// `fl!` invocation* and rejects an `<expr>` that cannot be a number. Comments
    /// and string literals are skipped, and the invocation is found by matching
    /// parentheses, so multi-line calls and the doc comments in this very file
    /// (which discuss `count = ` at length) are not false positives.
    #[test]
    fn no_call_site_stringifies_a_count() {
        // Patterns that produce a `String`/`&str` where a `Number` is required.
        const STRINGY: [&str; 5] = [
            ".to_string()",
            ".as_str()",
            "format!(",
            "String::from",
            "to_owned()",
        ];

        let mut offenders = Vec::new();
        for (path, src) in sources() {
            let code = strip_comments(&src);
            for (start, args) in fl_invocations(&code) {
                let Some((_, rest)) = args.split_once("count = ") else {
                    continue;
                };
                // The count expression ends at the next top-level comma.
                let expr = rest.split(',').next().unwrap_or(rest).trim();
                if expr.starts_with('"') || STRINGY.iter().any(|bad| expr.contains(bad)) {
                    let line = src[..start].lines().count();
                    offenders.push(format!("{}:{line}: count = {expr}", path.display()));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "these pass a string as `count`, which silently disables plural \
             selection in every locale:\n  {}",
            offenders.join("\n  ")
        );
    }

    /// Every catalogue entry must be reachable from a call site.
    ///
    /// The mirror of I24 in PLAN.md (seven callerless helpers in `core/`): the
    /// extraction pass wrote entries for strings it found, and a few of those
    /// turned out to be superseded — `action-copy` by `misc-terminal-copy-command`,
    /// `action-copied` by `app-copied-to-clipboard`, `state-error` by
    /// `settings-load-failed`, and so on. A translator paid for all of them.
    ///
    /// Every id resolves through `fl!` (nothing reaches `LANGUAGE_LOADER`
    /// directly), so "appears in no `fl!` call" is exactly "unreachable" — this
    /// cannot be fooled by a dynamically-built id.
    #[test]
    fn every_catalogue_entry_has_a_caller() {
        let mut referenced = std::collections::HashSet::new();
        for (_, src) in sources() {
            let code = strip_comments(&src);
            for (_, args) in fl_invocations(&code) {
                // The id is the first argument, always a literal (`fl!` requires
                // one — the macro validates it at compile time).
                if let Some(rest) = args.trim_start().strip_prefix('"')
                    && let Some(id) = rest.split('"').next()
                {
                    referenced.insert(id.to_string());
                }
            }
        }

        let ftl = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/i18n/en/gosh_distrobox_manager.ftl"
        ))
        .expect("the catalogue must be readable at its embedding path");

        let unreferenced: Vec<&str> = ftl
            .lines()
            .filter_map(|line| line.split_once(" = ").map(|(id, _)| id.trim()))
            .filter(|id| !id.starts_with('#') && !referenced.contains(*id))
            .collect();

        assert!(
            unreferenced.is_empty(),
            "these catalogue entries have no caller — a translator was billed for \
             strings the app cannot display. Either wire them up or delete them:\n  {}",
            unreferenced.join("\n  ")
        );
    }

    /// Every `.rs` under `app/src`, paired with its source.
    fn sources() -> Vec<(std::path::PathBuf, String)> {
        let src_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        walk(src_dir)
            .into_iter()
            .filter_map(|p| std::fs::read_to_string(&p).ok().map(|s| (p, s)))
            .collect()
    }

    /// Each `fl!` invocation in `code`, as (byte offset, argument-list text).
    ///
    /// Found by matching parentheses rather than by line, so multi-line calls
    /// are read whole and a `)` inside a nested call does not end one early.
    fn fl_invocations(code: &str) -> Vec<(usize, &str)> {
        code.match_indices("fl!(")
            .filter_map(|(start, _)| {
                let open = start + "fl!".len();
                let close = matching_paren(code, open)?;
                Some((start, &code[open + 1..close]))
            })
            .collect()
    }

    /// The index just past the `)` that closes the `(` at `open`.
    fn matching_paren(s: &str, open: usize) -> Option<usize> {
        let mut depth = 0usize;
        for (i, c) in s[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(open + i);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Comment contents blanked to spaces, preserving byte offsets so a reported
    /// line number still points at the real source. Without this the `count = `
    /// in this file's own prose is reported as a defect.
    ///
    /// String literals are *kept*, deliberately: `count = "1"` is the most direct
    /// form of the bug and has to stay visible to the check. Tracking strings is
    /// also what keeps a `//` inside one from being mistaken for a comment.
    ///
    /// Deliberately not a full Rust lexer: line comments, block comments and
    /// `"`-literals are everything that can appear in an `fl!` argument list.
    fn strip_comments(src: &str) -> String {
        #[derive(PartialEq)]
        enum St {
            Code,
            Line,
            Block,
            Str,
        }
        let b: Vec<char> = src.chars().collect();
        let mut out = b.clone();
        let mut st = St::Code;
        let mut i = 0;
        while i < b.len() {
            let (c, n) = (b[i], b.get(i + 1).copied());
            match st {
                St::Code => {
                    if c == '/' && n == Some('/') {
                        st = St::Line;
                        out[i] = ' ';
                    } else if c == '/' && n == Some('*') {
                        st = St::Block;
                        out[i] = ' ';
                    } else if c == '"' {
                        st = St::Str;
                    }
                    i += 1;
                }
                St::Line => {
                    if c == '\n' {
                        st = St::Code;
                    } else {
                        out[i] = ' ';
                    }
                    i += 1;
                }
                St::Block => {
                    if c == '*' && n == Some('/') {
                        out[i] = ' ';
                        out[i + 1] = ' ';
                        st = St::Code;
                        i += 2;
                        continue;
                    }
                    if c != '\n' {
                        out[i] = ' ';
                    }
                    i += 1;
                }
                St::Str => {
                    if c == '\\' {
                        i += 2;
                        continue;
                    }
                    if c == '"' {
                        st = St::Code;
                    }
                    i += 1;
                }
            }
        }
        out.into_iter().collect()
    }

    #[cfg(test)]
    fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return out;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
        out
    }
}
