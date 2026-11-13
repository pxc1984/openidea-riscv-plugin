mod language;

use language::{functions, highlights};
use openidea_plugin::language_features::{Document, GutterAction, LanguageFeatures};
use openidea_plugin::providers::{CompletionItem, CompletionKind, Providers};
use openidea_plugin::syntax::{Fold, Symbol, SymbolKind, SyntaxProvider, Token};
use openidea_plugin::{
    Error, Invocation, PickItem, Plugin, PluginRunConfiguration, RunConfigurations, host, register_plugin,
};

#[derive(Default)]
pub struct Riscv;

fn config_id(path: &str) -> String {
    let hash = path.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3));
    format!("file-{hash:016x}")
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
    fn run_command(&mut self, id: &str, ctx: &Invocation, arg: Option<&str>) -> Result<Option<String>, Error> {
        if id != "run" {
            return Err(Error::NotFound(id.into()));
        }
        let editor = ctx.editor().ok_or_else(|| Error::Invalid("Open a RISC-V assembly file".into()))?;
        if editor.language() != "riscv" {
            return Err(Error::Invalid("Open a RISC-V assembly file".into()));
        }
        let path = editor.path();
        let source = editor.document()?.text;
        let found = functions(&source);
        let selected = if let Some(name) = arg {
            Some(found.iter().find(|f| f.name == name).ok_or_else(|| Error::NotFound(name.into()))?)
        } else {
            let items: Vec<_> = std::iter::once(PickItem::new("", "Run program"))
                .chain(found.iter().map(|f| PickItem::new(&f.name, &format!("{}()", f.name))))
                .collect();
            let Some(name) = host::pick("RISC-V: Run File or Function", &items)? else {
                return Ok(None);
            };
            found.iter().find(|f| f.name == name)
        };
        let mut args = Vec::new();
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
        host::start_run(&config_id(&path), &args, false).map(Some)
    }
}

impl RunConfigurations for Riscv {
    fn configurations(&mut self, root: &str) -> Vec<PluginRunConfiguration> {
        let Some(project) = host::project_of(root) else {
            return vec![];
        };
        let mut paths = Vec::new();
        for pattern in ["**/*.asm", "**/*.s", "**/*.S"] {
            paths.extend(project.find_files(pattern, 1000).unwrap_or_default());
        }
        paths.sort();
        paths.dedup();
        paths
            .into_iter()
            .take(1000)
            .map(|path| PluginRunConfiguration {
                id: config_id(&path),
                name: format!("RISC-V: {}", path.strip_prefix(root).unwrap_or(&path).trim_start_matches('/')),
                program: "openidea-riscv".into(),
                args: vec![path, "--max-steps".into(), "10000000".into()],
                cwd: Some(root.into()),
                ..Default::default()
            })
            .collect()
    }
}

impl SyntaxProvider for Riscv {
    fn highlights(&mut self, _: &str, text: &str) -> Vec<Token> {
        highlights(text)
    }
    fn outline(&mut self, _: &str, text: &str) -> Vec<Symbol> {
        let found = functions(text);
        found
            .iter()
            .enumerate()
            .map(|(i, f)| Symbol {
                name: f.name.clone(),
                detail: f.parameters.as_ref().map(|p| p.join(", ")),
                kind: SymbolKind::Function,
                range: f.offset..found.get(i + 1).map_or(text.len(), |f| f.offset),
                name_range: f.offset..f.offset + f.name.len(),
                parent: None,
            })
            .collect()
    }
    fn folding(&mut self, _: &str, _: &str) -> Vec<Fold> {
        vec![]
    }
}

impl Providers for Riscv {
    fn completion(&mut self, _: &Document, _: openidea_plugin::Position, _: Option<&str>) -> Vec<CompletionItem> {
        let mut items: Vec<_> = language::words()
            .into_iter()
            .map(|(word, register)| {
                CompletionItem::new(word, if register { CompletionKind::Variable } else { CompletionKind::Keyword })
            })
            .collect();
        for prefix in ['x', 'f'] {
            for n in 0..32 {
                items.push(CompletionItem::new(&format!("{prefix}{n}"), CompletionKind::Variable));
            }
        }
        items
    }
}

impl LanguageFeatures for Riscv {
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

register_plugin!(Riscv, syntax, providers, language_features, run_configurations);

#[cfg(test)]
mod tests;
