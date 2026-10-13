# Firmware

Rust firmware for the central Pico and optional future servo-output expansion modules. The current program blinks the onboard LED and generates a fixed PIO servo pulse on GP0 of Raspberry Pi Pico with RP2040, targeting `thumbv6m-none-eabi`. Operation with an attached servo is not verified.

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
| pico/src/main.rs | Board initialization, driver construction, and blink loop |
| pico/src/drivers/servo_pio.rs | Fixed PIO pulse generation and hardware resource ownership |
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

The configured Cargo runner is `elf2uf2-rs`. From `firmware/pico`, run:

```bash
cargo run --locked
```

Cargo builds successfully before invoking the converter with the ELF path. The resulting UF2 is placed beside the ELF file. This command converts only; copying to RPI-RP2 remains manual. `cargo build` alone does not run the converter. A failed build can leave an older UF2 file present; transfer only after a successful build and conversion. No debug probe is configured.

### Verification record

On 2026-10-12, the project owner reported successful `cargo check`, `cargo build`, UF2 conversion, and USB transfer on Windows using Git Bash. The onboard LED was observed blinking on the physical Pico.

This confirms the basic build and hardware execution path. Exact signal timing, Ubuntu execution, and servo functionality are not verified. Clippy and formatter check results have not been reported.

### Version control

Commit source files, linker configuration, and Cargo.lock. Generated ELF and UF2 files remain under the ignored `target/` directory.

## Fixed PIO pulse driver

The driver uses PIO0 state machine 0 and GP0 (physical Pico pin 1). It owns the running state machine, TX FIFO handle, and configured pin; the PIO manager remains in main. This single-output implementation is a learning step, not a finalized multi-channel design.

The state machine runs at 1 MHz with an integer divider derived from the system clock. The nominal period is 20,000 us and high time is 1,500 us. GPIO is set low before the output direction is enabled and both initial counts are queued before starting execution.

Each phase takes count + 3 cycles: one SET, one MOV, and count + 1 executions of JMP X--. The initial counts are therefore 1497 and 18497. The program reads them once; runtime pulse updates, calibration, and output-disable controls are not implemented.

The dependencies remain compatible with rp-pico 0.9: rp2040-hal 0.10.2 and pio/pio-proc 0.2. Resolved versions are recorded in Cargo.lock.

### Bench observation: 2026-10-13

The owner reported a successful build, physical execution, a blinking onboard LED, and a visible periodic pulse, without an attached servo.

| Quantity | Nominal | Approximate reading |
| --- | --- | --- |
| Period | 20 ms | 19.8 ms |
| High time | 1.5 ms | 1.4 ms |

Instrument: Leader 1041, 40 MHz, two-channel analog oscilloscope. Measurement uncertainty was not quantified. No measured amplitude was reported. The readings confirm basic signal generation; they do not establish the actual timing error or servo compatibility. Counter values were not adjusted based on these readings.

For reproducing the signal check, connect the probe to GP0 through the planned 180-ohm series resistor and its ground clip to Pico GND (for example physical pin 3). Keep the servo disconnected. The exact wiring and power arrangement of the reported observation were not recorded.
