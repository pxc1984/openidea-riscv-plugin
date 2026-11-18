use super::*;
use openidea_plugin::mock::MockHost;
use openidea_plugin::syntax::Highlight;

const SOURCE: &str = ".data\nmsg: .string \"#fake:\"\n.text\n.globl sum, poly\n# int64_t sum(int64_t a, int64_t b)\nsum: add a0, a0, a1\nret\n// int64_t poly(int64_t x)\npoly: mul a0, a0, a0\nret\n";

#[test]
fn discovers_functions_signatures_and_text_sections() {
    let found = functions(SOURCE);
    assert_eq!(found.iter().map(|f| (f.name.as_str(), f.line)).collect::<Vec<_>>(), [("sum", 5), ("poly", 8)]);
    assert_eq!(found[0].parameters.as_ref().unwrap(), &["int64_t a", "int64_t b"]);
    assert_eq!(functions("/*\n.globl fake\nfake: ret\n*/\n.text\nactual: ret")[0].name, "actual");
    assert_eq!(functions(".data\ndata: .word 1\n.section .text.foo\nmain: ret")[0].name, "main");
    assert_eq!(functions("# f(void)\nf: ret")[0].parameters, Some(vec![]));
}

#[test]
fn highlights_comments_strings_registers_and_instructions_on_utf8_boundaries() {
    let text = "# 中文 😀\nmain: li x10, 0x2a\n.asciz \"hello #世界\"\n/* open";
    let tokens = highlights(text);
    for (needle, expected) in [
        ("# 中文", Highlight::Comment),
        ("main", Highlight::Label),
        ("li", Highlight::Keyword),
        ("x10", Highlight::Variable),
        ("0x2a", Highlight::Number),
        ("\"hello", Highlight::String),
        ("/* open", Highlight::Comment),
    ] {
        let at = text.find(needle).unwrap();
        assert_eq!(tokens.iter().find(|t| t.range.start == at).unwrap().kind, expected);
    }
    assert!(tokens.windows(2).all(|w| w[0].range.end <= w[1].range.start));
    assert!(tokens.iter().all(|t| text.is_char_boundary(t.range.start) && text.is_char_boundary(t.range.end)));
}

#[test]
fn validates_integer_ranges_bases_and_arity() {
    assert_eq!(arguments("-9223372036854775808, 0xffffffffffffffff 0b10 046", Some(4)).unwrap().len(), 4);
    for invalid in ["18446744073709551616", "-9223372036854775809", "1.5", "1; echo hi", "08", "--1", "0x", "1+2"] {
        assert!(arguments(invalid, None).is_err(), "{invalid}");
    }
    assert!(arguments("1", Some(2)).is_err());
    assert!(arguments("", Some(0)).unwrap().is_empty());
}

const LOOP: &str = "        .text\n        .globl count\n        .equ LIMIT, 10\n# int64_t count(int64_t n)\ncount:  li      t0, 0\n1:      addi    t0, t0, 1\n        blt     t0, a0, 1b\n        beqz    a0, done\n        mv      a0, t0 # result\ndone:   ret\n";

fn doc(text: &str) -> Document {
    Document { path: "/p/a.s".into(), language: "riscv".into(), version: 1, text: text.into() }
}

fn at(text: &str, needle: &str, delta: usize) -> Position {
    text::position(text, text.find(needle).unwrap() + delta)
}

#[test]
fn hover_explains_instructions_registers_numbers_and_labels() {
    let mut plugin = Riscv;
    let hover =
        |plugin: &mut Riscv, needle, delta| plugin.hover(&doc(LOOP), at(LOOP, needle, delta)).map(|h| h.markdown);
    assert!(hover(&mut plugin, "blt", 1).unwrap().contains("rs1, rs2, label"));
    assert!(hover(&mut plugin, "t0, 0", 0).unwrap().contains("`x5`"));
    assert!(hover(&mut plugin, "LIMIT, 10", 8).unwrap().contains("0xa"));
    assert!(hover(&mut plugin, "done\n", 0).unwrap().contains("label"));
    assert!(hover(&mut plugin, "a0, done", 5).unwrap().contains("line 10"));
    assert!(hover(&mut plugin, "1b", 0).unwrap().contains("line 6"));
    assert!(hover(&mut plugin, "count(", 0).is_none(), "comments have no hover");
    let text = "main: li a0, -1\n";
    assert!(plugin.hover(&doc(text), at(text, "1", 0)).unwrap().markdown.contains("0xffffffffffffffff"));
}

