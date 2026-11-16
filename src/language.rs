use openidea_plugin::syntax::{Highlight, Token};
use regex::Regex;
use std::{collections::HashSet, sync::OnceLock};

pub const INSTRUCTIONS: &str = "li la lla lui auipc mv nop ret j jr jal jalr call tail
add sub addi addw subw addiw neg negw sext.b sext.h sext.w zext.b zext.h zext.w
sll srl sra slli srli srai sllw srlw sraw slliw srliw sraiw
and or xor andi ori xori not andn orn xnor slt sltu slti sltiu seqz snez sltz sgtz
beq bne blt bge bltu bgeu beqz bnez blez bgez bltz bgtz bgt ble bgtu bleu
lb lbu lh lhu lw lwu ld sb sh sw sd
mul mulw mulh mulhu mulhsu div divu rem remu divw divuw remw remuw
sh1add sh2add sh3add sh1add.uw sh2add.uw sh3add.uw add.uw slli.uw
clz ctz cpop clzw ctzw cpopw min max minu maxu rev8 orc.b rol ror rori rolw rorw roriw
bclr bclri bext bexti binv binvi bset bseti ecall ebreak fence fence.i pause
csrrw csrrs csrrc csrrwi csrrsi csrrci csrr csrw csrs csrc csrwi csrsi csrci rdcycle rdtime rdinstret
flw fld fsw fsd fmv.w.x fmv.x.w fmv.d.x fmv.x.d
fadd.s fsub.s fmul.s fdiv.s fsqrt.s fmin.s fmax.s fsgnj.s fsgnjn.s fsgnjx.s feq.s flt.s fle.s fclass.s
fadd.d fsub.d fmul.d fdiv.d fsqrt.d fmin.d fmax.d fsgnj.d fsgnjn.d fsgnjx.d feq.d flt.d fle.d fclass.d
fmadd.s fmsub.s fnmadd.s fnmsub.s fmadd.d fmsub.d fnmadd.d fnmsub.d
fcvt.s.d fcvt.d.s fcvt.w.s fcvt.wu.s fcvt.l.s fcvt.lu.s fcvt.w.d fcvt.wu.d fcvt.l.d fcvt.lu.d
fcvt.s.w fcvt.s.wu fcvt.s.l fcvt.s.lu fcvt.d.w fcvt.d.wu fcvt.d.l fcvt.d.lu
fmv.s fmv.d fneg.s fneg.d fabs.s fabs.d";
pub const REGISTERS: &str = "zero ra sp gp tp fp t0 t1 t2 s0 s1 a0 a1 a2 a3 a4 a5 a6 a7 s2 s3 s4 s5 s6 s7 s8 s9 s10 s11 t3 t4 t5 t6
ft0 ft1 ft2 ft3 ft4 ft5 ft6 ft7 fs0 fs1 fa0 fa1 fa2 fa3 fa4 fa5 fa6 fa7 fs2 fs3 fs4 fs5 fs6 fs7 fs8 fs9 fs10 fs11 ft8 ft9 ft10 ft11";
pub const DIRECTIVES: &str = ".text .data .bss .section .globl .global .type .size .equ .byte .half .word .dword .float .double .ascii .asciz .string .zero .skip .align";

fn lexer() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?s:/\*.*?(?:\*/|$))|//[^\n]*|\#[^\n]*|"(?:[^"\\]|\\.)*(?:"|$)|'(?:[^'\\]|\\.)*(?:'|$)|[0-9]+[fb]\b|0[xX][0-9a-fA-F]+|0[bB][01]+|[0-9]+(?:\.[0-9]*)?(?:[eE][+-]?[0-9]+)?|[A-Za-z_.$][\w.$]*"#).unwrap())
}

