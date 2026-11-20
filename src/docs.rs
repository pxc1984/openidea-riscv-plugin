//! Reference text for hover, completion details and operand help.
use std::collections::HashMap;
use std::sync::OnceLock;

const TABLE: &str = "
li|rd, imm|Load immediate: rd = imm (any 64-bit constant)
la|rd, symbol|Load address: rd = &symbol
lla|rd, symbol|Load local address: rd = &symbol
lui|rd, imm20|Load upper immediate: rd = imm20 << 12
auipc|rd, imm20|Add upper immediate to pc: rd = pc + (imm20 << 12)
mv|rd, rs|Copy register: rd = rs
nop||No operation
ret||Return from function: jump to ra
j|label|Jump to label
jr|rs|Jump to the address in rs
jal|[rd,] label|Jump and link: rd = pc + 4 (default ra), jump to label
jalr|[rd,] rs[, offset]|Jump and link register: rd = pc + 4, jump to rs + offset
call|label|Call function: ra = return address, jump to label
tail|label|Tail call: jump to label without saving ra
add|rd, rs1, rs2|rd = rs1 + rs2
sub|rd, rs1, rs2|rd = rs1 - rs2
addi|rd, rs1, imm12|rd = rs1 + imm12 (imm12 in -2048..2047)
addw|rd, rs1, rs2|rd = sext32(rs1 + rs2)
subw|rd, rs1, rs2|rd = sext32(rs1 - rs2)
addiw|rd, rs1, imm12|rd = sext32(rs1 + imm12)
neg|rd, rs|rd = -rs
negw|rd, rs|rd = sext32(-rs)
sext.b|rd, rs|Sign-extend the low byte
sext.h|rd, rs|Sign-extend the low 16 bits
sext.w|rd, rs|Sign-extend the low 32 bits
zext.b|rd, rs|Zero-extend the low byte
zext.h|rd, rs|Zero-extend the low 16 bits
zext.w|rd, rs|Zero-extend the low 32 bits
sll|rd, rs1, rs2|Shift left logical: rd = rs1 << rs2[5:0]
srl|rd, rs1, rs2|Shift right logical (unsigned, zero fill): rd = rs1 >>u rs2[5:0]
sra|rd, rs1, rs2|Shift right arithmetic (signed, sign fill): rd = rs1 >>s rs2[5:0]
slli|rd, rs1, shamt|rd = rs1 << shamt (0..63)
srli|rd, rs1, shamt|rd = rs1 >>u shamt (0..63)
srai|rd, rs1, shamt|rd = rs1 >>s shamt (0..63)
sllw|rd, rs1, rs2|32-bit shift left, sign-extended
srlw|rd, rs1, rs2|32-bit logical shift right, sign-extended
sraw|rd, rs1, rs2|32-bit arithmetic shift right, sign-extended
slliw|rd, rs1, shamt|32-bit shift left by shamt (0..31), sign-extended
srliw|rd, rs1, shamt|32-bit logical shift right (zero fill) by shamt (0..31), sign-extended
sraiw|rd, rs1, shamt|32-bit arithmetic shift right (sign fill) by shamt (0..31), sign-extended
and|rd, rs1, rs2|rd = rs1 & rs2
or|rd, rs1, rs2|rd = rs1 | rs2
xor|rd, rs1, rs2|rd = rs1 ^ rs2
andi|rd, rs1, imm12|rd = rs1 & imm12
ori|rd, rs1, imm12|rd = rs1 | imm12
xori|rd, rs1, imm12|rd = rs1 ^ imm12
not|rd, rs|rd = ~rs
andn|rd, rs1, rs2|rd = rs1 & ~rs2 (Zbb)
orn|rd, rs1, rs2|rd = rs1 | ~rs2 (Zbb)
xnor|rd, rs1, rs2|rd = rs1 ^ ~rs2 (Zbb)
slt|rd, rs1, rs2|Set if less than (signed): rd = (rs1 <s rs2) ? 1 : 0
sltu|rd, rs1, rs2|Set if less than (unsigned): rd = (rs1 <u rs2) ? 1 : 0
slti|rd, rs1, imm12|Set if less than immediate (signed): rd = (rs1 <s imm12) ? 1 : 0
sltiu|rd, rs1, imm12|Set if less than immediate (unsigned): rd = (rs1 <u imm12) ? 1 : 0
seqz|rd, rs|Set if equal to zero: rd = (rs == 0) ? 1 : 0
snez|rd, rs|Set if not equal to zero: rd = (rs != 0) ? 1 : 0
sltz|rd, rs|Set if less than zero: rd = (rs < 0) ? 1 : 0
sgtz|rd, rs|Set if greater than zero: rd = (rs > 0) ? 1 : 0
beq|rs1, rs2, label|Branch if equal: rs1 == rs2
bne|rs1, rs2, label|Branch if not equal: rs1 != rs2
blt|rs1, rs2, label|Branch if less than (signed): rs1 <s rs2
bge|rs1, rs2, label|Branch if greater than or equal (signed): rs1 >=s rs2
bltu|rs1, rs2, label|Branch if less than (unsigned): rs1 <u rs2
bgeu|rs1, rs2, label|Branch if greater than or equal (unsigned): rs1 >=u rs2
bgt|rs1, rs2, label|Branch if greater than (signed): rs1 >s rs2
ble|rs1, rs2, label|Branch if less than or equal (signed): rs1 <=s rs2
bgtu|rs1, rs2, label|Branch if greater than (unsigned): rs1 >u rs2
bleu|rs1, rs2, label|Branch if less than or equal (unsigned): rs1 <=u rs2
beqz|rs, label|Branch if equal to zero: rs == 0
bnez|rs, label|Branch if not equal to zero: rs != 0
blez|rs, label|Branch if less than or equal to zero: rs <= 0
bgez|rs, label|Branch if greater than or equal to zero: rs >= 0
bltz|rs, label|Branch if less than zero: rs < 0
bgtz|rs, label|Branch if greater than zero: rs > 0
lb|rd, offset(rs1)|Load signed byte
lbu|rd, offset(rs1)|Load unsigned byte
lh|rd, offset(rs1)|Load signed 16-bit halfword
lhu|rd, offset(rs1)|Load unsigned 16-bit halfword
lw|rd, offset(rs1)|Load signed 32-bit word
lwu|rd, offset(rs1)|Load unsigned 32-bit word
ld|rd, offset(rs1)|Load 64-bit doubleword
sb|rs2, offset(rs1)|Store low byte of rs2
sh|rs2, offset(rs1)|Store low 16 bits of rs2
sw|rs2, offset(rs1)|Store low 32 bits of rs2
sd|rs2, offset(rs1)|Store 64-bit rs2
mul|rd, rs1, rs2|rd = low 64 bits of rs1 * rs2
mulw|rd, rs1, rs2|rd = sext32(rs1 * rs2)
mulh|rd, rs1, rs2|High 64 bits of signed × signed product
mulhu|rd, rs1, rs2|High 64 bits of unsigned × unsigned product
mulhsu|rd, rs1, rs2|High 64 bits of signed × unsigned product
div|rd, rs1, rs2|Signed division (÷0 gives -1)
divu|rd, rs1, rs2|Unsigned division (÷0 gives all ones)
rem|rd, rs1, rs2|Signed remainder (÷0 gives rs1)
remu|rd, rs1, rs2|Unsigned remainder (÷0 gives rs1)
divw|rd, rs1, rs2|32-bit signed division, sign-extended
divuw|rd, rs1, rs2|32-bit unsigned division, sign-extended
remw|rd, rs1, rs2|32-bit signed remainder, sign-extended
remuw|rd, rs1, rs2|32-bit unsigned remainder, sign-extended
sh1add|rd, rs1, rs2|rd = rs2 + (rs1 << 1) (Zba)
sh2add|rd, rs1, rs2|rd = rs2 + (rs1 << 2) (Zba)
sh3add|rd, rs1, rs2|rd = rs2 + (rs1 << 3) (Zba)
sh1add.uw|rd, rs1, rs2|rd = rs2 + (zext32(rs1) << 1) (Zba)
sh2add.uw|rd, rs1, rs2|rd = rs2 + (zext32(rs1) << 2) (Zba)
sh3add.uw|rd, rs1, rs2|rd = rs2 + (zext32(rs1) << 3) (Zba)
add.uw|rd, rs1, rs2|rd = rs2 + zext32(rs1) (Zba)
slli.uw|rd, rs1, shamt|rd = zext32(rs1) << shamt (Zba)
clz|rd, rs|Count leading zero bits (Zbb)
ctz|rd, rs|Count trailing zero bits (Zbb)
cpop|rd, rs|Count set bits (Zbb)
clzw|rd, rs|Count leading zeros of the low 32 bits (Zbb)
ctzw|rd, rs|Count trailing zeros of the low 32 bits (Zbb)
cpopw|rd, rs|Count set bits in the low 32 bits (Zbb)
min|rd, rs1, rs2|Signed minimum (Zbb)
max|rd, rs1, rs2|Signed maximum (Zbb)
minu|rd, rs1, rs2|Unsigned minimum (Zbb)
maxu|rd, rs1, rs2|Unsigned maximum (Zbb)
rev8|rd, rs|Reverse byte order (Zbb)
orc.b|rd, rs|Each byte becomes 0xff if non-zero, else 0x00 (Zbb)
rol|rd, rs1, rs2|Rotate left (Zbb)
ror|rd, rs1, rs2|Rotate right (Zbb)
rori|rd, rs1, shamt|Rotate right by immediate (Zbb)
rolw|rd, rs1, rs2|32-bit rotate left, sign-extended (Zbb)
rorw|rd, rs1, rs2|32-bit rotate right, sign-extended (Zbb)
roriw|rd, rs1, shamt|32-bit rotate right by immediate, sign-extended (Zbb)
bclr|rd, rs1, rs2|Clear bit rs2 of rs1 (Zbs)
bclri|rd, rs1, bit|Clear bit of rs1 (Zbs)
bext|rd, rs1, rs2|Extract bit rs2 of rs1 (Zbs)
bexti|rd, rs1, bit|Extract bit of rs1 (Zbs)
binv|rd, rs1, rs2|Invert bit rs2 of rs1 (Zbs)
binvi|rd, rs1, bit|Invert bit of rs1 (Zbs)
bset|rd, rs1, rs2|Set bit rs2 of rs1 (Zbs)
bseti|rd, rs1, bit|Set bit of rs1 (Zbs)
ecall||Environment call: service number in a7, arguments in a0–a6
ebreak||Breakpoint: stops the simulator
fence|[pred, succ]|Memory ordering fence (no effect in the simulator)
fence.i||Instruction fence (no effect in the simulator)
pause||Pause hint (no effect in the simulator)
csrrw|rd, csr, rs1|Atomic read/write CSR
csrrs|rd, csr, rs1|Atomic read and set CSR bits
csrrc|rd, csr, rs1|Atomic read and clear CSR bits
csrrwi|rd, csr, uimm|Atomic read/write CSR with immediate
csrrsi|rd, csr, uimm|Atomic read and set CSR bits with immediate
csrrci|rd, csr, uimm|Atomic read and clear CSR bits with immediate
csrr|rd, csr|Read CSR
csrw|csr, rs|Write CSR
csrs|csr, rs|Set CSR bits
csrc|csr, rs|Clear CSR bits
csrwi|csr, uimm|Write CSR immediate
csrsi|csr, uimm|Set CSR bits with immediate
csrci|csr, uimm|Clear CSR bits with immediate
rdcycle|rd|Read the cycle counter
rdtime|rd|Read the timer
rdinstret|rd|Read the retired-instruction counter
flw|fd, offset(rs1)|Load 32-bit float
fld|fd, offset(rs1)|Load 64-bit double
fsw|fs2, offset(rs1)|Store 32-bit float
fsd|fs2, offset(rs1)|Store 64-bit double
fmv.w.x|fd, rs|Move the low 32 bits of an integer register to a float register
fmv.x.w|rd, fs|Move float bits to an integer register (sign-extended)
fmv.d.x|fd, rs|Move 64 bits of an integer register to a float register
fmv.x.d|rd, fs|Move double bits to an integer register
fmv.s|fd, fs|Copy float register
fmv.d|fd, fs|Copy double register
fneg.s|fd, fs|fd = -fs
fneg.d|fd, fs|fd = -fs
fabs.s|fd, fs|fd = |fs|
fabs.d|fd, fs|fd = |fs|
";

