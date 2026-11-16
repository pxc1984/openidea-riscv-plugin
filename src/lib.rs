mod docs;
mod editor;
mod format;
mod language;
mod update;

use editor::Context;
use language::{blank_comments, functions, highlights};
use openidea_plugin::diagnostics::DiagnosticsProvider;
use openidea_plugin::hover::{Hover, HoverProvider};
use openidea_plugin::language_features::{
    Document, FormatOptions, GutterAction, Highlight as Occurrence, LanguageFeatures, Parameter, ParameterLabel,
    Signature, SignatureHelp,
};
use openidea_plugin::providers::{CompletionItem, CompletionKind, Providers};
use openidea_plugin::syntax::{Fold, FoldKind, Symbol, SymbolKind, SyntaxProvider, Token};
use openidea_plugin::{
    Diagnostic, Error, Invocation, Location, PickItem, Plugin, Position, Range, TextEdit, host, register_plugin, text,
};

#[derive(Default)]
pub struct Riscv;

/// The terminal title and simulator argv of the last run, separated by U+001F.
const LAST_RUN: &str = "last-run";
const SIMULATOR: &str = "openidea-riscv";
const MAX_STEPS: &str = "10000000";

/// Runs the simulator in a terminal tab. Not through a host run configuration: those are offered only
/// when a literal marker file exists in the project root, which an arbitrary assembly project lacks.
fn run_simulator(title: &str, argv: &[String]) -> Result<Option<String>, Error> {
    let last = std::iter::once(title).chain(argv.iter().map(String::as_str)).collect::<Vec<_>>();
    host::storage::set(LAST_RUN, &last.join("\u{1f}"))?;
    host::open_terminal(title, SIMULATOR, argv)?;
    Ok(None)
}

fn arguments(text: &str, count: Option<usize>) -> Result<Vec<String>, Error> {
    let args: Vec<_> = text.split(|c: char| c.is_whitespace() || c == ',').filter(|s| !s.is_empty()).collect();
    if count.is_some_and(|n| n != args.len()) {
        return Err(Error::Invalid(format!("Expected {} integer arguments", count.unwrap())));
    }
    for arg in &args {
        let unsigned = arg.strip_prefix(['+', '-']).unwrap_or(arg);
        let (digits, radix) = if let Some(s) = unsigned.strip_prefix("0x").or_else(|| unsigned.strip_prefix("0X")) {
            (s, 16)
        } else if let Some(s) = unsigned.strip_prefix("0b").or_else(|| unsigned.strip_prefix("0B")) {
            (s, 2)
        } else if unsigned.starts_with('0') && unsigned.len() > 1 {
            (unsigned, 8)
        } else {
            (unsigned, 10)
        };
        let valid_digits = !digits.is_empty() && digits.chars().all(|c| c.is_digit(radix));
        let value = u64::from_str_radix(digits, radix).ok();
        if !valid_digits || value.is_none() || (arg.starts_with('-') && value.unwrap() > (1u64 << 63)) {
            return Err(Error::Invalid("Enter 64-bit integers in decimal, hexadecimal, binary or octal".into()));
        }
    }
    Ok(args.into_iter().map(str::to_owned).collect())
}

impl Plugin for Riscv {
    fn activate(&mut self, ctx: &openidea_plugin::Context) -> Result<(), Error> {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        // Проверка обновлений не должна мешать активации: без сети или разрешения просто молчим.
        if let Err(error) = update::check(&ctx.plugin_version, now) {
            host::log(openidea_plugin::LogLevel::Debug, &format!("update check: {error}"));
        }
        Ok(())
    }

    fn run_command(&mut self, id: &str, ctx: &Invocation, arg: Option<&str>) -> Result<Option<String>, Error> {
        if id == update::COMMAND {
            host::open_url(update::DOWNLOAD_URL)?;
            return Ok(None);
        }
        if !matches!(id, "run" | "run-at-caret" | "rerun") {
            return Err(Error::NotFound(id.into()));
        }
        // Хост гасит ошибку команды из иконки в гуттере (только console.warn), поэтому сообщаем сами,
        // иначе неудачный запуск выглядит как "ничего не произошло".
        launch(id, ctx, arg).or_else(|error| {
            host::notify(openidea_plugin::NotificationLevel::Error, "RISC-V", &format!("Run failed: {error}"));
            Ok(None)
        })
    }
}

