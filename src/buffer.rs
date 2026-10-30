use std::path::PathBuf;

pub struct Buffer {
    pub path: Option<PathBuf>,
    pub lines: Vec<Vec<char>>,
    pub cx: usize,
    pub cy: usize,
    pub top: usize,
    pub hscroll: usize,
    pub dirty: bool,
}

fn read_lines(path: &PathBuf) -> Vec<Vec<char>> {
    match std::fs::read_to_string(path) {
        Ok(s) => {
            let mut lines: Vec<Vec<char>> = s
                .split('\n')
                .map(|line| line.replace('\t', "    ").chars().collect())
                .collect();
            if lines.last().map_or(false, |v| v.is_empty()) {
                lines.pop();
            }
            if lines.is_empty() {
                lines.push(Vec::new());
            }
            lines
        }
        Err(_) => vec![Vec::new()],
    }
}

impl Buffer {
    pub fn new(path: Option<PathBuf>) -> Self {
        let lines = path.as_ref().map(read_lines).unwrap_or_else(|| vec![Vec::new()]);
        Self { path, lines, cx: 0, cy: 0, top: 0, hscroll: 0, dirty: false }
    }

    pub fn name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "未命名".to_string())
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        if let Some(ref p) = self.path {
            let mut content = self.lines.iter().map(|l| l.iter().collect::<String>()).collect::<Vec<_>>().join("\n");
            content.push('\n');
            std::fs::write(p, content)?;
            self.dirty = false;
        }
        Ok(())
    }

    pub fn insert_char(&mut self, ch: char) {
        let line = &mut self.lines[self.cy];
        let pos = self.cx.min(line.len());
        line.insert(pos, ch);
        self.cx = pos + 1;
        self.dirty = true;
    }

    pub fn newline(&mut self) {
        let line = self.lines[self.cy].clone();
        let pos = self.cx.min(line.len());
        let (head, tail) = line.split_at(pos);
        let indent: Vec<char> = head.iter().take_while(|c| **c == ' ').cloned().collect();
        self.lines[self.cy] = head.to_vec();
        let mut new_line = indent.clone();
        new_line.extend_from_slice(tail);
        self.lines.insert(self.cy + 1, new_line);
        self.cy += 1;
        self.cx = indent.len();
        self.dirty = true;
    }

    pub fn backspace(&mut self) {
        if self.cx > 0 {
            let line = &mut self.lines[self.cy];
            let pos = self.cx.min(line.len());
            if pos > 0 {
                line.remove(pos - 1);
                self.cx = pos - 1;
                self.dirty = true;
            }
        } else if self.cy > 0 {
            let cur = self.lines.remove(self.cy);
            self.cy -= 1;
            self.cx = self.lines[self.cy].len();
            self.lines[self.cy].extend(cur);
            self.dirty = true;
        }
    }

    pub fn delete(&mut self) {
        if self.cx < self.lines[self.cy].len() {
            self.lines[self.cy].remove(self.cx);
            self.dirty = true;
        } else if self.cy < self.lines.len() - 1 {
            let next = self.lines.remove(self.cy + 1);
            self.lines[self.cy].extend(next);
            self.dirty = true;
        }
    }

    pub fn move_up(&mut self) {
        if self.cy > 0 {
            self.cy -= 1;
            self.cx = self.cx.min(self.lines[self.cy].len());
        }
    }
    pub fn move_down(&mut self) {
        if self.cy < self.lines.len() - 1 {
            self.cy += 1;
            self.cx = self.cx.min(self.lines[self.cy].len());
        }
    }
    pub fn move_left(&mut self) {
        if self.cx > 0 {
            self.cx -= 1;
        } else if self.cy > 0 {
            self.cy -= 1;
            self.cx = self.lines[self.cy].len();
        }
    }
    pub fn move_right(&mut self) {
        if self.cx < self.lines[self.cy].len() {
            self.cx += 1;
        } else if self.cy < self.lines.len() - 1 {
            self.cy += 1;
            self.cx = 0;
        }
    }
    pub fn home(&mut self) { self.cx = 0; }
    pub fn end(&mut self) { self.cx = self.lines[self.cy].len(); }
}