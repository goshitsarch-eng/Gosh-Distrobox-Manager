#[derive(Debug, Default, Clone)]
pub struct DesktopEntry {
    pub name: String,
    pub exec: String,
    pub icon: String,
}

/// Extracts the first string enclosed in the specified quote character from a line of text.
/// Returns the extracted string without quotes, or None if no quoted string is found.
///
/// This is useful for parsing shell scripts and config files that use quoted strings.
///
/// # Examples
/// ```
/// # use gosh_distrobox_core::backends::desktop_file::extract_quoted_string;
/// let line = "exec '/usr/bin/vim' \"$@\"";
/// assert_eq!(extract_quoted_string(line, '\'').as_deref(), Some("/usr/bin/vim"));
/// ```
pub fn extract_quoted_string(line: &str, quote_char: char) -> Option<String> {
    let start = line.find(quote_char)?;
    let end = line[start + 1..].find(quote_char)?;
    Some(line[start + 1..start + 1 + end].to_string())
}

/// Decode the Desktop Entry Spec's *value* escapes: `\s` → space, `\n` → LF,
/// `\t` → TAB, `\r` → CR, `\\` → `\`.
///
/// This is the spec's `string` value rule, and it applies to every string
/// value — `Name`, `Icon` and `Exec` alike. Critically it runs **before** the
/// `Exec` key's own quoting rules, not after, and that order is observable.
/// GLib, the reference implementation, exposes the decoded value even for
/// `Exec` (`get_string("Exec")` on the file `Exec=/bin/echo a\sb` returns
/// `/bin/echo a b`) and tokenizes *that*, so `Exec=… --dir\s"my dir"` yields
/// argv `["--dir", "my dir"]`: the decoded space is a real separator, and the
/// pair of quotes still groups. Tokenizing first and decoding after would
/// instead leave the space inside the token and produce the single nonsense
/// argument `["--dirsmy dir"]` — which is what this crate did before D27.
///
/// The two layers compose in a way that is easy to get wrong in either
/// direction, so the reference behaviour is pinned by test: `a\\sb` is `asb`
/// (the value layer turns `\\` into `\`, then the tokenizer consumes that
/// backslash as the `\X` literal-`X` rule for the following `s`), and `\n`
/// inside single quotes still becomes a real newline (`Name`-style decoding
/// has already happened by the time quoting is considered, so single quotes
/// cannot protect an escape).
///
/// An unrecognised escape is passed through with its backslash rather than
/// rejected. GLib refuses the whole file instead; this is the one deliberate
/// divergence, and it is the permissive direction, because the string comes
/// out of a container and a malformed `Name` must not cost the user the app.
fn unescape_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Field codes that expand to an argument this app cannot supply, and so are
/// removed outright (architecture.md §6.4, row B4).
///
/// `%f %F %u %U` want a file/URI we do not have; `%c` wants the translated
/// name; `%k` wants the desktop file's path; `%v %m %d %D %n %N` are
/// deprecated.
///
/// `%i` is the one deliberate exception, and the reason is not that the pair
/// cannot be formed — the caller *does* hold `ExportableApp::entry.icon`, so
/// it could emit `--icon <name>`. It is dropped because the name is not ours
/// to pass: these entries come out of a container, and `distrobox-export`
/// rewrites `Icon=` on the way through (distrobox 1.8.2.5, `distrobox-export`
/// lines 582-590, replacing the key with a host-side absolute path under
/// `/run/host`). Expanding `%i` would hand the containerised program a host
/// path it cannot open — a broken `--icon` is worse than no `--icon`, since
/// `%i` "Should not expand to any arguments if the `Icon` key is empty or
/// missing" (Desktop Entry Spec), and for our purposes it effectively is.
const DROPPED_FIELD_CODES: &[char] = &[
    'f', 'F', 'u', 'U', 'i', 'c', 'k', 'v', 'm', 'd', 'D', 'n', 'N',
];

