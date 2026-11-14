//! The `qk_text(text, needle, op, case_sensitive)` SQL function behind text filters. SQLite's own
//! `LIKE` and `lower()` only fold ASCII letters, and it has no regular expressions.

use regex::{Regex, RegexBuilder};
use rusqlite::Connection;
use rusqlite::functions::{Context, FunctionFlags};

use crate::model::TextOp;

/// Name of the SQL function.
pub(super) const FUNCTION: &str = "qk_text";

/// Upper bound for a compiled pattern, so a huge repetition cannot eat memory.
const REGEX_SIZE_LIMIT: usize = 1 << 20;

/// The `op` argument for an operation.
pub(super) fn op_code(op: TextOp) -> i64 {
    match op {
        TextOp::Contains => 0,
        TextOp::StartsWith => 1,
        TextOp::EndsWith => 2,
        TextOp::Equals => 3,
        TextOp::Matches => 4,
    }
}

/// Compiles a filter's pattern; `None` when it is not a valid regular expression.
pub fn compile(pattern: &str, case_sensitive: bool) -> Option<Regex> {
    RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .size_limit(REGEX_SIZE_LIMIT)
        .build()
        .ok()
}

/// Whether `text` passes a text filter (the same rules as the SQL function).
pub fn text_matches(text: &str, needle: &str, op: TextOp, case_sensitive: bool) -> bool {
    if op == TextOp::Matches {
        return compile(needle, case_sensitive).is_some_and(|re| re.is_match(text));
    }
    if case_sensitive {
        compare(text, needle, op)
    } else {
        compare(&fold(text), &fold(needle), op)
    }
}

fn fold(text: &str) -> String {
    text.to_lowercase()
}

fn compare(text: &str, needle: &str, op: TextOp) -> bool {
    match op {
        TextOp::Contains => text.contains(needle),
        TextOp::StartsWith => text.starts_with(needle),
        TextOp::EndsWith => text.ends_with(needle),
        TextOp::Equals | TextOp::Matches => text == needle,
    }
}

/// Registers the function on a connection.
pub(super) fn register(conn: &Connection) -> rusqlite::Result<()> {
    conn.create_scalar_function(
        FUNCTION,
        4,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        evaluate,
    )
}

fn evaluate(ctx: &Context<'_>) -> rusqlite::Result<bool> {
    let Some(text) = ctx.get::<Option<String>>(0)? else {
        return Ok(false);
    };
    let needle: String = ctx.get(1)?;
    let case_sensitive: bool = ctx.get(3)?;
    match ctx.get::<i64>(2)? {
        4 => {
            // Compiled once per statement: the pattern is the same for every row.
            let re = ctx.get_or_create_aux(1, |_| -> rusqlite::Result<Option<Regex>> {
                Ok(compile(&needle, case_sensitive))
            })?;
            Ok(re.as_ref().as_ref().is_some_and(|re| re.is_match(&text)))
        }
        code => {
            let op = match code {
                1 => TextOp::StartsWith,
                2 => TextOp::EndsWith,
                3 => TextOp::Equals,
                _ => TextOp::Contains,
            };
            Ok(text_matches(&text, &needle, op, case_sensitive))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_without_case_by_default() {
        assert!(text_matches(
            "Brütal Legend",
            "BRÜTAL",
            TextOp::StartsWith,
            false
        ));
        assert!(!text_matches(
            "Brütal Legend",
            "BRÜTAL",
            TextOp::StartsWith,
            true
        ));
        assert!(text_matches(
            "Brütal Legend",
            "legend",
            TextOp::EndsWith,
            false
        ));
        assert!(text_matches(
            "Brütal Legend",
            "l le",
            TextOp::Contains,
            false
        ));
        assert!(text_matches("Balatro", "balatro", TextOp::Equals, false));
        assert!(!text_matches("Balatro 2", "balatro", TextOp::Equals, false));
    }

    #[test]
    fn matches_regular_expressions() {
        assert!(text_matches(
            "Trine 4",
            r"^trine \d$",
            TextOp::Matches,
            false
        ));
        assert!(!text_matches(
            "Trine 4",
            r"^trine \d$",
            TextOp::Matches,
            true
        ));
        // Not a valid pattern: matches nothing instead of failing the query.
        assert!(!text_matches("Trine 4", "trine (", TextOp::Matches, false));
    }

    #[test]
    fn runs_in_sql() {
        let conn = Connection::open_in_memory().unwrap();
        register(&conn).unwrap();
        let hit = |text: Option<&str>, needle: &str, op: TextOp| -> bool {
            conn.query_row(
                "SELECT qk_text(?1, ?2, ?3, 0)",
                rusqlite::params![text, needle, op_code(op)],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert!(hit(Some("Ölfass"), "ÖL", TextOp::StartsWith));
        assert!(hit(Some("Ape Out"), "^ape", TextOp::Matches));
        assert!(!hit(None, "", TextOp::Contains));
    }
}
