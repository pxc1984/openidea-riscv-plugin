//! Source analysis behind hover, completion, navigation, operand help and live diagnostics.
use crate::docs;
use crate::language::{Function, blank_comments, functions};
use openidea_plugin::{Diagnostic, Range, Severity, text};
use std::collections::HashSet;
use std::ops::Range as Span;

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b'$')
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(is_word) && !name.starts_with(|c: char| c.is_ascii_digit())
}

fn is_register(name: &str) -> bool {
    docs::register(name).is_some()
}

/// The identifier or number around byte `at`.
pub fn word_at(code: &str, at: usize) -> Option<Span<usize>> {
    let bytes = code.as_bytes();
    let mut start = at.min(bytes.len());
    while start > 0 && is_word(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = at.min(bytes.len());
    while end < bytes.len() && is_word(bytes[end]) {
        end += 1;
    }
    (start < end).then_some(start..end)
}

/// One source line split like the assembler does: leading labels, mnemonic, operands.
pub struct Line<'a> {
    pub labels: Vec<(&'a str, usize)>,
    pub op: Option<(&'a str, usize)>,
    pub operands: Vec<(&'a str, usize)>,
}

pub fn parse_line(code: &str, start: usize) -> Line<'_> {
    let end = code[start..].find('\n').map_or(code.len(), |n| start + n);
    let raw = &code[start..end];
    let mut labels = Vec::new();
    let mut rest = 0;
    while let Some(colon) = raw[rest..].find(':') {
        let name = raw[rest..rest + colon].trim();
        if !(valid_name(name) || (!name.is_empty() && name.bytes().all(|c| c.is_ascii_digit()))) {
            break;
        }
        let at = rest + raw[rest..].find(name).unwrap();
        labels.push((name, start + at));
        rest += colon + 1;
    }
    let tail = &raw[rest..];
    let lead = tail.len() - tail.trim_start().len();
    let body = tail.trim();
    let mut operands = Vec::new();
    let op = (!body.is_empty()).then(|| {
        let split = body.find(char::is_whitespace).unwrap_or(body.len());
        let op_at = start + rest + lead;
        let mut offset = op_at + split;
        let mut depth = 0;
        let mut from = offset;
        let args = &code[offset..start + rest + lead + body.len()];
        for (i, c) in args.char_indices() {
            match c {
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    operands.push((offset + i, from));
                    from = offset + i + 1;
                }
                _ => {}
            }
        }
        offset += args.len();
        operands.push((offset, from));
        (&body[..split], op_at)
    });
    let operands = operands
        .into_iter()
        .filter_map(|(end, from)| {
            let s = &code[from..end];
            let t = s.trim();
            (!t.is_empty()).then(|| (t, from + s.find(t).unwrap()))
        })
        .collect();
    Line { labels, op, operands }
}

fn line_starts(code: &str) -> impl Iterator<Item = usize> + '_ {
    std::iter::once(0).chain(code.match_indices('\n').map(|(i, _)| i + 1))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    pub name: String,
    pub at: usize,
    pub constant: bool,
}

pub fn labels(code: &str) -> Vec<Label> {
    let mut found = Vec::new();
    for start in line_starts(code) {
        let line = parse_line(code, start);
        for (name, at) in line.labels {
            found.push(Label { name: name.into(), at, constant: false });
        }
        if let Some((".equ" | ".equiv" | ".eqv" | ".set", _)) = line.op
            && let Some((first, at)) = line.operands.first()
        {
            let name = first.split_whitespace().next().unwrap_or("");
            if valid_name(name) {
                found.push(Label { name: name.into(), at: *at, constant: true });
            }
        }
    }
    found
}

/// Resolve `1f` / `1b` style numeric references from `from`.
fn numeric_target(all: &[Label], name: &str, from: usize) -> Option<usize> {
    let (digits, dir) = name.split_at(name.len().checked_sub(1)?);
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut same = all.iter().filter(|l| l.name == digits);
    match dir {
        "f" => same.find(|l| l.at > from).map(|l| l.at),
        "b" => same.rfind(|l| l.at <= from).map(|l| l.at),
        _ => None,
    }
}

/// Where the reference `name` at `from` is defined.
pub fn resolve(all: &[Label], name: &str, from: usize) -> Option<usize> {
    numeric_target(all, name, from).or_else(|| all.iter().find(|l| l.name == name).map(|l| l.at))
}

fn range(text: &str, span: Span<usize>) -> Range {
    Range { start: text::position(text, span.start), end: text::position(text, span.end) }
}

pub fn line_range(text: &str, line: usize) -> Range {
    let start = line_starts(text).nth(line).unwrap_or(text.len());
    let raw = &text[start..text[start..].find('\n').map_or(text.len(), |n| start + n)];
    let lead = raw.len() - raw.trim_start().len();
    range(text, start + lead..start + raw.trim_end().len().max(lead))
}