#[test]
fn definition_and_highlights_follow_labels_and_registers() {
    let mut plugin = Riscv;
    let def = plugin.definition(&doc(LOOP), at(LOOP, "a0, done", 5));
    assert_eq!(def[0].range.start, at(LOOP, "done:", 0));
    let local = plugin.definition(&doc(LOOP), at(LOOP, "1b", 0));
    assert_eq!(local[0].range.start, at(LOOP, "1:", 0));
    let marks = plugin.document_highlights(&doc(LOOP), at(LOOP, "t0, 0", 0));
    assert_eq!(marks.len(), 5);
    assert_eq!(marks.iter().filter(|m| m.kind == "write").count(), 2);
    assert!(plugin.document_highlights(&doc(LOOP), at(LOOP, "# result", 3)).is_empty());
}

#[test]
fn diagnostics_report_assembler_errors_unknown_labels_and_typos() {
    assert!(editor::diagnostics(LOOP).is_empty(), "{:?}", editor::diagnostics(LOOP));
    let bad = "main: addd a0, a0, a1\n  beqz a0, dnoe\n  j 2f\ndone: ret\n";
    let found = editor::diagnostics(bad);
    let messages: Vec<_> = found.iter().map(|d| (d.range.start.line, d.message.as_str())).collect();
    assert!(messages.contains(&(0, "unknown instruction `addd` — did you mean `add`?")), "{messages:?}");
    assert!(messages.contains(&(1, "undefined label `dnoe` — did you mean `done`?")), "{messages:?}");
    assert!(messages.contains(&(2, "no local label for `2f`")), "{messages:?}");
    let arity = editor::diagnostics("main: add a0, a1\n");
    assert_eq!(arity[0].message, "wrong number of operands for add");
    assert_eq!(arity[0].range, editor::line_range("main: add a0, a1\n", 0));
    let discard = editor::diagnostics("main: addi zero, a0, 1\n ret\n");
    assert!(discard[0].unnecessary && discard[0].severity == openidea_plugin::Severity::Warning);
    assert!(editor::diagnostics("").is_empty());
    assert!(editor::diagnostics("# todo\n").is_empty());
    assert_eq!(editor::diagnostics(".data\nx: .word 1\n")[0].severity, openidea_plugin::Severity::Info);
}

#[test]
fn completion_depends_on_position() {
    let mut plugin = Riscv;
    let labels = |items: &[CompletionItem]| items.iter().map(|i| i.label.clone()).collect::<Vec<_>>();
    let text = "count: ad\n";
    let items = plugin.completion(&doc(text), at(text, "ad", 2), None);
    assert!(labels(&items).contains(&"addi".into()) && !labels(&items).contains(&"a0".into()));
    assert_eq!(items.iter().find(|i| i.label == "addi").unwrap().detail.as_deref(), Some("rd, rs1, imm12"));
    let text = "count: beqz a0, \ndone: ret\n";
    let items = plugin.completion(&doc(text), at(text, ", \n", 2), None);
    let best = items.iter().min_by_key(|i| i.sort_text.clone()).unwrap();
    assert!(matches!(best.label.as_str(), "count" | "done"), "{}", best.label);
    assert!(labels(&items).contains(&"a0".into()));
    let text = "main: ld a0, 8(\n";
    let items = plugin.completion(&doc(text), at(text, "(\n", 1), None);
    assert_eq!(items.iter().min_by_key(|i| i.sort_text.clone()).unwrap().label, "sp");
    let text = "main: ret # a\n";
    assert!(plugin.completion(&doc(text), at(text, "a\n", 1), None).is_empty());
}

#[test]
fn signature_help_tracks_the_operand() {
    let mut plugin = Riscv;
    let text = "main: sd ra, 8(sp\n";
    let help = plugin.signature_help(&doc(text), at(text, "sp\n", 2)).unwrap();
    assert_eq!(help.signatures[0].label, "sd rs2, offset(rs1)");
    assert_eq!(help.active_parameter, 1);
    assert_eq!(help.signatures[0].parameters[1].label, ParameterLabel::Offsets((8, 19)));
    assert!(plugin.signature_help(&doc(text), at(text, "sd", 1)).is_none());
}

