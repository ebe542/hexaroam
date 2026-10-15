# Firmware

Rust firmware for the central Pico and optional future servo-output expansion modules. The current program blinks the onboard LED and generates adjustable PIO servo pulses on GP0 and GP1 of Raspberry Pi Pico with RP2040, targeting `thumbv6m-none-eabi`. Operation with an attached servo is not verified.

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
| pico/src/drivers/servo_pio.rs | PIO output, encoding, and private transport adapter |
| servo-signal/src/lib.rs | Hardware-independent limits and submission logic |
| servo-signal/src/tests.rs | Host unit tests using a test transport |
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

## PIO pulse driver and shared signal logic

The application uses PIO0 state machine 0 with GP0 (physical Pico pin 1) and state machine 1 with GP1 (physical pin 2). ServoPio is generic over the PIO block, state machine index, and pin identifier. The associated P::PinFunction ties the GPIO function to the selected PIO block at compile time. Each channel owns its running state machine, private PioTransport adapter, limits, and pin. The PIO manager remains in main. This two-channel implementation is a learning step, not a finalized 18-channel design.

The module-level install_program function loads the eight-instruction program once. main creates a second handle with unsafe InstalledProgram::share before passing the handles to the two channel constructors. Sharing does not duplicate instructions in PIO memory. The safety condition is that the program must be uninstalled only once and only after all state machines using it have been uninitialized. This firmware never uninstalls it. Each constructor starts its channel separately; rising edges are not synchronized.

### Responsibilities

- `servo-signal` is a hardware-independent `no_std` library. PulseLimits validates inclusive application limits; submit_pulse validates requests and enforces a single pending update through PulseTransport.
- The private PioTransport implements the transport with the actual TX FIFO and converts microseconds to the PIO data word.
- The servo_pio module contains the PIO program and timing calculations. ServoPio initializes a channel from an installed program handle and owns its hardware resources.
- main selects the current experiment limits of 1000–2000 us and the initial pulse of 1500 us. These are test settings, not calibrated MZ996 limits.

The Pico package depends on the shared library through `path = "../servo-signal"`. See the [shared library guide](servo-signal/README.md) for API contracts and host test commands.

### Update semantics

A valid request is accepted only when the TX FIFO is empty. Exactly one complete 32-bit word is written. A pending request yields Busy without another write; requests outside the configured inclusive limits yield OutOfRange before the FIFO is accessed.

Ok means queued, not already active. At each period boundary, PIO uses PULL NOBLOCK and MOV X, OSR to adopt a pending word or retain the previous word. X retains the full last command; Y is the countdown register. No interrupt or DMA is required.

The policy requires a single writer through the private adapter. It prevents a backlog beyond one pending update, but does not overwrite that update. The application should retry its current desired target rather than queue rejected historical targets.

### PIO data and timing

The state machine runs at 1 MHz, using an integer divider derived from the system clock. The period is nominally 20,000 us. The low 16 bits contain the high-phase count and the high 16 bits contain the low-phase count; OUT shifts right.

The program occupies eight instructions (16 bytes). Labels and wrap directives occupy no instruction slots. There is no extra cycle for wrapping.

| Phase | Instructions accounted for | Cycles |
| --- | --- | --- |
| High | SET high; JMP Y-- loop | high count + 2 |
| Low | SET low; OUT; JMP Y-- loop; next PULL, MOV, OUT | low count + 6 |

Therefore high count = pulse width - 2 and low count = 20,000 - pulse width - 6. At 1500 us, the counts are 1498 and 18494. Encoding remains private to the PIO driver and must be reviewed with any program change.

The representable pulse range is 2–19,994 us for this program. This is a technical encoding range, not a servo operating range. Before configuring the state machine, the constructor rejects unsupported application limits with UnsupportedPulseLimits and an initial pulse outside those limits with InitialPulseOutOfRange. Clock-divider checks and the separate program installation can still panic; the constructor is not a fully fallible hardware initialization API.

GPIO is set low before enabling output direction. A validated initial data word is queued before starting PIO. Both channels initially output 1500 us for approximately one second. The application then requests -45, 0, and +45 degrees on GP0 and +45, 0, and -45 degrees on GP1 through separate Servo instances and synthetic calibrations. These map to 1000/1500/2000 us and 2000/1500/1000 us respectively. Commands cycle approximately once per second while the LED keeps blinking. Its demonstration loop uses expect on submission errors. Angle conversion and two-point calibration are implemented in servo-control. Physical servo calibration, output-disable controls, and multi-channel coordination are not implemented.

The dependencies remain rp-pico 0.9, rp2040-hal 0.10.2, and pio/pio-proc 0.2, with resolved versions recorded in Cargo.lock.

### Bench observations

2026-10-15, two-channel integration: the owner reported a successful local check with 68 passing tests, firmware execution with a blinking LED, and both pulse outputs following the agreed sequences. Rising edges were observed to be unsynchronized, as expected from separate state-machine starts. No new numerical timing or amplitude measurements were reported. Remote CI for this firmware revision has not yet been reported.

2026-10-13, initial fixed-pulse program: the owner observed approximately 19.8 ms period and 1.4 ms high time versus nominal 20 ms and 1.5 ms. That earlier program used separate startup counts and a different loop; its count corrections are not the current encoding.

2026-10-14, shared-library integration: the owner reported six passing host unit tests, a successful Pico build, and physical execution with apparently unchanged behavior of the three alternating pulse widths. The three widths had already been visually distinguished before extraction.

Instrument: Leader 1041, 40 MHz, two-channel analog oscilloscope. No servo was attached. Measurement uncertainty was not quantified and no measured amplitude was reported. These observations confirm basic operation, not precise timing accuracy, absence of short update glitches, or servo compatibility. Unit tests do not verify hardware FIFO behavior or ASM cycle timing. Initial configuration error paths and Clippy have not been specifically verified. The two-channel local check includes Rust formatting checks and the Pico build; Ubuntu hardware execution remains unverified.

For reproducing the signal check, connect the probes to GP0 and GP1 through the planned 180-ohm series resistors and their ground clips to Pico GND (for example physical pin 3). Keep the servo disconnected. Exact wiring and power details of the reported observations were not recorded.

## Angle library

The [servo-control library](servo-control/README.md) provides an i32 centidegree Angle type, two-point calibration, and Servo angle commands through PulseOutput. On 2026-10-14 the owner reported a successful full local check with 21 control tests, 6 signal tests, 3 Python tests, formatting, and the existing Pico build. The library is now integrated into the Pico firmware: ServoPio implements PulseOutput and main owns a Servo configured with synthetic -45-degree / 1000-us and +45-degree / 2000-us calibration points. ServoPio still starts with an initial 1500-us pulse before the wrapper is constructed. Constructing Servo itself emits no command.

On 2026-10-14 the owner reported a successful full check, UF2 generation/upload, and firmware behavior as expected after integration. No servo was attached. This verifies the integrated command path without establishing physical joint angles, a measured servo calibration, or new precise timing measurements. The previous wrapper revision passed CI; a remote result for this integration is pending.