/// Apply the Desktop Entry Spec's field-code rules to one already-tokenized
/// argument. `%%` is the only escape (a literal `%`); a known code is removed;
/// anything else after a `%` — including a trailing lone `%` — is left alone
/// rather than guessed at.
fn strip_field_codes(token: &str) -> String {
    let mut out = String::new();
    let mut chars = token.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some(code) if DROPPED_FIELD_CODES.contains(&code) => {}
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// Split a desktop entry's `Exec=` value into real argv elements (B4,
/// architecture.md §6.4).
///
/// Previously `launch_app` stripped four field codes with a `str::replace`
/// fold and passed the whole remaining string as ONE `Command` argument.
/// `distrobox-enter` ends in `exec "$@"` with no `eval` and no re-split
/// (distrobox 1.8.2.5, the `--` branch: `shift; break` then `exec "$@"`), so
/// one element is one argv slot and the fused string became a single program
/// name — `exec "/usr/bin/foo --title My Document"` looks for a program
/// literally called that, and the app did not launch at all. Splitting is
/// what makes the Exec work, and it also restores the boundaries the entry
/// declared: `--title "My Document"` must reach the program as two slots,
/// with the space inside the second one intact.
///
/// Three passes, in the order the spec defines them:
///
/// **A — value escapes**, via `unescape_value`. This pass must come first, and
/// this function therefore takes the value **as written in the file**, not a
/// pre-decoded one — `parse_desktop_file` deliberately leaves `DesktopEntry.exec`
/// raw for exactly this reason. `\s` is a space *before* tokenizing, so it is a
/// real separator; decoding after tokenizing would glue it into a neighbour.
///
/// **B — tokenize** on unquoted, unescaped whitespace. Double quotes follow
/// the spec: `"…"` groups, and inside them `\` escapes only `"`, `` ` ``, `$`
/// and `\`, with any other backslash literal (backslash and all). `\X` outside
/// quotes means a literal `X`, and adjacent runs concatenate into one element
/// (`a"b c"d` → `ab cd`).
///
/// **Single quotes are not a spec feature** — the spec defines double-quote
/// enclosing only, and its reserved-character list merely mentions `'`. They
/// are supported here because both implementations of the key do support them
/// and treat the contents as fully literal (GLib by way of UNIX98 tokenization,
/// KDE by way of `KShell::splitArgs`), so honouring them matches what the entry
/// author's desktop actually does rather than what this crate prefers. An
/// unterminated quote runs to the end of the string and still yields its token —
/// the signature has no `Result`, and a malformed `Exec` in a container must not
/// panic the app.
///
/// The separator set is space, TAB and LF — **not** CR, even though pass A can
/// produce a CR from `\r`. That looks like an inconsistency between the two
/// passes and is in fact what the reference implementation does; see the note
/// on the whitespace arm in the tokenizer.
///
/// That set is GLib's. The spec itself says only "Arguments are separated by a
/// space" and never names CR, TAB or LF; GLib's `g_shell_parse_argv` adds TAB
/// and LF; KDE's `KShell::splitArgs` splits on a literal space **only** and
/// keeps TAB and LF inside the argument. This crate follows GLib, on two
/// grounds: the `Exec` values it tokenizes are read out of host `.desktop`
/// files, which the desktop's own GLib-based parser would tokenize the same
/// way, and the spec's reserved-character list names tab and newline — a list
/// that only means anything if those characters need quoting because they
/// separate arguments. Splitting on them is the reading that makes the spec
/// self-consistent; KDE's is a stricter subset, not a contradiction of it.
///
/// **C — field codes** per token, via `strip_field_codes`.
///
/// A token that held text before pass B and was entirely field codes is
/// dropped (`%u` on its own yields no empty argv slot), but a token that was
/// *already* empty — a bare `""` — is preserved as an empty argument, which
/// is what the spec's quoting rules ask for.
pub fn split_exec(exec: &str) -> Vec<String> {
    // Pass A: the value layer, before anything is tokenized.
    let decoded = unescape_value(exec);
    let exec = decoded.as_str();

    // Pass B.
    let mut tokens: Vec<String> = Vec::new();
    let mut cur = String::new();
    // `started` distinguishes "no token in progress" from "an empty token in
    // progress" (`""` must survive), which `cur.is_empty()` cannot.
    let mut started = false;
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // Space, TAB and LF only — **NOT** CR, and the asymmetry is
            // deliberate rather than an oversight. `\r` *is* in the value
            // escape table (pass A turns `Exec=…\r…` into a real CR), but the
            // tokenizer below does not treat the result as a separator: GLib's
            // `g_shell_parse_argv` splits on space/TAB/LF and keeps a CR
            // inside the token, so `Exec=/bin/echo a\rb` is argv
            // `["/bin/echo", "a\rb"]` — one element, CR and all — not three.
            // Measured, not inferred: `"\\r"` in quotes is a one-char token and
            // a bare `\r` never separates, while `\t` and `\n` both do.
            //
            // TAB and LF are the crate's choice, not the spec's. The spec says
            // only "separated by a space" and never names them; GLib's switch
            // (`gshell.c`, `tokenize_command_line`) delimits on `'\n'` and
            // `' '`/`'\t'`, while KDE's `KShell::splitArgs` compares against a
            // literal space and nothing else, so `a\tb` is two arguments there
            // and one here. Following GLib is the deliberate call — the
            // rationale is on `split_exec`.
            ' ' | '\t' | '\n' => {
                if started {
                    tokens.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            '\'' => {
                started = true;
                for q in chars.by_ref() {
                    if q == '\'' {
                        break;
                    }
                    cur.push(q);
                }
            }
            '"' => {
                started = true;
                while let Some(q) = chars.next() {
                    match q {
                        '"' => break,
                        '\\' => match chars.peek() {
                            Some(&n) if matches!(n, '"' | '`' | '$' | '\\') => {
                                cur.push(n);
                                chars.next();
                            }
                            // A backslash before a newline inside double
                            // quotes: the backslash is consumed and the
                            // newline KEPT, so `"a\` LF `b"` is the single
                            // argument `a` LF `b`. Verified against GLib on
                            // both forms — it collapses `"a\` LF `b"` and
                            // `"a` LF `b"` to the same element, i.e. the
                            // backslash disappears and the newline does not.
                            // (This is *not* the unquoted rule, where both
                            // characters vanish; the two arms differ, and the
                            // differential run is what showed it.)
                            Some(&'\n') => {
                                cur.push('\n');
                                chars.next();
                            }
                            // Any other backslash inside double quotes is
                            // literal, backslash and all (spec rule).
                            _ => cur.push('\\'),
                        },
                        _ => cur.push(q),
                    }
                }
            }
            '\\' => match chars.next() {
                // A backslash immediately before a newline is a **line
                // continuation**: both characters are consumed and the token
                // is *not* broken, so `a` `\` LF `b` is the single argument
                // `ab`. This is reachable through pass A, which turns a
                // `\n` escape into a real LF — file text `a\\\nb` decodes to
                // exactly that sequence, and GLib's tokenizer cancels the
                // pair, launching `["ab"]`. Pushing the LF here instead (which
                // is what this arm did first) put a literal newline inside the
                // argument, an element GLib never produces.
                //
                // `started` is deliberately left alone rather than set: a
                // continuation is invisible, so an input that is *only* a
                // continuation produces no token at all, matching GLib's
                // discard of an empty unquoted token.
                Some('\n') => {}
                Some(n) => {
                    started = true;
                    cur.push(n);
                }
                None => {
                    started = true;
                    cur.push('\\');
                }
            },
            _ => {
                started = true;
                cur.push(c);
            }
        }
    }
    if started {
        tokens.push(cur);
    }

    // Pass C.
    let mut out = Vec::with_capacity(tokens.len());
    for token in tokens {
        let stripped = strip_field_codes(&token);
        if !token.is_empty() && stripped.is_empty() {
            continue;
        }
        out.push(stripped);
    }
    out
}

/// Parse a `.desktop` file's `[Desktop Entry]` into the three keys the app uses.
///
/// **`exec` is returned RAW — deliberately, and it is the one asymmetry here.**
/// `Name` and `Icon` are values whose escapes are decoded (`unescape_value`),
/// because nothing downstream re-reads them as a command; `Exec` is not,
/// because `split_exec` performs that same value pass itself as its first step.
/// Decoding it here as well would apply the rule **twice**, and for `Exec` —
/// whose value rule feeds a tokenizer — that is not cosmetic. Take the file
/// text `--name=a\\sb`: decoded once it is `--name=a\sb`, which tokenizes to
/// **two** arguments (`--name=a`, `b`, since the surviving `\s` is an escaped
/// space); decoded twice it is `--name=asb`, one argument. Both look plausible
/// and only one is right, so the ownership is stated rather than implied:
/// `DesktopEntry.exec` is the file's text, unmodified, and every consumer must
/// go through `split_exec` rather than tokenizing it.
pub fn parse_desktop_file(content: &str) -> anyhow::Result<DesktopEntry> {
    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut in_desktop_entry = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "[Desktop Entry]" {
            in_desktop_entry = true;
            continue;
        }

        if !in_desktop_entry || !trimmed.contains('=') {
            continue;
        }

        let mut parts = trimmed.splitn(2, '=');
        if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
            match key.trim() {
                // See the fn doc: `Name`/`Icon` are decoded here, `Exec` is not.
                "Name" => name = Some(unescape_value(value.trim())),
                "Exec" => exec = Some(value.trim().to_string()),
                "Icon" => icon = Some(unescape_value(value.trim())),
                _ => {}
            }
        }

        if name.is_some() && exec.is_some() && icon.is_some() {
            break; // Exit early if we have all required fields
        }
    }

    let name = name.ok_or_else(|| anyhow::anyhow!("Missing Name key"))?;
    let exec = exec.ok_or_else(|| anyhow::anyhow!("Missing Exec key"))?;
    let icon = icon.unwrap_or_default();

    Ok(DesktopEntry { name, icon, exec })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_desktop_entry() {
        let content = r#"
[Desktop Entry]
Name=Firefox
Exec=/usr/bin/firefox %u
Icon=firefox
        "#;
        let entry = parse_desktop_file(content).unwrap();
        assert_eq!(&entry.name, "Firefox");
        assert_eq!(&entry.exec, "/usr/bin/firefox %u");
        assert_eq!(&entry.icon, "firefox");
    }

    /// The value-escape rule reaches `Name`/`Icon` (nothing re-tokenizes them)
    /// but NOT `Exec` (which `split_exec` decodes itself). Pinned together
    /// because the interesting failure is the *asymmetry* going wrong. The
    /// `--name=a\\sb` fixture is what makes a double decode visible: once it
    /// yields four arguments, twice it yields five — and neither throws, so
    /// only an assertion can tell them apart.
    #[test]
    fn name_and_icon_are_unescaped_but_exec_stays_raw() {
        let content = "[Desktop Entry]\n\
            Name=Foo\\sBar\n\
            Exec=/usr/bin/foo --dir\\s\"my dir\" --name=a\\\\sb\n\
            Icon=foo\\sbar\n";
        let entry = parse_desktop_file(content).unwrap();
        assert_eq!(entry.name, "Foo Bar", "Name decodes \\s");
        assert_eq!(entry.icon, "foo bar", "Icon decodes \\s");
        // Byte-for-byte what the file said: the value pass belongs to split_exec.
        assert_eq!(entry.exec, r#"/usr/bin/foo --dir\s"my dir" --name=a\\sb"#);
        // The pairing that makes the asymmetry correct. The last argument is
        // the discriminating one: the file text `a\\sb` survives the value
        // layer as `a\sb`, and the tokenizer's outside-quote `\X` rule then
        // consumes that backslash, leaving `a` + `sb` fused. GLib, the
        // reference implementation, lands in the same place (`a\\sb` → `asb`).
        assert_eq!(
            split_exec(&entry.exec),
            vec!["/usr/bin/foo", "--dir", "my dir", "--name=asb"]
        );
        // Assert the diverging alternative too, so this cannot pass by
        // accident: a second value pass turns `\s` into a real space and the
        // last argument splits in two. If the two ever agreed, the raw-vs-
        // decoded distinction would be untested no matter what the line above
        // says.
        assert_ne!(
            split_exec(&unescape_value(&entry.exec)),
            split_exec(&entry.exec),
            "a second value pass must be observable, or the asymmetry is untested"
        );
    }

    /// A real newline escape in `Name` decodes too — the same rule, and the
    /// one case where the decoded value cannot be re-parsed as a `.desktop`
    /// line (which is why the decode must happen AFTER line splitting, as it
    /// does: `lines()` reads the raw file first).
    #[test]
    fn name_unescape_happens_after_line_splitting() {
        let entry =
            parse_desktop_file("[Desktop Entry]\nName=First\\nSecond\nExec=/bin/true\nIcon=x\n")
                .unwrap();
        assert_eq!(entry.name, "First\nSecond");
    }

    #[test]
    fn test_missing_desktop_entry_section() {
        let content = r#"
[Some Other Section]
Name=Firefox
Exec=/usr/bin/firefox %u
Icon=firefox
        "#;
        let result = parse_desktop_file(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_required_fields() {
        let content = r#"
[Desktop Entry]
Icon=firefox
        "#;
        let result = parse_desktop_file(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_sections() {
        let content = r#"
[Desktop Action NewWindow]
Name=New Window
Exec=firefox --new-window

[Desktop Entry]
Name=Firefox
Exec=/usr/bin/firefox
Icon=firefox

[Desktop Action NewPrivateWindow]
Name=New Private Window
Exec=firefox --private-window
        "#;
        let entry = parse_desktop_file(content).unwrap();
        assert_eq!(&entry.name, "Firefox");
        assert_eq!(&entry.exec, "/usr/bin/firefox");
    }

    #[test]
    fn test_fields_with_equals_in_value() {
        let content = r#"
[Desktop Entry]
Name=Test=App
Exec=/usr/bin/test --param=value
Icon=test-icon
        "#;
        let entry = parse_desktop_file(content).unwrap();
        assert_eq!(&entry.name, "Test=App");
        assert_eq!(&entry.exec, "/usr/bin/test --param=value");
    }

    // ---- B4: split_exec -------------------------------------------------

    #[test]
    fn split_exec_plain_argv_is_each_word() {
        assert_eq!(
            split_exec("/usr/bin/firefox --new-window"),
            vec!["/usr/bin/firefox", "--new-window"]
        );
    }

    /// Every code the spec says to remove, including the ones the old
    /// `[" %f", " %u", " %F", " %U"]` fold never knew about.
    #[test]
    fn split_exec_strips_known_field_codes() {
        assert_eq!(
            split_exec("/usr/bin/firefox %f %F %u %U %i %c %k %v %m %d %D %n %N"),
            vec!["/usr/bin/firefox"]
        );
    }

    /// The old fold's needles all carried a leading space, so a code at
    /// offset 0 survived into argv. `parse_desktop_file` trims the value, so
    /// `%U firefox` is the shape a real entry can produce.
    #[test]
    fn split_exec_strips_field_code_at_offset_zero() {
        assert_eq!(split_exec("%U firefox"), vec!["firefox"]);
    }

    /// A field code is `%` plus exactly ONE char, so `%u` inside `%ulevel`
    /// is a code and the rest is literal text — `--opt %ulevel/bar` is
    /// `--opt level/bar`. The old fold was anchored on `" %u"` (leading
    /// space), so it only ever matched a code that followed a space; a code
    /// glued to the preceding text (`--opt=%u`, `bar%ubaz`) survived into
    /// argv as literal `%u`. Position, not just presence, is the fix.
    #[test]
    fn split_exec_strips_codes_not_preceded_by_a_space() {
        assert_eq!(
            split_exec("/usr/bin/foo --opt %ulevel/bar"),
            vec!["/usr/bin/foo", "--opt", "level/bar"]
        );
        assert_eq!(
            split_exec("/usr/bin/foo bar%ubaz"),
            vec!["/usr/bin/foo", "barbaz"]
        );
        assert_eq!(
            split_exec("/usr/bin/foo --opt=%u"),
            vec!["/usr/bin/foo", "--opt="]
        );
    }

    /// `%%` is the only escape, and it must be consumed as a PAIR so `%%f`
    /// is a literal `%f` rather than a stripped `%f`.
    #[test]
    fn split_exec_percent_percent_is_a_literal_percent() {
        assert_eq!(
            split_exec("/usr/bin/echo 100%%"),
            vec!["/usr/bin/echo", "100%"]
        );
        assert_eq!(split_exec("/usr/bin/echo %%f"), vec!["/usr/bin/echo", "%f"]);
    }

    /// An unrecognised code is left alone (spec's leave-as-is convention),
    /// as is a trailing lone `%`.
    #[test]
    fn split_exec_leaves_unknown_and_lone_percent_alone() {
        assert_eq!(split_exec("/usr/bin/foo %z"), vec!["/usr/bin/foo", "%z"]);
        assert_eq!(split_exec("/usr/bin/foo 50%"), vec!["/usr/bin/foo", "50%"]);
    }

    /// A quoted argument stays ONE element with its space intact — the case
    /// the single-slot bug could never get right.
    #[test]
    fn split_exec_quoted_argument_stays_one_element() {
        assert_eq!(
            split_exec("/usr/bin/foo --title \"My Document\" %u"),
            vec!["/usr/bin/foo", "--title", "My Document"]
        );
    }

    /// Inside double quotes only `"`, `` ` ``, `$` and `\` are escapable;
    /// any other backslash is literal, backslash and all.
    ///
    /// The `\n` here arrives **after** the value pass has already turned it
    /// into a real newline, so it is a plain character by the time quoting is
    /// considered — which is why the expectation is a newline and not the two
    /// characters `\n`. GLib agrees: `Exec="/bin/echo 'a\nb'"` tokenizes to
    /// `["/bin/echo", "a\nb"]` with a real LF.
    #[test]
    fn split_exec_double_quote_escapes_only_the_spec_set() {
        assert_eq!(split_exec(r#"/bin/echo "a\"b""#), vec!["/bin/echo", "a\"b"]);
        assert_eq!(split_exec(r#"/bin/echo "a\nb""#), vec!["/bin/echo", "a\nb"]);
        assert_eq!(
            split_exec(r"/bin/echo plain\ backslash"),
            vec!["/bin/echo", r"plain backslash"]
        );
    }

    /// A backslash before a newline is a line continuation, and the case that
    /// reaches it is not exotic: pass A decodes `\n` into a real LF, so the
    /// file text `a\\\nb` (an escaped backslash, then an escaped newline)
    /// arrives at the tokenizer as `a` `\` LF `b` and must become the single
    /// argument `ab`.
    ///
    /// Verified against GLib, which launches exactly `["ab"]` for that file.
    /// Treating the LF as an ordinary escaped character instead — the first
    /// implementation — produced one argument containing a literal newline,
    /// an argv element no reference implementation ever emits.
    #[test]
    fn split_exec_backslash_before_decoded_newline_is_a_continuation() {
        // The decoded form, spelled directly.
        assert_eq!(split_exec("/bin/echo a\\\nb"), vec!["/bin/echo", "ab"]);
        // …and the file text that decodes into it, so the two layers are shown
        // composing rather than assumed to.
        assert_eq!(
            split_exec(r"/bin/echo a\\\nb"),
            vec!["/bin/echo", "ab"],
            "escaped backslash + escaped newline is a continuation"
        );
        // A continuation does not break the token: the neighbours join.
        assert_eq!(split_exec("/bin/echo xa\\\nby"), vec!["/bin/echo", "xaby"]);
        // A continuation with nothing before it yields no argument at all,
        // matching GLib's discard of an empty unquoted token.
        assert_eq!(split_exec("/bin/echo \\\n"), vec!["/bin/echo"]);
        // Inside double quotes the rule is NOT the same, and assuming it was
        // is exactly the mistake the differential run caught: the backslash is
        // consumed but the newline is KEPT, so this is one argument holding a
        // real LF — identical to the same input without the backslash, which
        // is what GLib produces for both.
        assert_eq!(
            split_exec("/bin/echo \"a\\\nb\""),
            vec!["/bin/echo", "a\nb"]
        );
        assert_eq!(
            split_exec("/bin/echo \"a\\\nb\""),
            split_exec("/bin/echo \"a\nb\""),
            "the backslash is invisible inside quotes; the newline is not"
        );
    }

    /// Single quotes take no escapes at all — but "no escapes" is a rule about
    /// the **tokenizer** layer, and it cannot reach back and un-decode a value
    /// escape that already fired. So `\n` inside single quotes is still a real
    /// newline: the value layer ran first, and it does not look at quotes.
    /// Matches GLib's `['/bin/echo', 'a\nb']` for exactly this input.
    #[test]
    fn split_exec_single_quotes_are_fully_literal() {
        assert_eq!(split_exec(r#"/bin/echo 'a\nb'"#), vec!["/bin/echo", "a\nb"]);
        // A backslash the value layer left alone (an unrecognised escape) IS
        // protected by the single quotes, which is the case that shows the two
        // layers are genuinely separate.
        assert_eq!(
            split_exec(r#"/bin/echo 'a\qb'"#),
            vec!["/bin/echo", r"a\qb"]
        );
        assert_eq!(split_exec("/bin/echo 'a b'"), vec!["/bin/echo", "a b"]);
    }

    /// `\s` is the spec's *escaped space*, and because the value layer runs
    /// before tokenizing it is a real separator — so `--dir\s"my dir"` is two
    /// argv slots, not one fused `--dirsmy dir`. This is the case that proves
    /// the pass ORDER; it is also the exact input that motivated it, since the
    /// value-escaping style appears in desktop files in the wild.
    ///
    /// Reference behaviour (GLib `GDesktopAppInfo`):
    /// `Exec=/bin/echo --dir\s"my dir"` → `["/bin/echo", "--dir", "my dir"]`.
    #[test]
    fn split_exec_value_escaped_space_separates_arguments() {
        assert_eq!(
            split_exec(r#"/bin/echo --dir\s"my dir""#),
            vec!["/bin/echo", "--dir", "my dir"]
        );
        assert_eq!(
            split_exec(r"/usr/bin/foo --dir\shere"),
            vec!["/usr/bin/foo", "--dir", "here"]
        );
    }

    /// The same escape **inside** double quotes stays one element: the space is
    /// decoded first, then the quotes group it. (Without the grouping it would
    /// split; with a naive post-hoc decode it would stay literal as
    /// `My\sDocument`.)
    #[test]
    fn split_exec_value_escaped_space_inside_quotes_stays_one_argument() {
        assert_eq!(
            split_exec(r#"/usr/bin/foo "My\sDocument""#),
            vec!["/usr/bin/foo", "My Document"]
        );
        assert_eq!(
            split_exec(r#"/usr/bin/foo 'My\sDocument'"#),
            vec!["/usr/bin/foo", "My Document"]
        );
    }

    /// `\\` is one backslash, and it survives the value pass into the
    /// tokenizer — where a `\` outside quotes is the `\X` → literal `X` rule.
    /// So `a\\sb` is `asb`, the composition GLib produces. Getting this wrong
    /// in either direction is easy: `parse_desktop_file` must NOT decode this
    /// earlier, or `C:\\path` would lose its backslash entirely.
    #[test]
    fn split_exec_doubled_backslash_composes_with_the_tokenizer() {
        assert_eq!(split_exec(r"/bin/echo a\\sb"), vec!["/bin/echo", "asb"]);
        // Path values then keep the backslashes the user wrote — the value
        // pass is idempotent-with-`\\` here only because the tokenizer would
        // otherwise eat ONE level. Pinned so neither pass can be "simplified"
        // without this failing.
        assert_eq!(split_exec(r"/bin/echo a\\\\sb"), vec!["/bin/echo", r"a\sb"]);
    }

    /// `\t` decodes in the value pass and is tokenizer whitespace by the time
    /// it runs, so it separates arguments; a REAL tab is whitespace too, so the
    /// decoded and literal forms agree. Matches GLib.
    #[test]
    fn split_exec_decoded_tab_is_a_separator() {
        assert_eq!(split_exec(r"/bin/echo a\tb"), vec!["/bin/echo", "a", "b"]);
        assert_eq!(split_exec("/bin/echo a\tb"), vec!["/bin/echo", "a", "b"]);
    }

    /// CR is the exception, and pinning it matters because the obvious guess is
    /// wrong. `\r` IS decoded by the value pass, but the tokenizer does not
    /// treat the result as a separator: GLib gives `["/bin/echo", "a\rb"]` for
    /// the same input — one element carrying a carriage return. Splitting here
    /// would silently rewrite an argument the entry declared.
    ///
    /// Verified against GLib (`g_shell_parse_argv`) rather than inferred from
    /// the escape table: `\r` is in the table, yet `a\rb` stays one token,
    /// while `\t` and `\n` in the same position split.
    #[test]
    fn split_exec_decoded_cr_stays_inside_the_argument() {
        assert_eq!(
            split_exec(r"/bin/echo a\rb"),
            vec!["/bin/echo", "a\rb"],
            "a decoded CR is not tokenizer whitespace"
        );
        // A literal CR behaves identically — one rule, not two paths.
        assert_eq!(split_exec("/bin/echo a\rb"), vec!["/bin/echo", "a\rb"]);
        // And a quoted one is the same single element (the unquoted case above
        // is the one that would break if CR were added to the separator arm).
        assert_eq!(split_exec("/bin/echo \"\r\""), vec!["/bin/echo", "\r"]);
    }

    /// An escape the spec does not define is **not** rejected: the value layer
    /// keeps the backslash (this crate's one deliberate divergence from GLib,
    /// which refuses the whole file — permissive on purpose, since the string
    /// comes from a container and a stray `\q` must not cost the user the app).
    ///
    /// But "the value layer keeps it" is not the same as "argv contains it",
    /// and this test pins the difference rather than assuming it: the tokenizer
    /// then applies its own outside-quote `\X` → literal `X` rule, so a bare
    /// `a\qb` reaches the program as `aqb`. The backslash is observable only
    /// where that rule cannot fire — inside quotes, which is asserted in
    /// `split_exec_single_quotes_are_fully_literal`.
    ///
    /// GLib does the same thing with a kept backslash: `Exec=/bin/echo a\\sb`
    /// is `asb` there too. The divergence is only in whether the file loads.
    #[test]
    fn split_exec_undefined_escape_loads_and_composes_with_the_tokenizer() {
        assert_eq!(split_exec(r"/bin/echo a\qb"), vec!["/bin/echo", "aqb"]);
        assert_eq!(split_exec(r"/bin/echo ab\"), vec!["/bin/echo", "ab\\"]);
        assert_eq!(
            split_exec(r"/bin/echo 'a\qb'"),
            vec!["/bin/echo", r"a\qb"],
            "quotes are the one place a kept backslash survives to argv"
        );
    }

    /// Adjacent quoted runs are one element.
    #[test]
    fn split_exec_adjacent_quoted_runs_concatenate() {
        assert_eq!(
            split_exec(r#"/bin/echo a"b c"d"#),
            vec!["/bin/echo", "ab cd"]
        );
    }

    /// A malformed Exec must not panic: the signature has no `Result`, and
    /// this string comes from a container.
    #[test]
    fn split_exec_unterminated_quote_yields_its_token() {
        assert_eq!(
            split_exec("/bin/echo \"unterminated"),
            vec!["/bin/echo", "unterminated"]
        );
        assert_eq!(
            split_exec("/bin/echo 'unterminated"),
            vec!["/bin/echo", "unterminated"]
        );
        assert_eq!(split_exec("/bin/echo \\"), vec!["/bin/echo", "\\"]);
    }

    /// A token that was only field codes disappears (no empty argv slot);
    /// a token that was already an empty string is preserved.
    #[test]
    fn split_exec_field_code_only_token_is_dropped_but_empty_string_survives() {
        assert_eq!(split_exec("foo %u"), vec!["foo"]);
        assert_eq!(split_exec("foo \"\""), vec!["foo", ""]);
        assert_eq!(split_exec("%u"), Vec::<String>::new());
        assert_eq!(split_exec(""), Vec::<String>::new());
        assert_eq!(split_exec("   \t  "), Vec::<String>::new());
    }

    /// Tabs separate tokens like spaces do (they are in the spec's
    /// reserved-character set), and a tab before a code does not hide it.
    #[test]
    fn split_exec_separates_on_tabs() {
        assert_eq!(split_exec("foo\tbar"), vec!["foo", "bar"]);
        assert_eq!(split_exec("foo\t%u"), vec!["foo"]);
    }

    /// The security-shaped assertion: shell metacharacters in a
    /// container-supplied Exec are ordinary characters inside one element.
    /// There is no shell anywhere in this path, and no element is re-split.
    #[test]
    fn split_exec_keeps_metacharacters_inside_one_element() {
        assert_eq!(
            split_exec("/usr/bin/foo \"bar; rm -rf /\" %u"),
            vec!["/usr/bin/foo", "bar; rm -rf /"]
        );
    }

    /// `%i` contributes no phantom argument — it is dropped, not expanded
    /// into an empty (or `--icon`-shaped) slot.
    #[test]
    fn split_exec_percent_i_adds_no_argument() {
        assert_eq!(
            split_exec("/usr/bin/foo %i --evil"),
            vec!["/usr/bin/foo", "--evil"]
        );
    }

    #[test]
    fn test_extract_quoted_string_single_quotes() {
        let line = "exec '/usr/bin/vim' \"$@\"";
        assert_eq!(
            extract_quoted_string(line, '\''),
            Some("/usr/bin/vim".to_string())
        );
    }

    #[test]
    fn test_extract_quoted_string_double_quotes() {
        let line = r#"exec "distrobox-enter" -n test"#;
        assert_eq!(
            extract_quoted_string(line, '"'),
            Some("distrobox-enter".to_string())
        );
    }

    #[test]
    fn test_extract_quoted_string_no_quotes() {
        let line = "exec /usr/bin/vim";
        assert_eq!(extract_quoted_string(line, '\''), None);
    }

    #[test]
    fn test_extract_quoted_string_incomplete_quotes() {
        let line = "exec '/usr/bin/vim";
        assert_eq!(extract_quoted_string(line, '\''), None);
    }

    #[test]
    fn test_extract_quoted_string_empty() {
        let line = "exec ''";
        assert_eq!(extract_quoted_string(line, '\''), Some("".to_string()));
    }
}
