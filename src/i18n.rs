//! Embedded, English-first UI translations. Protocol identifiers never pass through this module.
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    sync::OnceLock,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "en")]
    English,
    #[serde(rename = "zh-CN")]
    Chinese,
    #[serde(rename = "ja")]
    Japanese,
    #[serde(rename = "ko")]
    Korean,
}
impl Language {
    pub const ALL: [Self; 4] = [Self::English, Self::Chinese, Self::Japanese, Self::Korean];
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Chinese => "zh-CN",
            Self::Japanese => "ja",
            Self::Korean => "ko",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Chinese => "简体中文",
            Self::Japanese => "日本語",
            Self::Korean => "한국어",
        }
    }
}
thread_local! { static CURRENT: Cell<Language> = const { Cell::new(Language::English) }; }
pub fn current() -> Language {
    CURRENT.with(Cell::get)
}
pub fn set_language(language: Language) {
    CURRENT.with(|v| v.set(language));
}
type Catalog = BTreeMap<String, String>;
struct Catalogs {
    values: [Catalog; 4],
    templates: Vec<String>,
    legacy: BTreeMap<String, String>,
    localized: [BTreeSet<String>; 4],
}
fn catalogs() -> &'static Catalogs {
    static DATA: OnceLock<Catalogs> = OnceLock::new();
    DATA.get_or_init(|| {
        let values: [Catalog; 4] = [
            include_str!("../locales/en.json"),
            include_str!("../locales/zh-CN.json"),
            include_str!("../locales/ja.json"),
            include_str!("../locales/ko.json"),
        ]
        .map(|s| serde_json::from_str(s).expect("validated built-in language catalog"));
        let templates = values[0]
            .keys()
            .filter(|s| {
                s.contains('{') && s.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3
            })
            .cloned()
            .collect();
        let legacy = values[1]
            .iter()
            .map(|(en, zh)| (zh.clone(), en.clone()))
            .collect();
        let localized = std::array::from_fn(|i| values[i].values().cloned().collect());
        Catalogs {
            values,
            templates,
            legacy,
            localized,
        }
    })
}
fn index(language: Language) -> usize {
    match language {
        Language::English => 0,
        Language::Chinese => 1,
        Language::Japanese => 2,
        Language::Korean => 3,
    }
}
pub fn text(message: &str) -> &str {
    text_in(current(), message)
}
pub fn text_in(language: Language, message: &str) -> &str {
    if language != Language::English && catalogs().localized[index(language)].contains(message) {
        return message;
    }
    let message = if message
        .chars()
        .any(|c| ('\u{3400}'..='\u{9fff}').contains(&c))
    {
        catalogs()
            .legacy
            .get(message)
            .map(String::as_str)
            .unwrap_or(message)
    } else {
        message
    };
    if language == Language::English {
        return message;
    }
    catalogs().values[index(language)]
        .get(message)
        .map(String::as_str)
        .unwrap_or(message)
}
/// Localized text for searching, including English and every bundled translation.
pub fn search_text(message: &str) -> String {
    let mut result = message.to_owned();
    for language in Language::ALL.into_iter().skip(1) {
        result.push(' ');
        result.push_str(text_in(language, message));
    }
    result
}
/// Split a Rust format template without interpreting user data as format syntax.
fn parts(template: &str) -> Option<Vec<(&str, bool)>> {
    let mut parts = vec![];
    let mut offset = 0;
    while let Some(start) = template[offset..].find('{') {
        let start = offset + start;
        if template[start..].starts_with("{{") {
            return None;
        }
        parts.push((&template[offset..start], false));
        let end = start + template[start..].find('}')? + 1;
        parts.push((&template[start..end], true));
        offset = end;
    }
    parts.push((&template[offset..], false));
    Some(parts)
}
fn interpolate<'a>(source: &str, translation: &str, rendered: &'a str) -> Option<String> {
    let source = parts(source)?;
    let mut remaining = rendered;
    let mut arguments = BTreeMap::<String, &'a str>::new();
    let mut position = 0;
    for (i, (part, slot)) in source.iter().enumerate() {
        if !slot {
            remaining = remaining.strip_prefix(part)?;
            continue;
        }
        let next = source.get(i + 1)?.0;
        if next.is_empty() && i + 2 < source.len() {
            return None;
        }
        let length = if next.is_empty() {
            remaining.len()
        } else {
            remaining.find(next)?
        };
        let key = &part[1..part.len() - 1];
        let key = if key.is_empty() || key.starts_with(':') {
            let key = position.to_string();
            position += 1;
            key
        } else {
            key.to_owned()
        };
        arguments.insert(key, &remaining[..length]);
        remaining = &remaining[length..];
    }
    if !remaining.is_empty() {
        return None;
    }
    let mut result = String::new();
    let mut position = 0;
    for (part, slot) in parts(translation)? {
        if !slot {
            result.push_str(part);
            continue;
        }
        let key = &part[1..part.len() - 1];
        let key = if key.is_empty() || key.starts_with(':') {
            let key = position.to_string();
            position += 1;
            key
        } else {
            key.to_owned()
        };
        result.push_str(arguments.get(&key)?);
    }
    Some(result)
}
pub fn translated(message: &str) -> Cow<'_, str> {
    translated_in(current(), message)
}
pub fn translated_in(language: Language, message: &str) -> Cow<'_, str> {
    let direct = text_in(language, message);
    if language == Language::English || direct != message {
        return Cow::Borrowed(direct);
    }
    let data = catalogs();
    for template in &data.templates {
        let prefix = template.split('{').next().unwrap_or("");
        if !message.starts_with(prefix) {
            continue;
        }
        let suffix = template.rsplit('}').next().unwrap_or("");
        if !message.ends_with(suffix) {
            continue;
        }
        let translation = text_in(language, template);
        if translation != template {
            if let Some(value) = interpolate(template, translation, message) {
                return Cow::Owned(value);
            }
        }
    }
    Cow::Borrowed(message)
}
pub trait Localize {
    type Output;
    fn localized(self) -> Self::Output;
}
pub fn localize<T: Localize>(value: T) -> T::Output {
    value.localized()
}
impl<'a> Localize for &'a str {
    type Output = Cow<'a, str>;
    fn localized(self) -> Self::Output {
        translated(self)
    }
}
impl Localize for String {
    type Output = String;
    fn localized(self) -> String {
        match translated(&self) {
            Cow::Borrowed(v) if v == self => self,
            value => value.into_owned(),
        }
    }
}
impl Localize for &String {
    type Output = String;
    fn localized(self) -> String {
        translated(self).into_owned()
    }
}
impl<'a> Localize for Cow<'a, str> {
    type Output = String;
    fn localized(self) -> String {
        translated(&self).into_owned()
    }
}
#[cfg(all(windows, feature = "desktop"))]
impl Localize for eframe::egui::RichText {
    type Output = Self;
    fn localized(self) -> Self {
        self
    }
}
#[cfg(all(windows, feature = "desktop"))]
impl Localize for eframe::egui::WidgetText {
    type Output = Self;
    fn localized(self) -> Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalogs_have_matching_keys_and_format_fields() {
        let data = catalogs();
        for catalog in &data.values {
            assert_eq!(
                catalog.keys().collect::<Vec<_>>(),
                data.values[0].keys().collect::<Vec<_>>()
            );
            for (key, value) in catalog {
                let slots = |s: &str| {
                    let mut out = parts(s)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|(_, slot)| *slot)
                        .map(|(s, _)| s.to_owned())
                        .collect::<Vec<_>>();
                    out.sort();
                    out
                };
                assert_eq!(slots(key), slots(value), "{key}: {value}");
                assert!(!value.is_empty());
            }
        }
    }
    #[test]
    fn english_default_fallback_and_languages_are_independent() {
        assert_eq!(Language::default(), Language::English);
        assert_eq!(text_in(Language::English, "File"), "File");
        assert_eq!(text_in(Language::Japanese, "File"), "ファイル");
        assert_eq!(text_in(Language::Korean, "File"), "파일");
        assert_eq!(text_in(Language::Chinese, "File"), "文件");
        assert_eq!(text_in(Language::English, "图层"), "Layers");
        assert_eq!(text_in(Language::Japanese, "图层"), "レイヤー");
        assert_eq!(
            text_in(Language::Japanese, text_in(Language::Japanese, "Color")),
            "色"
        );
        assert_eq!(
            text_in(Language::Korean, "third_party.custom"),
            "third_party.custom"
        );
    }
    #[test]
    fn dynamic_numbers_and_user_data_are_not_reformatted() {
        assert_eq!(translated_in(Language::Japanese, "Layer 03"), "レイヤー 03");
        assert_eq!(
            interpolate(
                "Save failed: {error}",
                "保存に失敗: {error}",
                "Save failed: C:/用户/{draft}.mcp"
            )
            .unwrap(),
            "保存に失敗: C:/用户/{draft}.mcp"
        );
        assert_eq!(
            translated_in(Language::Korean, "History 1.25 MB"),
            "History 1.25 MB"
        );
        assert!(interpolate("Layer {}", "レイヤー {}", "Not a layer").is_none());
    }
}