pub struct Instruction {
    pub syntax: &'static str,
    pub summary: String,
}

fn float_name(suffix: &str) -> &'static str {
    match suffix {
        "s" => "single",
        "d" => "double",
        "w" => "int32",
        "wu" => "uint32",
        "l" => "int64",
        "lu" => "uint64",
        _ => "?",
    }
}

pub fn instruction(op: &str) -> Option<Instruction> {
    static INDEX: OnceLock<HashMap<&str, (&str, &str)>> = OnceLock::new();
    let index = INDEX.get_or_init(|| {
        TABLE
            .lines()
            .filter_map(|l| {
                let mut parts = l.splitn(3, '|');
                Some((parts.next()?, (parts.next()?, parts.next()?)))
            })
            .collect()
    });
    if let Some((syntax, summary)) = index.get(op) {
        return Some(Instruction { syntax, summary: (*summary).to_owned() });
    }
    let (base, suffix) = op.rsplit_once('.')?;
    let precision = float_name(suffix);
    let binary = |what: &str| Some(Instruction { syntax: "fd, fs1, fs2", summary: format!("{what} ({precision})") });
    match base {
        "fadd" => binary("fd = fs1 + fs2"),
        "fsub" => binary("fd = fs1 - fs2"),
        "fmul" => binary("fd = fs1 * fs2"),
        "fdiv" => binary("fd = fs1 / fs2"),
        "fmin" => binary("fd = min(fs1, fs2)"),
        "fmax" => binary("fd = max(fs1, fs2)"),
        "fsgnj" => binary("fd = |fs1| with the sign of fs2"),
        "fsgnjn" => binary("fd = |fs1| with the opposite sign of fs2"),
        "fsgnjx" => binary("fd = fs1 with sign(fs1) xor sign(fs2)"),
        "fsqrt" => Some(Instruction { syntax: "fd, fs1", summary: format!("fd = sqrt(fs1) ({precision})") }),
        "feq" | "flt" | "fle" => Some(Instruction {
            syntax: "rd, fs1, fs2",
            summary: format!(
                "rd = (fs1 {} fs2) ? 1 : 0 ({precision})",
                match base {
                    "feq" => "==",
                    "flt" => "<",
                    _ => "<=",
                }
            ),
        }),
        "fclass" => {
            Some(Instruction { syntax: "rd, fs1", summary: format!("Classify fs1 into a 10-bit mask ({precision})") })
        }
        "fmadd" | "fmsub" | "fnmadd" | "fnmsub" => Some(Instruction {
            syntax: "fd, fs1, fs2, fs3",
            summary: format!(
                "fd = {} ({precision}, fused)",
                match base {
                    "fmadd" => "fs1 * fs2 + fs3",
                    "fmsub" => "fs1 * fs2 - fs3",
                    "fnmadd" => "-(fs1 * fs2) - fs3",
                    _ => "-(fs1 * fs2) + fs3",
                }
            ),
        }),
        _ if base.starts_with("fcvt.") => {
            let to = &base[5..];
            let syntax = match (matches!(to, "s" | "d"), matches!(suffix, "s" | "d")) {
                (true, true) => "fd, fs[, rm]",
                (true, false) => "fd, rs[, rm]",
                _ => "rd, fs[, rm]",
            };
            Some(Instruction { syntax, summary: format!("Convert {precision} to {}", float_name(to)) })
        }
        _ => {
            let (name, width) = op.split_once('.')?;
            let width = if width.starts_with('w') { "32-bit" } else { "64-bit" };
            let summary = match name {
                "lr" => {
                    return Some(Instruction { syntax: "rd, (rs1)", summary: format!("Load-reserved {width}") });
                }
                "sc" => format!("Store-conditional {width}: rd = 0 on success"),
                _ if name.starts_with("amo") => {
                    format!("Atomic {width} {}: rd = mem[rs1]; mem[rs1] op= rs2", &name[3..])
                }
                _ => return None,
            };
            Some(Instruction { syntax: "rd, rs2, (rs1)", summary })
        }
    }
}