fn launch(id: &str, ctx: &Invocation, arg: Option<&str>) -> Result<Option<String>, Error> {
    if id == "rerun"
        && let Some(last) = host::storage::get(LAST_RUN)?
    {
        let mut parts = last.split('\u{1f}').map(str::to_owned);
        let title = parts.next().unwrap_or_default();
        return run_simulator(&title, &parts.collect::<Vec<_>>());
    }
    let editor = ctx.editor().ok_or_else(|| Error::Invalid("Open a RISC-V assembly file".into()))?;
    if editor.language() != "riscv" {
        return Err(Error::Invalid("Open a RISC-V assembly file".into()));
    }
    let path = editor.path();
    if host::open_editors()?.iter().any(|e| e.path == path && e.dirty) {
        return Err(Error::Invalid("Save the file first: the simulator runs it from disk".into()));
    }
    let source = editor.document()?.text;
    let found = functions(&source);
    let caret = editor.selections()?.first().and_then(|r| text::offset(&source, r.start));
    let at_caret = (id == "run-at-caret").then(|| caret.and_then(|c| found.iter().rfind(|f| f.offset <= c))).flatten();
    let selected = if let Some(function) = at_caret {
        Some(function)
    } else if let Some(name) = arg {
        Some(found.iter().find(|f| f.name == name).ok_or_else(|| Error::NotFound(name.into()))?)
    } else {
        let items: Vec<_> = std::iter::once(PickItem::new("", "Run program"))
            .chain(found.iter().map(|f| {
                let signature = f.parameters.as_ref().map_or(String::new(), |p| p.join(", "));
                PickItem::new(&f.name, &format!("{}({signature})", f.name)).description(&format!("line {}", f.line + 1))
            }))
            .collect();
        let Some(name) = host::pick("RISC-V: Run File or Function", &items)? else {
            return Ok(None);
        };
        found.iter().find(|f| f.name == name)
    };
    let mut args = vec![path.clone(), "--max-steps".into(), MAX_STEPS.into()];
    let file_name = path.rsplit(['/', '\\']).next().unwrap_or(&path);
    let mut title = format!("RISC-V: {file_name}");
    if let Some(function) = selected {
        args.extend(["--function".into(), function.name.clone()]);
        let key = format!("arguments:{path}:{}", function.name);
        let remembered = host::storage::get(&key)?.unwrap_or_default();
        let prompt = match &function.parameters {
            Some(p) => format!("Arguments for {}({})", function.name, p.join(", ")),
            None => format!("Integer arguments for {} (a0–a7, then stack)", function.name),
        };
        let mut initial = remembered;
        loop {
            let Some(input) = host::input(&prompt, Some(&initial))? else {
                return Ok(None);
            };
            match arguments(&input, function.parameters.as_ref().map(Vec::len)) {
                Ok(values) => {
                    host::storage::set(&key, &input)?;
                    title = format!("RISC-V: {}({})", function.name, values.join(", "));
                    args.push("--".into());
                    args.extend(values);
                    break;
                }
                Err(error) => {
                    host::notify(openidea_plugin::NotificationLevel::Warning, "RISC-V", &error.to_string());
                    initial = input;
                }
            }
        }
    }
    run_simulator(&title, &args)
}

/// The last line of function `i`: before the next function, without the blank lines and the
/// next function's signature comment in between.
fn last_code_line(found: &[language::Function], i: usize, lines: &[&str]) -> usize {
    let start = found[i].line as usize;
    let mut end = found.get(i + 1).map_or(lines.len(), |n| n.line as usize).saturating_sub(1);
    while end > start
        && lines.get(end).is_none_or(|l| {
            let l = l.trim();
            l.is_empty() || l.starts_with('#') || l.starts_with("//")
        })
    {
        end -= 1;
    }
    end
}

