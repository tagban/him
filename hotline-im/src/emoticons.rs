//! Emoji and the old text faces, both ways (the same tables as the Discord bridge).
//!
//! Classic Hotline clients can't show emoji, so a modern client sends faces in their
//! place (`to_faces`) where old clients will read it, and shows faces as emoji
//! (`to_emoji`) when it displays them.

/// Emoji → the face a classic client shows. Longest keys are tried first.
const EMOJI: &[(&str, &str)] = &[
    ("☺️", ":)"), ("❤️", "<3"), ("😀", ":D"), ("😄", ":D"), ("😁", ":D"), ("😅", ":P"), ("😂", "XD"),
    ("🤣", "XD"), ("🙂", ":)"), ("🙃", "(:"), ("😉", ";)"), ("😊", ":)"), ("😇", "o:)"), ("🥰", "<3"),
    ("😍", "<3"), ("🤩", ":O"), ("😘", ":*"), ("😗", ":*"), ("😚", ":*"), ("😙", ":*"), ("😋", ":P"),
    ("😛", ":P"), ("😜", ";P"), ("🤪", "8P"), ("😝", "xP"), ("🤑", "$"), ("🤗", "\\o/"), ("🤭", ":X"),
    ("🤫", ":X"), ("🤔", ":?"), ("🤐", ":X"), ("🤨", "o.O"), ("😐", ":|"), ("😑", "-_-"), ("😶", ":|"),
    ("😏", ";)"), ("😒", ":/"), ("🙄", "o.O"), ("😬", ":S"), ("🤥", ":L"), ("😌", ":)"), ("😔", ":("),
    ("😪", ":|"), ("😴", "zzZ"), ("😷", ":S"), ("🤒", ":S"), ("🤕", ":S"), ("🤢", ":S"), ("🤮", ":O"),
    ("🤧", ":S"), ("🥵", "!!"), ("🥶", "??"), ("🥴", "8S"), ("😵", "Xo"), ("🤯", ":O"), ("🤠", "8)"),
    ("🥳", "\\o/"), ("😎", "8)"), ("🤓", "B)"), ("🧐", "8."), ("😕", ":/"), ("😟", ":("), ("🙁", ":("),
    ("😮", ":O"), ("😯", ":O"), ("😲", ":O"), ("😳", ":O"), ("🥺", ":("), ("😦", ":O"), ("😧", ":O"),
    ("😨", ":O"), ("😰", ":S"), ("😥", ":("), ("😢", ":("), ("😭", "=("), ("😱", ":O"), ("😖", ":S"),
    ("😣", ":S"), ("😞", ":("), ("😓", ":("), ("😩", "X("), ("😫", "X("), ("🥱", ":O"), ("😤", ">:("),
    ("😡", ">:("), ("😠", ">:("), ("🤬", ":@"), ("😈", ">:)"), ("👿", ">:("), ("💀", "[x]"), ("💩", "(p)"),
    ("👍", "(Y)"), ("👎", "(N)"), ("❤", "<3"), ("💔", "</3"), ("🙌", "\\o/"), ("😆", "XD"), ("😃", ":D"),
];

/// Face → emoji, for showing. A face counts only standing on its own, so "http://" and "a:b" stay put.
const FACES: &[(&str, &str)] = &[
    (":-)", "🙂"), (":)", "🙂"), ("=)", "🙂"), (";-)", "😉"), (";)", "😉"), (":-D", "😀"), (":D", "😀"),
    ("XD", "😆"), ("xD", "😆"), (":-(", "🙁"), (":(", "🙁"), ("=(", "😢"), (":'(", "😢"), (":-P", "😛"),
    (":P", "😛"), (":p", "😛"), (";P", "😜"), (":-O", "😮"), (":O", "😮"), (":o", "😮"), (":-*", "😘"),
    (":*", "😘"), ("8-)", "😎"), ("8)", "😎"), ("B)", "🤓"), (":-/", "😕"), (":/", "😕"), (":|", "😐"),
    (":-|", "😐"), (":S", "😖"), (">:(", "😠"), (">:)", "😈"), (":@", "🤬"), ("o:)", "😇"), ("O:)", "😇"),
    ("<3", "❤️"), ("</3", "💔"), ("(Y)", "👍"), ("(y)", "👍"), ("(N)", "👎"), ("\\o/", "🙌"), ("-_-", "😑"),
    ("o.O", "🤨"), ("O.o", "🤨"), ("zzZ", "😴"),
];

fn longest_first(t: &'static [(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
    let mut v = t.to_vec();
    v.sort_by_key(|(k, _)| std::cmp::Reverse(k.len()));
    v
}

/// Emoji as text faces, for classic clients. Emoji without a face are left as they are.
pub fn to_faces(text: &str) -> String {
    let table = longest_first(EMOJI);
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'outer: while let Some(c) = rest.chars().next() {
        if !c.is_ascii() {
            for (k, v) in &table {
                if let Some(after) = rest.strip_prefix(k) {
                    out.push_str(v);
                    // A stray variation selector after the emoji goes too.
                    rest = after.strip_prefix('\u{FE0F}').unwrap_or(after);
                    continue 'outer;
                }
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Text faces as emoji, for showing.
pub fn to_emoji(text: &str) -> String {
    let table = longest_first(FACES);
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let b = text.as_bytes();
    'outer: while i < text.len() {
        let starts_word = i == 0 || b[i - 1].is_ascii_whitespace();
        if starts_word {
            for (k, v) in &table {
                if text[i..].starts_with(k) {
                    let end = i + k.len();
                    let ends_word = end == text.len() || matches!(b[end], b' ' | b'\t' | b'\n' | b'\r' | b'.' | b',' | b'!' | b'?');
                    if ends_word {
                        out.push_str(v);
                        i = end;
                        continue 'outer;
                    }
                }
            }
        }
        let c = text[i..].chars().next().unwrap();
        out.push(c);
        i += c.len_utf8();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_become_faces() {
        assert_eq!(to_faces("hi 😀 👍"), "hi :D (Y)");
        assert_eq!(to_faces("love ❤️ you"), "love <3 you");
        assert_eq!(to_faces("café 🦄"), "café 🦄");
    }

    #[test]
    fn faces_become_emoji_only_on_their_own() {
        assert_eq!(to_emoji("hi :) and :D!"), "hi 🙂 and 😀!");
        assert_eq!(to_emoji("http://x.com a:b :Dx"), "http://x.com a:b :Dx");
        assert_eq!(to_emoji(">:( <3"), "😠 ❤️");
    }

    #[test]
    fn round_trip() {
        assert_eq!(to_emoji(&to_faces("ok 😉")), "ok 😉");
    }
}
