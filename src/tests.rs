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
