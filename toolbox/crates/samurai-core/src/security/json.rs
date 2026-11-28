//! A small JSON reader for tool output such as the LUKS2 metadata (the workspace has no crates).

/// Nesting deeper than this is refused instead of risking the stack.
const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Json {
    Null,
    Bool(bool),
    /// Kept as written: nothing here does arithmetic.
    Number(String),
    Text(String),
    List(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        self.entries()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Members of an object; empty for anything else.
    pub fn entries(&self) -> &[(String, Json)] {
        match self {
            Json::Object(members) => members,
            _ => &[],
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Json::Text(s) => Some(s),
            _ => None,
        }
    }
}

struct Reader<'a> {
    rest: std::str::Chars<'a>,
}

impl Reader<'_> {
    fn peek(&mut self) -> Option<char> {
        self.rest = self.rest.as_str().trim_start().chars();
        self.rest.clone().next()
    }

    fn eat(&mut self, c: char) -> bool {
        let found = self.peek() == Some(c);
        if found {
            self.rest.next();
        }
        found
    }

    fn string(&mut self) -> Option<String> {
        if !self.eat('"') {
            return None;
        }
        let mut out = String::new();
        loop {
            match self.rest.next()? {
                '"' => return Some(out),
                '\\' => out.push(match self.rest.next()? {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    'b' => '\u{8}',
                    'f' => '\u{c}',
                    'u' => {
                        let hex: String = self.rest.by_ref().take(4).collect();
                        // surrogate halves have no meaning here
                        char::from_u32(u32::from_str_radix(&hex, 16).ok()?).unwrap_or('\u{fffd}')
                    }
                    other @ ('"' | '\\' | '/') => other,
                    _ => return None,
                }),
                c => out.push(c),
            }
        }
    }

    fn value(&mut self, depth: usize) -> Option<Json> {
        if depth > MAX_DEPTH {
            return None;
        }
        match self.peek()? {
            '"' => self.string().map(Json::Text),
            '{' => {
                self.eat('{');
                let mut members = Vec::new();
                while !self.eat('}') {
                    if !members.is_empty() && !self.eat(',') {
                        return None;
                    }
                    let key = self.string()?;
                    if !self.eat(':') {
                        return None;
                    }
                    members.push((key, self.value(depth + 1)?));
                }
                Some(Json::Object(members))
            }
            '[' => {
                self.eat('[');
                let mut items = Vec::new();
                while !self.eat(']') {
                    if !items.is_empty() && !self.eat(',') {
                        return None;
                    }
                    items.push(self.value(depth + 1)?);
                }
                Some(Json::List(items))
            }
            _ => {
                let text = self.rest.as_str();
                let end = text
                    .find(|c: char| !(c.is_ascii_alphanumeric() || "+-.".contains(c)))
                    .unwrap_or(text.len());
                let (word, tail) = text.split_at(end);
                self.rest = tail.chars();
                match word {
                    "true" => Some(Json::Bool(true)),
                    "false" => Some(Json::Bool(false)),
                    "null" => Some(Json::Null),
                    n if n.parse::<f64>().is_ok() => Some(Json::Number(n.to_string())),
                    _ => None,
                }
            }
        }
    }
}

/// The whole text as one value; `None` for anything malformed or trailing.
pub fn parse(text: &str) -> Option<Json> {
    let mut reader = Reader { rest: text.chars() };
    let value = reader.value(0)?;
    reader.peek().is_none().then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nested_documents() {
        let v = parse(r#" {"a":{"b":["2",true,null,-1.5e3]},"c":"q\"A\n"} "#).unwrap();
        assert_eq!(v.get("c").and_then(Json::text), Some("q\"A\n"));
        assert_eq!(
            v.get("a").and_then(|a| a.get("b")),
            Some(&Json::List(vec![
                Json::Text("2".into()),
                Json::Bool(true),
                Json::Null,
                Json::Number("-1.5e3".into())
            ]))
        );
    }

    #[test]
    fn refuses_malformed_and_hostile_input() {
        for bad in [
            "",
            "{",
            "{\"a\" 1}",
            "[1 2]",
            "{}x",
            "tru",
            "\"open",
            "{a:1}",
        ] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
        assert_eq!(parse(&"[".repeat(100_000)), None);
    }
}
