# Modular Pico / servo system, revision D proposal

> Revision D adds the socketed 24LC256 EEPROM and resolves the servo/UART/I2C GPIO assignment. It retains revision C's discrete TL431/LM393/MOSFET energy dump on page 3. See the [design calculations, parts and commissioning plan](discrete-shunt-design.md). The circuit is drawn and connection-checked; physical build, signal quality, startup, transient response and cooling remain unverified.

This is the current modular schematic proposal. It supersedes revision A's standalone one-servo arrangement as the system architecture. No physical board has been built or electrically commissioned. The root project contains three schematic pages representing separate physical assemblies joined by external cables, not one routed PCB.

## Open and review

- [KiCad project](pico-servo-system.kicad_pro): open this project so the two child sheets and project symbol table are available.
- [Three-page PDF](pico-servo-system.pdf): three A3 landscape pages, directly exported with KiCad 10.0.7.
- [Placement concept](placement-concept.svg): conservative board envelopes and reserved regions, not a drilling template or routed layout.
- [Bill of materials](bom.csv): all schematic parts with Fit/DNP status, plus off-schematic sockets/cable/board requirements.
- [Port pinout](port-pinout.csv): fixed 12-pin connector mapping for A/B/C.
- [ERC report](erc-report.json), [connection checks](connection-check.json), and [XML netlist](pico-servo-system.net).

Keep pico-servo-system.kicad_sch, servo-output-module.kicad_sch, bench-power.kicad_sch, HexaRoam.kicad_sym and sym-lib-table beside the project. The symbol definitions are embedded. The local library files retain the earlier generic shunt symbol for reference; revision C does not instantiate it. The new protection circuit uses standard KiCad symbols.

## Page 1: controller baseboard

One RP2040 Raspberry Pi Pico mounts on two 1x20 female sockets. Put its USB socket at the board edge. This is the same Pico controller, not an added MCU per module.

| Connector | Assignment |
| --- | --- |
| J1 | External Pico supply: pin 1 Manson +5 V, pin 2 GND |
| J2, port A | GP0..GP5 and six GND returns |
| J3, port B | GP6..GP11 and six GND returns |
| J4, port C | GP12, GP13, GP14, GP15, GP22, GP26 and six GND returns |
| J5 | UART1: pin 1 TX/GP20, pin 2 RX/GP21, pin 3 GND; 3.3-V TTL only |
| J6 | Pins 1..8: EEPROM SDA/GP18, EEPROM SCL/GP19, UART0 TX/GP16, UART0 RX/GP17, GP27, GP28, 3V3 output, GND |

D2 is the existing 1N5819: anode to J1 +5 V, cathode band toward VSYS/Pico pin 39. Pico's onboard USB diode supplies the other path; there is no added VBUS-to-VSYS connection. This follows the archived Pico datasheet section 4.5, figure 16. TP1 exposes VSYS, TP2 USB VBUS, TP3 GP0, TP4 GP1, and TP5 GND. The power flag on VSYS is an ERC source assertion, not a physical component.

Current firmware drives GP0/GP1 and UART TX. Routing the other signals does not enable them in firmware. UART RX is reserved for later use. Pin 33 AGND, pin 35 ADC_VREF, RUN and 3V3_EN are not externally routed; the Pico module provides its internal connections/defaults.

The separate [RE200-LF baseboard layout](../pico-baseboard-re200-lf/README.md) maps this controller sheet to the existing perfboard, with 1:1 placement/solder-side views and a complete wire list. It describes the earlier controller wiring and has not been updated for revision D's EEPROM or GP22/GP26 port-C remapping. Do not use its old port-C/J6 wire list for this revision; its mechanical placement remains a historical proposal.

## EEPROM and peripheral allocation

