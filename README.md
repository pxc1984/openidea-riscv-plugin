# RISC-V for OpenIDEA

RISC-V assembly detection (`.asm`, `.s`, `.S`), highlighting, register and instruction completion, function outline, and Run gutter buttons. The RV64 teaching simulator from commit `70feb6d2640cff3944a5221bd6ea22e96c16778d` is a standalone helper in `runner/`; neither the desktop app nor `openidea-server` embeds it.

## Install

From this repository (Rust with the `wasm32-wasip2` target is required):

```sh
rustup target add wasm32-wasip2
cargo install --path runner --locked
./build.sh
```

In Settings → Plugins → Install Plugin from Disk, select this plugin's folder. Allow `process:openidea-riscv` to enable execution. If the IDE cannot find the helper on PATH, choose its absolute executable path in the plugin's permissions in Settings. Highlighting and completion work without the process permission. For remote projects, install the helper and grant its executable on the server.

## Updates

When the IDE starts, the plugin asks GitHub for the release marked **Latest** (drafts and pre-releases are skipped). If it is newer than the installed version, a notification offers **Download**, which saves `openidea.riscv.oiplugin` from that release; install it with Settings → Plugins → Install Plugin from Disk. The plugin cannot install itself. GitHub is asked at most once in 12 hours, because the IDE also reactivates idle plugins during a session. The check needs the optional `net:api.github.com` permission and stays silent without it or without network. **RISC-V: Download Latest Plugin Release** downloads the package at any time.

## Use

While editing, the plugin works without the helper:

- **Live diagnostics.** The simulator's assembler checks the unsaved buffer: unknown instructions (with a "did you mean" hint), wrong operand counts, undefined labels and missing `1f`/`1b` local labels, plus a warning when a result is written to `zero`.
- **Hover.** Instructions show their operand syntax and meaning, registers their `xN`/`fN` number and calling-convention role, numbers their decimal/hex/binary values, and labels where they are defined (with the C signature for functions).
- **Completion by position.** Mnemonics and directives at the start of a statement; at an operand, registers (integer or float to match the operand) and the file's labels and `.equ` constants, with labels first where a branch or call target is expected and `sp` first inside `offset(`. Snippets add an exported function and an exit `ecall`, plus skeletons for teaching tasks: `test main` (reads test arguments with `syscall 5` until `-1`, calls a function and prints results), branchless `select` and `min` (no branches: `slt`/`srai` masks), a counted `loop`, `sum array`, struct field access through a pointer, and a `.data` array.
- **Operand help** names the operand being typed, e.g. `sd rs2, offset(rs1)`.
- **Reformat Code** (also on selection and on save) aligns labels at column 0, mnemonics at column 8 and instruction operands at column 16, puts one space after directives and `, ` between operands, lines up the trailing comments of consecutive code lines in one column, and uses tabs or spaces as the editor is set. It only changes whitespace outside strings and comments; a line with a comment inside its code is left as is. Pick the plugin in Settings → Editor → Code Style → Formatter → Plugin Formatters.
- **Navigation.** Go to Declaration on a label, constant or `1b`/`1f` reference; occurrences of the label or register under the caret are highlighted, with writes to registers marked. Functions fold.

Open an assembly file and invoke **RISC-V: Run File or Function** from Find Action, the editor toolbar or its context menu. Choose **Run program** for the automatic `_start` / `main` entry, or choose a function. Gutter buttons run the function on their line.

A preceding C signature comment, such as `# int64_t sum(int64_t a, int64_t b)`, names the arguments and enforces their count. Arguments accept decimal, hexadecimal, binary and octal 64-bit integers, are remembered per file/function, and are passed as separate argv entries. Dismissing either prompt cancels the run.

Run commands and gutter icons start the `openidea-riscv` simulator in a Terminal tab on the saved file: the whole program, or `--function label -- arguments` for a function. A file with unsaved changes is not run. Rerun repeats the last run without prompts; invoke the plugin command again to change the arguments. The instruction limit is 10 million. Debugging is not provided.

