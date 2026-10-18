//! Parser for Valve's text KeyValues format (`libraryfolders.vdf`, `appmanifest_*.acf`, ...).
//!
//! The format is a sequence of `"key" "value"` pairs and `"key" { ... }` objects. Quotes are
//! optional for simple tokens, `//` starts a line comment, and values may be followed by a
//! platform conditional such as `[$WIN32]`, which is skipped.

// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md

/// A parsed value: either a string or an object with ordered, possibly duplicated keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Vdf {
    Str(String),
    Obj(Vec<(String, Vdf)>),
}

/// A parse failure with the 1-based line it was detected on.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("VDF parse error on line {line}: {message}")]
pub struct VdfError {
    pub line: usize,
    pub message: String,
}

impl Vdf {
    /// First child with the given key (ASCII case-insensitive). `None` for strings.
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        self.entries()
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    }

    /// First child with the given key, if it is a string.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key)?.as_str()
    }

    /// The string value, if this is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Vdf::Str(s) => Some(s),
            Vdf::Obj(_) => None,
        }
    }

    /// Ordered children; empty for strings.
    pub fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Obj(entries) => entries,
            Vdf::Str(_) => &[],
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Str(String),
    Open,
    Close,
}

struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    line: usize,
}

impl<'a> Lexer<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            chars: text.trim_start_matches('\u{feff}').chars().peekable(),
            line: 1,
        }
    }

    fn err(&self, message: &str) -> VdfError {
        VdfError {
            line: self.line,
            message: message.to_string(),
        }
    }

    /// Next token with the line it started on, or `None` at end of input.
    fn next_token(&mut self) -> Result<Option<(Token, usize)>, VdfError> {
        loop {
            let Some(&c) = self.chars.peek() else {
                return Ok(None);
            };
            match c {
                '\n' => {
                    self.line += 1;
                    self.chars.next();
                }
                c if c.is_whitespace() => {
                    self.chars.next();
                }
                '/' => {
                    self.chars.next();
                    if self.chars.peek() == Some(&'/') {
                        while let Some(&c) = self.chars.peek() {
                            if c == '\n' {
                                break;
                            }
                            self.chars.next();
                        }
                    } else {
                        // A lone slash is an ordinary unquoted character.
                        let line = self.line;
                        let mut s = String::from('/');
                        self.read_bare(&mut s);
                        return Ok(Some((Token::Str(s), line)));
                    }
                }
                '{' => {
                    self.chars.next();
                    return Ok(Some((Token::Open, self.line)));
                }
                '}' => {
                    self.chars.next();
                    return Ok(Some((Token::Close, self.line)));
                }
                '"' => {
                    let line = self.line;
                    self.chars.next();
                    let s = self.read_quoted()?;
                    return Ok(Some((Token::Str(s), line)));
                }
                _ => {
                    let line = self.line;
                    let mut s = String::new();
                    self.read_bare(&mut s);
                    // `[$WIN32]`-style conditionals carry no data for us.
                    if s.starts_with('[') && s.ends_with(']') {
                        continue;
                    }
                    return Ok(Some((Token::Str(s), line)));
                }
            }
        }
    }

    fn read_bare(&mut self, out: &mut String) {
        while let Some(&c) = self.chars.peek() {
            if c.is_whitespace() || matches!(c, '{' | '}' | '"') {
                break;
            }
            out.push(c);
            self.chars.next();
        }
    }

    fn read_quoted(&mut self) -> Result<String, VdfError> {
        let mut out = String::new();
        loop {
            let Some(c) = self.chars.next() else {
                return Err(self.err("unterminated string"));
            };
            match c {
                '"' => return Ok(out),
                '\n' => {
                    self.line += 1;
                    out.push('\n');
                }
                '\\' => match self.chars.peek().copied() {
                    Some('\\') => {
                        out.push('\\');
                        self.chars.next();
                    }
                    Some('"') => {
                        out.push('"');
                        self.chars.next();
                    }
                    Some('n') => {
                        out.push('\n');
                        self.chars.next();
                    }
                    Some('t') => {
                        out.push('\t');
                        self.chars.next();
                    }
                    // Unknown escape (e.g. a single-backslash Windows path): keep it literally.
                    _ => out.push('\\'),
                },
                c => out.push(c),
            }
        }
    }
}

/// Parses a document consisting of a single root `"key" value` pair and returns both.
pub fn parse(text: &str) -> Result<(String, Vdf), VdfError> {
    let mut lexer = Lexer::new(text);
    let key = match lexer.next_token()? {
        Some((Token::Str(k), _)) => k,
        Some((_, line)) => {
            return Err(VdfError {
                line,
                message: "expected a key".into(),
            });
        }
        None => return Err(lexer.err("empty document")),
    };
    let value = match lexer.next_token()? {
        Some((Token::Str(s), _)) => Vdf::Str(s),
        Some((Token::Open, _)) => parse_object(&mut lexer)?,
        Some((Token::Close, line)) => {
            return Err(VdfError {
                line,
                message: "unexpected '}'".into(),
            });
        }
        None => return Err(lexer.err("expected a value after the root key")),
    };
    // Anything after the root pair other than whitespace/comments is malformed.
    if let Some((_, line)) = lexer.next_token()? {
        return Err(VdfError {
            line,
            message: "unexpected content after the root object".into(),
        });
    }
    Ok((key, value))
}