impl SyntaxProvider for Riscv {
    fn highlights(&mut self, _: &str, text: &str) -> Vec<Token> {
        highlights(text)
    }
    fn outline(&mut self, _: &str, text: &str) -> Vec<Symbol> {
        let found = functions(text);
        let lines: Vec<_> = text.split_inclusive('\n').collect();
        found
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let last = last_code_line(&found, i, &lines);
                let end = lines[..=last].iter().map(|l| l.len()).sum::<usize>();
                Symbol {
                    name: f.name.clone(),
                    detail: f.parameters.as_ref().map(|p| p.join(", ")),
                    kind: SymbolKind::Function,
                    range: f.offset..lines.get(last).map_or(end, |l| end - (l.len() - l.trim_end().len())),
                    name_range: f.offset..f.offset + f.name.len(),
                    parent: None,
                }
            })
            .collect()
    }
    fn folding(&mut self, _: &str, text: &str) -> Vec<Fold> {
        let found = functions(text);
        let lines: Vec<_> = text.split_inclusive('\n').collect();
        found
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                let end = last_code_line(&found, i, &lines);
                (end > f.line as usize).then_some(Fold {
                    start_line: f.line,
                    end_line: end as u32,
                    kind: FoldKind::Region,
                })
            })
            .collect()
    }
}

fn item(label: &str, kind: CompletionKind, detail: Option<String>, doc: Option<String>, sort: &str) -> CompletionItem {
    let mut item = CompletionItem::new(label, kind);
    item.detail = detail;
    item.documentation = doc;
    item.sort_text = Some(format!("{sort}{label}"));
    item
}

fn registers() -> impl Iterator<Item = String> {
    language::REGISTERS
        .split_whitespace()
        .map(str::to_owned)
        .chain(["x", "f"].into_iter().flat_map(|p| (0..32).map(move |n| format!("{p}{n}"))))
}

impl Providers for Riscv {
    fn completion(&mut self, doc: &Document, at: Position, _: Option<&str>) -> Vec<CompletionItem> {
        let Some(offset) = text::offset(&doc.text, at) else {
            return vec![];
        };
        let code = blank_comments(&doc.text);
        let prefix_start = editor::word_at(&code, offset).map_or(offset, |w| w.start.min(offset));
        let replace = Some(editor::span_range(&doc.text, prefix_start..offset));
        let mut items = Vec::new();
        match editor::context(&doc.text, &code, offset) {
            Context::Comment => return vec![],
            Context::Mnemonic => {
                for op in language::INSTRUCTIONS.split_whitespace() {
                    let info = docs::instruction(op);
                    let mut it = item(
                        op,
                        CompletionKind::Keyword,
                        info.as_ref().map(|i| i.syntax.to_owned()),
                        info.map(|i| i.summary),
                        "1",
                    );
                    it.insert_text = format!("{op} ");
                    items.push(it);
                }
                for d in language::DIRECTIVES.split_whitespace() {
                    items.push(item(d, CompletionKind::Keyword, None, docs::directive(d).map(str::to_owned), "2"));
                }
                let mut function = item(
                    "function",
                    CompletionKind::Snippet,
                    Some("exported function with C signature".into()),
                    None,
                    "3",
                );
                function.insert_text = "        .globl ${1:name}\n# int64_t ${1:name}(${2:int64_t a})\n${1:name}:\n        $0\n        ret".into();
                function.snippet = true;
                items.push(function);
                let mut syscall =
                    item("ecall exit", CompletionKind::Snippet, Some("exit(a0) via RARS service 10".into()), None, "3");
                syscall.insert_text = "li a7, 10\necall".into();
                syscall.filter_text = Some("ecall exit".into());
                items.push(syscall);
            }
            Context::Operand { op, index, before } => {
                let info = docs::instruction(op);
                let slot = info
                    .as_ref()
                    .and_then(|i| docs::operands(i.syntax).get(index).map(|s| s.to_string()))
                    .unwrap_or_default();
                let wants_label = slot.contains("label") || slot == "symbol" || op.starts_with('.');
                let float = slot.starts_with('f');
                if op.starts_with('.') && !matches!(op, ".word" | ".dword" | ".globl" | ".global" | ".type" | ".size") {
                    return vec![];
                }
                if !op.starts_with('.') {
                    let after_paren = before.rfind('(').is_some_and(|p| !before[p..].contains(')'));
                    for r in registers() {
                        let is_float = r.starts_with('f') && r != "fp";
                        let rank = if after_paren {
                            if r == "sp" { "0" } else { "1" }
                        } else if wants_label || is_float != float {
                            "5"
                        } else {
                            "1"
                        };
                        items.push(item(
                            &r,
                            CompletionKind::Variable,
                            docs::register(&r).map(|d| d.replace("**", "")),
                            None,
                            rank,
                        ));
                    }
                }
                let functions = functions(&doc.text);
                let mut seen = std::collections::HashSet::new();
                for label in editor::labels(&code) {
                    if editor::valid_name(&label.name) && seen.insert(label.name.clone()) {
                        let f = functions.iter().find(|f| f.name == label.name);
                        let kind = if label.constant {
                            CompletionKind::Constant
                        } else if f.is_some() {
                            CompletionKind::Function
                        } else {
                            CompletionKind::Reference
                        };
                        let detail = f.map(|f| {
                            f.parameters.as_ref().map_or("function".into(), |p| format!("({})", p.join(", ")))
                        });
                        items.push(item(&label.name, kind, detail, None, if wants_label { "0" } else { "3" }));
                    }
                }
            }
        }
        for it in &mut items {
            it.replace = replace;
        }
        items
    }

