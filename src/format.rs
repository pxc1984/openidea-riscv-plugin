//! Reformat in the style of the examples: labels at column 0, mnemonics at column 8,
//! instruction operands at column 16, directive operands after one space, and the trailing
//! comments of consecutive code lines in one column.
use crate::editor::parse_line;
use crate::language::mask;
use std::ops::Range as Span;

const MNEMONIC_COLUMN: usize = 8;
const OPERAND_COLUMN: usize = 16;

pub struct Indent {
    pub tab_size: usize,
    pub tabs: bool,
}

/// A reformatted line: its code and trailing comment. A line left as is has all its text in `code`.
struct Line {
    code: String,
    comment: Option<String>,
}

/// Replacements (byte span of a line without its terminator, new text) for the lines in `lines`.
pub fn edits(text: &str, lines: Span<usize>, indent: &Indent) -> Vec<(Span<usize>, String)> {
    // Строки маскируем непробельным символом: запятые, `#` и `:` внутри кавычек не делят операнды.
    let code = mask(text, b'_');
    let mut start = 0;
    let mut spans = Vec::new();
    let mut formatted = Vec::new();
    for raw in text.split_inclusive('\n') {
        let content = raw.trim_end_matches(['\n', '\r']);
        let span = start..start + content.len();
        start += raw.len();
        formatted.push(format_line(content, &code[span.clone()], indent));
        spans.push(span);
    }
    // Группа выравнивания комментариев идёт до первой строки без хвостового комментария,
    // поэтому её колонку считаем по всему файлу, даже если форматируется только выделение.
    let mut block_end = 0;
    let mut comment_column = 0;
    let mut found = Vec::new();
    for (i, (line, span)) in formatted.iter().zip(spans).enumerate() {
        let mut new = line.code.clone();
        if let Some(comment) = &line.comment {
            if i >= block_end {
                block_end = (i..formatted.len()).find(|&j| formatted[j].comment.is_none()).unwrap_or(formatted.len());
                let widest = formatted[i..block_end].iter().map(|l| column(&l.code, indent.tab_size)).max();
                comment_column = widest.unwrap_or(0) + 1;
            }
            pad(&mut new, comment_column, indent);
            new.push_str(comment);
        }
        if lines.contains(&i) && new != text[span.clone()] {
            found.push((span, new));
        }
    }
    found
}

fn format_line(raw: &str, code: &str, indent: &Indent) -> Line {
    let unchanged = Line { code: raw.trim_end().to_owned(), comment: None };
    if code.trim().is_empty() {
        return unchanged;
    }
    let line = parse_line(code, 0);
    let mut out = String::new();
    for (name, _) in &line.labels {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(name);
        out.push(':');
    }
    if let Some((op, at)) = line.op {
        pad(&mut out, MNEMONIC_COLUMN, indent);
        out.push_str(&raw[at..at + op.len()]);
        if !line.operands.is_empty() {
            if op.starts_with('.') {
                out.push(' ');
            } else {
                pad(&mut out, OPERAND_COLUMN, indent);
            }
            let operands: Vec<_> = line.operands.iter().map(|(o, at)| &raw[*at..*at + o.len()]).collect();
            out.push_str(&operands.join(", "));
        }
    }
    let comment = raw[code.trim_end().len()..].trim();
    // Комментарий перед кодом или между операндами при разборе теряется: такую строку не трогаем.
    let significant = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    if significant(&out) + &significant(comment) != significant(raw) {
        return unchanged;
    }
    Line { code: out, comment: (!comment.is_empty()).then(|| comment.to_owned()) }
}

fn column(s: &str, tab_size: usize) -> usize {
    s.chars().fold(0, |col, c| if c == '\t' { (col / tab_size + 1) * tab_size } else { col + 1 })
}

/// Moves to `target`, or by one step when already there.
fn pad(out: &mut String, target: usize, indent: &Indent) {
    let mut col = column(out, indent.tab_size);
    if indent.tabs {
        loop {
            out.push('\t');
            col = (col / indent.tab_size + 1) * indent.tab_size;
            if col >= target {
                break;
            }
        }
    } else {
        out.push_str(&" ".repeat(target.saturating_sub(col).max(1)));
    }
}
