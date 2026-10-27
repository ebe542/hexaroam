# Single-servo bench board, revision A proposal

The current system architecture is [revision C: modular Pico / servo system](pico-servo-system/README.md), including the Pico, protected external supply, eighteen signal routes and a six-channel output-module exemplar. This earlier Pololu-based document is historical; use revision C for current supply wiring and parts.

Status: design proposal for assembly on a 100 x 160 mm plated-hole prototyping board. Not assembled or electrically verified. This is a one-servo bench circuit, not an 18-servo power distributor.

## Intended equipment and operating points

- Raspberry Pi Pico powered by its existing USB connection.
- Miuzei MZ996, identified by the supplied leaflet; the order/model code reported by the owner is MS21-1-D.
- Separate current-limited servo supply. The PeakTech 6225A manual warns against regenerative motor loads: do not directly connect the servo to its output. The isolation and transient clamp below are intended to address that hazard; their actual performance must be measured before commissioning.
- Proposed supply setting: approximately 5.5 V before D1. Expect a lower, load-dependent servo voltage after D1; measure it at J2. Target roughly 5.0-5.3 V, remaining within the leaflet's 4.8-7.4 V range during operation.
- Proposed shunt set point: approximately 6.0 V, above the normal rail. Verify threshold and dynamic overshoot. This is not a precision regulator for the normal supply voltage.
- Start with no mechanical load and a fixed 1500-us pulse, not the automatic angle-sweep demo.

## Power circuit

```text
J1 +SUPPLY -- F1 -- SW1 -- A D1 K -----------+------------ J2 RED (+VS)
                              SR540          |
                              band ->        +-- C1 1000 uF / 25 V --+
                                             |   positive at +VS     |
                                             +-- C2 100 nF ----------+
                                             |                       |
                                             +-- U1 SHUNT +          |
                                                 SHUNT - ------------+
                                                                     |
J1 -SUPPLY ----------------------------------------------------------+-- J2 BLACK
                                                                     |
                                                       separate Pico GND lead
```

C1, C2 and the shunt connect in parallel between +VS and GND. D1 is in series; its cathode band faces the servo rail. Connect C1 negative to GND. U1 is a complete two-terminal shunt module, not a voltage regulator placed in series.

Use a common ground junction near the power connector/C1. Route the servo's positive and return currents directly on short reinforced conductors. The Pico ground and signal-buffer ground connect separately to this junction. Servo current must not flow through Pico headers or the thin logic ground wiring.

## Non-inverting signal level buffer

Two NPN stages raise the pulse amplitude from 3.3 V to approximately the servo supply voltage. The two inversions cancel. The buffer uses +VS rather than a separately powered 5-V logic rail.

```text
                       +VS                             +VS
                        |                               |
                      R2 1k                           R4 1k
                        |                               |
                        +--- R3 4k7 --- B Q2             +--- R6 180R --- J2 ORANGE
                        |              C ---------------+                    |
                        C              E                                   R7 100k
GP0 --- R1 4k7 ---+--- B Q1              |                                    |
                 |      E              |                                    |
               R5 100k  |              |                                    |
                 |      |              |                                    |
GND -------------+------+--------------+------------------------------------+
```

Q1 and Q2: BC547B, NPN, TO-92. C, B and E denote collector, base and emitter; identify physical lead order using the purchased manufacturer's datasheet. Do not substitute physical pin positions from a different transistor type.

Explicit connection list:

| Node | Connections |
| --- | --- |
| Q1 base | R1 from GP0; R5 to GND |
| Q1 collector | R2 to +VS; R3 to Q2 base |
| Q1 emitter | GND |
| Q2 base | R3 from Q1 collector |
| Q2 collector | R4 to +VS; R6 to servo signal |
| Q2 emitter | GND |
| Servo signal | R6 from Q2 collector; R7 to GND |

With GP0 Low or disconnected, R5 keeps Q1 off, Q2 is driven on, and the servo signal is Low. With GP0 High, Q1 pulls Q2's base drive Low and R4 raises the servo signal. Without +VS, the buffer provides no active High output to the servo. No normal 5-V signal path is connected to the Pico GPIO.

This is a transistor pull-up output, not a push-pull logic driver. Its actual High level depends on the servo input load, and its propagation delays can change pulse width slightly. Measure GP0 and the servo signal together before connecting the motor. Servo input thresholds are not documented in the supplied leaflet.

## Parts

