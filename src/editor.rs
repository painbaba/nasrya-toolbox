//! A single-line text editor for the args / hash prompt: cursor movement,
//! word jumps, delete, paste, and per-key history navigation.

use std::collections::HashMap;

/// The line being edited. The cursor is a char index (0..=len).
#[derive(Debug, Default, Clone)]
pub struct LineEditor {
    chars: Vec<char>,
    cursor: usize,
}

impl LineEditor {
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    #[cfg(test)]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Replace the contents; the cursor goes to the end.
    pub fn set(&mut self, s: &str) {
        self.chars = s.chars().collect();
        self.cursor = self.chars.len();
    }

    /// Take the contents, leaving the editor empty.
    pub fn take(&mut self) -> String {
        let s = self.text();
        self.set("");
        s
    }

    pub fn insert(&mut self, c: char) {
        self.chars.insert(self.cursor, c);
        self.cursor += 1;
    }

    /// Paste: line breaks become spaces (the prompt is one line), other
    /// control characters are dropped.
    pub fn insert_str(&mut self, s: &str) {
        for c in s.chars() {
            match c {
                '\r' => {}
                '\n' | '\t' => self.insert(' '),
                c if c.is_control() => {}
                c => self.insert(c),
            }
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.chars.len());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.chars.len();
    }

    /// Ctrl+Left: to the start of the previous word.
    pub fn word_left(&mut self) {
        while self.cursor > 0 && self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }
        while self.cursor > 0 && !self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }
    }

    /// Ctrl+Right: to the end of the next word.
    pub fn word_right(&mut self) {
        let n = self.chars.len();
        while self.cursor < n && self.chars[self.cursor].is_whitespace() {
            self.cursor += 1;
        }
        while self.cursor < n && !self.chars[self.cursor].is_whitespace() {
            self.cursor += 1;
        }
    }

    /// Ctrl+W: delete the word before the cursor.
    pub fn delete_word_left(&mut self) {
        let end = self.cursor;
        self.word_left();
        self.chars.drain(self.cursor..end);
    }

    /// Ctrl+U: delete everything before the cursor.
    pub fn clear_to_start(&mut self) {
        self.chars.drain(..self.cursor);
        self.cursor = 0;
    }

    /// The part of the line to draw in `width` columns, keeping the cursor
    /// visible: (visible text, cursor column within it).
    pub fn view(&self, width: usize) -> (String, usize) {
        let width = width.max(1);
        // leave room for the cursor itself at the end of the line
        let start = (self.cursor + 1).saturating_sub(width);
        let end = (start + width).min(self.chars.len());
        (self.chars[start..end].iter().collect(), self.cursor - start)
    }
}

/// Previously used lines, per key (a tool name, or the hash prompt), with
/// Up/Down navigation like a shell.
#[derive(Debug, Default)]
pub struct History {
    entries: HashMap<String, Vec<String>>,
    /// (key, index into entries[key]) while browsing; None = editing a new line
    pos: Option<(String, usize)>,
    /// What was being typed before Up was first pressed.
    draft: String,
}

const MAX_PER_KEY: usize = 50;

impl History {
    /// Remember `line` for `key` (skips blanks and immediate repeats).
    pub fn push(&mut self, key: &str, line: &str) {
        self.pos = None;
        if line.trim().is_empty() {
            return;
        }
        let list = self.entries.entry(key.to_string()).or_default();
        if list.last().map(String::as_str) != Some(line) {
            list.push(line.to_string());
            if list.len() > MAX_PER_KEY {
                list.remove(0);
            }
        }
    }

    /// Stop browsing (call when a prompt opens or closes).
    pub fn reset(&mut self) {
        self.pos = None;
    }

    /// Up: the previous entry for `key`, or None if there's nothing older.
    pub fn older(&mut self, key: &str, current: &str) -> Option<String> {
        let list = self.entries.get(key)?;
        let idx = match &self.pos {
            Some((k, i)) if k == key => i.checked_sub(1)?,
            _ => {
                self.draft = current.to_string();
                list.len().checked_sub(1)?
            }
        };
        self.pos = Some((key.to_string(), idx));
        Some(list[idx].clone())
    }

