# Discrete servo-rail shunt: revision C design notes

Status: analytical proposal, 2026-10-27. Not built, simulated, or electrically qualified. Revision C KiCad, PDF, netlist and BOM now implement this replacement for the unavailable Pololu module. ERC and explicit netlist checks cover the drawing, not electrical startup, transient or thermal behavior. Component aliases in the calculations map to the schematic below.

## Scope and operation

For the ONE-servo bench setup, retain the existing fuse, optional switch, SR540 isolation diode and 1000-uF/25-V rail capacitor. Connect the new shunt across SERVO_VS and GND, downstream of the isolation diode. Intended normal measured servo voltage is 5.0-5.3 V. Do not raise the supply to force it above the shunt threshold with the servo connected.

A TL431 reference and LM393 comparator drive a logic-level MOSFET, switching a 2.2-ohm dump resistor to ground. Positive feedback provides hysteresis. No firmware or Pico power is required. This is an energy dump, not a precision series voltage regulator, a supply disconnect, or independent redundant protection.

```text
SERVO_VS ---- RLOAD 2.2 ohm ---- DRAIN ---- Q201 ---- power GND
     |                             |
     +---- D202 cathode      anode -+

SERVO_VS -- RT 13.7k -- SENSE -- RB 10k -- quiet GND
                          |
                         RF 270k
                          |
SERVO_VS -- RP 2.2k ----- OUT -- RG 100 -- Q201 gate
                                             |
                                           RPD 100k
                                             |
                                         Q201 source

SERVO_VS -- RREF 330 -- VREF (TL431 cathode tied to REF)
                         TL431 anode -- quiet GND

LM393: (+) SENSE; (-) VREF; open-collector output OUT.
```

D202 is reverse-biased normally: cathode to SERVO_VS, anode to DRAIN. It provides a local path for dump-resistor/wiring inductance at turn-off; it is not a substitute for absorbing servo-returned energy.

## Schematic reference mapping

| Calculation alias | Revision C reference | Value |
| --- | --- | --- |
| RREF | R201 | 330 ohm |
| RT | R202 | 13.7k |
| RB | R203 | 10k |
| RF | R204 | 270k |
| RP | R205 | 2.2k |
| RG | R206 | 100 ohm |
| RPD | R207 | 100k |
| RLOAD | R208 | 2.2 ohm / HS25 |
| TL431 | U201 | ST TL431CZ |
| LM393 | U202 A/B/C | One DIP-8 IC, three displayed units |
| MOSFET | Q201 | IRLZ44NPBF |
| D202 | D202 | 1N5822 |
| CDEC | C203 | 100nF |

## Component connections

- Q201: Infineon IRLZ44NPBF, TO-220, drain to RLOAD, source to power ground; tab is drain. Do not bolt an uninsulated drain tab to grounded metal.
- LM393 DIP-8: pin 8 SERVO_VS, pin 4 quiet ground; pin 3 SENSE, pin 2 VREF, pin 1 OUT. Unused comparator: pin 5 ground, pin 6 VREF, pin 7 unconnected. Use a socket if desired.
- TL431CZ: STMicroelectronics TO-92, cathode and reference tied to VREF, anode to quiet ground. The reviewed ST TO-92 top-view diagram matches the standard symbol mapping REF=1, A=2, K=3; check the actual package orientation before soldering. Do not infer the pinout from a differently sourced TL431.
- CDEC: 100 nF ceramic >=25 V directly between LM393 supply pins. Retain the existing rail bypass capacitor. Do not add a capacitor directly across TL431 without checking its capacitive-load stability region.
- RPD connects at the MOSFET gate, after RG. RF returns from OUT before RG. These distinct points matter for the threshold calculation.
- Sense SERVO_VS at the protected rail capacitor. Join quiet reference/comparator ground to Q201 source/rail-capacitor negative at a short star connection. Keep dump current out of sense/reference ground conductors.

## Nominal calculations

Small resistors: 1% metal film, >=0.25 W. RLOAD: 2.2 ohm, 1%, ARCOL HS25 chassis resistor; see thermal restrictions below.

At the comparator threshold, SENSE = VREF. Kirchhoff's current equation gives:

```text
Vrail = VREF * (1 + RT/RB + RT/RF) - Vout * RT/RF
```

For VREF=2.495 V and assumed low output Vout=0.15 V, switch-on is 6.032 V. For switch-off, solve the output pull-up loading as well:

```text
D = 1/RP + 1/RF + 1/(RG + RPD)
A = (1/RP)/D
B = (VREF/RF)/D
Vout = A*Vrail + B
Voff = [VREF*(1+RT/RB+RT/RF) - B*RT/RF] / [1+A*RT/RF]
```