#[test]
fn functions_fold_without_the_next_signature() {
    let mut plugin = Riscv;
    let text = include_str!("../examples/riscv-app/functions.asm");
    let folds = SyntaxProvider::folding(&mut plugin, "riscv", text);
    assert_eq!(folds.iter().map(|f| (f.start_line, f.end_line)).collect::<Vec<_>>(), [(4, 5), (8, 9), (12, 22)]);
}

#[test]
fn examples_are_clean_and_large_files_are_fast() {
    for text in [include_str!("../examples/riscv-app/functions.asm"), include_str!("../examples/riscv-app/hello.asm")] {
        assert!(editor::diagnostics(text).is_empty(), "{:?}", editor::diagnostics(text));
    }
    let big = (0..3000).map(|i| format!("l{i}: addi a0, a0, 1 # step\n  bnez a0, l{i}\n")).collect::<String>();
    let start = std::time::Instant::now();
    editor::diagnostics(&big);
    Riscv.completion(&doc(&big), Position { line: 10, character: 20 }, None);
    assert!(start.elapsed().as_millis() < if cfg!(debug_assertions) { 3000 } else { 100 }, "{:?}", start.elapsed());
}

#[test]
fn outline_ranges_end_at_the_last_instruction() {
    let text = include_str!("../examples/riscv-app/functions.asm");
    let outline = Riscv.outline("riscv", text);
    let sum = &text[outline[0].range.clone()];
    assert!(sum.starts_with("sum:") && sum.ends_with("ret"), "{sum:?}");
    assert!(text[outline[2].range.clone()].ends_with("ret"));
}

/// The completion snippets for teaching patterns arrive as real snippets in mnemonic context.
#[test]
fn completion_offers_teaching_snippets_in_mnemonic_context() {
    let mut plugin = Riscv;
    let text = "main: \n";
    let items = plugin.completion(&doc(text), at(text, ": \n", 2), None);
    for (label, detail) in [
        ("test main", Some("main reading test arguments")),
        ("branchless select", Some("a0 = cond ? a : b")),
        ("branchless min", Some("a0 = min(a2, a3)")),
        ("loop", Some("counted loop")),
        ("sum array", Some("sum of a .word array")),
        ("struct", Some("fields of struct")),
        ("data", Some(".data section")),
    ] {
        let it = items.iter().find(|i| i.label == label).unwrap_or_else(|| panic!("no `{label}` snippet"));
        assert_eq!(it.kind, Some(CompletionKind::Snippet), "{label}");
        assert!(it.snippet, "{label}");
        assert_eq!(it.sort_text.as_deref(), Some(format!("3{label}").as_str()), "{label}");
        assert!(it.detail.as_deref().unwrap_or_default().starts_with(detail.unwrap()), "{label}");
        assert!(it.insert_text.starts_with("        "), "{label}: {:?}", it.insert_text);
        assert!(it.insert_text.contains("$0"), "final tab stop in {label}");
        assert!(it.insert_text.contains("${1:"), "tab stops in {label}");
    }
}

#[test]
fn run_tests_prompts_remembers_the_directory_and_passes_it_to_the_simulator() {
    let (mock, mut plugin) = setup();
    mock.answer_next_prompt(Some(&["/project/tests"]));
    assert_eq!(plugin.run_command("run-tests", &mock.invocation(), None).unwrap(), None);
    let runs = mock.terminals();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].title, "RISC-V: a b; c.asm (tests)");
    assert_eq!(runs[0].program, "openidea-riscv");
    assert_eq!(runs[0].args, ["/project/a b; c.asm".to_owned(), "--tests".into(), "/project/tests".into()]);
    assert_eq!(host::storage::get("tests:/project/a b; c.asm").unwrap().as_deref(), Some("/project/tests"));

    // Dismissing the prompt cancels without a terminal and without a failure notification.
    mock.answer_next_prompt(None);
    assert!(plugin.run_command("run-tests", &mock.invocation(), None).unwrap().is_none());
    assert_eq!(mock.terminals().len(), 1);
    assert!(mock.notifications().is_empty(), "{:?}", mock.notifications());
}