    /// Down: the next newer entry, then back to the draft.
    pub fn newer(&mut self, key: &str) -> Option<String> {
        let list = self.entries.get(key)?;
        match &self.pos {
            Some((k, i)) if k == key => {
                if i + 1 < list.len() {
                    let idx = i + 1;
                    self.pos = Some((key.to_string(), idx));
                    Some(list[idx].clone())
                } else {
                    self.pos = None;
                    Some(std::mem::take(&mut self.draft))
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ed(s: &str) -> LineEditor {
        let mut e = LineEditor::default();
        e.set(s);
        e
    }

    #[test]
    fn edit_in_the_middle() {
        let mut e = ed("-u https://old.example/x");
        for _ in 0.."old.example/x".len() {
            e.left();
        }
        for _ in 0.."old".len() {
            e.delete();
        }
        e.insert_str("new");
        assert_eq!(e.text(), "-u https://new.example/x");
        e.home();
        e.insert('!');
        assert_eq!(e.text(), "!-u https://new.example/x");
        e.end();
        e.backspace();
        assert_eq!(e.text(), "!-u https://new.example/");
    }

    #[test]
    fn word_motion_and_deletion() {
        let mut e = ed("dir -u  https://t -w list.txt");
        e.word_left();
        assert_eq!(e.cursor(), "dir -u  https://t -w ".len());
        e.word_left();
        e.word_left();
        assert_eq!(e.cursor(), "dir -u  ".len());
        e.word_right();
        assert_eq!(e.cursor(), "dir -u  https://t".len());
        e.delete_word_left();
        assert_eq!(e.text(), "dir -u   -w list.txt");
        e.clear_to_start();
        assert_eq!(e.text(), " -w list.txt");
        assert_eq!(e.cursor(), 0);
    }

    #[test]
    fn boundaries_dont_panic() {
        let mut e = ed("");
        e.left();
        e.backspace();
        e.delete();
        e.word_left();
        e.word_right();
        e.delete_word_left();
        e.clear_to_start();
        e.right();
        assert_eq!((e.text(), e.cursor()), (String::new(), 0));
    }

    #[test]
    fn paste_flattens_lines() {
        let mut e = ed("");
        e.insert_str("-H \"A: b\"\r\n-v\tx\u{7}");
        assert_eq!(e.text(), "-H \"A: b\" -v x");
    }

    #[test]
    fn unicode_is_char_based() {
        let mut e = ed("héllo");
        e.left();
        e.left();
        e.left();
        e.backspace();
        assert_eq!(e.text(), "hllo");
    }

    #[test]
    fn view_keeps_cursor_visible() {
        let e = ed("0123456789");
        assert_eq!(e.view(20), ("0123456789".to_string(), 10));
        let (txt, col) = e.view(5);
        assert_eq!((txt.as_str(), col), ("6789", 4));
        let mut e = ed("0123456789");
        e.home();
        assert_eq!(e.view(5), ("01234".to_string(), 0));
    }

    #[test]
    fn history_up_down_with_draft() {
        let mut h = History::default();
        h.push("nmap", "-sV a");
        h.push("nmap", "-sV b");
        h.push("nmap", "-sV b"); // repeat ignored
        h.push("ffuf", "-u x");
        assert_eq!(h.older("nmap", "draft").as_deref(), Some("-sV b"));
        assert_eq!(h.older("nmap", "ignored").as_deref(), Some("-sV a"));
        assert_eq!(h.older("nmap", "ignored"), None); // oldest reached
        assert_eq!(h.newer("nmap").as_deref(), Some("-sV b"));
        assert_eq!(h.newer("nmap").as_deref(), Some("draft"));
        assert_eq!(h.newer("nmap"), None);
        assert_eq!(h.older("gobuster", ""), None);
    }

    #[test]
    fn history_is_capped() {
        let mut h = History::default();
        for i in 0..(MAX_PER_KEY + 10) {
            h.push("t", &i.to_string());
        }
        let mut oldest = None;
        while let Some(x) = h.older("t", "") {
            oldest = Some(x);
        }
        assert_eq!(oldest.as_deref(), Some("10"));
    }
}
