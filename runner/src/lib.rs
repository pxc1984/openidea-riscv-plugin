//! RV64 source simulator for standalone teaching programs and integer ABI function calls.
mod asm;
mod cpu;
mod expr;

use std::fmt;
use std::io::{self, Cursor, Write};
use std::path::PathBuf;

pub use asm::Program;
pub use cpu::{Machine, Outcome};
pub type Result<T> = std::result::Result<T, Error>;

const DEFAULT_MAX_STEPS: u64 = 10_000_000;

#[derive(Debug, Clone)]
pub struct Error {
    pub line: Option<usize>,
    pub message: String,
}
impl Error {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self { line: None, message: message.into() }
    }
    pub(crate) fn at(mut self, line: usize) -> Self {
        if self.line.is_none() {
            self.line = Some(line);
        }
        self
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(line) = self.line {
            write!(f, "{line}: ")?;
        }
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}

pub fn integer(text: &str) -> Result<u64> {
    expr::eval(text, &|name| Err(Error::new(format!("expected integer, found '{name}'"))))
}

/// Standalone simulator entry point.
pub fn cli(args: Vec<String>) -> i32 {
    let options = match parse_options(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("RISC-V: {error}");
            return 2;
        }
    };
    if let Some(tests_dir) = &options.tests {
        return match run_test_suite(&options.file, tests_dir, options.max_steps) {
            Ok((code, report)) => {
                print!("{report}");
                io::stdout().flush().ok();
                code
            }
            Err(error) => {
                eprintln!("{}:{error}", options.file);
                1
            }
        };
    }
    match run_file(&options.file, options.function.as_deref(), &options.values, options.max_steps) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{}:{error}", options.file);
            1
        }
    }
}

struct Options {
    file: String,
    function: Option<String>,
    values: Vec<u64>,
    max_steps: u64,
    tests: Option<String>,
}

fn parse_options(args: Vec<String>) -> Result<Options> {
    let mut file = None;
    let mut function = None;
    let mut values = Vec::new();
    let mut max_steps = DEFAULT_MAX_STEPS;
    let mut tests = None;
    let mut iter = args.into_iter();
    // Lookahead-разбор опций: итератор читается и в заголовке, и внутри веток.
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--function" => function = Some(iter.next().ok_or_else(|| Error::new("--function requires a label"))?),
            "--tests" => tests = Some(iter.next().ok_or_else(|| Error::new("--tests requires a directory"))?),
            "--max-steps" => {
                max_steps = iter
                    .next()
                    .and_then(|s| s.parse().ok())
                    .filter(|n| *n > 0)
                    .ok_or_else(|| Error::new("--max-steps requires a positive integer"))?
            }
            "--" => {
                for value in iter.by_ref() {
                    values.push(integer(&value)?);
                }
                break;
            }
            _ if file.is_none() && !arg.starts_with('-') => file = Some(arg),
            _ => return Err(Error::new(format!("unknown option: {arg}"))),
        }
    }
    let file = file.ok_or_else(|| {
        Error::new(
            "usage: openidea-riscv <file.asm> [--function label] [--max-steps N] [--tests DIR] [-- integer arguments]",
        )
    })?;
    Ok(Options { file, function, values, max_steps, tests })
}

/// Расширения файлов тестов: `NN.dat` — вход, `NN.ans` — эталонный stdout.
const TEST_INPUT_EXT: &str = "dat";
const TEST_ANSWER_EXT: &str = "ans";
const MISSING_LINE: &str = "<no line>";