/// The snippets expand into real, assembling programs: placeholders become their defaults.
#[test]
fn snippets_assemble_in_the_simulator() {
    let expand = |text: &str| {
        let mut out = String::new();
        let mut rest = text;
        while let Some(at) = rest.find("${") {
            out.push_str(&rest[..at]);
            let (body, tail) = rest[at + 2..].split_once('}').unwrap();
            out.push_str(body.split_once(':').map_or(body, |(_, d)| d));
            rest = tail;
        }
        out + &rest.replace("$0", "")
    };
    for (label, _, insert) in snippets() {
        let expanded = expand(insert);
        match label {
            // Data alone has no code yet; the simulator reports that as a normal empty program.
            "data" => {
                assert!(
                    ide_riscv::Program::assemble(&expanded).is_err_and(|e| e.message == "no executable instructions"),
                    "{label}: {expanded:?}"
                );
                assert!(ide_riscv::Program::assemble(&format!("{expanded}        .text\nmain:   ret")).is_ok());
            }
            _ => assert!(ide_riscv::Program::assemble(&expanded).is_ok(), "{label}: {expanded:?}"),
        }
    }
    // The `test main` loop really reads arguments until -1, calls and prints: run it end to end.
    let source = expand(snippets().find(|(l, ..)| *l == "test main").unwrap().2)
        .replace("        call    name", "        call    double")
        + "# int64_t double(int64_t a)\ndouble: add     a0, a0, a0\n        ret\n";
    let program = ide_riscv::Program::assemble(&source).unwrap();
    let mut machine = ide_riscv::Machine::new(program, None, &[]).unwrap();
    let mut output = Vec::new();
    // Two full test cases (one line per argument), then EOF reads as -1 and main exits.
    let input = "3\n-2\n7\n5\n0\n1\n".to_owned().into_bytes();
    let outcome = machine.run(10_000, &mut std::io::Cursor::new(input), &mut output, &mut Vec::new()).unwrap();
    assert!(!outcome.returned && outcome.exit_code == 0 && output == b"6\n10\n", "{outcome:?} {output:?}");
}

const CAPABILITIES: &[&str] = &["editor.read", "storage", "process:openidea-riscv"];

fn setup() -> (MockHost, Riscv) {
    let mock = MockHost::install().with_capabilities(CAPABILITIES);
    let path = "/project/a b; c.asm";
    mock.add_file(path, SOURCE);
    mock.open_editor(path, "riscv", SOURCE, &[]);
    (mock, Riscv)
}

fn simulator_argv(path: &str, run: &[&str]) -> Vec<String> {
    [path, "--max-steps", "10000000"].iter().chain(run).map(|s| s.to_string()).collect()
}

#[test]
fn gutter_run_prompts_remembers_and_preserves_argv() {
    let (mock, mut plugin) = setup();
    mock.answer_next_prompt(Some(&["40 2"]));
    assert_eq!(plugin.run_command("run", &mock.invocation(), Some("sum")).unwrap(), None);
    let runs = mock.terminals();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].title, "RISC-V: sum(40, 2)");
    assert_eq!(runs[0].program, "openidea-riscv");
    assert_eq!(runs[0].args, simulator_argv("/project/a b; c.asm", &["--function", "sum", "--", "40", "2"]));
    assert_eq!(host::storage::get("arguments:/project/a b; c.asm:sum").unwrap().as_deref(), Some("40 2"));
    let markers = plugin.gutter_actions(&Document { text: SOURCE.into(), ..Default::default() });
    assert_eq!(
        markers.iter().map(|m| (m.line, m.argument.as_deref())).collect::<Vec<_>>(),
        [(5, Some("sum")), (8, Some("poly"))]
    );
}

#[test]
fn cancellation_and_bad_arity_do_not_launch() {
    let (mock, mut plugin) = setup();
    mock.answer_next_prompt(None);
    assert!(plugin.run_command("run", &mock.invocation(), Some("sum")).unwrap().is_none());
    mock.answer_next_prompt(Some(&["1"]));
    mock.answer_next_prompt(None);
    assert!(plugin.run_command("run", &mock.invocation(), Some("sum")).unwrap().is_none());
    assert!(mock.terminals().is_empty());
    assert_eq!(mock.notifications().len(), 1);
}

