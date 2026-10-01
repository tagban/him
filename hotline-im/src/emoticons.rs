//! Emoji and the old text faces, both ways (the same tables as the Discord bridge).
//!
//! Hotline clients (HIM's classic app among them) can't show modern emoji, so HIM sends
//! plain text: emoji with a classic face become the face (😀 → `:D`), and the rest become
//! their Unicode name between colons (🦄 → `:unicorn_face:`). A modern client turns both
//! back into emoji when it shows them (`to_emoji`), so nothing is lost between two of them.

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

/// Characters that are emoji (not the accented letters and symbols Hotline has always had).
fn is_emoji(c: char) -> bool {
    matches!(c as u32, 0x1F000..=0x1FAFF | 0x2300..=0x23FF | 0x2600..=0x27BF | 0x2B00..=0x2BFF | 0x3030 | 0x303D | 0x3297 | 0x3299)
}

/// Joiners and modifiers that belong to the emoji before them.
fn is_emoji_tail(c: char) -> bool {
    matches!(c as u32, 0xFE0E | 0xFE0F | 0x200D | 0x1F3FB..=0x1F3FF | 0x20E3 | 0xE0020..=0xE007F)
}

fn regional(c: char) -> Option<char> {
    let v = c as u32;
    (0x1F1E6..=0x1F1FF).contains(&v).then(|| char::from(b'a' + (v - 0x1F1E6) as u8))
}

/// `UNICORN FACE` → `:unicorn_face:`
fn short_name(c: char) -> Option<String> {
    let name = unicode_names2::name(c)?.to_string();
    Some(format!(":{}:", name.to_lowercase().replace([' ', '-'], "_")))
}

/// Emoji as plain text for Hotline clients: a classic face where there is one, else the
/// emoji's name between colons. Skin tones and joined sequences go with their first emoji.
pub fn to_faces(text: &str) -> String {
    let table = longest_first(EMOJI);
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'outer: while let Some(c) = rest.chars().next() {
        if !c.is_ascii() {
            for (k, v) in &table {
                if let Some(after) = rest.strip_prefix(k) {
                    out.push_str(v);
                    rest = skip_tail(after);
                    continue 'outer;
                }
            }
            // A flag: two regional indicator letters.
            if let Some(a) = regional(c) {
                let after = &rest[c.len_utf8()..];
                if let Some(b) = after.chars().next().and_then(regional) {
                    out.push_str(&format!(":flag_{a}{b}:"));
                    rest = &after[4..];
                    continue;
                }
            }
            if is_emoji(c) {
                if let Some(n) = short_name(c) {
                    out.push_str(&n);
                    rest = skip_tail(&rest[c.len_utf8()..]);
                    continue;
                }
            }
            if is_emoji_tail(c) {
                rest = &rest[c.len_utf8()..];
                continue;
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Past the modifiers, and anything joined on with a zero-width joiner.
fn skip_tail(mut rest: &str) -> &str {
    loop {
        match rest.chars().next() {
            Some('\u{200D}') => {
                rest = &rest[3..];
                if let Some(n) = rest.chars().next() {
                    rest = &rest[n.len_utf8()..];
                }
            }
            Some(c) if is_emoji_tail(c) => rest = &rest[c.len_utf8()..],
            _ => return rest,
        }
    }
}

/// Text faces and `:emoji_names:` as emoji, for showing.
pub fn to_emoji(text: &str) -> String {
    names_to_emoji(&faces_to_emoji(text))
}

/// `:unicorn_face:` → 🦄, `:flag_us:` → 🇺🇸; anything else between colons stays as it is.
fn names_to_emoji(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(':') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let end = after.find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
        if let Some(j) = end.filter(|j| *j > 1 && after[*j..].starts_with(':')) {
            let word = &after[..j];
            let found = if let Some(cc) = word.strip_prefix("flag_").filter(|cc| cc.len() == 2) {
                let mut f = String::new();
                for b in cc.bytes() {
                    f.push(char::from_u32(0x1F1E6 + (b - b'a') as u32).unwrap_or('?'));
                }
                Some(f)
            } else {
                unicode_names2::character(&word.replace('_', " ").to_uppercase())
                    .filter(|c| is_emoji(*c))
                    .map(String::from)
            };
            if let Some(e) = found {
                out.push_str(&e);
                rest = &after[j + 1..];
                continue;
            }
        }
        out.push(':');
        rest = after;
    }
    out.push_str(rest);
    out
}

fn faces_to_emoji(text: &str) -> String {
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
        assert_eq!(to_faces("café 🦄"), "café :unicorn_face:");
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
        assert_eq!(to_emoji(&to_faces("a 🦄 b 🇺🇸")), "a 🦄 b 🇺🇸");
    }

    #[test]
    fn every_emoji_becomes_plain_text() {
        assert_eq!(to_faces("ride a 🦄!"), "ride a :unicorn_face:!");
        assert_eq!(to_faces("go 🇺🇸"), "go :flag_us:");
        assert_eq!(to_faces("👋🏽 hi"), ":waving_hand_sign: hi"); // the skin tone goes with it
        assert_eq!(to_faces("👨‍👩‍👧 fam"), ":man: fam"); // a joined family: its first emoji
        assert!(to_faces("🍕🎉✨🚀").is_ascii());
        // Letters, accents and old symbols are left alone.
        assert_eq!(to_faces("café © ™"), "café © ™");
        // Colons that aren't emoji names stay put.
        assert_eq!(to_emoji("time: 10:30 :notathing: :d"), "time: 10:30 :notathing: :d");
    }
}
