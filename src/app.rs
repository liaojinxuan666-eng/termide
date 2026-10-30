use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::buffer::Buffer;
use crate::highlight::{highlight, lang_for, str_width, Tok};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus { Tree, Editor }

pub struct TreeNode {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
}

pub enum PromptAction {
    SaveAs,
    NewFile,
    Open,
    Search,
}

pub struct PromptState {
    pub label: String,
    pub input: String,
    pub action: PromptAction,
}

pub struct App {
    pub running: bool,
    pub root: PathBuf,
    pub show_tree: bool,
    pub show_hidden: bool,
    pub focus: Focus,
    pub tree: Vec<TreeNode>,
    pub expanded: HashSet<PathBuf>,
    pub tree_sel: usize,
    pub tree_top: usize,
    pub buffers: Vec<Buffer>,
    pub active: usize,
    pub message: String,
    pub prompt: Option<PromptState>,
}

const TREE_W: u16 = 28;

impl App {
    pub fn new(path: PathBuf) -> Self {
        let (root, open_file) = if path.is_file() {
            (path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from(".")), Some(path))
        } else {
            (path, None)
        };
        let mut app = Self {
            running: true,
            root,
            show_tree: true,
            show_hidden: false,
            focus: Focus::Editor,
            tree: Vec::new(),
            expanded: HashSet::new(),
            tree_sel: 0,
            tree_top: 0,
            buffers: Vec::new(),
            active: 0,
            message: String::new(),
            prompt: None,
        };
        app.rebuild_tree();
        if let Some(p) = open_file {
            app.open_file(p);
        }
        app
    }

    // ---------------- file tree ----------------
    fn walk(&self, dir: &Path, depth: usize, out: &mut Vec<TreeNode>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut entries: Vec<(PathBuf, String, bool)> = rd
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                if !self.show_hidden && name.starts_with('.') {
                    return None;
                }
                let p = e.path();
                let is_dir = p.is_dir();
                Some((p, name, is_dir))
            })
            .collect();
        entries.sort_by(|a, b| {
            b.2.cmp(&a.2).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
        });
        for (p, name, is_dir) in entries {
            out.push(TreeNode { path: p.clone(), name, depth, is_dir });
            if is_dir && self.expanded.contains(&p) {
                self.walk(&p, depth + 1, out);
            }
        }
    }

    fn rebuild_tree(&mut self) {
        let mut out = Vec::new();
        let root = self.root.clone();
        self.walk(&root, 0, &mut out);
        self.tree = out;
        if self.tree_sel >= self.tree.len() {
            self.tree_sel = self.tree.len().saturating_sub(1);
        }
    }

    // ---------------- file open ----------------
    fn open_file(&mut self, path: PathBuf) {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if let Some(i) = self.buffers.iter().position(|b| b.path.as_ref() == Some(&path)) {
            self.active = i;
            self.focus = Focus::Editor;
            return;
        }
        let buf = Buffer::new(Some(path));
        self.message = format!("已打开 {}", buf.name());
        self.buffers.push(buf);
        self.active = self.buffers.len() - 1;
        self.focus = Focus::Editor;
    }

    fn save(&mut self) {
        let need_path = self.buffers.get(self.active).map_or(false, |b| b.path.is_none());
        if need_path {
            self.start_prompt(PromptAction::SaveAs, "保存为: ");
            return;
        }
        if let Some(buf) = self.buffers.get_mut(self.active) {
            match buf.save() {
                Ok(_) => self.message = format!("已保存 {}", buf.name()),
                Err(e) => self.message = format!("保存失败: {e}"),
            }
        }
    }

    fn close_buffer(&mut self) {
        if self.buffers.is_empty() { return; }
        let dirty = self.buffers[self.active].dirty;
        if dirty {
            self.message = "文件有未保存修改 (Ctrl+S 保存后再关闭)".to_string();
            return;
        }
        self.buffers.remove(self.active);
        if self.active >= self.buffers.len() {
            self.active = self.buffers.len().saturating_sub(1);
        }
        if self.buffers.is_empty() {
            self.focus = Focus::Tree;
        }
    }

    fn start_prompt(&mut self, action: PromptAction, label: &str) {
        self.prompt = Some(PromptState { label: label.to_string(), input: String::new(), action });
    }

    fn finish_prompt(&mut self) {
        let Some(p) = self.prompt.take() else { return };
        let input = p.input.trim().to_string();
        if input.is_empty() {
            return;
        }
        match p.action {
            PromptAction::SaveAs => {
                if let Some(buf) = self.buffers.get_mut(self.active) {
                    let path = expand_path(&input);
                    buf.path = Some(path.clone());
                    match buf.save() {
                        Ok(_) => self.message = format!("已保存 {}", buf.name()),
                        Err(e) => self.message = format!("保存失败: {e}"),
                    }
                }
            }
            PromptAction::NewFile => {
                let path = expand_path(&input);
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if !path.exists() {
                    let _ = std::fs::File::create(&path);
                }
                self.rebuild_tree();
                self.open_file(path);
            }
            PromptAction::Open => {
                let path = expand_path(&input);
                if path.is_dir() {
                    self.root = path;
                    self.expanded.clear();
                    self.tree_sel = 0;
                    self.rebuild_tree();
                } else if path.exists() {
                    self.open_file(path);
                } else {
                    self.message = format!("路径不存在: {}", input);
                }
            }
            PromptAction::Search => {
                if let Some(buf) = self.buffers.get(self.active) {
                    let n = buf.lines.len();
                    let cur_y = buf.cy;
                    for off in 1..=n {
                        let y = (cur_y + off) % n;
                        let line: String = buf.lines[y].iter().collect();
                        if let Some(col) = line.find(&input) {
                            let ch_col = line[..col].chars().count();
                            let path_ok = true;
                            if path_ok {
                                if let Some(b) = self.buffers.get_mut(self.active) {
                                    b.cy = y;
                                    b.cx = ch_col;
                                }
                                self.message = format!("\"{}\" 在第 {} 行", input, y + 1);
                                return;
                            }
                        }
                    }
                    self.message = format!("未找到 \"{}\"", input);
                }
            }
        }
    }

    // ---------------- key handling ----------------
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.prompt.is_some() {
            self.handle_prompt_key(key);
            return self.running;
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl {
            match key.code {
                KeyCode::Char('q') => { self.running = false; return false; }
                KeyCode::Char('s') => { self.save(); return true; }
                KeyCode::Char('n') => { self.start_prompt(PromptAction::NewFile, "新建文件: "); return true; }
                KeyCode::Char('o') => { self.start_prompt(PromptAction::Open, "打开: "); return true; }
                KeyCode::Char('f') => { self.start_prompt(PromptAction::Search, "搜索: "); return true; }
                KeyCode::Char('w') => { self.close_buffer(); return true; }
                KeyCode::Char('b') => { self.show_tree = !self.show_tree; return true; }
                KeyCode::Char('h') => { self.show_hidden = !self.show_hidden; self.rebuild_tree(); return true; }
                _ => {}
            }
        }

        match self.focus {
            Focus::Tree => self.handle_tree_key(key),
            Focus::Editor => self.handle_editor_key(key),
        }
        self.running
    }

    fn handle_prompt_key(&mut self, key: KeyEvent) {
        let Some(ref mut p) = self.prompt else { return };
        match key.code {
            KeyCode::Enter => self.finish_prompt(),
            KeyCode::Esc => { self.prompt = None; }
            KeyCode::Backspace => { p.input.pop(); }
            KeyCode::Char(c) => {
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT {
                    p.input.push(c);
                }
            }
            _ => {}
        }
    }

    fn handle_tree_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => {
                if !self.buffers.is_empty() {
                    self.focus = Focus::Editor;
                }
            }
            KeyCode::Up => { self.tree_sel = self.tree_sel.saturating_sub(1); }
            KeyCode::Down => {
                if self.tree_sel + 1 < self.tree.len() {
                    self.tree_sel += 1;
                }
            }
            KeyCode::PageUp => { self.tree_sel = self.tree_sel.saturating_sub(10); }
            KeyCode::PageDown => {
                self.tree_sel = (self.tree_sel + 10).min(self.tree.len().saturating_sub(1));
            }
            KeyCode::Enter => self.tree_enter(),
            KeyCode::Right => {
                if let Some(node) = self.tree.get(self.tree_sel) {
                    if node.is_dir && !self.expanded.contains(&node.path) {
                        self.expanded.insert(node.path.clone());
                        self.rebuild_tree();
                    }
                }
            }
            KeyCode::Left => {
                if let Some(node) = self.tree.get(self.tree_sel) {
                    if node.is_dir && self.expanded.contains(&node.path) {
                        self.expanded.remove(&node.path);
                        self.rebuild_tree();
                    } else {
                        let depth = node.depth;
                        let cur = self.tree_sel;
                        for i in (0..cur).rev() {
                            if self.tree[i].depth < depth {
                                self.tree_sel = i;
                                break;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn tree_enter(&mut self) {
        let Some(node) = self.tree.get(self.tree_sel) else { return };
        let path = node.path.clone();
        let is_dir = node.is_dir;
        if is_dir {
            if self.expanded.contains(&path) {
                self.expanded.remove(&path);
            } else {
                self.expanded.insert(path);
            }
            self.rebuild_tree();
        } else {
            self.open_file(path);
        }
    }

    fn handle_editor_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => {
                self.focus = Focus::Tree;
                return;
            }
            _ => {}
        }
        let Some(buf) = self.buffers.get_mut(self.active) else { return };
        match key.code {
            KeyCode::Up => buf.move_up(),
            KeyCode::Down => buf.move_down(),
            KeyCode::Left => buf.move_left(),
            KeyCode::Right => buf.move_right(),
            KeyCode::Home => buf.home(),
            KeyCode::End => buf.end(),
            KeyCode::PageUp => { for _ in 0..10 { buf.move_up(); } }
            KeyCode::PageDown => { for _ in 0..10 { buf.move_down(); } }
            KeyCode::Enter => buf.newline(),
            KeyCode::Backspace => buf.backspace(),
            KeyCode::Delete => buf.delete(),
            KeyCode::Char(c) => buf.insert_char(c),
            _ => {}
        }
    }

    // ---------------- render ----------------
    pub fn render(&mut self, f: &mut Frame) {
        let area = f.area();
        if area.width < 24 || area.height < 5 {
            let p = Paragraph::new("终端窗口太小").style(Style::default().fg(Color::Yellow));
            f.render_widget(p, area);
            return;
        }

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(area);
        let body = chunks[0];
        let status = chunks[1];

        let tw = if self.show_tree { TREE_W.min(body.width.saturating_sub(20)) } else { 0 };

        if self.show_tree {
            let split = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(tw), Constraint::Min(10)])
                .split(body);
            self.render_tree(f, split[0]);
            self.render_editor(f, split[1]);
        } else {
            self.render_editor(f, body);
        }

        self.render_status(f, status);
        self.render_prompt(f, status);
    }

    fn render_tree(&mut self, f: &mut Frame, area: Rect) {
        if area.width < 4 || area.height < 2 { return; }
        let block = Block::default()
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(Color::DarkGray));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let title = " 文件 ";
        let _ = f.buffer_mut().set_stringn(area.x, area.y, title, area.width as usize, Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD));

        let visible = inner.height as usize;
        if visible == 0 { return; }
        if self.tree_sel < self.tree_top { self.tree_top = self.tree_sel; }
        if self.tree_sel >= self.tree_top + visible {
            self.tree_top = self.tree_sel - visible + 1;
        }

        for row in 0..visible {
            let idx = self.tree_top + row;
            let Some(node) = self.tree.get(idx) else { break };
            let y = inner.y + row as u16;
            let indent = "  ".repeat(node.depth);
            let icon = if node.is_dir {
                if self.expanded.contains(&node.path) { "▾ " } else { "▸ " }
            } else {
                "  "
            };
            let mut text = format!("{indent}{icon}{}", node.name);

            let mut style = if node.is_dir {
                Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            if idx == self.tree_sel && self.focus == Focus::Tree {
                style = style.bg(Color::DarkGray).fg(Color::White);
            }
            let w = area.width as usize;
            text = text.chars().take(w).collect();
            let _ = f.buffer_mut().set_stringn(inner.x, y, &text, inner.width as usize, style);
        }

        // 光标位置
        if self.focus == Focus::Tree {
            let row = self.tree_sel.saturating_sub(self.tree_top);
            f.set_cursor_position(Position::new(inner.x, inner.y + row as u16));
        }
    }

    fn render_editor(&mut self, f: &mut Frame, area: Rect) {
        if area.width < 4 || area.height < 2 { return; }

        if self.buffers.is_empty() {
            let msg = "  Ctrl+O 打开   Ctrl+N 新建   Tab 切换焦点   Ctrl+Q 退出";
            let p = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
            f.render_widget(p, area);
            return;
        }

        // 标签栏
        let tab_y = area.y;
        let mut tab_x = area.x;
        for (i, buf) in self.buffers.iter().enumerate() {
            let label = format!(" {}{} ", buf.name(), if buf.dirty { " ●" } else { "" });
            let style = if i == self.active {
                Style::default().bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let w = str_width(&label);
            if tab_x + w > area.x + area.width { break; }
            let _ = f.buffer_mut().set_stringn(tab_x, tab_y, &label, w as usize, style);
            tab_x += w;
        }

        let code_y = area.y + 1;
        let code_h = area.height.saturating_sub(1);
        if code_h == 0 { return; }

        let active = self.active;
        let buf = &mut self.buffers[active];
        let numw = buf.lines.len().to_string().len().max(3) as u16;
        let text_x = area.x + numw + 1;
        let text_w = area.width.saturating_sub(numw + 1);
        if text_w == 0 { return; }

        // 滚动
        if buf.cy < buf.top { buf.top = buf.cy; }
        if buf.cy >= buf.top + code_h as usize { buf.top = buf.cy - code_h as usize + 1; }
        if buf.cx < buf.hscroll { buf.hscroll = buf.cx; }
        if buf.cx >= buf.hscroll + text_w as usize {
            buf.hscroll = buf.cx - text_w as usize + 1;
        }

        let ext = buf.path.as_ref().and_then(|p| p.extension()).map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        let lang = lang_for(&ext);

        for row in 0..code_h as usize {
            let ln = buf.top + row;
            if ln >= buf.lines.len() { break; }
            let y = code_y + row as u16;

            // 行号
            let num = format!("{:>w$} ", ln + 1, w = numw as usize);
            let _ = f.buffer_mut().set_stringn(area.x, y, &num, (numw + 1) as usize, Style::default().fg(Color::DarkGray));

            let line = &buf.lines[ln];
            let toks = highlight(line, lang.as_ref());
            let start = buf.hscroll.min(line.len());
            let mut i = start;
            let mut x = text_x;
            while i < line.len() && x < text_x + text_w {
                let tok = toks[i];
                let mut j = i;
                while j < line.len() && toks[j] == tok { j += 1; }
                let s: String = line[i..j].iter().collect();
                let style = style_for(tok);
                let (nx, _) = f.buffer_mut().set_stringn(x, y, &s, (text_w - (x - text_x)) as usize, style);
                x = nx;
                i = j;
            }
        }

        // 光标
        if self.focus == Focus::Editor {
            let cy = code_y + (buf.cy.saturating_sub(buf.top)) as u16;
            let start = buf.hscroll.min(buf.lines[buf.cy].len());
            let prefix: String = buf.lines[buf.cy][start..buf.cx.min(buf.lines[buf.cy].len())].iter().collect();
            let cx = text_x + str_width(&prefix);
            f.set_cursor_position(Position::new(cx.min(text_x + text_w - 1), cy));
        }
    }

    fn render_status(&mut self, f: &mut Frame, area: Rect) {
        let text = if self.prompt.is_some() {
            // 由 render_prompt 覆盖
            String::new()
        } else if let Some(buf) = self.buffers.get(self.active) {
            let mut s = format!(" {} ", buf.name());
            if let Some(p) = &buf.path {
                s.push_str(&format!("| {} ", p.display()));
            }
            s.push_str(&format!("| Ln {} Col {}{} ", buf.cy + 1, buf.cx + 1, if buf.dirty { " ●" } else { "" }));
            let right = if self.message.is_empty() {
                " ^S保存 ^N新建 ^O打开 ^F搜索 ^W关闭 ^B侧栏 ^Q退出 "
            } else {
                &self.message
            };
            let total = str_width(&s) + str_width(right);
            let pad = area.width.saturating_sub(total) as usize;
            s.push_str(&" ".repeat(pad));
            s.push_str(right);
            s
        } else {
            " termide | Ctrl+O 打开 Ctrl+N 新建 Ctrl+Q 退出 ".to_string()
        };
        let _ = f.buffer_mut().set_stringn(area.x, area.y, &text, area.width as usize, Style::default().bg(Color::Cyan).fg(Color::Black));
    }

    fn render_prompt(&mut self, f: &mut Frame, area: Rect) {
        let Some(p) = self.prompt.as_ref() else { return };
        let s = format!("{}{}", p.label, p.input);
        let _ = f.buffer_mut().set_stringn(area.x, area.y, &s, area.width as usize, Style::default().bg(Color::Blue).fg(Color::White));
        let cx = area.x + str_width(&p.label) + str_width(&p.input);
        f.set_cursor_position(Position::new(cx.min(area.x + area.width - 1), area.y));
    }
}

fn style_for(tok: Tok) -> Style {
    match tok {
        Tok::Default => Style::default().fg(Color::White),
        Tok::Keyword => Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        Tok::Str => Style::default().fg(Color::Green),
        Tok::Comment => Style::default().fg(Color::Cyan),
        Tok::Number => Style::default().fg(Color::Yellow),
    }
}

fn expand_path(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(s)
}
