//! Sorting names and display names derived from paths.
//!
//! Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md: leading words (articles by
//! default, see the `sortPrefixes` setting) are removed, and standalone numbers are written as
//! zero-padded digits so "Game 10" sorts after "Game 9" and "Final Fantasy XII" after
//! "Final Fantasy 9". Roman numerals count only where they are clearly numbers ("I" and "X" need
//! to end the name or follow "Chapter"/"Episode"/"Season"; "Mega Man X Legacy" or "X-Men" stay),
//! and "One"/"Two"/"Three" only in those places too. A trailing edition ("GOTY Edition",
//! "Remastered", "Director's Cut", ...) is kept as written.

use std::path::Path;
use std::sync::{LazyLock, PoisonError, RwLock};

use regex::Regex;

/// Words ignored at the start of names until the settings say otherwise.
pub const DEFAULT_PREFIXES: [&str; 3] = ["The", "A", "An"];
const NUMBER_WIDTH: usize = 2;

/// Roman-looking words that are names, not numbers ("Bit Blaster XL", "Sonic Adventure DX").
const NOT_NUMERALS: [&str; 13] = [
    "XL", "XD", "DX", "XXX", "L", "C", "D", "M", "MII", "MIX", "MX", "MC", "DC",
];
/// Number words that become digits after "Chapter"/"Episode"/"Season" or at the end.
const NUMBER_WORDS: [(&str, u32); 3] = [("one", 1), ("two", 2), ("three", 3)];
/// Words before a number that make even "I", "X" and "One" count as one.
const COUNTING_WORDS: [&str; 3] = ["chapter", "season", "episode"];

/// A trailing edition, kept as written: "- GOTY Edition", " HD Remaster", " Director's Cut".
static EDITION: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(\s*[:-])?(\s+([a-z']+\s+(edition|cut)|hd|collection|remaster(ed)?|remake|ultimate|anthology|game of the))+$",
    )
    .ok()
});

/// The words ignored at the start of names, from the settings; `None` until they are set.
static PREFIXES: RwLock<Option<Vec<String>>> = RwLock::new(None);

/// Sets the words ignored at the start of names (the `sortPrefixes` setting). Returns whether
/// they changed, i.e. whether stored sorting names need recomputing.
pub fn set_ignored_prefixes(words: Vec<String>) -> bool {
    let mut current = PREFIXES.write().unwrap_or_else(PoisonError::into_inner);
    let changed = current
        .as_ref()
        .map_or_else(|| words != defaults(), |c| *c != words);
    *current = Some(words);
    changed
}

fn defaults() -> Vec<String> {
    DEFAULT_PREFIXES.iter().map(ToString::to_string).collect()
}

/// Returns the key used to sort `name`: an ignored leading word removed, numbers (also roman
/// ones) as digits padded to two places.
pub fn sorting_name(name: &str) -> String {
    let current = PREFIXES.read().unwrap_or_else(PoisonError::into_inner);
    match current.as_deref() {
        Some(words) => sorting_name_with(name, words),
        None => sorting_name_with(name, &defaults()),
    }
}

/// [`sorting_name`] with an explicit list of ignored leading words.
pub fn sorting_name_with(name: &str, prefixes: &[String]) -> String {
    let trimmed = name.trim();
    let (body, edition) = split_edition(trimmed);
    let numbered = write_numbers(body);
    format!("{}{edition}", strip_prefix(&numbered, prefixes))
}

/// Splits a trailing edition off a name: `("Borderlands", " Game of the Year Edition")`.
fn split_edition(name: &str) -> (&str, &str) {
    match EDITION.as_ref().and_then(|re| re.find(name)) {
        Some(m) => name.split_at(m.start()),
        None => (name, ""),
    }
}

fn strip_prefix<'a>(name: &'a str, prefixes: &[String]) -> &'a str {
    for article in prefixes.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        if name.len() > article.len()
            && name.is_char_boundary(article.len())
            && name[..article.len()].eq_ignore_ascii_case(article)
        {
            let rest = &name[article.len()..];
            if rest.starts_with(char::is_whitespace) {
                let rest = rest.trim_start();
                if !rest.is_empty() {
                    return rest;
                }
            }
        }
    }
    name
}

