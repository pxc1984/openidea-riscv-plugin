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