#[test]
fn program_runs_without_function_arguments_and_grants_are_enforced() {
    let (mock, mut plugin) = setup();
    mock.answer_next_prompt(Some(&[""]));
    plugin.run_command("run", &mock.invocation(), None).unwrap();
    assert_eq!(mock.terminals()[0].title, "RISC-V: a b; c.asm");
    assert_eq!(mock.terminals()[0].args, simulator_argv("/project/a b; c.asm", &[]));
    let mock = mock.with_capabilities(&["editor.read", "storage"]);
    mock.answer_next_prompt(Some(&["1 2"]));
    assert_eq!(plugin.run_command("run", &mock.invocation(), Some("sum")).unwrap(), None);
    assert_eq!(mock.terminals().len(), 1);
    assert!(mock.notifications().last().unwrap().body.starts_with("Run failed:"));
}

/// Регрессия: `.globl sum` с сигнатурой-комментарием на той же строке. Запуск не должен зависеть
/// от run-конфигураций хоста (их нет без маркер-файла в корне проекта -> "no owned run configuration").
#[test]
fn gutter_run_works_without_host_run_configurations() {
    let source = "        .globl  sum         # int64_t aplusb(int64_t a, int64_t b);\nsum:    add     a0, a0, a1\n        ret\n";
    let mock = MockHost::install().with_capabilities(CAPABILITIES);
    mock.add_file("/project/a.asm", source);
    mock.open_editor("/project/a.asm", "riscv", source, &[]);
    let mut plugin = Riscv;
    let action = plugin.gutter_actions(&doc(source)).pop().unwrap();
    assert_eq!((action.line, action.argument.as_deref()), (1, Some("sum")));
    mock.answer_next_prompt(Some(&["1 2"]));
    plugin.run_command(&action.command, &mock.invocation(), action.argument.as_deref()).unwrap();
    assert_eq!(mock.terminals()[0].args, simulator_argv("/project/a.asm", &["--function", "sum", "--", "1", "2"]));
    assert!(mock.notifications().is_empty(), "{:?}", mock.notifications());
}

#[test]
fn unsaved_file_is_reported_instead_of_running_stale_code() {
    let (mock, mut plugin) = setup();
    mock.type_in_editor("\n");
    assert_eq!(plugin.run_command("run", &mock.invocation(), Some("sum")).unwrap(), None);
    assert!(mock.terminals().is_empty());
    assert!(mock.notifications()[0].body.contains("Save the file first"), "{:?}", mock.notifications());
}

#[test]
fn run_at_caret_uses_the_enclosing_function_and_rerun_repeats_without_prompts() {
    let mock = MockHost::install().with_capabilities(CAPABILITIES);
    let path = "/project/f.asm";
    mock.add_file(path, SOURCE);
    // Caret on `mul a0, a0, a0` inside poly (line 8).
    mock.open_editor(path, "riscv", SOURCE, &[(8, 8, 8, 8)]);
    let mut plugin = Riscv;
    mock.answer_next_prompt(Some(&["7"]));
    plugin.run_command("run-at-caret", &mock.invocation(), None).unwrap();
    assert_eq!(mock.terminals()[0].args, simulator_argv(path, &["--function", "poly", "--", "7"]));
    plugin.run_command("rerun", &mock.invocation(), None).unwrap();
    let runs = mock.terminals();
    assert_eq!(runs.len(), 2, "rerun asks nothing");
    assert_eq!(runs[1], runs[0]);
}

