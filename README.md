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

## Use

Open an assembly file and invoke **RISC-V: Run File or Function** from Find Action, the editor toolbar or its context menu. Choose **Run program** for the automatic `_start` / `main` entry, or choose a function. Gutter buttons run the function on their line.

A preceding C signature comment, such as `# int64_t sum(int64_t a, int64_t b)`, names the arguments and enforces their count. Arguments accept decimal, hexadecimal, binary and octal 64-bit integers, are remembered per file/function, and are passed as separate argv entries. Dismissing either prompt cancels the run.

The plugin contributes whole-file configurations to the normal Run window. Function commands append `--function label -- arguments` to that file's configuration. Runs use the normal save, Stop, Rerun and run target handling. Rerun repeats the submitted arguments; invoke the plugin command again to change them. The default instruction limit is 10 million; saved configurations can change `--max-steps`. Debugging is not provided.

See [the fixture](examples/riscv-app/README.md) for the supported simulator dialect and examples. The former built-in RISC-V configuration type and Run Context producer are replaced by plugin configurations and commands; previously saved `kind = "riscv"` configurations should be recreated.

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
