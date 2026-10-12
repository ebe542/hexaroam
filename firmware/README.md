# Firmware

Rust firmware for the central Pico and optional future servo-output expansion modules. The current program blinks the onboard LED of Raspberry Pi Pico with RP2040, targeting `thumbv6m-none-eabi`. Servo control is not implemented yet.

See [development setup](../docs/development-setup.md) for prerequisites and [hardware references](../hardware/references/README.md) for archived datasheets.

## Pico LED blink firmware

The onboard LED on GPIO25 alternates between on and off with a requested delay of 500 ms per phase. GPIO and clocks are initialized through the Pico board support package. SysTick supplies the blocking delay. Panic handling stops execution; it does not print a diagnostic or automatically reset the board.

### Files

| File | Purpose |
| --- | --- |
| pico/Cargo.toml | Package metadata and dependencies |
| pico/Cargo.lock | Resolved dependency versions; commit this file |
| pico/.cargo/config.toml | Embedded target and linker flags |
| pico/memory.x | Flash, bootloader, and RAM layout |
| pico/build.rs | Makes the memory layout available to the linker |
| pico/src/main.rs | Clock initialization, GPIO configuration, and blink loop |
| ../rustfmt.toml | Repository-wide Rust formatting rules |

### Build

From the repository root:

```bash
cd firmware/pico
cargo check
cargo build
```

Run Cargo from this directory so its local `.cargo/config.toml` is loaded. `cargo check` checks code without producing a linked firmware image. `cargo build` produces the ELF executable at `target/thumbv6m-none-eabi/debug/hexaroam-pico`, without a filename extension.

### Convert to UF2

Install the converter once on the development host, using the version exercised in this project:

```bash
cargo install elf2uf2-rs --version 2.2.0 --locked
elf2uf2-rs --help
```

This tool is installed for the host, separately from firmware dependencies. See its [versioned documentation](https://docs.rs/crate/elf2uf2-rs/2.2.0).

From `firmware/pico`:

```bash
elf2uf2-rs \
  target/thumbv6m-none-eabi/debug/hexaroam-pico \
  target/thumbv6m-none-eabi/debug/hexaroam-pico.uf2
```

### Transfer over USB

1. Disconnect the Pico.
2. Hold BOOTSEL while connecting it with a USB data cable.
3. Release BOOTSEL; the RPI-RP2 drive appears.
4. Copy the generated UF2 file onto that drive.
5. The Pico restarts automatically and the drive disappears.

On Windows, use File Explorer to copy the file. On Ubuntu, use the file manager to copy it to the mounted RPI-RP2 volume; the Ubuntu workflow has not yet been verified in this project.

No debug probe or automatic Cargo runner is configured.

### Verification record

On 2026-10-12, the project owner reported successful `cargo check`, `cargo build`, UF2 conversion, and USB transfer on Windows using Git Bash. The onboard LED was observed blinking on the physical Pico.

This confirms the basic build and hardware execution path. Exact signal timing, Ubuntu execution, and servo functionality are not verified. Clippy and formatter check results have not been reported.

### Version control

Commit source files, linker configuration, and Cargo.lock. Generated ELF and UF2 files remain under the ignored `target/` directory.