/// Полный путь клика по иконке запуска у `.globl`-функции: gutter action -> команда плагина ->
/// argv терминала -> симулятор, исполняющий именно эту функцию.
#[test]
fn gutter_run_icon_executes_the_exported_function_in_the_simulator() {
    let source = include_str!("../examples/riscv-app/functions.asm");
    let path = "/project/functions.asm";
    // Мок-проект виртуальный, а симулятор читает файл с диска: кладём копию во временный файл.
    let disk = std::env::temp_dir().join(format!("riscv-gutter-{}.asm", std::process::id()));
    std::fs::write(&disk, source).unwrap();
    let mock = MockHost::install().with_capabilities(CAPABILITIES);
    mock.add_file(path, source);
    mock.open_editor(path, "riscv", source, &[]);
    let mut plugin = Riscv;

    let actions = plugin.gutter_actions(&Document { path: path.into(), text: source.into(), ..Default::default() });
    assert_eq!(
        actions.iter().map(|a| (a.line, a.argument.as_deref(), a.tooltip.as_str())).collect::<Vec<_>>(),
        [(4, Some("sum"), "Run sum()"), (8, Some("sum32"), "Run sum32()"), (12, Some("poly"), "Run poly()")]
    );
    let manifest = include_str!("../plugin.toml");
    for (action, (input, expected)) in
        actions.iter().zip([("41, 1", 42u64), ("0x7fffffff 1", 0xffff_ffff_8000_0000), ("2 3 4 5 6", 569)])
    {
        assert_eq!(action.icon.as_deref(), Some("run"));
        assert!(manifest.contains(&format!("id = \"{}\"", action.command)), "{} is not contributed", action.command);
        mock.answer_next_prompt(Some(&[input]));
        plugin.run_command(&action.command, &mock.invocation(), action.argument.as_deref()).unwrap();
        let run = mock.terminals().pop().expect("the icon starts the simulator");
        assert_eq!(run.program, "openidea-riscv");
        let mut argv = run.args.clone();
        assert_eq!(argv[0], path);
        argv[0] = disk.to_str().unwrap().into();
        assert_eq!(ide_riscv::cli(argv), 0, "simulator rejects {:?}", run.args);
        let function = &run.args[run.args.iter().position(|a| a == "--function").unwrap() + 1];
        assert_eq!(Some(function.as_str()), action.argument.as_deref());
        let values: Vec<u64> =
            run.args.iter().skip_while(|a| *a != "--").skip(1).map(|v| ide_riscv::integer(v).unwrap()).collect();
        let program = ide_riscv::Program::assemble(source).unwrap();
        let mut machine = ide_riscv::Machine::new(program, Some(function), &values).unwrap();
        let outcome =
            machine.run(10_000, &mut std::io::Cursor::new(Vec::new()), &mut Vec::new(), &mut Vec::new()).unwrap();
        assert!(outcome.returned);
        assert_eq!(outcome.value, expected, "{function}({input})");
    }
    assert_eq!(mock.terminals().len(), 3);
    assert!(mock.notifications().is_empty(), "{:?}", mock.notifications());
    std::fs::remove_file(&disk).ok();
}

fn reformat(text: &str, range: Option<openidea_plugin::Range>, insert_spaces: bool) -> String {
    let options = FormatOptions { tab_size: 8, insert_spaces };
    let mut edits = Riscv.format(&doc(text), range, options).unwrap();
    edits.sort_by_key(|e| std::cmp::Reverse(text::offset(text, e.range.start).unwrap()));
    let mut out = text.to_owned();
    for e in edits {
        out.replace_range(
            text::offset(text, e.range.start).unwrap()..text::offset(text, e.range.end).unwrap(),
            &e.text,
        );
    }
    out
}

#[test]
fn formatter_aligns_labels_mnemonics_and_operands() {
    for (input, expected) in [
        ("sum: add a0,a0,a1\n", "sum:    add     a0, a0, a1\n"),
        ("  ret  \n", "        ret\n"),
        (".text\n", "        .text\n"),
        (".globl   sum,poly\n", "        .globl sum, poly\n"),
        ("message: .asciz \"a, b: #c\"\n", "message: .asciz \"a, b: #c\"\n"),
        ("very_long_label: li a0,1\n", "very_long_label: li a0, 1\n"),
        ("fcvt.l.d a0,ft0\n", "        fcvt.l.d a0, ft0\n"),
        ("ld a0 , 8( sp )\n", "        ld      a0, 8( sp )\n"),
        ("1:addi t0,t0,1  # step\n", "1:      addi    t0, t0, 1 # step\n"),
        ("done:\n", "done:\n"),
        ("a: b: ret\n", "a: b:   ret\n"),
        (".byte ',', 'x'\n", "        .byte ',', 'x'\n"),
        ("   # int64_t f(void)   \n", "   # int64_t f(void)\n"),
        ("/* li a0,1\n  still comment */\n", "/* li a0,1\n  still comment */\n"),
        ("li /* keep */ a0,1\n", "li /* keep */ a0,1\n"),
        ("li a0,1\r\nret\r\n", "        li      a0, 1\r\n        ret\r\n"),
        ("\n\n", "\n\n"),
        ("ret", "        ret"),
    ] {
        assert_eq!(reformat(input, None, true), expected, "{input:?}");
        assert_eq!(reformat(expected, None, true), expected, "not idempotent: {expected:?}");
    }
}

