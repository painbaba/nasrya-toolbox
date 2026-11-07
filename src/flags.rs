//! Flag catcher: spots CTF flags in tool output (`picoCTF{...}`, `HTB{...}`,
//! and any `WORD{...}` that looks like one) so the dashboard can highlight,
//! list, copy and save them.
//!
//! No regex engine: a small scanner that returns character positions, so the
//! renderer can highlight exactly the flag even across wrapped rows.

/// Prefixes used by well-known platforms and events (compared lowercase).
/// These are trusted with any non-empty content.
pub const KNOWN: &[&str] = &[
    "flag", "ctf", "htb", "thm", "picoctf", "ductf", "csawctf", "csaw", "uiuctf", "actf", "lactf",
    "idek", "sekai", "dice", "hkcert", "bctf", "ictf", "nite", "tjctf", "wctf",
];

/// Longest prefix and content we consider, so a stray `{` far from its `}`
/// can't make a 2 kB "flag".
const MAX_PREFIX: usize = 24;
const MAX_INNER: usize = 160;

/// One flag found in a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Character (not byte) positions in the line, end exclusive.
    pub start: usize,
    pub end: usize,
    pub text: String,
    /// A known prefix (`picoCTF{`) rather than a generic `WORD{` guess.
    pub known: bool,
}

fn prefix_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// What a generic (unknown-prefix) flag may contain: no spaces, quotes,
/// colons, commas or braces, which is what keeps JSON and code out.
fn generic_inner_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '_' | '-'
                | '!'
                | '?'
                | '@'
                | '#'
                | '$'
                | '%'
                | '^'
                | '&'
                | '*'
                | '.'
                | '+'
                | '~'
                | '\''
        )
}

/// Every flag in `line`, left to right.
pub fn find(line: &str) -> Vec<Hit> {
    if !line.contains('{') {
        return Vec::new();
    }
    let c: Vec<char> = line.chars().collect();
    let mut hits = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if c[i] != '{' {
            i += 1;
            continue;
        }
        // the prefix: letters/digits/_/- right before the brace
        let mut j = i;
        while j > 0 && prefix_char(c[j - 1]) && i - (j - 1) <= MAX_PREFIX {
            j -= 1;
        }
        // it has to start with a letter (not `-x{` or `9{`)
        while j < i && !c[j].is_ascii_alphabetic() {
            j += 1;
        }
        if j == i {
            i += 1;
            continue;
        }
        // `${VAR}` / `%{x}` are templates, not flags
        if j > 0 && matches!(c[j - 1], '$' | '%') {
            i += 1;
            continue;
        }
        // the closing brace, on the same line, with nothing odd in between
        let mut k = i + 1;
        let mut closed = false;
        while k < c.len() && k - i <= MAX_INNER {
            match c[k] {
                '}' => {
                    closed = true;
                    break;
                }
                '{' => break,
                ch if ch.is_whitespace() || ch.is_control() => break,
                _ => k += 1,
            }
        }
        if !closed {
            i += 1;
            continue;
        }
        let prefix: String = c[j..i].iter().collect();
        let inner = &c[i + 1..k];
        let known = KNOWN.contains(&prefix.to_lowercase().as_str());
        let generic = !known
            && prefix.chars().any(|ch| ch.is_ascii_uppercase())
            && prefix.chars().count() >= 2
            && inner.len() >= 4
            && inner.iter().all(|&ch| generic_inner_char(ch))
            && inner.iter().any(|ch| ch.is_ascii_alphabetic());
        if (known && !inner.is_empty()) || generic {
            hits.push(Hit {
                start: j,
                end: k + 1,
                text: c[j..=k].iter().collect(),
                known,
            });
            i = k + 1;
        } else {
            i += 1;
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(line: &str) -> Vec<(String, bool)> {
        find(line).into_iter().map(|h| (h.text, h.known)).collect()
    }

    #[test]
    fn known_prefixes_any_case() {
        assert_eq!(
            texts("Secret: picoCTF{unp4ck3d_1t} done"),
            [("picoCTF{unp4ck3d_1t}".to_string(), true)]
        );
        assert_eq!(texts("HTB{a}"), [("HTB{a}".to_string(), true)]);
        assert_eq!(texts("flag{with spaces? no}"), []);
        assert_eq!(texts("FLAG{x_y-z!}"), [("FLAG{x_y-z!}".to_string(), true)]);
    }

    #[test]
    fn generic_flags_need_to_look_like_one() {
        assert_eq!(
            texts("key=HACK{9f2a_custom}"),
            [("HACK{9f2a_custom}".to_string(), false)]
        );
        // too short, lowercase word, JSON, templates, code
        for noise in [
            "opts={a:1}",
            "fmt{abcdef}",
            r#"{"key": "value"}"#,
            "${HOME}",
            "ACME{12345}",
            "Struct{a, b}",
            "if (x) {",
            "AB{x}",
        ] {
            assert_eq!(texts(noise), [], "{noise} was taken for a flag");
        }
    }

    #[test]
    fn several_per_line_and_char_positions() {
        let line = "→ ctf{one} and DUCTF{two_2}";
        let h = find(line);
        assert_eq!(h.len(), 2);
        // positions are in characters: the arrow is one char, three bytes
        let chars: Vec<char> = line.chars().collect();
        let s: String = chars[h[0].start..h[0].end].iter().collect();
        assert_eq!(s, "ctf{one}");
        assert_eq!(h[1].text, "DUCTF{two_2}");
    }

    #[test]
    fn unclosed_and_huge_are_ignored() {
        assert_eq!(texts("flag{never closed"), []);
        let long = format!("CTF{{{}}}", "a".repeat(500));
        assert_eq!(texts(&long), []);
    }
}
