//! Search and selection state shared by the command palette and shortcut editor.
#[cfg(test)]
use super::keymap::COMMANDS;
use super::keymap::{Keymap, Kind};
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Commands,
    Bindings,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commands_are_searchable_in_every_supported_language() {
        crate::i18n::load_test_catalogs();
        let map = Keymap::default();
        let mut palette = Palette::new(Mode::Commands, &map);
        for query in ["Undo", "撤销", "元に戻す", "실행 취소"] {
            palette.query(query.into(), &map);
            assert!(
                palette
                    .results
                    .iter()
                    .any(|i| COMMANDS[*i].id == "edit.undo"),
                "{query}"
            );
        }
    }
    #[test]
    fn searches_chinese_english_pinyin_bindings_and_fuzzy_words() {
        crate::i18n::load_test_catalogs();
        let map = Keymap::default();
        let mut p = Palette::new(Mode::Commands, &map);
        for (query, id) in [
            ("Undo", "edit.undo"),
            ("undo", "edit.undo"),
            ("cx", "edit.undo"),
            ("Ctrl+Shift+P", "commands.open"),
            ("und", "edit.undo"),
            ("> 保存", "file.save"),
            ("笔刷 防抖", "stabilizer.off"),
        ] {
            p.query(query.into(), &map);
            assert!(p.results.iter().any(|i| COMMANDS[*i].id == id), "{}", query);
        }
        p.query("不可能匹配的长词句 nonexistent".into(), &map);
        assert!(p.results.is_empty());
        p.step(100);
        assert_eq!(p.current(), None);
    }
    #[test]
    fn gestures_only_appear_in_editor_and_selection_stays_in_range() {
        let map = Keymap::default();
        let mut p = Palette::new(Mode::Commands, &map);
        assert!(p
            .results
            .iter()
            .all(|i| matches!(COMMANDS[*i].kind, Kind::Press(_))));
        p.step(999);
        assert_eq!(p.selected, p.results.len() - 1);
        p.step(-999);
        assert_eq!(p.selected, 0);
        let mut p = Palette::new(Mode::Bindings, &map);
        p.query("hold.pan".into(), &map);
        assert_eq!(COMMANDS[p.current().unwrap()].id, "hold.pan");
    }
}
pub struct Palette {
    pub mode: Mode,
    pub query: String,
    pub results: Vec<usize>,
    pub selected: usize,
    pub recording: Option<usize>,
    pub message: String,
}
impl Palette {
    pub fn new(mode: Mode, map: &Keymap) -> Self {
        let mut p = Self {
            mode,
            query: String::new(),
            results: vec![],
            selected: 0,
            recording: None,
            message: String::new(),
        };
        p.refresh(map);
        p
    }
    pub fn refresh(&mut self, map: &Keymap) {
        let q = self
            .query
            .trim()
            .trim_start_matches('>')
            .trim()
            .to_lowercase();
        let tokens: Vec<_> = q.split_whitespace().collect();
        let mut scored = vec![];
        for (i, c) in map.commands().enumerate() {
            if self.mode == Mode::Commands && !matches!(c.kind, Kind::Press(_)) {
                continue;
            }
            let title = format!(
                "{} {}",
                crate::i18n::search_text(c.category),
                crate::i18n::search_text_for(c.owner, c.title)
            )
            .to_lowercase();
            let hay = format!("{} {} {} {}", title, c.id, c.keywords, map.label(i)).to_lowercase();
            let mut score = 0;
            let mut ok = true;
            for token in &tokens {
                if title.contains(token) {
                    score += 100;
                } else if hay.contains(token) {
                    score += 40;
                } else {
                    let mut chars = hay.chars();
                    if token
                        .chars()
                        .all(|wanted| chars.by_ref().any(|ch| ch == wanted))
                    {
                        score += 5;
                    } else {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                scored.push((i, score));
            }
        }
        scored.sort_by_key(|(i, s)| (std::cmp::Reverse(*s), *i));
        self.results = scored.into_iter().map(|(i, _)| i).collect();
        self.selected = self.selected.min(self.results.len().saturating_sub(1));
    }
    pub fn query(&mut self, text: String, map: &Keymap) {
        self.query = text;
        self.selected = 0;
        self.refresh(map);
    }
    pub fn current(&self) -> Option<usize> {
        self.results.get(self.selected).copied()
    }
    pub fn step(&mut self, amount: i32) {
        self.selected = (self.selected as i32 + amount)
            .max(0)
            .min(self.results.len().saturating_sub(1) as i32) as usize;
    }
}
