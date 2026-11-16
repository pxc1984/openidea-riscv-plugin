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

fn setup() -> (MockHost, Riscv) {
    let mock =
        MockHost::install().with_capabilities(&["project.read", "editor.read", "storage", "process:openidea-riscv"]);
    let path = "/project/a b; c.asm";
    mock.add_file(path, SOURCE);
    mock.open_editor(path, "riscv", SOURCE, &[]);
    let mut plugin = Riscv;
    mock.set_run_configurations(plugin.configurations("/project"));
    (mock, plugin)
}

#[test]
fn gutter_run_prompts_remembers_and_preserves_argv() {
    let (mock, mut plugin) = setup();
    mock.answer_next_prompt(Some(&["40 2"]));
    plugin.run_command("run", &mock.invocation(), Some("sum")).unwrap();
    let runs = mock.configuration_runs();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].2, ["--function", "sum", "--", "40", "2"]);
    assert_eq!(host::storage::get("arguments:/project/a b; c.asm:sum").unwrap().as_deref(), Some("40 2"));
    let configs = plugin.configurations("/project");
    assert_eq!(configs[0].args[0], "/project/a b; c.asm");
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
    assert!(mock.configuration_runs().is_empty());
    assert_eq!(mock.notifications().len(), 1);
}

#[test]
fn program_runs_without_function_arguments_and_grants_are_enforced() {
    let (mock, mut plugin) = setup();
    mock.answer_next_prompt(Some(&[""]));
    plugin.run_command("run", &mock.invocation(), None).unwrap();
    assert!(mock.configuration_runs()[0].2.is_empty());
    let mock = mock.with_capabilities(&["project.read", "editor.read", "storage"]);
    mock.answer_next_prompt(Some(&["1 2"]));
    assert_eq!(plugin.run_command("run", &mock.invocation(), Some("sum")).unwrap(), None);
    assert_eq!(mock.configuration_runs().len(), 1);
    assert!(mock.notifications().last().unwrap().body.starts_with("Run failed:"));
}