U2 is a removable 24LC256-I/P on a DIP-8 socket. Pin 8 is Pico 3V3, pin 4 is GND, pins 1/2/3 (A0/A1/A2) are GND, pin 5 SDA is GP18 and pin 6 SCL is GP19 (I2C1). The resulting seven-bit device address is 0x50. WP/pin 7 is tied to 3V3 for read-only robot operation. C1 is 100nF across pins 8/4; place it directly beside U2 in the physical build. R1/R2 are 2k pull-ups from SDA/SCL to 3V3, intended for the planned 400kHz bus. Actual rise time and signal quality require measurement.

GP16/GP17 remain UART0 TX/RX for the existing protocol firmware and are exposed on J6 pins 3/4. GP20/GP21 are UART1 TX/RX on J5. Servo outputs 16/17 use GP22/GP26 on J4 pins 9/11, retaining 18 distinct servo outputs. GP27/GP28 remain available on J6 pins 5/6. J6 pins 1/2 expose the existing EEPROM bus; this does not authorize connecting a live programmer in parallel. Programming takes place with the EEPROM detached from the unpowered controller, and WP low at the separate station.

The symbol uses the existing archived [DS21203P family datasheet](../../references/eeprom/24AA256-MIC.pdf). U2 has the standard DIP-8, 7.62mm-row footprint; its removable socket is a separate BOM item. C1's 5mm-lead-pitch disc footprint and R1/R2's DIN0207 horizontal footprints are provisional mechanical selections, not measurements of owned parts. Match actual body dimensions and lead spacing before assembly. These footprints do not create a routed PCB.

## Fixed signal-module interface

Use keyed 2x6, 2.54-mm-pitch IDC headers and matching cable sockets. Cables are straight-through pin 1 to pin 1, with the pin-1 side marked. Start with short bench cables and measure signal shape at the module. Connector names are from the Pico's perspective.

| Pin | Local module signal | Port A | Port B | Port C |
| --- | --- | --- | --- | --- |
| 1 | S0 | GP0 | GP6 | GP12 |
| 3 | S1 | GP1 | GP7 | GP13 |
| 5 | S2 | GP2 | GP8 | GP14 |
| 7 | S3 | GP3 | GP9 | GP15 |
| 9 | S4 | GP4 | GP10 | GP22 |
| 11 | S5 | GP5 | GP11 | GP26 |
| 2,4,6,8,10,12 | GND reference | GND | GND | GND |

The connectors carry 3.3-V control signals and ground references only. They provide no servo-positive rail. Each output module requires dedicated power and adequately dimensioned power-return wiring; the signal ground cable is not a substitute for the power return. Define and verify the overall ground arrangement with the supplies, USB and oscilloscope before powering the setup.

The complete schematic models the first output module connected to port A. The same module design can later be built three times for 18 outputs; local S0..S5 map to the selected port. Leg/joint assignment stays in software configuration.

## Page 2: six-channel output module

J101 receives the signal cable. J102 is the independent servo-power input. J110..J115 are the six servo connectors, each pin 1 signal, pin 2 positive, pin 3 ground. Verify the actual cable orientation rather than copying the drawn symbol orientation.

Each channel repeats the non-inverting two-BC547B buffer: 2x4.7k base resistors, 2x1k collector pull-ups, 2x100k pull-downs, and a 180-ohm output resistor. Each channel has a 100-nF supply capacitor. C100 is the available 470-uF/35-V local bulk capacitor. TP101 exposes channel 0's buffered output; TP102 provides a nearby ground connection.

Populate only channel 0 initially: Q101/Q102, R101..R107, C101 and J110, together with J101/J102/C100/TP101/TP102. The five reserved channels are explicitly marked DNP (do not populate): 55 parts total. These are circuits and space reservations, not permission to attach six motors to the current power card.

The buffer raises its output toward the servo rail. It is proposed for the roughly 5-V bench setup. A later higher-voltage motor supply requires another check of the signal interface; the servo supply rating alone does not establish an allowable signal voltage. Verify pulse width, edge shape and loading with the actual servo and cable.