/// ABI name of every integer register `x0`..`x31`; `fp` is absent because it aliases `s0`.
const INTEGER_ABI: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4", "a5", "a6", "a7", "s2",
    "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4", "t5", "t6",
];

/// ABI name of every float register `f0`..`f31`.
const FLOAT_ABI: [&str; 32] = [
    "ft0", "ft1", "ft2", "ft3", "ft4", "ft5", "ft6", "ft7", "fs0", "fs1", "fa0", "fa1", "fa2", "fa3", "fa4", "fa5",
    "fa6", "fa7", "fs2", "fs3", "fs4", "fs5", "fs6", "fs7", "fs8", "fs9", "fs10", "fs11", "ft8", "ft9", "ft10", "ft11",
];

pub fn register(name: &str) -> Option<String> {
    let name = if name == "fp" { "s0" } else { name };
    let (n, float) = if let Some(n) = INTEGER_ABI.iter().position(|r| *r == name) {
        (n, false)
    } else if let Some(n) = FLOAT_ABI.iter().position(|r| *r == name) {
        (n, true)
    } else {
        let (prefix, digits) = name.split_at(1.min(name.len()));
        let n: usize = digits.parse().ok().filter(|n| *n < 32 && !digits.starts_with('+'))?;
        match prefix {
            "x" => (n, false),
            "f" => (n, true),
            _ => return None,
        }
    };
    let abi = if float { FLOAT_ABI[n] } else { INTEGER_ABI[n] };
    let role = if float {
        match &abi[..2] {
            "ft" => "temporary, caller-saved",
            "fs" => "saved, callee-saved",
            _ if n < 12 => "float argument / return value, caller-saved",
            _ => "float argument, caller-saved",
        }
    } else {
        match abi {
            "zero" => "hard-wired zero; writes are ignored",
            "ra" => "return address, caller-saved",
            "sp" => "stack pointer (16-byte aligned), callee-saved",
            "gp" => "global pointer",
            "tp" => "thread pointer",
            "s0" => "saved register / frame pointer `fp`, callee-saved",
            "a0" | "a1" => "argument / return value, caller-saved",
            _ if abi.starts_with('a') => "argument, caller-saved",
            _ if abi.starts_with('s') => "saved, callee-saved",
            _ => "temporary, caller-saved",
        }
    };
    Some(format!("**{abi}** (`{}{n}`) — {role}", if float { "f" } else { "x" }))
}