/// Прогон программы по каталогу тестов `NN.dat`/`NN.ans` (вход `_start`/`main`).
/// Возвращает (код выхода: 0 — все прошли, 1 — есть FAIL, отчёт).
pub fn run_test_suite(file: &str, tests_dir: &str, max_steps: u64) -> Result<(i32, String)> {
    let text = std::fs::read_to_string(file).map_err(|e| Error::new(e.to_string()))?;
    let program = Program::assemble(&text)?;
    let names = test_names(tests_dir)?;
    let mut passed = 0usize;
    let mut report = String::new();
    for name in &names {
        match run_single_test(&program, tests_dir, name, max_steps) {
            Ok(None) => {
                passed += 1;
                report.push_str(&format!("PASS {name}\n"));
            }
            Ok(Some(reason)) => report.push_str(&format!("FAIL {name}{reason}\n")),
            Err(error) => return Err(error),
        }
    }
    report.push_str(&format!("{passed}/{} tests passed\n", names.len()));
    Ok((i32::from(passed < names.len()), report))
}

/// Имена тестов в каталоге: стемы файлов `*.dat`, отсортированные.
fn test_names(tests_dir: &str) -> Result<Vec<String>> {
    let entries = std::fs::read_dir(tests_dir).map_err(|e| Error::new(e.to_string()))?;
    let mut names = Vec::new();
    for entry in entries {
        let path = entry.map_err(|e| Error::new(e.to_string()))?.path();
        if path.extension().and_then(|e| e.to_str()) == Some(TEST_INPUT_EXT) {
            let stem = path
                .file_stem()
                .and_then(|e| e.to_str())
                .ok_or_else(|| Error::new(format!("invalid test name: {}", path.display())))?;
            names.push(stem.to_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// Запуск одного теста: None — вывод совпал, Some — краткое описание расхождения.
fn run_single_test(program: &Program, tests_dir: &str, name: &str, max_steps: u64) -> Result<Option<String>> {
    let input_path = PathBuf::from(tests_dir).join(format!("{name}.{TEST_INPUT_EXT}"));
    let answer_path = PathBuf::from(tests_dir).join(format!("{name}.{TEST_ANSWER_EXT}"));
    let input = std::fs::read(&input_path).map_err(|e| Error::new(format!("{}: {e}", input_path.display())))?;
    let expected =
        std::fs::read_to_string(&answer_path).map_err(|e| Error::new(format!("{}: {e}", answer_path.display())))?;
    let mut machine = Machine::new(program.clone(), None, &[])?;
    let mut output = Vec::new();
    match machine.run(max_steps, &mut Cursor::new(input), &mut output, &mut io::sink()) {
        Ok(_) => Ok(first_mismatch(&expected, &String::from_utf8_lossy(&output))),
        Err(error) => Ok(Some(format!(": runtime error: {error}"))),
    }
}

/// Первое расхождение построчно (обе стороны триммируются); None — совпало.
fn first_mismatch(expected: &str, got: &str) -> Option<String> {
    let expected_lines: Vec<&str> = expected.lines().map(str::trim).collect();
    let got_lines: Vec<&str> = got.lines().map(str::trim).collect();
    for index in 0..expected_lines.len().max(got_lines.len()) {
        let expected = expected_lines.get(index).copied().unwrap_or(MISSING_LINE);
        let got = got_lines.get(index).copied().unwrap_or(MISSING_LINE);
        if expected != got {
            return Some(format!(": line {}: expected '{expected}', got '{got}'", index + 1));
        }
    }
    None
}

fn run_file(file: &str, function: Option<&str>, values: &[u64], max_steps: u64) -> Result<i32> {
    let text = std::fs::read_to_string(file).map_err(|e| Error::new(e.to_string()))?;
    let program = Program::assemble(&text)?;
    let mut machine = Machine::new(program, function, values)?;
    let outcome =
        machine.run(max_steps, &mut io::stdin().lock(), &mut io::stdout().lock(), &mut io::stderr().lock())?;
    if outcome.returned {
        println!(
            "Execution Done: {}({}) = {}",
            machine.entry(),
            values.iter().map(|v| (*v as i64).to_string()).collect::<Vec<_>>().join(", "),
            outcome.value as i64
        );
    }
    io::stdout().flush().ok();
    Ok(outcome.exit_code)
}