/// The unicode roman numeral characters (Ⅰ..ↈ); they are numbers, not letters, here.
fn is_unicode_roman(c: char) -> bool {
    ('\u{2160}'..='\u{2188}').contains(&c)
}

fn is_roman(c: char) -> bool {
    matches!(c, 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M') || is_unicode_roman(c)
}

/// A letter, digit or underscore (what regular expressions call a word character).
fn is_word(c: char) -> bool {
    (c.is_alphanumeric() || c == '_') && !is_unicode_roman(c)
}

fn roman_value(c: char) -> Option<u32> {
    Some(match c {
        'I' | 'Ⅰ' | 'ⅰ' => 1,
        'Ⅱ' | 'ⅱ' => 2,
        'Ⅲ' | 'ⅲ' => 3,
        'Ⅳ' | 'ⅳ' => 4,
        'V' | 'Ⅴ' | 'ⅴ' => 5,
        'Ⅵ' | 'ⅵ' | 'ↅ' => 6,
        'Ⅶ' | 'ⅶ' => 7,
        'Ⅷ' | 'ⅷ' => 8,
        'Ⅸ' | 'ⅸ' => 9,
        'X' | 'Ⅹ' | 'ⅹ' => 10,
        'Ⅺ' | 'ⅺ' => 11,
        'Ⅻ' | 'ⅻ' => 12,
        'L' | 'Ⅼ' | 'ⅼ' | 'ↆ' => 50,
        'C' | 'Ⅽ' | 'ⅽ' | 'Ↄ' | 'ↄ' => 100,
        'D' | 'Ⅾ' | 'ⅾ' => 500,
        'M' | 'Ⅿ' | 'ⅿ' | 'ↀ' => 1000,
        'ↁ' => 5000,
        'ↂ' => 10000,
        'ↇ' => 50000,
        'ↈ' => 100_000,
        _ => return None,
    })
}

/// The value of a well-formed roman numeral ("XIV" → 14); `None` for anything else ("IIII",
/// "IVX", "VX", "Gobliiins").
pub(crate) fn roman_to_int(text: &str) -> Option<u32> {
    if text.is_empty() {
        return None;
    }
    let mut total: i64 = 0;
    let mut largest_right = 0;
    let mut previous = 0;
    let mut repeats = 0;
    for c in text.chars().rev() {
        let value = roman_value(c)?;
        let subtract = value < largest_right;
        if subtract {
            // Only one smaller numeral may stand before a bigger one, and at most a fifth of it
            // (IV, IX, XL, XC, ...; not IIX, VX or IVX).
            if previous < largest_right || value * 5 > largest_right {
                return None;
            }
            total -= i64::from(value);
        } else {
            total += i64::from(value);
            largest_right = value;
        }
        if value == previous {
            // Only I, X, C, M (and the like) repeat, at most three times.
            if !is_one_or_power_of_ten(value) {
                return None;
            }
            repeats += 1;
            if repeats == 4 {
                return None;
            }
        } else {
            repeats = 1;
        }
        previous = value;
    }
    u32::try_from(total).ok()
}

fn is_one_or_power_of_ten(mut value: u32) -> bool {
    while value > 9 && value.is_multiple_of(10) {
        value /= 10;
    }
    value == 1
}

fn padded(value: u32) -> String {
    format!("{value:0NUMBER_WIDTH$}")
}

/// Whether the text right before `start` names a chapter, season or episode, or the word at
/// `start..end` is at most `max_after` characters from the end.
fn counts_as_number(chars: &[char], start: usize, end: usize, max_after: usize) -> bool {
    if chars.len() - end <= max_after {
        return true;
    }
    let before: String = chars[start.saturating_sub(9)..start]
        .iter()
        .collect::<String>()
        .to_lowercase();
    COUNTING_WORDS.iter().any(|w| before.contains(w))
}

/// "X-Men", "X-Wing": an X glued to a word that is not itself a numeral.
fn before_dash_and_word(chars: &[char], end: usize) -> bool {
    if chars.len() - end <= 1 || chars[end] != '-' {
        return false;
    }
    let mut word = String::new();
    for &c in chars[end + 1..].iter().take_while(|c| !c.is_whitespace()) {
        if !c.is_alphabetic() {
            return false;
        }
        word.push(c);
    }
    NOT_NUMERALS.contains(&word.as_str()) || roman_to_int(&word).is_none()
}

/// What a roman word becomes, if it is a number here.
fn roman_number(chars: &[char], start: usize, end: usize) -> Option<String> {
    let word: String = chars[start..end].iter().collect();
    match word.as_str() {
        "I" => counts_as_number(chars, start, end, 0).then(|| padded(1)),
        "X" => (counts_as_number(chars, start, end, 4) && !before_dash_and_word(chars, end))
            .then(|| padded(10)),
        w if NOT_NUMERALS.contains(&w) => None,
        w => roman_to_int(w).map(padded),
    }
}

/// The name with standalone numbers (also roman ones and "One"/"Two"/"Three" where they count)
/// as padded digits, the same rules as [`sorting_name`] except that a lone "X" stays: "Final
/// Fantasy VII" and "Final Fantasy 7" give the same text, "Mega Man X" and "Mega Man 10" do not.
/// Used to find the same game under slightly different names.
pub(crate) fn numbers_as_digits(name: &str) -> String {
    write_numbers_with(name, false)
}

/// Rewrites standalone numbers as padded digits. A number must start a word that is not the
/// first of the name and not glued to a dot ("XIII", "S.T.A.L.K.E.R.", "F1" stay).
fn write_numbers(name: &str) -> String {
    write_numbers_with(name, true)
}

fn write_numbers_with(name: &str, lone_x: bool) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let word_start = prev.is_none_or(|p| !is_word(p));
        if word_start && i > 0 && prev != Some('.') {
            if is_roman(c) {
                let end = i + chars[i..].iter().take_while(|&&c| is_roman(c)).count();
                let next = chars.get(end).copied();
                let skip = !lone_x && end - i == 1 && c == 'X';
                if !skip && next.is_none_or(|n| !is_word(n) && n != '.') {
                    match roman_number(&chars, i, end) {
                        Some(number) => out.push_str(&number),
                        None => out.extend(&chars[i..end]),
                    }
                    i = end;
                    continue;
                }
            } else if c.is_ascii_digit() {
                let end = i + chars[i..].iter().take_while(|c| c.is_ascii_digit()).count();
                if chars.get(end).is_none_or(|&n| !is_word(n)) {
                    let digits: String = chars[i..end].iter().collect();
                    out.push_str(&format!("{digits:0>NUMBER_WIDTH$}"));
                    i = end;
                    continue;
                }
            }
        }
        if word_start && let Some((len, value)) = number_word(&chars, i) {
            if counts_as_number(&chars, i, i + len, 0) {
                out.push_str(&padded(value));
            } else {
                out.extend(&chars[i..i + len]);
            }
            i += len;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// "One", "two", "THREE" as a whole word at `i`: its length and value.
fn number_word(chars: &[char], i: usize) -> Option<(usize, u32)> {
    NUMBER_WORDS.iter().find_map(|&(word, value)| {
        let len = word.chars().count();
        let candidate = chars.get(i..i + len)?;
        let matches = candidate
            .iter()
            .zip(word.chars())
            .all(|(c, w)| c.to_lowercase().eq(std::iter::once(w)));
        let ends = chars.get(i + len).is_none_or(|&n| !is_word(n));
        (matches && ends).then_some((len, value))
    })
}

/// Derives a readable game name from an executable path: the file stem, unless it looks generic
/// (`game.exe`, `launcher.exe`, ...), in which case the parent folder name is used.
pub fn display_name_from_path(path: &str) -> String {
    const GENERIC: [&str; 9] = [
        "game", "launcher", "start", "play", "run", "main", "app", "client", "bin",
    ];
    // Split on both separators so Windows paths are handled on every OS.
    let segments: Vec<&str> = path.split(['/', '\\']).filter(|s| !s.is_empty()).collect();
    let file_name = segments.last().copied().unwrap_or_default();
    let stem = Path::new(file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem_lower = stem.to_lowercase();
    let is_generic = stem.is_empty()
        || GENERIC
            .iter()
            .any(|g| stem_lower == *g || stem_lower.starts_with(&format!("{g}-")))
        || stem_lower.ends_with("-win64-shipping")
        || stem_lower.ends_with("_x64");
    let raw = if is_generic {
        segments
            .iter()
            .rev()
            .skip(1)
            .find(|n| {
                !n.ends_with(':')
                    && !matches!(
                        n.to_lowercase().as_str(),
                        "bin" | "binaries" | "win64" | "win32" | "x64"
                    )
            })
            .map(|n| (*n).to_string())
            .unwrap_or(stem)
    } else {
        stem
    };
    prettify(&raw)
}

fn prettify(raw: &str) -> String {
    let replaced: String = raw
        .chars()
        .map(|c| if c == '_' { ' ' } else { c })
        .collect();
    replaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    fn sorted(name: &str) -> String {
        sorting_name_with(name, &defaults())
    }

    #[test]
    fn ignores_the_configured_leading_words() {
        assert_eq!(
            sorting_name_with("Der Planer 4", &words(&["Der"])),
            "Planer 04"
        );
        assert_eq!(
            sorting_name_with("The Witcher", &words(&["Der"])),
            "The Witcher"
        );
        assert_eq!(sorting_name_with("The Witcher", &[]), "The Witcher");
        assert_eq!(sorting_name_with("The Witcher 3", &[]), "The Witcher 03");
        assert_eq!(
            sorting_name_with("the witcher", &words(&[" The "])),
            "witcher"
        );
        // A name that is only the word keeps it.
        assert_eq!(sorting_name_with("The", &words(&["The"])), "The");
    }

    #[test]
    fn strips_leading_articles() {
        assert_eq!(sorting_name("The Witcher 3"), "Witcher 03");
        assert_eq!(sorted("the Witcher 3"), "Witcher 03");
        assert_eq!(sorted("A Plague Tale"), "Plague Tale");
        assert_eq!(sorted("An Untitled Story"), "Untitled Story");
        assert_eq!(sorted("Theatrhythm"), "Theatrhythm");
        assert_eq!(sorted("AnUsual Game"), "AnUsual Game");
        assert_eq!(sorted("The"), "The");
    }

    #[test]
    fn pads_standalone_numbers() {
        assert_eq!(sorted("Portal 2"), "Portal 02");
        assert_eq!(sorted("Cyberpunk 2077"), "Cyberpunk 2077");
        assert_eq!(sorted("Half-Life 2: Episode 1"), "Half-Life 02: Episode 01");
        assert_eq!(sorted("Left 4 Dead"), "Left 04 Dead");
        assert_eq!(sorted("F1 2024"), "F1 2024");
        assert_eq!(sorted("Battlefield 1942"), "Battlefield 1942");
        assert_eq!(
            sorted("Crazy Machines 1.5 New from the Lab"),
            "Crazy Machines 01.5 New from the Lab"
        );
        assert_eq!(
            sorted("Carmageddon 2: Carpocalypse Now"),
            "Carmageddon 02: Carpocalypse Now"
        );
    }

    #[test]
    fn reads_roman_numerals() {
        let cases = [
            ("Final Fantasy XIII-2", "Final Fantasy 13-02"),
            ("Final Fantasy \u{216B}", "Final Fantasy 12"),
            (
                "FINAL FANTASY X/X-2 HD Remaster",
                "FINAL FANTASY 10/10-02 HD Remaster",
            ),
            ("Warhammer \u{2182}\u{2187}", "Warhammer 40000"),
            ("SOULCALIBUR IV", "SOULCALIBUR 04"),
            ("Quake III: Team Arena", "Quake 03: Team Arena"),
            ("Battlefield V", "Battlefield 05"),
            ("Metal Slug XX", "Metal Slug 20"),
            ("Test X-", "Test 10-"),
            ("Rocky II", "Rocky 02"),
        ];
        for (name, expected) in cases {
            assert_eq!(sorted(name), expected, "{name}");
        }
    }

    #[test]
    fn counting_words_make_i_x_and_number_words_count() {
        assert_eq!(
            sorted("Tales of Monkey Island: Chapter I - Launch of the Screaming Narwhal"),
            "Tales of Monkey Island: Chapter 01 - Launch of the Screaming Narwhal"
        );
        assert_eq!(sorted("KOBOLD: Chapter I"), "KOBOLD: Chapter 01");
        assert_eq!(
            sorted("Half-Life 2: Episode One"),
            "Half-Life 02: Episode 01"
        );
        assert_eq!(sorted("Unravel Two"), "Unravel 02");
    }

    #[test]
    fn keeps_trailing_editions_as_written() {
        assert_eq!(
            sorted("THE KING OF FIGHTERS XIV STEAM EDITION"),
            "KING OF FIGHTERS 14 STEAM EDITION"
        );
        assert_eq!(
            sorted("The Elder Scrolls II: Daggerfall Unity - GOG Cut"),
            "Elder Scrolls 02: Daggerfall Unity - GOG Cut"
        );
        assert_eq!(
            sorted("Borderlands 2 Game of the Year Edition"),
            "Borderlands 02 Game of the Year Edition"
        );
    }

    #[test]
    fn leaves_words_that_only_look_like_numbers() {
        let names = [
            "SHENZHEN I/O",
            "XIII",
            "X: Beyond the Frontier",
            "X3: Terran Conflict",
            "X-COM",
            "Gobliiins",
            "Before I Forget",
            "A.I.M. Racing",
            "S.T.A.L.K.E.R.: Shadow of Chernobyl",
            "Metal Wolf Chaos XD",
            "Prince of Persia: The Two Thrones",
            "Daemon X Machina",
            "Bit Blaster XL",
            "STAR WARS X-Wing vs TIE Fighter: Balance of Power Campaigns",
            "Star Wars: X-Wing Alliance",
            "Acceleration of Suguri X-Edition",
            "Guilty Gear X2 #Reload",
            "Mega Man X Legacy Collection",
            "LEGO DC Super-Villains",
            "Constant C",
            "Metroid: Other M",
            "Worms W M D",
            "Sonic Adventure DX",
            "Zone of The Enders: The 2nd Runner M\u{2200}RS",
            "",
        ];
        for name in names {
            assert_eq!(sorted(name), name, "{name}");
        }
        assert_eq!(sorted("The Uncanny X-Men"), "Uncanny X-Men");
        assert_eq!(sorted("  "), "");
    }

    #[test]
    fn validates_roman_numerals() {
        let valid = [
            ("I", 1),
            ("IV", 4),
            ("VIII", 8),
            ("IX", 9),
            ("XIX", 19),
            ("CCLXXXI", 281),
            ("MCMLVIII", 1958),
            ("MCMXCIX", 1999),
        ];
        for (text, value) in valid {
            assert_eq!(roman_to_int(text), Some(value), "{text}");
        }
        for text in ["IVX", "VIX", "IIII", "XXL", "IIX", "VX", "asdf", ""] {
            assert_eq!(roman_to_int(text), None, "{text}");
        }
    }

    #[test]
    fn padded_names_sort_naturally() {
        let mut names = vec!["Game 10", "Game 9", "Game 1", "Game XI"];
        names.sort_by_key(|n| sorted(n));
        assert_eq!(names, vec!["Game 1", "Game 9", "Game 10", "Game XI"]);
    }

    #[test]
    fn display_name_uses_file_stem() {
        assert_eq!(
            display_name_from_path(r"C:\Games\Hollow Knight\hollow_knight.exe"),
            "hollow knight"
        );
        assert_eq!(
            display_name_from_path("/opt/games/Celeste/Celeste"),
            "Celeste"
        );
    }

    #[test]
    fn display_name_falls_back_to_folder_for_generic_exes() {
        assert_eq!(
            display_name_from_path(r"C:\Games\Outer Wilds\game.exe"),
            "Outer Wilds"
        );
        assert_eq!(
            display_name_from_path(r"D:\Stuff\Deep Rock\bin\launcher.exe"),
            "Deep Rock"
        );
    }
}
