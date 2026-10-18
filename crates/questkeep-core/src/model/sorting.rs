//! Sorting names and display names derived from paths.
//!
//! Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md: leading articles are removed
//! and standalone numbers are zero-padded so "Game 10" sorts after "Game 9".

use std::path::Path;

const ARTICLES: [&str; 3] = ["the", "a", "an"];
const NUMBER_WIDTH: usize = 2;

/// Returns the key used to sort `name`: leading article removed, numbers padded to two digits.
pub fn sorting_name(name: &str) -> String {
    let trimmed = name.trim();
    let without_article = strip_article(trimmed);
    pad_numbers(without_article)
}

fn strip_article(name: &str) -> &str {
    for article in ARTICLES {
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

/// Pads runs of ASCII digits that are not glued to letters or a decimal point (`2077` stays,
/// `4` becomes `04`, `v1.2` stays).
fn pad_numbers(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let before = start.checked_sub(1).map(|j| chars[j]);
            let after = chars.get(i).copied();
            let glued_before = before.is_some_and(|b| b.is_alphanumeric() || b == '.');
            let glued_after = after.is_some_and(|a| a.is_alphanumeric() || a == '.');
            let digits: String = chars[start..i].iter().collect();
            if !glued_before && !glued_after && digits.len() < NUMBER_WIDTH {
                out.extend(std::iter::repeat_n('0', NUMBER_WIDTH - digits.len()));
            }
            out.push_str(&digits);
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
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

    #[test]
    fn strips_leading_articles() {
        assert_eq!(sorting_name("The Witcher 3"), "Witcher 03");
        assert_eq!(sorting_name("A Plague Tale"), "Plague Tale");
        assert_eq!(sorting_name("An Untitled Story"), "Untitled Story");
        assert_eq!(sorting_name("Theatrhythm"), "Theatrhythm");
        assert_eq!(sorting_name("The"), "The");
    }

    #[test]
    fn pads_standalone_numbers() {
        assert_eq!(sorting_name("Portal 2"), "Portal 02");
        assert_eq!(sorting_name("Cyberpunk 2077"), "Cyberpunk 2077");
        assert_eq!(
            sorting_name("Half-Life 2: Episode 1"),
            "Half-Life 02: Episode 01"
        );
        assert_eq!(sorting_name("Left 4 Dead"), "Left 04 Dead");
        assert_eq!(sorting_name("F1 2024"), "F1 2024");
        assert_eq!(sorting_name("Version 1.5"), "Version 1.5");
    }

    #[test]
    fn padded_names_sort_naturally() {
        let mut names = vec!["Game 10", "Game 9", "Game 1"];
        names.sort_by_key(|n| sorting_name(n));
        assert_eq!(names, vec!["Game 1", "Game 9", "Game 10"]);
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
