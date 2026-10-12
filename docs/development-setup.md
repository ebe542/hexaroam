# Development setup

This guide covers Windows with Git Bash and Ubuntu with Bash. The selected board is Raspberry Pi Pico with RP2040. It documents the development environment; firmware dependencies, linker configuration, flashing, and debugging will be selected separately.

## Verification status

As of 2026-10-12:

- The project owner confirmed Rust and Cargo are available on Windows, with stable MSVC active, rustfmt and Clippy installed, and rust-analyzer installed in VS Code.
- The owner confirmed that Rust formatting settings were added.
- Installing the Pico target has been proposed but not yet confirmed.
- A successful Windows host build has not been reported; MSVC linker readiness is not established by version commands alone.
- Ubuntu instructions have not been executed for this project.

Reported Windows versions (an observation, not a project version requirement):

```text
rustc 1.99.0 (b940084d7 2026-09-28)
cargo 1.99.0 (5f94df478 2026-08-27)
```

No project toolchain version is pinned yet. Record the versions used when reporting build results.

## Windows: Git Bash

### Install Rust

1. Install Git for Windows and VS Code if needed.
2. Download the appropriate `rustup-init.exe` from the [official Rust installation page](https://rust-lang.org/tools/install/).
3. Run the installer and select the standard installation using the stable MSVC toolchain. On an x64 Windows host the toolchain is `stable-x86_64-pc-windows-msvc`.
4. If prerequisites are missing, follow the installer prompts or the [MSVC prerequisites guide](https://rust-lang.github.io/rustup/installation/windows-msvc.html). Windows host executables require the MSVC C++ build tools and Windows SDK. The Visual Studio installer workload is “Desktop development with C++”.
5. Restart Git Bash. If VS Code still uses the old PATH, close all VS Code windows and reopen it.

Git Bash is the shell; it does not require switching Rust to the GNU toolchain.

Check availability in a new Git Bash terminal:

```bash
rustup --version
rustc --version
cargo --version
rustup show
```

Expect stable MSVC to be active. These commands confirm tool availability, not successful linking.

### VS Code terminal

Select Git Bash as the default terminal profile, or merge this entry into the existing workspace settings:

```json
{
  "terminal.integrated.defaultProfile.windows": "Git Bash"
}
```

## Ubuntu: Bash

Install basic host build and download tools:

```bash
sudo apt update
sudo apt install build-essential curl ca-certificates git
```

Install Rust using the command from the [official installation page](https://rust-lang.org/tools/install/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Select the standard installation. Run rustup as your normal user, not with sudo.

Open a new terminal, or load the environment into the current Bash session:

```bash
source "$HOME/.cargo/env"
```

Check availability:

```bash
rustup --version
rustc --version
cargo --version
rustup show
```

Expect an active stable Linux toolchain matching the machine architecture. On an x64 machine the host is `x86_64-unknown-linux-gnu`; ARM64 hosts use a different target.

Install VS Code using its [official Linux installation instructions](https://code.visualstudio.com/docs/setup/linux). Use Bash on Ubuntu; the Windows terminal-profile setting does not select an Ubuntu shell.

This guide does not configure WSL USB access or Linux debug-probe permissions. Those depend on the later flashing/debugging workflow.

## Shared Rust components

Check what is installed:

```bash
rustup component list --installed
```

If rustfmt or Clippy is missing:

```bash
rustup component add rustfmt clippy
```

- rustup manages toolchains, components, and targets.
- rustc compiles Rust.
- Cargo manages packages, dependencies, and builds.
- rustfmt formats source code.
- Clippy provides additional code diagnostics.

## VS Code: rust-analyzer and formatting

Install the extension with ID `rust-lang.rust-analyzer` from the Extensions view. See the [rust-analyzer editor installation guide](https://rust-analyzer.github.io/book/editor_features.html#vs-code).

Merge this language block into the existing `.vscode/settings.json`; do not replace unrelated settings:

```json
{
  "[rust]": {
    "editor.defaultFormatter": "rust-lang.rust-analyzer",
    "editor.formatOnSave": true
  }
}
```

The surrounding braces illustrate a settings object. When merging, keep only one top-level object and separate entries with commas.

The existing TOML formatter may be retained for future Cargo.toml files. Workspace settings are currently ignored by Git and must be configured locally on each machine.

Code analysis for the firmware will become available once a Cargo package and its target configuration exist.

## Raspberry Pi Pico target

For the RP2040 Cortex-M0+ cores, install:

```bash
rustup target add thumbv6m-none-eabi
rustup target list --installed
```

Expect `thumbv6m-none-eabi` plus the host target. The host target remains installed.

The embedded target supplies the Rust target libraries; it does not create firmware, install a board support package, or configure flashing. Bare-metal firmware will use `no_std`, with the runtime, memory layout, and hardware access discussed before implementation. See [rustup cross-compilation](https://rust-lang.github.io/rustup/cross-compilation.html).

No C/C++ Pico SDK is required for the planned Rust HAL approach. The exact Rust dependencies and firmware build workflow remain to be selected.

## Next steps

1. Confirm the embedded target installation.
2. Agree on firmware files, dependencies, and USB programming tools.
3. Implement and build a board-LED blink program.
4. Verify execution on the Pico, then proceed to one servo.

The project owner performs implementation, builds, tests, and commits. Setup command output and hardware results should be reported separately from documentation review.
