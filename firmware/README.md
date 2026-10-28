# Firmware

Rust firmware for the central Pico and optional future servo-output expansion modules. The current program blinks the onboard LED and generates fixed servo pulses on GP0 and GP1 through one PIO state machine and DMA of Raspberry Pi Pico with RP2040, targeting `thumbv6m-none-eabi`. Operation with an attached servo is not verified.

See [development setup](../docs/development-setup.md) for prerequisites and [hardware references](../hardware/references/README.md) for archived datasheets.

## Pico LED blink firmware

The onboard LED on GPIO25 alternates between on and off approximately every 500 ms. GPIO and clocks are initialized through the Pico board support package. The hardware timer supplies timestamps for nonblocking LED updates, allowing the main loop to service DMA continuously. The normal firmware now initializes UART1 logging and selects the bounded UART panic handler; see [logging](logging/README.md). Its main loop services servo DMA before the logger. The periodic alive message belongs to this demo, not a general runtime logging policy.

### Files

| File | Purpose |
| --- | --- |
| pico/Cargo.toml | Package metadata and dependencies |
| pico/Cargo.lock | Resolved dependency versions; commit this file |
| pico/.cargo/config.toml | Embedded target and linker flags |
| pico/memory.x | Flash, bootloader, and RAM layout |
| pico/build.rs | Makes the memory layout available to the linker |
| pico/src/main.rs | Board clock, GPIO, and timer initialization; starts the demo |
| pico/src/uart_logging.rs | Nonblocking HAL UART backend for the shared logger |
| pico/src/panic_uart.rs | Bounded direct UART1 panic output; selected only by uart_panic |
| logging/src/lib.rs | Hardware-independent buffered logging |
| logging/src/bounded_writer.rs | Direct formatted output with a shared attempt budget |
| pico/src/servo_demo.rs | Fixed plan construction, static buffer allocation, DMA servicing, and LED updates |
| servo-pio/src/output.rs | Prepared PIO output, fixed DMA transfer, and polling service |
| servo-pio/src/program.rs | Checked multi-channel PIO assembly |
| servo-pio/src/encoding.rs | Internal mask/counter encoding |
| servo-pio/src/buffer.rs | Complete encoded-plan storage |
| pico/src/drivers/servo_pio.rs | Historical per-channel driver; no longer compiled by main.rs |
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

## Current fixed-plan PIO/DMA demo

`servo_demo::run()` uses PIO0 SM0 and DMA channel 0 for both GP0 (physical pin 1) and GP1 (physical pin 2). The configured plan has a 20,000-us period, a 1000-us High pulse on GP0, and a 2000-us High pulse on GP1. Both rising edges share one GPIO-mask update. This demo uses pulse widths directly; the angle library and ServoBank are not in its active command path.

PreparedOutput validates GPIO assignments and the integer divider for a 25 MHz PIO clock, installs the program, and sets the selected pins Low before enabling their output directions. start_fixed encodes the entire plan into a statically allocated OutputBuffer<6>. It writes the first complete entry into the FIFO, starts DMA for the remaining words, and then starts PIO. A static read-only slice keeps the encoded payload valid for all subsequent transfers. It cannot be updated in this fixed-plan experiment.

FixedOutput::service() polls DMA completion. Once complete, it retrieves the channel and TX handle and starts another transfer of the full plan. FIFO ordering keeps plans consecutive. There is no DMA completion interrupt yet. Call service frequently; blocking delays would risk underflow. The long final Low interval of the test plan provides rearm time, but a general worst-case latency guarantee has not yet been established.

The PIO program checks both words before applying each GPIO state. A missing word forces outputs Low, sets PIO IRQ flag 0, and enters a persistent loop. The demo does not yet expose fault status or implement controlled recovery. Dynamic plan updates, double-buffer switching, and application-level angle integration remain pending.

On 2026-10-17, the owner reported firmware execution and approximately 1 ms and 2 ms High durations with synchronous rising edges on the analog oscilloscope. The preceding full local check passed 114 tests, formatting checks, and the Pico build. A new period measurement, deliberate underflow tests, recovery, 18-channel timing, and attached-servo operation were not reported. Remote CI for this DMA integration is pending.

See the [PIO driver guide](servo-pio/README.md) for encoding and timing details. To reproduce the signal check, probe GP0 and GP1 against Pico GND, with servos disconnected.

## Separate hardware tests

The [hardware-test guide](pico/hardware_tests/README.md) describes independently selectable firmware for continuous fixed output and both underflow cases. Build all ELF and UF2 files with python tools/dev.py hardware-build from the repository root. Upload and evaluation remain manual. Tests are stored under hardware_tests and registered as Cargo example targets; the normal application does not need editing to select a fault experiment.

The owner reported a successful missing-entry experiment and immediate fault behavior in the missing-counter experiment, with no visible startup edges. The local check passed 121 unit tests and all hardware-test UF2 files were created. Exact period/fault timing and controlled recovery remain unverified. CI for this revision is pending.

## Two-buffer switching experiment

SwitchingOutput is a separate public driver that accepts complete replacement SignalPlan values with the startup period and output set. It uses one DMA channel and two exclusive static buffers. A prepared plan is selected after the current transfer completes; FIFO ordering keeps complete plans consecutive. submit_plan confirms acceptance only, not GPIO activation. The normal servo_demo remains fixed-plan.