**RISC-V: Run Tests in Directory** (also in the editor context menu) asks for a directory of tests — numbered `NN.dat` inputs with matching `NN.ans` expected outputs — and runs the simulator with `--tests <dir>`, which prints PASS/FAIL per case and exits nonzero on a failure. EOF (a missing argument) reads as `-1` for `syscall 5`, so a `test main` loop over the inputs ends cleanly. The directory is remembered per file like function arguments; dismissing the prompt cancels the run.

See [the fixture](examples/riscv-app/README.md) for the supported simulator dialect and examples. The former built-in RISC-V configuration type and Run Context producer are replaced by plugin commands; previously saved `kind = "riscv"` configurations should be recreated.

## Vim

Everything is reachable from the keyboard. Plugin commands are IDE actions, so map them in
`~/.ideavimrc` (or `~/.openideavimrc`) next to the IDE actions the plugin feeds:

```vim
let mapleader = " "
nmap gd <Action>(GotoDeclaration)          " label, constant or 1b/1f under the caret
nmap K  <Action>(QuickJavaDoc)             " instruction, register or number reference
imap <C-k> <Action>(ParameterInfo)         " operands of the instruction being typed
nmap ]m <Action>(MethodDown)               " next / previous function
nmap [m <Action>(MethodUp)
nmap ]d <Action>(GotoNextError)            " next assembler problem
nmap [d <Action>(GotoPreviousError)
nmap <leader>r <Action>(plugin.openidea.riscv.run-at-caret)
nmap <leader>R <Action>(plugin.openidea.riscv.rerun)
nmap <leader>p <Action>(plugin.openidea.riscv.run)
```

- **RISC-V: Run Function at Caret** runs the function around the caret (it prompts for its arguments; outside a function it offers the picker). **RISC-V: Rerun Last** repeats the last plugin run, arguments included, without prompting.
- `gcc` / `gc{motion}` comment with `#`; `*`, `%` and `ci(` (`8(sp)`) work as usual. As in Vim's default `iskeyword`, `w` stops at the `.` of `sext.w`.
- Needs OpenIDEA with assembly support in Vim mode (`af` / `if` from the function outline, labels at column 0 and statements one tab in for Enter, `o`/`O` and `=`). Older versions leave those to the generic fallback.

## Development

```sh
cargo test --manifest-path Cargo.toml
cargo clippy --manifest-path Cargo.toml --all-targets -- -D warnings
./build.sh
```

The plugin tests include the simulator's execution suite. The helper is a separate native Cargo package; install it on each machine that executes programs. The WASM component uses the standard plugin API and needs no IDE changes.

## Releases

Pushing a tag whose commit is on `main` builds a draft GitHub release named after
the tag. For example, after merging the release changes into `main`:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The draft includes `openidea.riscv.oiplugin` and simulator ZIPs for Linux x64,
macOS Intel/Apple Silicon, and Windows x64. Download the plugin and use
**Settings → Plugins → Install Plugin from Package File**. Extract the matching
simulator ZIP, put `openidea-riscv` (`openidea-riscv.exe` on Windows) on PATH, or
select its absolute path in the plugin's process permissions. On macOS/Linux,
run `chmod +x` on the extracted executable if the ZIP extractor drops permissions.

Update the versions in `plugin.toml`, both Cargo manifests, and their lockfiles
before tagging a new version. Review and publish the draft when ready.

## Origin and license

Extracted from [OpenIDEA](https://github.com/l0ng-ai/openidea), branch `feature/riscv-lsp` at commit `47943a84`, including its standalone simulator and sample project. The SDK is pinned to the public OpenIDEA commit used by that branch; no local IDE checkout is required. Licensed under Apache-2.0; see [LICENSE](LICENSE).
