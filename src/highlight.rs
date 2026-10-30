use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tok {
    Default,
    Keyword,
    Str,
    Comment,
    Number,
}

pub struct Lang {
    pub keywords: &'static [&'static str],
    pub line_comments: &'static [&'static str],
}

macro_rules! lang {
    ($kw:expr, $lc:expr) => {
        Lang { keywords: $kw, line_comments: $lc }
    };
}

const PY_KW: &[&str] = &[
    "False","None","True","and","as","assert","async","await","break","class","continue","def",
    "del","elif","else","except","finally","for","from","global","if","import","in","is","lambda",
    "nonlocal","not","or","pass","raise","return","try","while","with","yield","match","case",
];
const RS_KW: &[&str] = &[
    "as","break","const","continue","crate","else","enum","extern","false","fn","for","if","impl",
    "in","let","loop","match","mod","move","mut","pub","ref","return","self","Self","static","struct",
    "super","trait","true","type","unsafe","use","where","while","async","await","dyn",
];
const JS_KW: &[&str] = &[
    "await","break","case","catch","class","const","continue","debugger","default","delete","do",
    "else","enum","export","extends","false","finally","for","function","if","import","in",
    "instanceof","let","new","null","return","super","switch","this","throw","true","try","typeof",
    "var","void","while","with","yield","of","async","static","get","set",
];
const C_KW: &[&str] = &[
    "auto","break","case","char","const","continue","default","do","double","else","enum","extern",
    "float","for","goto","if","inline","int","long","register","return","short","signed","sizeof",
    "static","struct","switch","typedef","union","unsigned","void","volatile","while","class",
    "namespace","template","typename","public","private","protected","virtual","new","delete",
    "nullptr","true","false","using",
];
const GO_KW: &[&str] = &[
    "break","case","chan","const","continue","default","defer","else","fallthrough","for","func",
    "go","goto","if","import","interface","map","package","range","return","select","struct",
    "switch","type","var","nil","true","false",
];
const JAVA_KW: &[&str] = &[
    "abstract","assert","boolean","break","byte","case","catch","char","class","const","continue",
    "default","do","double","else","enum","extends","final","finally","float","for","if",
    "implements","import","instanceof","int","interface","long","native","new","package","private",
    "protected","public","return","short","static","super","switch","synchronized","this","throw",
    "throws","transient","try","void","volatile","while","true","false","null","var","record",
];
const SH_KW: &[&str] = &[
    "if","then","else","elif","fi","case","esac","for","while","until","do","done","in","function",
    "select","time","return","break","continue","local","export","readonly","declare","unset",
    "shift","source","alias","echo","cd","exit",
];

pub fn lang_for(ext: &str) -> Option<Lang> {
    match ext {
        "py" => Some(lang!(PY_KW, &["#"])),
        "rs" => Some(lang!(RS_KW, &["//"])),
        "js" | "jsx" | "ts" | "tsx" => Some(lang!(JS_KW, &["//"])),
        "c" | "h" | "cpp" | "hpp" | "cc" => Some(lang!(C_KW, &["//"])),
        "go" => Some(lang!(GO_KW, &["//"])),
        "java" => Some(lang!(JAVA_KW, &["//"])),
        "sh" | "bash" => Some(lang!(SH_KW, &["#"])),
        "toml" | "yml" | "yaml" | "ini" => Some(lang!(&[], &["#"])),
        "json" => Some(lang!(&[], &[])),
        _ => None,
    }
}

fn starts_with_chars(line: &[char], at: usize, pat: &[char]) -> bool {
    at + pat.len() <= line.len() && line[at..at + pat.len()] == *pat
}

pub fn highlight(line: &[char], lang: Option<&Lang>) -> Vec<Tok> {
    let n = line.len();
    let mut toks = vec![Tok::Default; n];
    let Some(lang) = lang else { return toks };

    let lcs: Vec<Vec<char>> = lang.line_comments.iter().map(|s| s.chars().collect()).collect();
    let mut i = 0;
    while i < n {
        // 行注释
        let mut matched = false;
        for lc in &lcs {
            if !lc.is_empty() && starts_with_chars(line, i, lc) {
                for t in toks.iter_mut().take(n).skip(i) { *t = Tok::Comment; }
                matched = true;
                break;
            }
        }
        if matched { break; }

        let c = line[i];

        // 字符串
        if c == '"' || c == '\'' || c == '`' {
            let q = c;
            let mut j = i + 1;
            while j < n {
                if line[j] == '\\' { j += 2; continue; }
                if line[j] == q { j += 1; break; }
                j += 1;
            }
            let end = j.min(n);
            for t in toks.iter_mut().take(end).skip(i) { *t = Tok::Str; }
            i = end;
            continue;
        }

        // 数字
        if c.is_ascii_digit() {
            let mut j = i;
            while j < n && (line[j].is_ascii_alphanumeric() || line[j] == '.' || line[j] == '_') {
                j += 1;
            }
            for t in toks.iter_mut().take(j).skip(i) { *t = Tok::Number; }
            i = j;
            continue;
        }

        // 标识符 / 关键字
        if c.is_alphabetic() || c == '_' {
            let mut j = i;
            while j < n && (line[j].is_alphanumeric() || line[j] == '_') {
                j += 1;
            }
            let word: String = line[i..j].iter().collect();
            if lang.keywords.contains(&word.as_str()) {
                for t in toks.iter_mut().take(j).skip(i) { *t = Tok::Keyword; }
            }
            i = j;
            continue;
        }

        i += 1;
    }
    toks
}

pub fn char_width(c: char) -> u16 {
    UnicodeWidthChar::width(c).unwrap_or(0) as u16
}

pub fn str_width(s: &str) -> u16 {
    s.chars().map(char_width).sum()
}