The switching_signal_plans hardware test alternates the GP0/GP1 pulse assignments and checks Busy on an immediate second submission. On 2026-10-17 the owner reported expected operation, synchronous rising edges, and alternating High durations. Full verification passed 128 unit tests. See the [driver guide](servo-pio/README.md#two-buffer-signal-plan-replacement) and [hardware-test guide](pico/hardware_tests/README.md). Exact period/update timing, fault recovery, and 18-channel verification remain open; CI for this revision is pending.

## Persistent switching-output fault

SwitchingOutput detects and latches the reserved PIO underflow flag through is_faulted, submit_plan, and service. Once detected, new submissions return Faulted before Busy or validation, and service does not rearm DMA. Clearing the hardware flag does not resume PIO or clear the driver state. Existing DMA resources remain bound until an explicit restart safely aborts the transfer; service alone does not recover output. See [controlled restart](servo-pio/README.md#explicit-restart-after-a-fault).

On 2026-10-17 the owner reported all checks passing and faulted_signal_output running as expected. It checks fault precedence with a ready plan, persistence after hardware-flag clearing, and continued Low outputs during subsequent servicing. See the [hardware-test guide](pico/hardware_tests/README.md#persistent-driver-fault-test). The unit-test total remains 128; remote CI for this revision is pending.

## Historical per-channel PIO driver

The following sections describe the previous implementation retained in src/drivers. They are not the active DMA demo.

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

## Angle library and historical integration

The library remains available, but the current fixed-plan DMA demo does not issue angle commands. The integration history below describes earlier firmware.

The [servo-control library](servo-control/README.md) provides an i32 centidegree Angle type, two-point calibration, and Servo angle commands through PulseOutput. On 2026-10-14 the owner reported a successful full local check with 21 control tests, 6 signal tests, 3 Python tests, formatting, and the existing Pico build. The library is now integrated into the Pico firmware: ServoPio implements PulseOutput and main owns a Servo configured with synthetic -45-degree / 1000-us and +45-degree / 2000-us calibration points. ServoPio still starts with an initial 1500-us pulse before the wrapper is constructed. Constructing Servo itself emits no command.

On 2026-10-14 the owner reported a successful full check, UF2 generation/upload, and firmware behavior as expected after integration. No servo was attached. This verifies the integrated command path without establishing physical joint angles, a measured servo calibration, or new precise timing measurements. The previous wrapper revision passed CI; a remote result for this integration is pending.

## UART logging and panic diagnostics

The [logging guide](logging/README.md) describes nonblocking Info, Debug, and Error output, whole-message rejection when the buffer is full, and bounded direct panic output. The [hardware-test guide](pico/hardware_tests/README.md#uart-connection-and-terminal) documents UART1 on GP20, the Waveshare TTL adapter, HTerm settings, and separate uart_diagnostics and uart_panic programs.

On 2026-10-17 the owner reported 17 passing logging tests and 145 passing tests in the full runner. Buffered output with LF-only line endings was received in HTerm. In uart_panic, the panic message appeared after approximately five seconds and the LED stayed on. These manual checks are additional to the unit tests. The owner subsequently confirmed simultaneous UART output and servo pulses in the regular demo, which now selects the UART panic handler. Early panic without UART initialization and a stalled UART have not been separately tested. The owner reported green CI for the earlier logging revision.

## Controlled signal-output restart

SwitchingOutput now offers explicit restart with a validated complete SignalPlan after a latched fault. It rejects healthy-state restart and invalid replacement plans, aborts its owned DMA channel with bounded polling, discards old FIFO contents, and starts the supplied plan from the beginning. An abort timeout leaves resources bound and the driver faulted. The normal application continues to use FixedOutput; its logging integration does not automatically switch it to this driver.

On 2026-10-17 the owner reported 149 passing unit tests and successful execution of restart_signal_output and restart_active_dma. Both showed initial pulses, a Low interval, successful restart, and expected LED phases. The second test additionally asserts that DMA is still active immediately before restart. See the [driver contract](servo-pio/README.md#explicit-restart-after-a-fault) and [hardware procedures](pico/hardware_tests/README.md#controlled-restart-tests). Forced abort-timeout verification remains pending, and CI for this recovery revision has not yet been reported.

## Calibration storage format

The [servo-storage library](servo-storage/README.md) provides hardware-independent calibration encoding/decoding, an interchangeable CRC-32 interface, journal-slot envelopes, newest-valid-record selection, and append/sector management through `JournalStorage`. On 2026-10-28 the owner reported 66 passing storage tests, followed by 238 passing tests and all 13 checks passing in the full local runner. The owner reported green CI for the combined journal-management revision. Selected read/write/erase failures and retries are simulated in host tests. The Pico linker now reserves the last 8 KiB for the journal; the flash backend, UART configuration and startup enforcement remain future work; no actual flash writes are performed. A valid record still requires completeness checks before normal operation.

### Reserved calibration flash

`pico/memory.x` excludes `0x101FE000..0x10200000` from firmware FLASH and exposes the journal boundaries as linker symbols. The two 4-KiB sectors contain no initialized firmware section. The owner reported the full 13-check verification passing after this change; read-only ELF inspection confirmed the boundaries and absence of overlapping segments. No hardware storage operation or upload was performed. See the [storage guide](servo-storage/README.md#pico-flash-reservation) for addresses, limits and the verification record.