/// Иконка в гуттере у `.globl` с сигнатурой-комментарием на той же строке. Если хост не может
/// запустить (нет конфигурации, проект не доверен), гуттер глотает ошибку команды,
/// поэтому плагин обязан показать её уведомлением.
#[test]
fn gutter_run_failures_are_reported_instead_of_silently_dropped() {
    let source = "        .globl  sum         # int64_t aplusb(int64_t a, int64_t b);\nsum:    add     a0, a0, a1\n        ret\n";
    let mock =
        MockHost::install().with_capabilities(&["project.read", "editor.read", "storage", "process:openidea-riscv"]);
    mock.add_file("/project/a.asm", source);
    mock.open_editor("/project/a.asm", "riscv", source, &[]);
    let mut plugin = Riscv;
    let action = plugin.gutter_actions(&doc(source)).pop().unwrap();
    assert_eq!((action.line, action.argument.as_deref()), (1, Some("sum")));

    mock.answer_next_prompt(Some(&["1 2"]));
    assert_eq!(plugin.run_command(&action.command, &mock.invocation(), action.argument.as_deref()).unwrap(), None);
    assert!(mock.configuration_runs().is_empty());
    let shown = mock.notifications();
    assert_eq!(shown.len(), 1);
    assert!(shown[0].body.starts_with("Run failed:"), "{:?}", shown[0].body);

    mock.set_run_configurations(plugin.configurations("/project"));
    mock.answer_next_prompt(Some(&["1 2"]));
    plugin.run_command(&action.command, &mock.invocation(), action.argument.as_deref()).unwrap();
    assert_eq!(mock.configuration_runs()[0].2, ["--function", "sum", "--", "1", "2"]);
    assert_eq!(mock.notifications().len(), 1);
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
fn run_at_caret_uses_the_enclosing_function_and_rerun_repeats_without_prompts() {
    let mock =
        MockHost::install().with_capabilities(&["project.read", "editor.read", "storage", "process:openidea-riscv"]);
    let path = "/project/f.asm";
    mock.add_file(path, SOURCE);
    // Caret on `mul a0, a0, a0` inside poly (line 8).
    mock.open_editor(path, "riscv", SOURCE, &[(8, 8, 8, 8)]);
    let mut plugin = Riscv;
    mock.set_run_configurations(plugin.configurations("/project"));
    mock.answer_next_prompt(Some(&["7"]));
    plugin.run_command("run-at-caret", &mock.invocation(), None).unwrap();
    assert_eq!(mock.configuration_runs()[0].2, ["--function", "poly", "--", "7"]);
    plugin.run_command("rerun", &mock.invocation(), None).unwrap();
    let runs = mock.configuration_runs();
    assert_eq!(runs.len(), 2, "rerun asks nothing");
    assert_eq!(runs[1].1, runs[0].1);
    assert_eq!(runs[1].2, runs[0].2);
}

#[test]
fn outline_ranges_end_at_the_last_instruction() {
    let text = include_str!("../examples/riscv-app/functions.asm");
    let outline = Riscv.outline("riscv", text);
    let sum = &text[outline[0].range.clone()];
    assert!(sum.starts_with("sum:") && sum.ends_with("ret"), "{sum:?}");
    assert!(text[outline[2].range.clone()].ends_with("ret"));
}

/// Полный путь клика по иконке запуска у `.globl`-функции: gutter action -> команда плагина ->
/// argv, который хост добавляет к конфигурации -> симулятор, исполняющий именно эту функцию.
#[test]
fn gutter_run_icon_executes_the_exported_function_in_the_simulator() {
    let source = include_str!("../examples/riscv-app/functions.asm");
    let path = "/project/functions.asm".to_owned();
    // Мок-проект виртуальный, а симулятор читает файл с диска: кладём копию во временный файл.
    let disk = std::env::temp_dir().join(format!("riscv-gutter-{}.asm", std::process::id()));
    std::fs::write(&disk, source).unwrap();
    let mock =
        MockHost::install().with_capabilities(&["project.read", "editor.read", "storage", "process:openidea-riscv"]);
    mock.add_file(&path, source);
    mock.open_editor(&path, "riscv", source, &[]);
    let mut plugin = Riscv;
    let configs: Vec<_> = plugin.configurations("/project").into_iter().filter(|c| c.args[0] == path).collect();
    mock.set_run_configurations(configs.clone());

    let actions = plugin.gutter_actions(&Document { path: path.clone(), text: source.into(), ..Default::default() });
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
        let (_, config_id, run_args, debug) = mock.configuration_runs().pop().unwrap();
        assert!(!debug);
        let config = configs.iter().find(|c| c.id == config_id).expect("run uses the file's configuration");
        assert_eq!(config.program, "openidea-riscv");
        // Хост запускает program с args конфигурации и дописанными аргументами запуска.
        let mut argv: Vec<String> = config.args.iter().chain(&run_args).cloned().collect();
        assert_eq!(argv[0], path);
        argv[0] = disk.to_str().unwrap().into();
        assert_eq!(ide_riscv::cli(argv), 0, "simulator rejects {:?}", run_args);
        let function = &run_args[run_args.iter().position(|a| a == "--function").unwrap() + 1];
        assert_eq!(Some(function.as_str()), action.argument.as_deref());
        let values: Vec<u64> =
            run_args.iter().skip_while(|a| *a != "--").skip(1).map(|v| ide_riscv::integer(v).unwrap()).collect();
        let program = ide_riscv::Program::assemble(source).unwrap();
        let mut machine = ide_riscv::Machine::new(program, Some(function), &values).unwrap();
        let outcome =
            machine.run(10_000, &mut std::io::Cursor::new(Vec::new()), &mut Vec::new(), &mut Vec::new()).unwrap();
        assert!(outcome.returned);
        assert_eq!(outcome.value, expected, "{function}({input})");
    }
    assert_eq!(mock.configuration_runs().len(), 3);
    std::fs::remove_file(&disk).ok();
}