The [RE200-LF output-module layout](../servo-output-re200-lf/README.md) places all six channels, separates the channel-0 first assembly from DNP wiring, and includes the measured 470-uF capacitor. Its reinforced load-path proposal does not establish six-servo power capability.

## Page 3: protected ONE-servo bench supply

The one-servo input path remains J201 -> F201 -> optional SW201 -> SR540 D201 -> protected SERVO_VS. C201 is the available 1000-uF/25-V capacitor, C202 a 100-nF bypass, and J202 the output. Set the supply provisionally near 5.5 V before D201 and measure the load-dependent voltage at the actual servo. Normal target at the servo remains approximately 5.0-5.3 V.

The external shunt module has been replaced by U201 (ST TL431CZ reference), U202 (one LM393 DIP-8, shown as active/unused/power units), Q201 (Infineon IRLZ44NPBF), R201..R208, D202 (1N5822), and C203. The positive feedback resistor R204 returns from the comparator output before gate resistor R206. R208 is the 2.2-ohm HS25 chassis resistor, excluded from the board and mounted separately on a suitable heatsink. D202 cathode faces SERVO_VS; its anode faces Q201 drain.

Calculated nominal switch points are 6.03 V on and 5.76 V off. At 6 V the resistor takes approximately 2.73 A / 16.4 W while switched on. These values do not establish the maximum rail transient, continuous thermal capability, or sufficient absorption for any motor load. [The design note](discrete-shunt-design.md) records assumptions, tolerances, startup/current-limit interaction, candidate cooling and manual qualification steps. TP203 is VREF, TP204 SENSE, TP205 gate and TP206 drain; TP201/TP202 remain rail/GND.

The PeakTech manual warns against regenerative motor loads. This circuit is a proposed mitigation, not verified approval of that operating condition. First check the protection without a servo. Use short power conductors, local capacitance, and a deliberate star connection between quiet sensing ground and the power return at the rail capacitor/Q201 source. The schematic GND symbol does not specify physical current routing.

Before six or eighteen servos, size the supply, fuse, diode, terminals, conductors, local capacitance and transient absorption using measured current and returned energy. The current card remains a ONE-servo proposal. J102's full-module power connector and the common power distributor still require engineering decisions. The earlier Pololu-based guide is historical and must not be used as the wiring description for this revision.

The [power-card mechanical choices](power-layout-components.md) record the selected board-mounted fuse holder and separate DC switch before its perfboard layout. C201 dimensions still require measurement.

## Place reservation and purchasing

The SVG reserves a 100x160-mm Euro board for the controller and another 100x160-mm board for one six-channel output module, plus a 100x160-mm power-card reserve and an external cooling assembly. These are conservative bench envelopes rather than final robot packaging. Allow access to the USB socket, keyed plug latches, screw terminals and shunt adjustment, and reserve mechanical clearance before drilling or soldering.

For the initial system, the BOM includes all three baseboard signal headers and one output-module header, one short 12-way cable with two matching female IDC connectors, and two 20-pin Pico sockets. The second and third six-channel modules are not yet counted as fitted assemblies. Existing Pico, 1N5819, SR540, 1000-uF/25-V and 470-uF/35-V parts can be reused. The updated BOM includes the discrete energy-dump parts, an external heatsink/mounting assembly, an optional DIP-8 socket and the separate 1k setup resistor. Remaining connectors/passives still need inventory confirmation. Cooling is a candidate arrangement pending mechanical and temperature checks.

## Earlier revision C verification

On 2026-10-27 KiCad 10.0.7 loaded all three revision C pages, exported the PDF, and reported zero ERC errors/warnings with standard checks and no item-specific exclusions. The exported netlist was checked for all 18 GPIO assignments, IDC ground pins, UART/expansion, the Pico supply diode and separate positive rails, all six NPN buffers, and complete pin partitions of the redesigned power circuit. This includes the TL431 reference/cathode tie, all three units of the single LM393, feedback before the gate resistor, MOSFET G/D/S, dump resistor and diode orientation. No Pololu component remains in the current netlist.

