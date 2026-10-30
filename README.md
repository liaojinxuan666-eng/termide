# termide

A minimal terminal IDE — Rust rewrite of the original Python prototype.

## Features
- File tree with expand/collapse
- Multi-tab editor with dirty markers
- Syntax highlighting (py, rs, js/ts, c/cpp, go, java, sh, yaml/toml/ini, json)
- Chinese-width aware cursor placement
- Ctrl+S/N/O/F/W/B/H/Q and Tab focus switching

## Build
```bash
cargo build --release