pub fn highlights(text: &str) -> Vec<Token> {
    lexer()
        .find_iter(text)
        .map(|m| {
            let word = m.as_str();
            let kind = if word.starts_with('#') || word.starts_with("//") || word.starts_with("/*") {
                Highlight::Comment
            } else if word.starts_with(['"', '\'']) {
                Highlight::String
            } else if word.as_bytes()[0].is_ascii_digit() {
                if word.ends_with(['f', 'b']) && !word.starts_with("0x") && !word.starts_with("0X") {
                    Highlight::Label
                } else {
                    Highlight::Number
                }
            } else if text[m.end()..].trim_start().starts_with(':') {
                Highlight::Label
            } else if word.starts_with('.') {
                Highlight::Macro
            } else if REGISTERS.split_whitespace().any(|w| w == word)
                || word.strip_prefix(['x', 'f']).and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| n < 32)
            {
                Highlight::Variable
            } else if INSTRUCTIONS.split_whitespace().any(|w| w == word)
                || word.starts_with("amo")
                || word.starts_with("lr.")
                || word.starts_with("sc.")
            {
                Highlight::Keyword
            } else {
                Highlight::Label
            };
            Token { range: m.range(), kind }
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub line: u32,
    pub offset: usize,
    pub parameters: Option<Vec<String>>,
}

/// `text` with comments and string literals replaced by spaces, keeping offsets and newlines.
pub fn blank_comments(text: &str) -> String {
    mask(text, b' ')
}

/// `text` with comments replaced by spaces and string literals by `string_fill`, keeping offsets and newlines.
pub fn mask(text: &str, string_fill: u8) -> String {
    let mut code = text.as_bytes().to_vec();
    for t in highlights(text) {
        let fill = match t.kind {
            Highlight::Comment => b' ',
            Highlight::String => string_fill,
            _ => continue,
        };
        for b in &mut code[t.range] {
            if *b != b'\n' {
                *b = fill;
            }
        }
    }
    String::from_utf8(code).unwrap()
}

pub fn functions(text: &str) -> Vec<Function> {
    let code = blank_comments(text);
    let mut exports = HashSet::new();
    for line in code.lines().map(str::trim) {
        let (directive, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        if matches!(directive, ".globl" | ".global") {
            exports.extend(
                rest.split(|c: char| c.is_whitespace() || c == ',').filter(|s| !s.is_empty()).map(str::to_owned),
            );
        }
        if directive == ".type"
            && let Some((name, ty)) = rest.split_once(',')
            && ty.trim().trim_start_matches(['@', '%']) == "function"
        {
            exports.insert(name.trim().to_owned());
        }
    }
    static LABEL: OnceLock<Regex> = OnceLock::new();
    let label = LABEL.get_or_init(|| Regex::new(r"^\s*([A-Za-z_$][\w.$]*)\s*:").unwrap());
    static SIGNATURE: OnceLock<Regex> = OnceLock::new();
    let signature = SIGNATURE.get_or_init(|| Regex::new(r"([A-Za-z_$][\w.$]*)\s*\(([^)]*)\)\s*;?$").unwrap());
    let original: Vec<_> = text.lines().collect();
    let mut candidates = Vec::new();
    let mut in_text = true;
    let mut offset = 0;
    for (line, raw) in code.split_inclusive('\n').enumerate() {
        let trimmed = raw.trim();
        let directive = trimmed.split_whitespace().next().unwrap_or("");
        if matches!(directive, ".data" | ".bss" | ".rodata") {
            in_text = false;
        }
        if directive == ".text" {
            in_text = true;
        }
        if directive == ".section" {
            let section = trimmed[8..].trim().split([',', ' ', '\t']).next().unwrap_or("");
            in_text = section == ".text" || section.starts_with(".text.");
        }
        if in_text && let Some(c) = label.captures(raw) {
            let name = c.get(1).unwrap();
            let comment = line
                .checked_sub(1)
                .and_then(|n| original.get(n))
                .map(|s| s.trim().trim_start_matches('#').trim_start_matches("//").trim());
            let parameters = comment.and_then(|s| signature.captures(s)).filter(|s| &s[1] == name.as_str()).map(|s| {
                if s[2].trim().is_empty() || s[2].trim() == "void" {
                    vec![]
                } else {
                    s[2].split(',').map(|s| s.trim().to_owned()).collect()
                }
            });
            candidates.push(Function {
                name: name.as_str().into(),
                line: line as u32,
                offset: offset + name.start(),
                parameters,
            });
        }
        offset += raw.len();
    }
    let marked = candidates
        .iter()
        .any(|f| exports.contains(&f.name) || f.parameters.is_some() || matches!(f.name.as_str(), "main" | "_start"));
    if marked {
        candidates.retain(|f| {
            exports.contains(&f.name) || f.parameters.is_some() || matches!(f.name.as_str(), "main" | "_start")
        });
    } else {
        candidates.truncate(1);
    }
    candidates
}