fn line_start(code: &str, at: usize) -> usize {
    code[..at].rfind('\n').map_or(0, |n| n + 1)
}

fn in_comment(text: &str, code: &str, at: usize) -> bool {
    let at = at.min(text.len());
    // Blanked bytes that were not blank in the source are comments or strings.
    let line = line_start(code, at);
    text.as_bytes()[line..at].iter().zip(&code.as_bytes()[line..at]).any(|(a, b)| a != b)
}

pub enum Context<'a> {
    Comment,
    Mnemonic,
    Operand { op: &'a str, index: usize, before: &'a str },
}

pub fn context<'a>(text: &str, code: &'a str, at: usize) -> Context<'a> {
    if in_comment(text, code, at) {
        return Context::Comment;
    }
    let line = parse_line(code, line_start(code, at));
    match line.op {
        Some((op, op_at)) if at > op_at + op.len() => {
            let before = &code[op_at + op.len()..at];
            Context::Operand { op, index: docs::active_operand(before), before }
        }
        _ => Context::Mnemonic,
    }
}

fn number(word: &str) -> Option<u64> {
    let (neg, body) = word.strip_prefix('-').map_or((false, word), |s| (true, s));
    let value = if let Some(h) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()?
    } else if let Some(b) = body.strip_prefix("0b").or_else(|| body.strip_prefix("0B")) {
        u64::from_str_radix(b, 2).ok()?
    } else {
        body.parse().ok()?
    };
    Some(if neg { value.wrapping_neg() } else { value })
}

fn signature_of(f: &Function) -> String {
    match &f.parameters {
        Some(p) => format!("{}({})", f.name, p.join(", ")),
        None => format!("{}()", f.name),
    }
}

pub fn hover(text: &str, at: usize) -> Option<(String, Span<usize>)> {
    let code = blank_comments(text);
    if in_comment(text, &code, at) {
        return None;
    }
    let Some(mut span) = word_at(&code, at) else {
        return None;
    };
    let word = &code[span.clone()];
    if let Some(value) = number(word) {
        if span.start > 0 && code.as_bytes()[span.start - 1] == b'-' {
            span.start -= 1;
        }
        let value = number(&code[span.clone()]).unwrap_or(value);
        return Some((
            format!(
                "`{}` = `{:#x}` = `{:#b}`{}",
                value as i64,
                value,
                value,
                if (value as i64) < 0 { format!(" (unsigned `{value}`)") } else { String::new() }
            ),
            span,
        ));
    }
    if let Some(doc) = docs::register(word) {
        return Some((doc, span));
    }
    if word.starts_with('.') && docs::directive(word).is_some() {
        return Some((format!("`{word}` — {}", docs::directive(word)?), span));
    }
    if let Context::Mnemonic = context(text, &code, span.start)
        && let Some(i) = docs::instruction(word)
    {
        return Some((format!("```riscv\n{word} {}\n```\n{}", i.syntax, i.summary), span));
    }
    let all = labels(&code);
    let target = resolve(&all, word, span.start)?;
    let line = text::position(text, target).line as usize;
    let label = all.iter().find(|l| l.at == target)?;
    let function = functions(text).into_iter().find(|f| f.offset == target);
    let markdown = if let Some(f) = function {
        format!("**function** `{}` — line {}", signature_of(&f), line + 1)
    } else if label.constant {
        let value =
            text[target..].lines().next().unwrap_or("").split_once([',', ' ', '\t']).map(|x| x.1).unwrap_or("").trim();
        format!("**constant** `{}` = `{value}` — line {}", label.name, line + 1)
    } else {
        format!("**label** `{}` — line {}", label.name, line + 1)
    };
    Some((markdown, span))
}

pub fn definition(text: &str, at: usize) -> Option<Span<usize>> {
    let code = blank_comments(text);
    if in_comment(text, &code, at) {
        return None;
    }
    let span = word_at(&code, at)?;
    let word = &code[span.clone()];
    let all = labels(&code);
    let target = resolve(&all, word, span.start)?;
    let name = &all.iter().find(|l| l.at == target)?.name;
    Some(target..target + name.len())
}

/// Whether the first operand of a syntax like `rd, rs1, rs2` is a destination the instruction writes.
fn writes_destination(syntax: &str) -> bool {
    ["rd", "fd", "[rd"].iter().copied().any(|prefix| syntax.starts_with(prefix))
}