#[test]
fn formatter_uses_tabs_when_the_editor_does() {
    assert_eq!(reformat("sum: add a0,a0,a1\n.text\n", None, false), "sum:\tadd\ta0, a0, a1\n\t.text\n");
    assert_eq!(reformat("fcvt.l.d a0,ft0\n", None, false), "\tfcvt.l.d\ta0, ft0\n");
}

#[test]
fn formatter_keeps_examples_and_formats_only_the_selection() {
    for text in [include_str!("../examples/riscv-app/functions.asm"), include_str!("../examples/riscv-app/hello.asm")] {
        assert_eq!(reformat(text, None, true), text);
    }
    let text = "li a0,1\nli a1,2\nli a2,3\n";
    let second = openidea_plugin::Range { start: at(text, "li a1", 0), end: at(text, "li a1", 3) };
    assert_eq!(reformat(text, Some(second), true), "li a0,1\n        li      a1, 2\nli a2,3\n");
}

#[test]
fn formatted_code_computes_the_same_results() {
    let messy = "  .text\n.globl sum,poly\n# int64_t sum(int64_t a, int64_t b)\nsum:add a0,a0,a1\n ret\n// int64_t poly(int64_t x)\npoly: mul a0 ,a0,a0 # square\n ret\n";
    let formatted = reformat(messy, None, false);
    assert_ne!(formatted, messy);
    for (function, args, expected) in [("sum", [40, 2], 42), ("poly", [7, 0], 49)] {
        let run = |source: &str| {
            let program = ide_riscv::Program::assemble(source).unwrap();
            let mut machine = ide_riscv::Machine::new(program, Some(function), &args).unwrap();
            machine.run(1000, &mut std::io::Cursor::new(Vec::new()), &mut Vec::new(), &mut Vec::new()).unwrap().value
        };
        assert_eq!((run(messy), run(&formatted)), (expected, expected), "{function}");
    }
    let names = |text: &str| functions(text).into_iter().map(|f| (f.name, f.line, f.parameters)).collect::<Vec<_>>();
    assert_eq!(names(&formatted), names(messy));
}

#[test]
fn formatter_aligns_trailing_comments_of_consecutive_lines() {
    for (input, spaces, tabs) in [
        (
            "li a0,1 # one\naddi a0,a0,100 # add\nret # back\n",
            "        li      a0, 1       # one\n        addi    a0, a0, 100 # add\n        ret                 # back\n",
            "\tli\ta0, 1\t\t# one\n\taddi\ta0, a0, 100\t# add\n\tret\t\t\t# back\n",
        ),
        (
            "li a0,1 # a\nret\nmv a0,a1 # b\n",
            "        li      a0, 1 # a\n        ret\n        mv      a0, a1 # b\n",
            "\tli\ta0, 1\t# a\n\tret\n\tmv\ta0, a1\t# b\n",
        ),
        (
            "li a0,1 # a\n\nla a0,message # b\n",
            "        li      a0, 1 # a\n\n        la      a0, message # b\n",
            "\tli\ta0, 1\t# a\n\n\tla\ta0, message\t# b\n",
        ),
        (
            "li a0,1 # a\n# only\nli a1,22 # b\n",
            "        li      a0, 1 # a\n# only\n        li      a1, 22 # b\n",
            "\tli\ta0, 1\t# a\n# only\n\tli\ta1, 22\t# b\n",
        ),
        (
            "done: # end\nli /* x */ a0,1 # kept\nret # r\n",
            "done: # end\nli /* x */ a0,1 # kept\n        ret # r\n",
            "done:\t# end\nli /* x */ a0,1 # kept\n\tret\t# r\n",
        ),
    ] {
        assert_eq!(reformat(input, None, true), spaces, "{input:?}");
        assert_eq!(reformat(spaces, None, true), spaces, "not idempotent: {spaces:?}");
        assert_eq!(reformat(input, None, false), tabs, "{input:?}");
        assert_eq!(reformat(tabs, None, false), tabs, "not idempotent: {tabs:?}");
    }
    let text = "li a0,1 # a\naddi a0,a0,100 # b\n";
    let first = openidea_plugin::Range { start: at(text, "li", 0), end: at(text, "li", 2) };
    assert_eq!(reformat(text, Some(first), true), "        li      a0, 1       # a\naddi a0,a0,100 # b\n");
}