The project has 110 schematic components, including 55 DNP reservations, and 75 nets including five explicit unconnected pins. R208 remains in the BOM as an off-board part; U202 is counted once despite its three displayed units. The PDF's new page was rendered and visually reviewed. [connection-check.json](connection-check.json) records the exported source/PDF/netlist SHA-256 values and the scope of the checks.

This does not establish a physical build, correct clamp sizing, loaded voltage levels, heating, timing accuracy or 18-channel performance. Firmware was not changed and no firmware tests were run for this drawing change.

## Revision D verification

On 2026-11-08 KiCad 10.0.7 loaded all three pages and exported a new PDF and XML netlist. ERC reported zero errors and warnings with the current project configuration and no item-specific exclusions. The report lists disabled check categories; zero ERC findings do not mean all possible checks were enabled.

Netlist assertions checked EEPROM power, address 0x50, WP high, both 2k pull-ups, C1, distinct UART0/I2C1/UART1 nets, the complete J5/J6 pinout, all 18 servo assignments, IDC ground pins and Pico supply diode/positive-rail separation. Projected pin-to-net partitions and component values on the output-module and power sheets were compared with the prior published revision C netlist and are unchanged. This preserves the earlier topology review; it does not add hardware or thermal verification. All three exported PDF pages were rendered for visual review. The output-module drawing was shifted left by 7.62mm to keep the right-hand labels within the page, without changing its pin-to-net partitions. The title blocks use revision D; the design remains an unbuilt proposal.

There are 114 schematic components, 55 DNP reservations and 75 nets. The BOM includes U2/C1/R1/R2 and a removable EEPROM socket; port-pinout.csv uses GP22/GP26 for the final two port-C signals. The connection-check.json hashes identify the reviewed sources, exports and associated tables. The earlier RE200-LF controller wiring guide is not a revision D assembly guide. No firmware tests or hardware commissioning were performed for this schematic update.

## Re-export

From this directory in Windows Git Bash:

```bash
KICAD_CLI="/c/Program Files/KiCad/10.0/bin/kicad-cli.exe"
"$KICAD_CLI" sch erc --format json --exit-code-violations \
  --output erc-report.json pico-servo-system.kicad_sch
"$KICAD_CLI" sch export pdf --black-and-white \
  --exclude-pdf-property-popups --exclude-pdf-hierarchical-links \
  --output pico-servo-system.pdf pico-servo-system.kicad_sch
"$KICAD_CLI" sch export netlist --format kicadxml \
  --output pico-servo-system.net pico-servo-system.kicad_sch
```

For Ubuntu with KiCad installed, set KICAD_CLI=kicad-cli. Recheck connections/BOM after schematic edits; the stored validation record describes the exported revision only.

## Symbol provenance

Standard symbols come from the installed KiCad 10.0 libraries: MCU_Module, Device, Switch, Connector_Generic, Connector, Transistor_BJT, Transistor_FET, Reference_Voltage, Comparator and power. The inherited BC547 graphics are resolved from Q_NPN_CBE. Embedded library data retains the [KiCad Libraries License and electronic-design exception](https://gitlab.com/kicad/libraries/kicad-symbols/-/blob/master/LICENSE.md). The unused local external-shunt symbol is original HexaRoam material under MIT. U201 uses the pin-compatible standard TL431LP symbol with the ST TL431CZ value and ST datasheet; the ST TO-92 pin drawing was checked before assignment. Resolved inherited symbols preserve their standard pin definitions.

## Supply perfboard assembly guide

The [RE200-LF power layout](../servo-power-re200-lf/README.md) and [four-page PDF](../servo-power-re200-lf/assembly-guide.pdf) complement the baseboard and output-module layouts. Board-mounted fuse, off-board switch and cooled external resistor match the selected mechanical arrangement. All three remain unbuilt proposals. The one-servo supply does not qualify six-/eighteen-servo power distribution.