| Ref | Quantity | Value or requirement | Owner inventory / purchase |
| --- | --- | --- | --- |
| D1 | 1 | SR540 Schottky diode | Available; confirm marking/manufacturer |
| C1 | 1 | 1000 uF, 25 V electrolytic | Available |
| C2 | 1 | 100 nF, at least 25 V, ceramic | Confirm inventory |
| Q1, Q2 | 2 | BC547B, TO-92 | Purchase if absent |
| R1, R3 | 2 | 4.7 kohm, 0.25 W | Confirm inventory |
| R2, R4 | 2 | 1 kohm, 0.25 W | Confirm inventory |
| R5, R7 | 2 | 100 kohm, 0.25 W | Confirm inventory |
| R6 | 1 | 180 ohm, 0.25 W | Confirm inventory |
| U1 | 1 | Adjustable low-voltage shunt, e.g. Pololu #3778 | Missing |
| F1 | 1 | Fuse holder and provisional T2.5A fuse | Coordinate final rating with supply limit and measured startup current |
| SW1 | 1 | Servo-power switch rated at least 5 A at 12 V DC | Purchase if absent |
| J1 | 1 | Two-pole screw terminal, rated at least 5 A | Servo supply input |
| J2 | 1 | Three-pole screw terminal, rated at least 5 A | Servo signal, positive and ground; use a matching servo extension/adapter |
| J3 | 1 | Two-pole header for GP0/GND | Logic connection |
| TP | 4 | Test points: GND, +VS, GP0, servo signal | Prefer secure ground attachment |
| Board | 1 | 100 x 160 mm individual-pad prototyping board | Avoid relying on narrow stripboard tracks for motor current |
| Wire | As needed | Short 0.75 mm2 flexible power wires; thinner wire for logic | Insulated, with strain relief |
| Setup resistor | 1 | 1 kohm, 0.25 W | For isolated shunt adjustment at modest test voltage |

Fuse rating is provisional for one servo, not a measured trip-current design. A fuse protects the wiring against sustained faults; it does not clamp voltage or provide reliable transistor/servo protection. SR540's nominal 5-A rating also requires attention to temperature and lead cooling.

The Pololu #3778 has an onboard potentiometer and adjusts over approximately 4-18 V. No extra potentiometer is needed for that version. A fixed 13.2-V #3771 must not be used unmodified on this rail.

Pololu shunt boards clamp occasional short transients. The 15-W designation is not a continuous dump-load rating. Adjust and test the shunt separately through the setup resistor before connecting it across a supply or attaching the servo. Continuous voltage above its set point can destroy it.

## Assembly and verification order

1. Assemble and inspect without the Pico or servo connected. Check diode and capacitor polarity and transistor lead assignments.
2. Adjust and verify U1 separately using the manufacturer's current-limited resistor procedure.
3. Fit the power circuit. Verify normal rail voltage and absence of unintended clamp current, initially without the servo.
4. Verify ground references before connecting USB, the supply return and an earth-referenced oscilloscope. Scope ground clips go to common GND only.
5. Connect the Pico and test the buffer without the servo. Compare Low/High levels, pulse width and edge timing at GP0 and J2 signal. Confirm signal Low when +VS is off and when GP0 is Low.
6. Agree on the fixed-center commissioning firmware and current-limit setting before attaching the servo. Enable servo power before starting pulses; stop pulses/hold GP0 Low before switching servo power off. Do not use the automatic sweep for initial commissioning.
7. With the servo attached unloaded, measure +VS dips/spikes and diode temperature. Confirm the clamp can absorb actual returned energy before broader movement tests.

## Later expansion

Repeat the signal-buffer channel for additional servos if retained after measurement. Recalculate supply current, protection, connectors, conductors, local decoupling and transient absorption for every added power group. Eighteen possible servo currents must not be routed through this one-servo board's 5-A parts or ordinary signal headers.

## Sources

- Servo electrical limits and cable colors: owner-supplied photographs of the Miuzei MZ996 leaflet, 2026-10-27. Running-current figures are not stall-current ratings.
- PeakTech operating restrictions: owner-supplied PeakTech 6225A/6226 manual, revision 09/2021, PDF page 2.
- [Pololu #3778, Fine-Adjust LV shunt](https://www.pololu.com/product/3778): connection, adjustment, and transient-only operation.
- [onsemi BC546/BC547 family datasheet](https://www.onsemi.com/pdf/datasheet/bc546-d.pdf): transistor ratings and package connections.
- [Example SR540 manufacturer specifications](https://semiware.com/schottky-diodes/sr540/): the actual purchased diode's manufacturer remains unconfirmed.

## KiCad schematic and PDF

The [KiCad project and export guide](single-servo-test-board/README.md) contains an editable native schematic, a one-page [PDF](single-servo-test-board/single-servo-test-board.pdf), and the ERC/netlist reports. KiCad 10.0.7 loaded and exported the design on 2026-10-27; ERC reported zero errors and warnings. Exported pin connectivity and the PDF layout were checked. The physical circuit remains an unbuilt proposal.