/// Every occurrence of the symbol under the caret, and whether it is written there.
pub fn occurrences(text: &str, at: usize) -> Vec<(Span<usize>, bool)> {
    let code = blank_comments(text);
    let Some(span) = word_at(&code, at) else {
        return vec![];
    };
    let word = &code[span.clone()];
    if number(word).is_some() || word.bytes().next().is_some_and(|c| c.is_ascii_digit()) {
        return vec![];
    }
    let register = is_register(word);
    let canonical = |w: &str| {
        if register { docs::register(w) } else { Some(w.to_owned()) }
    };
    let wanted = canonical(word);
    let mut found = Vec::new();
    for start in line_starts(&code) {
        let line = parse_line(&code, start);
        for (name, at) in &line.labels {
            if !register && *name == word {
                found.push((*at..*at + name.len(), true));
            }
        }
        let Some((op, _)) = line.op else { continue };
        let writes = docs::instruction(op).is_some_and(|i| writes_destination(i.syntax));
        for (index, (operand, operand_at)) in line.operands.iter().enumerate() {
            let mut i = 0;
            let bytes = operand.as_bytes();
            while i < bytes.len() {
                if !is_word(bytes[i]) {
                    i += 1;
                    continue;
                }
                let s = i;
                while i < bytes.len() && is_word(bytes[i]) {
                    i += 1;
                }
                let token = &operand[s..i];
                if canonical(token) == wanted {
                    let write = register && index == 0 && writes;
                    let constant_def = !register && matches!(op, ".equ" | ".equiv" | ".eqv" | ".set") && index == 0;
                    found.push((operand_at + s..operand_at + i, write || constant_def));
                }
            }
        }
    }
    found
}

pub fn diagnostics(text: &str) -> Vec<Diagnostic> {
    let code = blank_comments(text);
    let mut out = Vec::new();
    let mut lines = HashSet::new();
    let all = labels(&code);
    let names: HashSet<_> = all.iter().map(|l| l.name.as_str()).collect();
    for (number, start) in line_starts(&code).enumerate() {
        let line = parse_line(&code, start);
        let Some((op, op_at)) = line.op else { continue };
        if op.starts_with('.') {
            continue;
        }
        let Some(info) = docs::instruction(op) else {
            if ide_riscv::Program::assemble(&format!(".text\n{op}"))
                .is_err_and(|e| e.message.starts_with("unsupported"))
            {
                let hint = closest(op).map(|s| format!(" — did you mean `{s}`?")).unwrap_or_default();
                out.push(Diagnostic::new(
                    range(text, op_at..op_at + op.len()),
                    Severity::Error,
                    format!("unknown instruction `{op}`{hint}"),
                ));
                lines.insert(number);
            }
            continue;
        };
        let syntax = docs::operands(info.syntax);
        for (index, (operand, at)) in line.operands.iter().enumerate() {
            let expects_label = syntax.get(index).is_some_and(|s| s.contains("label") || *s == "symbol")
                || (syntax.len() == 1 && syntax[0].contains("label"));
            if !expects_label {
                continue;
            }
            let digits = operand.trim_end_matches(['f', 'b']);
            if operand.len() == digits.len() + 1 && !digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit()) {
                if numeric_target(&all, operand, *at).is_none() {
                    out.push(Diagnostic::new(
                        range(text, *at..*at + operand.len()),
                        Severity::Error,
                        format!("no local label for `{operand}`"),
                    ));
                    lines.insert(number);
                }
                continue;
            }
            if valid_name(operand) && !names.contains(operand) && !is_register(operand) {
                let hint = names
                    .iter()
                    .filter(|n| distance(n, operand) <= 2)
                    .min_by_key(|n| distance(n, operand))
                    .map(|s| format!(" — did you mean `{s}`?"))
                    .unwrap_or_default();
                out.push(Diagnostic::new(
                    range(text, *at..*at + operand.len()),
                    Severity::Error,
                    format!("undefined label `{operand}`{hint}"),
                ));
                lines.insert(number);
            }
        }
        if let Some((rd, at)) = line.operands.first()
            && matches!(*rd, "zero" | "x0")
            && (info.syntax.starts_with("rd,") && !matches!(op, "jal" | "jalr"))
        {
            let mut d = Diagnostic::new(
                range(text, *at..*at + rd.len()),
                Severity::Warning,
                format!("`{op}` result is discarded: writes to `{rd}` are ignored"),
            );
            d.unnecessary = true;
            out.push(d);
        }
    }
    if code.trim().is_empty() {
        return out;
    }
    if let Err(error) = ide_riscv::Program::assemble(text) {
        match error.line.map(|n| n - 1) {
            Some(line) if lines.contains(&line) => {}
            Some(line) => out.push(Diagnostic::new(line_range(text, line), Severity::Error, error.message)),
            None if error.message == "no executable instructions" => out.push(Diagnostic::new(
                line_range(text, 0),
                Severity::Info,
                "no instructions yet: add a `.text` label such as `main:`",
            )),
            None => out.push(Diagnostic::new(line_range(text, 0), Severity::Error, error.message)),
        }
    }
    for d in &mut out {
        d.source = Some("RISC-V".into());
    }
    out
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

fn closest(op: &str) -> Option<&'static str> {
    crate::language::INSTRUCTIONS
        .split_whitespace()
        .filter(|s| distance(s, op) <= if op.len() > 3 { 2 } else { 1 })
        .min_by_key(|s| distance(s, op))
}

pub fn span_range(text: &str, span: Span<usize>) -> Range {
    range(text, span)
}