#[test]
fn release_tags_compare_as_versions() {
    for (tag, installed, newer) in [
        ("v0.1.6", "0.1.5", true),
        ("v0.2.0", "0.1.9", true),
        ("v0.1.10", "0.1.9", true),
        ("v1.0.0", "0.99.99", true),
        ("v0.1.5", "0.1.5", false),
        ("v0.1.4", "0.1.5", false),
        ("0.1.6", "0.1.5", true),
        ("latest", "0.1.5", false),
        ("v0.1.6-rc1", "0.1.5", false),
        ("v0.1", "0.1.5", false),
        ("v0.1.6.1", "0.1.5", false),
    ] {
        assert_eq!(update::is_newer(tag, installed), newer, "{tag} vs {installed}");
    }
}

/// The mock host and the number of requests to GitHub so far.
fn github(tag: &'static str) -> (MockHost, std::rc::Rc<std::cell::Cell<u32>>) {
    let mock = MockHost::install().with_capabilities(&["storage", "net:api.github.com"]);
    let fetches = std::rc::Rc::new(std::cell::Cell::new(0));
    let counter = fetches.clone();
    mock.on_fetch(move |request| {
        counter.set(counter.get() + 1);
        assert!(request.url.ends_with("/repos/pxc1984/openidea-riscv-plugin/releases/latest"), "{}", request.url);
        assert!(request.headers.iter().any(|(k, _)| k == "User-Agent"));
        Ok(openidea_plugin::HttpResponse {
            status: 200,
            headers: vec![],
            body: format!("{{\n  \"url\": \"x\",\n  \"tag_name\": \"{tag}\",\n  \"draft\": false\n}}").into_bytes(),
        })
    });
    (mock, fetches)
}

#[test]
fn newer_release_is_offered_once_per_interval_and_downloads_the_package() {
    let (mock, fetches) = github("v9.0.0");
    update::check("0.1.5", 1_000).unwrap();
    let offers = mock.notifications_with_actions();
    assert_eq!(offers.len(), 1);
    assert!(offers[0].notification.body.contains("v9.0.0") && offers[0].notification.body.contains("0.1.5"));
    let action = &offers[0].actions[0];
    assert!(include_str!("../plugin.toml").contains(&format!("id = \"{}\"", action.command)));
    plugin_run(&mock, &action.command);
    assert_eq!(mock.opened_urls(), [update::DOWNLOAD_URL]);

    update::check("0.1.5", 1_000 + update::CHECK_INTERVAL_SECS - 1).unwrap();
    assert_eq!((fetches.get(), mock.notifications_with_actions().len()), (1, 1), "re-activation in one session");
    update::check("0.1.5", 1_000 + update::CHECK_INTERVAL_SECS).unwrap();
    assert_eq!((fetches.get(), mock.notifications_with_actions().len()), (2, 2), "next start offers again");
}

fn plugin_run(mock: &MockHost, command: &str) {
    assert_eq!(Riscv.run_command(command, &mock.invocation(), None).unwrap(), None);
}

#[test]
fn current_or_unreadable_releases_and_refused_network_stay_silent() {
    for tag in ["v0.1.5", "v0.1.4", "nightly"] {
        let (mock, fetches) = github(tag);
        update::check("0.1.5", 1_000).unwrap();
        assert_eq!(fetches.get(), 1);
        assert!(mock.notifications_with_actions().is_empty() && mock.notifications().is_empty(), "{tag}");
    }
    let mock = MockHost::install().with_capabilities(&["storage", "net:api.github.com"]);
    mock.on_fetch(|_| Ok(openidea_plugin::HttpResponse { status: 404, headers: vec![], body: b"{}".to_vec() }));
    update::check("0.1.5", 1_000).unwrap();
    assert!(mock.notifications_with_actions().is_empty());

    let mock = MockHost::install().with_capabilities(&["storage"]);
    assert!(update::check("0.1.5", 1_000).is_err());
    let context = openidea_plugin::Context { plugin_version: "0.1.5".into(), ..Default::default() };
    Riscv.activate(&context).expect("activation survives a failed check");
    assert!(mock.notifications().is_empty() && mock.notifications_with_actions().is_empty());
}
