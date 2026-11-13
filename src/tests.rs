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
    assert!(plugin.run_command("run", &mock.invocation(), Some("sum")).is_err());
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