    fn definition(&mut self, doc: &Document, at: Position) -> Vec<Location> {
        text::offset(&doc.text, at)
            .and_then(|o| editor::definition(&doc.text, o))
            .map(|span| Location { path: doc.path.clone(), range: editor::span_range(&doc.text, span) })
            .into_iter()
            .collect()
    }
}

impl HoverProvider for Riscv {
    fn hover(&mut self, doc: &Document, at: Position) -> Option<Hover> {
        let (markdown, span) = editor::hover(&doc.text, text::offset(&doc.text, at)?)?;
        Some(Hover::new(markdown).range(editor::span_range(&doc.text, span)))
    }
}

impl DiagnosticsProvider for Riscv {
    fn diagnostics(&mut self, doc: &Document) -> Vec<Diagnostic> {
        editor::diagnostics(&doc.text)
    }
}

impl LanguageFeatures for Riscv {
    fn format(&mut self, doc: &Document, range: Option<Range>, options: FormatOptions) -> Result<Vec<TextEdit>, Error> {
        let lines = range.map_or(0..usize::MAX, |r| r.start.line as usize..r.end.line as usize + 1);
        let indent = format::Indent { tab_size: options.tab_size.max(1) as usize, tabs: !options.insert_spaces };
        Ok(format::edits(&doc.text, lines, &indent)
            .into_iter()
            .map(|(span, text)| TextEdit { range: editor::span_range(&doc.text, span), text })
            .collect())
    }

    fn signature_help(&mut self, doc: &Document, at: Position) -> Option<SignatureHelp> {
        let offset = text::offset(&doc.text, at)?;
        let code = blank_comments(&doc.text);
        let Context::Operand { op, index, .. } = editor::context(&doc.text, &code, offset) else {
            return None;
        };
        let info = docs::instruction(op)?;
        let label = format!("{op} {}", info.syntax);
        let mut parameters = Vec::new();
        let mut from = op.len() + 1;
        for part in docs::operands(info.syntax) {
            let start = label[from..].find(part)? + from;
            from = start + part.len();
            let utf16 = |i: usize| label[..i].encode_utf16().count() as u32;
            parameters
                .push(Parameter { label: ParameterLabel::Offsets((utf16(start), utf16(from))), documentation: None });
        }
        let active = (index as u32).min(parameters.len().saturating_sub(1) as u32);
        Some(SignatureHelp {
            signatures: vec![Signature { label, documentation: Some(info.summary), parameters }],
            active_signature: 0,
            active_parameter: active,
        })
    }

    fn document_highlights(&mut self, doc: &Document, at: Position) -> Vec<Occurrence> {
        let Some(offset) = text::offset(&doc.text, at) else {
            return vec![];
        };
        editor::occurrences(&doc.text, offset)
            .into_iter()
            .map(|(span, write)| Occurrence {
                range: editor::span_range(&doc.text, span),
                kind: if write { "write" } else { "read" }.into(),
            })
            .collect()
    }

    fn gutter_actions(&mut self, doc: &Document) -> Vec<GutterAction> {
        functions(&doc.text)
            .into_iter()
            .map(|f| GutterAction {
                line: f.line,
                icon: Some("run".into()),
                tooltip: format!("Run {}()", f.name),
                command: "run".into(),
                argument: Some(f.name),
            })
            .collect()
    }
}

register_plugin!(Riscv, syntax, providers, hover, diagnostics, language_features);

#[cfg(test)]
mod tests;