pub fn directive(name: &str) -> Option<&'static str> {
    Some(match name {
        ".text" => "Switch to the code section",
        ".data" => "Switch to the writable data section",
        ".bss" => "Switch to the zero-initialised data section",
        ".rodata" => "Switch to the read-only data section",
        ".section" => "Switch section: `.section .text|.rodata|.data|.bss`",
        ".globl" | ".global" => "Export symbols; exported labels get Run buttons",
        ".type" => "`.type name, @function` marks a function (accepted metadata)",
        ".size" => "Symbol size metadata (accepted, ignored)",
        ".equ" | ".equiv" | ".eqv" => "Define a constant: `.equ NAME, expression`",
        ".byte" => "Emit 8-bit values",
        ".half" => "Emit 16-bit values",
        ".word" => "Emit 32-bit values",
        ".dword" => "Emit 64-bit values",
        ".float" => "Emit 32-bit floats",
        ".double" => "Emit 64-bit doubles",
        ".ascii" => "Emit a string without a terminator",
        ".asciz" | ".string" => "Emit a NUL-terminated string",
        ".zero" => "Emit N zero bytes",
        ".skip" | ".space" => "Reserve N bytes: `.skip N[, fill]`",
        ".align" => "Align to 2**N bytes",
        _ => return None,
    })
}

/// Index of the operand that `text` (everything after the mnemonic, up to the caret) is in.
pub fn active_operand(text: &str) -> usize {
    let mut depth = 0i32;
    let mut index = 0;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth <= 0 => index += 1,
            _ => {}
        }
    }
    index
}

pub fn operands(syntax: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for (i, c) in syntax.char_indices() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(syntax[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    if !syntax[start..].trim().is_empty() {
        parts.push(syntax[start..].trim());
    }
    parts
}