This gives Voff=5.755 V, output high=approximately 5.61 V, gate=approximately 5.60 V. These are calculated typical operating points, not guaranteed clamp limits. The 2.2k pull-up keeps low-state comparator sink current below 3 mA around the threshold, rather than assuming a near-zero low voltage at arbitrary current.

At 4.8 V, RREF provides approximately 7 mA; at 6 V approximately 10.6 mA. Its dissipation at 6 V is approximately 37 mW. This keeps the reference near the 10-mA datasheet characterization condition. Verify VREF on the built circuit.

For screening only, a corner calculation used all six divider/drive resistors at +/-1%, reference 2.495 V +/-55 mV initial tolerance and +/-17 mV temperature deviation, comparator offset +/-9 mV, input bias +/-250 nA, and output low 0-0.4 V. Predicted static thresholds span approximately 5.75-6.31 V on and 5.50-6.02 V off. This is not a certified bound: actual supplier/manufacturer limits, reference-current effects, noise, propagation, gate dynamics and wiring are not fully covered. Measure both thresholds and retain margin to the normal rail.

At 6 V, the dump draws approximately 2.73 A and dissipates 16.36 W. At 6.4 V and minimum 1%-tolerance resistance, these rise to approximately 2.94 A and 18.81 W. Returned current exceeding the dump's available current still raises rail voltage. The existing input fuse does not necessarily interrupt a locally circulating regenerative current.

The nominal 1000-uF capacitor stores only 1.755 mJ between 6.0 and 5.7 V. A net 1-A charging current raises its voltage by 10 mV per 10 us, ignoring ESR/ESL. The LM393 propagation time alone is not the system response time; gate charge through the 2.2k pull-up also matters. Measure the complete waveform, including local MOSFET drain overshoot. No maximum transient rail voltage has been established.

## Startup, shutdown and failures

Below normal supply voltage the reference and comparator input common-mode requirements are not assured. Dump-load conduction during ramp-up/down is possible; its duration is not established. With a low supply-current limit, unwanted conduction could prevent the rail from reaching the normal operating range. RPD alone does not establish guaranteed inactive startup. Startup is a required measurement with servo disconnected; add a defined undervoltage inhibit if startup is not reliable. At the intended >=4.8-V operating rail, the approximately 2.5-V inputs are within the classic LM393 common-mode range, with margin checked against the delivered device.

Once normal rail voltage is established and below the switch-on threshold, the comparator actively holds the gate low. With supply absent, RPD discharges the gate. Verify repeated startup, slow ramps and shutdown; do not assume a quiet startup from the ideal DC equations.

A shorted Q201 or stuck-high drive causes a continuous resistor load. An open resistor, open Q201, missing gate drive or some reference faults can eliminate protection. This is not fail-safe against all single faults. The laboratory supply's current limit and input fuse have separate purposes; neither proves regenerative-energy protection.

## Thermal and mechanical design

The HS25's 25-W rating requires the manufacturer's mounting and cooling conditions. Published ARCOL sheets differ in free-air ratings; do not qualify the build by selecting the larger number. Even the supplier sheet's 12.5-W free-air rating is below the nominal 16.36-W continuous load. A bare resistor on perfboard is not an acceptable continuous-fault design.

Candidate: separate open-air heatsink in the <=1.5-K/W class, for example Reichelt SK 08 100 SA, with a flat mounting region, drilled to the actual resistor dimensions and attached with thermal compound. At a conservative 20 W and 40 C ambient, this gives an idealized heatsink rise of 30 C before interface and local spreading effects. This is a preliminary mounting target, not a measured resistor hotspot temperature or approval of that heatsink/resistor combination. Check the manufacturer's curve and measure temperature during controlled dummy-load tests.

Keep hot components away from cables, electrolytic capacitors and fingers. Reserve external heatsink space; the former approximately 80x60-mm power-board envelope does not include this cooling assembly. Size power tracks/wires/terminals for dump current as well as servo current. A MOSFET heatsink decision requires measured switch-waveform loss and case temperature; advertised headline current/power ratings do not establish perfboard performance.

## Incremental procurement list

These parts replace the external shunt module; they are NOT a replacement for the complete system BOM. Supplier listings were checked on 2026-10-27; stock can change. Buy only after reviewing this draft and the cooling arrangement.