/// Parses object members up to the matching `}` (the `{` is already consumed).
fn parse_object(lexer: &mut Lexer<'_>) -> Result<Vdf, VdfError> {
    let mut entries = Vec::new();
    loop {
        let key = match lexer.next_token()? {
            Some((Token::Str(k), _)) => k,
            Some((Token::Close, _)) => return Ok(Vdf::Obj(entries)),
            Some((Token::Open, line)) => {
                return Err(VdfError {
                    line,
                    message: "unexpected '{', expected a key".into(),
                });
            }
            None => return Err(lexer.err("unterminated object, missing '}'")),
        };
        let value = match lexer.next_token()? {
            Some((Token::Str(s), _)) => Vdf::Str(s),
            Some((Token::Open, _)) => parse_object(lexer)?,
            Some((Token::Close, line)) => {
                return Err(VdfError {
                    line,
                    message: format!("key \"{key}\" has no value"),
                });
            }
            None => return Err(lexer.err("unterminated object, missing '}'")),
        };
        entries.push((key, value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flat_and_nested_objects() {
        let (key, v) =
            parse(r#""AppState" { "appid" "620" "UserConfig" { "name" "Portal 2" } }"#).unwrap();
        assert_eq!(key, "AppState");
        assert_eq!(v.get_str("appid"), Some("620"));
        assert_eq!(
            v.get("UserConfig").and_then(|u| u.get_str("name")),
            Some("Portal 2")
        );
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let (_, v) = parse(r#""R" { "AppID" "1" }"#).unwrap();
        assert_eq!(v.get_str("appid"), Some("1"));
        assert_eq!(v.get_str("APPID"), Some("1"));
        assert_eq!(v.get_str("missing"), None);
    }

    #[test]
    fn get_str_ignores_objects() {
        let (_, v) = parse(r#""R" { "a" { } }"#).unwrap();
        assert_eq!(v.get_str("a"), None);
        assert!(v.get("a").is_some());
    }

    #[test]
    fn decodes_known_escapes() {
        let (_, v) =
            parse(r#""R" { "p" "D:\\Games\\Steam" "q" "say \"hi\"" "n" "a\nb\tc" }"#).unwrap();
        assert_eq!(v.get_str("p"), Some(r"D:\Games\Steam"));
        assert_eq!(v.get_str("q"), Some("say \"hi\""));
        assert_eq!(v.get_str("n"), Some("a\nb\tc"));
    }

    #[test]
    fn unknown_escapes_keep_the_backslash() {
        let (_, v) = parse(r#""R" { "p" "D:\SteamLibrary\x" }"#).unwrap();
        assert_eq!(v.get_str("p"), Some(r"D:\SteamLibrary\x"));
    }

    #[test]
    fn skips_line_comments() {
        let text = "// header\n\"R\" // trailing\n{\n // inside\n \"a\" \"1\" // after\n}\n// end";
        let (_, v) = parse(text).unwrap();
        assert_eq!(v.get_str("a"), Some("1"));
    }

    #[test]
    fn skips_conditionals() {
        let text = "\"R\"\n{\n \"a\" \"1\" [$WIN32]\n \"b\" \"2\" [!$X360]\n \"c\" { \"d\" \"3\" } [$LINUX]\n}";
        let (_, v) = parse(text).unwrap();
        assert_eq!(v.get_str("a"), Some("1"));
        assert_eq!(v.get_str("b"), Some("2"));
        assert_eq!(v.get("c").and_then(|c| c.get_str("d")), Some("3"));
        assert_eq!(v.entries().len(), 3);
    }

    #[test]
    fn supports_unquoted_tokens() {
        let (key, v) = parse("R { a 1 b {} path D:/x/y }").unwrap();
        assert_eq!(key, "R");
        assert_eq!(v.get_str("a"), Some("1"));
        assert_eq!(v.get_str("path"), Some("D:/x/y"));
    }

    #[test]
    fn empty_objects() {
        let (_, v) = parse(r#""R" { "apps" { } "x" "y" }"#).unwrap();
        assert_eq!(v.get("apps"), Some(&Vdf::Obj(vec![])));
        let (_, empty) = parse(r#""R" {}"#).unwrap();
        assert!(empty.entries().is_empty());
    }

    #[test]
    fn keeps_duplicate_keys_in_order() {
        let (_, v) = parse(r#""R" { "k" "1" "k" "2" "j" "3" "k" "4" }"#).unwrap();
        let pairs: Vec<_> = v
            .entries()
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str().unwrap_or("")))
            .collect();
        assert_eq!(pairs, [("k", "1"), ("k", "2"), ("j", "3"), ("k", "4")]);
        assert_eq!(v.get_str("k"), Some("1"));
    }

    #[test]
    fn handles_bom_and_crlf() {
        let (key, v) = parse("\u{feff}\"R\"\r\n{\r\n\t\"a\"\t\t\"1\"\r\n}\r\n").unwrap();
        assert_eq!(key, "R");
        assert_eq!(v.get_str("a"), Some("1"));
    }

    #[test]
    fn unterminated_string_reports_line() {
        let err = parse("\"R\"\n{\n \"a\" \"oops\n}").unwrap_err();
        assert!(err.message.contains("unterminated string"));
        assert_eq!(err.line, 4);
        assert!(err.to_string().contains("line 4"));
    }

    #[test]
    fn unterminated_object_is_an_error() {
        let err = parse("\"R\"\n{\n \"a\" \"1\"\n").unwrap_err();
        assert!(err.message.contains("missing '}'"));
    }

    #[test]
    fn stray_tokens_are_errors() {
        assert!(parse("").is_err());
        assert!(parse("\"R\"").is_err());
        assert!(parse("\"R\" { \"a\" }").is_err());
        assert!(parse("\"R\" { } }").is_err());
        assert!(parse("\"R\" { { } }").is_err());
        assert!(parse("{ }").is_err());
    }

    #[test]
    fn root_may_be_a_plain_string() {
        let (k, v) = parse(r#""a" "b""#).unwrap();
        assert_eq!((k.as_str(), v.as_str()), ("a", Some("b")));
    }
}
