//! Strict JSON reader for the settings schema. No runtime or serialization dependencies.
use std::collections::BTreeMap;

enum Value {
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
    Number(u32),
}
struct Reader<'a> {
    text: &'a str,
    at: usize,
    depth: usize,
}
impl<'a> Reader<'a> {
    fn error(&self, message: &str) -> String {
        format!("settings.json at byte {}: {}", self.at + 1, message)
    }
    fn ws(&mut self) {
        while self
            .text
            .as_bytes()
            .get(self.at)
            .map_or(false, |b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.at += 1;
        }
    }
    fn take(&mut self, b: u8) -> bool {
        self.ws();
        if self.text.as_bytes().get(self.at) == Some(&b) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, b: u8) -> Result<(), String> {
        if self.take(b) {
            Ok(())
        } else {
            Err(self.error(&format!("Expected {}", b as char)))
        }
    }
    fn hex(&mut self) -> Result<u16, String> {
        let end = self.at + 4;
        let digits = self
            .text
            .get(self.at..end)
            .ok_or_else(|| self.error("Incomplete Unicode escape"))?;
        let value =
            u16::from_str_radix(digits, 16).map_err(|_| self.error("Invalid Unicode escape"))?;
        self.at = end;
        Ok(value)
    }
    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let ch = self.text[self.at..]
                .chars()
                .next()
                .ok_or_else(|| self.error("字符串未闭合"))?;
            self.at += ch.len_utf8();
            match ch {
                '"' => return Ok(out),
                '\\' => {
                    let escape = *self
                        .text
                        .as_bytes()
                        .get(self.at)
                        .ok_or_else(|| self.error("Incomplete escape"))?;
                    self.at += 1;
                    out.push(match escape {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let first = self.hex()?;
                            let scalar = if (0xd800..=0xdbff).contains(&first) {
                                if !self.text[self.at..].starts_with("\\u") {
                                    return Err(self.error("Missing low Unicode surrogate"));
                                }
                                self.at += 2;
                                let second = self.hex()?;
                                if !(0xdc00..=0xdfff).contains(&second) {
                                    return Err(self.error("Invalid low Unicode surrogate"));
                                }
                                0x10000
                                    + (((first - 0xd800) as u32) << 10)
                                    + (second - 0xdc00) as u32
                            } else {
                                first as u32
                            };
                            char::from_u32(scalar)
                                .ok_or_else(|| self.error("Invalid Unicode character"))?
                        }
                        _ => return Err(self.error("Invalid string escape")),
                    });
                }
                ch if ch < ' ' => {
                    return Err(self
                        .error("Strings cannot contain unescaped newlines or control characters"))
                }
                ch => out.push(ch),
            }
        }
    }
    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        if self.depth >= 8 {
            return Err(self.error("Settings are nested too deeply"));
        }
        self.depth += 1;
        let value = match self.text.as_bytes().get(self.at).copied() {
            Some(b'"') => Value::String(self.string()?),
            Some(b'{') => {
                self.at += 1;
                let mut map = BTreeMap::new();
                if !self.take(b'}') {
                    loop {
                        let key = self.string()?;
                        self.expect(b':')?;
                        let value = self.value()?;
                        if map.insert(key, value).is_some() {
                            return Err(self.error("字段重复"));
                        }
                        if self.take(b'}') {
                            break;
                        }
                        self.expect(b',')?;
                    }
                }
                Value::Object(map)
            }
            Some(b'[') => {
                self.at += 1;
                let mut items = vec![];
                if !self.take(b']') {
                    loop {
                        items.push(self.value()?);
                        if self.take(b']') {
                            break;
                        }
                        self.expect(b',')?;
                    }
                }
                Value::Array(items)
            }
            Some(b'0'..=b'9') => {
                let start = self.at;
                while self
                    .text
                    .as_bytes()
                    .get(self.at)
                    .map_or(false, |b| b.is_ascii_digit())
                {
                    self.at += 1;
                }
                let digits = &self.text[start..self.at];
                if digits.len() > 1 && digits.starts_with('0') {
                    return Err(self.error("数字不能有前导零"));
                }
                Value::Number(digits.parse().map_err(|_| self.error("版本号超出范围"))?)
            }
            _ => return Err(self.error("需要对象、数组、字符串或版本号")),
        };
        self.depth -= 1;
        Ok(value)
    }
}
pub(super) fn read(text: &str) -> Result<Vec<(String, Vec<String>)>, String> {
    if text.len() > 1024 * 1024 {
        return Err("settings.json 超过 1 MB".into());
    }
    let mut reader = Reader {
        text: text.trim_start_matches('\u{feff}'),
        at: 0,
        depth: 0,
    };
    let mut root = match reader.value()? {
        Value::Object(map) => map,
        _ => return Err("settings.json 顶层需要对象".into()),
    };
    reader.ws();
    if reader.at != reader.text.len() {
        return Err(reader.error("文档末尾有多余内容"));
    }
    if let Some(version) = root.remove("version") {
        if !matches!(version, Value::Number(1)) {
            return Err("不支持的 settings.json 版本".into());
        }
    }
    let keys = match root.remove("keybindings") {
        None => BTreeMap::new(),
        Some(Value::Object(map)) => map,
        _ => return Err("keybindings 需要对象".into()),
    };
    if let Some(key) = root.keys().next() {
        return Err(format!("未知设置字段：{}", key));
    }
    keys.into_iter()
        .map(|(id, value)| {
            let keys = match value {
                Value::String(s) => vec![s],
                Value::Array(items) => items
                    .into_iter()
                    .map(|v| match v {
                        Value::String(s) => Ok(s),
                        _ => Err(format!("{} 的绑定需要字符串", id)),
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                _ => return Err(format!("{} 需要字符串或字符串数组", id)),
            };
            Ok((id, keys))
        })
        .collect()
}
fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            ch if ch < ' ' => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}
pub(super) fn write(bindings: impl Iterator<Item = (&'static str, Vec<String>)>) -> String {
    let lines = bindings
        .map(|(id, keys)| {
            format!(
                "    {}: [{}]",
                quote(id),
                keys.iter().map(|s| quote(s)).collect::<Vec<_>>().join(", ")
            )
        })
        .collect::<Vec<_>>();
    format!(
        "{{\n  \"version\": 1,\n  \"keybindings\": {{\n{}\n  }}\n}}\n",
        lines.join(",\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_json_errors_are_rejected_without_panics() {
        for text in [
            "",
            "[]",
            "{",
            r#"{"keybindings": []}"#,
            r#"{"keybindings":{"a":[4]}}"#,
            r#"{"version":2}"#,
            r#"{"version":01}"#,
            r#"{"version":1,"version":1}"#,
            r#"{"keybindings":{"a":"B",}}"#,
            r#"{"keybindings":{"a":"\ud800"}}"#,
            r#"{"keybindings":{"a":"\udc00"}}"#,
            r#"{"keybindings":{"a":"\u🚀"}}"#,
            r#"{} trailing"#,
        ] {
            assert!(read(text).is_err(), "{}", text);
        }
    }
    #[test]
    fn unicode_and_escaped_strings_round_trip() {
        let input = r#"{"keybindings":{"汉字\uD83D\uDE80":["Ctrl+\\","\"","\n"]}}"#;
        let parsed = read(input).unwrap();
        assert_eq!(parsed[0].0, "汉字🚀");
        assert_eq!(parsed[0].1, vec!["Ctrl+\\", "\"", "\n"]);
        assert_eq!(
            read(&write(std::iter::once(("test", parsed[0].1.clone())))).unwrap()[0].1,
            parsed[0].1
        );
    }
}