| Qty | Item | Function / Reichelt search name |
| --- | --- | --- |
| 1 | ST TL431CZ, TO-92 | Reference; prefer identifiable manufacturer over the earlier generic TL 431 ACLP listing |
| 1 | LM 393 DIP | Comparator; identify delivered manufacturer and confirm datasheet |
| 1 | IRLZ 44N / IRLZ44NPBF | Logic-level MOSFET, Infineon |
| 1 | ARC HS25 2R2 F | 2.2-ohm 25-W chassis resistor, 1% |
| 1 | 1N 5822 | 3-A Schottky for local dump-branch inductance |
| 1 each | 13.7k, 10k, 270k, 2.2k, 330, 100, 100k | Seven metal-film resistors, 1%, >=0.25 W |
| 1 | 100 nF >=25 V ceramic | Local comparator bypass |
| 1 optional | DIP-8 socket | Comparator socket |
| 1 for setup | 1k >=0.25 W | Current-limiting resistor for the low-energy threshold test |
| 1 assembly | Suitable heatsink, mounting hardware, thermal compound | Cooling candidate described above; mechanical review pending |

No potentiometer is required in this fixed-threshold proposal. A later change of servo supply requires recalculation rather than turning an undocumented adjustment. Reuse existing SR540, 1000-uF/25-V and 470-uF/35-V capacitors in their existing roles.

## Manual verification to prepare before connecting a servo

1. With all power off, verify polarity, isolation diode direction, the dump branch and no unintended VBUS/VSYS connection. Establish the common-ground arrangement before connecting grounded instruments.
2. Leave servo and Pico disconnected. Use a current-limited supply through a 1k >=0.25-W test resistor to feed the protected rail directly at approximately 7 V. Ensure the capacitor is discharged before changes. Check the approximately 2.5-V reference and the comparator/gate states. This low-energy test cannot validate full dump current.
3. Observe repeated capacitor charge/discharge cycles: record rail turn-on/off levels and gate waveform together. The limited source cannot hold the rail above the threshold continuously when the dump is on. Repeat power ramps and shutdown; record any startup load pulses or failure to reach the normal rail under current limiting.
4. Separately prepare a controlled current-injection/dummy-load and thermal test with an energy/current limit. Do not rely on step 2 or a DC voltage reading to qualify switching response, heat dissipation, component failure behavior or returned-energy capacity. Define this test before performing it; firmware tests do not cover it.
5. Only after the protection tests, measure the first unloaded servo with the oscilloscope at the actual servo connector, beginning near neutral and with conservative pulse limits. Use observed current and returned-energy waveforms to decide whether the dump is sufficient.

## Sources and procurement evidence

- [ST TL431 datasheet, Rev 17](https://www.st.com/resource/en/datasheet/tl431.pdf)
- [TI LM393 family datasheet](https://www.ti.com/lit/ds/symlink/lm393.pdf): classic LM393 specifications; do not silently substitute improved B-version limits.
- [Infineon IRLZ44N datasheet](https://www.infineon.com/assets/row/public/documents/24/49/infineon-irlz44n-datasheet-en.pdf)
- [ARCOL manufacturer sheet hosted by Reichelt](https://cdn-reichelt.de/documents/datenblatt/B300/KLHS_DATA_E1.pdf) and [derating curves](https://cdn-reichelt.de/documents/datenblatt/B300/KMHS_DATA_E2.pdf)
- Reichelt: [TL431CZ](https://www.reichelt.de/de/de/shop/produkt/spannungsreferenz_2_495_36v_100ma_to-92-386873), [LM393](https://www.reichelt.de/de/de/shop/produkt/komparator_2-fach_dip-8-10510), [IRLZ44N](https://www.reichelt.de/de/de/shop/produkt/mosfet_n-ch_55v_47a_110w_to-220ab-129819), [HS25](https://www.reichelt.de/de/de/shop/produkt/drahtwiderstand_axial_25_w_2_2_ohm_1_-233458), [13.7k resistor](https://www.reichelt.de/de/de/shop/produkt/widerstand_metallschicht_13_7_kohm_0207_0_6_w_1_-11502), [1N5822](https://www.reichelt.de/de/de/shop/produkt/schottkydiode_40_v_3_a_do-201ad-41852), [heatsinks](https://www.reichelt.de/de/de/shop/kategorie/profilkuehlkoerper-7772).

The earlier generic TL 431 ACLP listing's linked PDF did not identify the advertised reference correctly when checked; use the delivered manufacturer marking and an appropriate primary datasheet. The ST listing also has inconsistent descriptive text mentioning TS431; the stated manufacturer part TL431CZ and actual device identification must control selection. Never substitute a 1.24-V TS431 into these calculations.

Five unmodified datasheet downloads are stored under ignored local-data/servo-protection with SHA-256 hashes. Direct ST downloads timed out; the supplier-hosted TL431/TL432 PDF is Rev 15 (February 2021), while the primary web reference consulted above is Rev 17. Compare those revisions before schematic release. They remain third-party documents, outside the project's MIT license. Local storage does not establish redistribution permission.
