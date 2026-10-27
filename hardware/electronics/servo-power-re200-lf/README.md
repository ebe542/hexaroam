# One-servo protected supply on Roth RE200-LF

Revision P1, 2026-10-27. **UNBUILT placement and hand-wiring proposal.** The schematic remains [revision C](../pico-servo-system/README.md); electrical calculations and commissioning limits are in [discrete-shunt-design.md](../pico-servo-system/discrete-shunt-design.md). This layout does not establish successful protection or a current rating.

## Files and physical scope

- [Editable KiCad board](servo-power-re200-lf.kicad_pcb) and [project](servo-power-re200-lf.kicad_pro).
- [Four-page assembly guide](assembly-guide.pdf): 1 component view, 2 mirrored solder view, 3 load wiring, 4 control wiring. A4 landscape, actual size; verify the 50-mm calibration bar and real hole alignment before use.
- [Pin map](pin-map.csv), [wire list](wiring.csv), [assembly BOM](assembly-bom.csv), [layout model](layout-model.json), [check record](layout-check.json).
- SVG views provide the same geometry with hover labels. Hole IDs always refer to the component view: C01..C61, R01..R38. Solder views mirror columns horizontally only.

26 schematic components correspond to 54 physical pins, plus four existing-pad wire junctions BV/BG/BD/GS. The board has 30 footprints, 58 occupied holes and 47 insulated wire connections. U202 pin7 is intentionally unconnected. No copper tracks are provided; KiCad unrouted indications are expected. Do not export Gerbers for fabrication. Native front-side THT rings are virtual; the actual RE200-LF is single-sided, unplated, with underside pads only.

SW201 and R208 footprints represent **two-pole cable interfaces**, not the physical switch and resistor. Install the actual APEM switch separately on its bracket and the actual ARCOL HS25 2.2-ohm resistor on a suitable external heatsink. Never replace either external component with a wire short. If the optional switch is deliberately omitted, bypass its interface only with a suitably rated conductor. Keep the dump loop short and supported; the layout reserves board space, not a thermal enclosure design.

This supply is for **one servo only**. The Pico baseboard has its own Manson-to-VSYS feed; it is not powered from J202. Connect shared signal ground according to the system schematic. Three output modules reserve eighteen servo signals, but the present supply is not an eighteen-servo distribution board.

## Parts and fit

Use [the BOM](assembly-bom.csv) together with [mechanical part references](../pico-servo-system/power-layout-components.md). Four Phoenix 1715721 / MKDS 1.5/2-5.08 terminals are proposed for J201, J202 and the SW201/R208 interfaces. Equivalent terminals require their own body, drill, pin and current-rating check.

- C201, owner measured: 1000 uF/25 V, diameter 10 mm, height 20 mm, approximately 5-mm wire spacing. Gently form to 5.08 mm without stressing the seal. Pin1 positive, pin2 negative. C202/C203 are provisional 100-nF disc envelopes; match actual parts before soldering.
- F201 holder: ESKA 503.400, cover 503.310, fuse 5x20 mm. Body 22x9x13.5 mm. Native pitch 22.60 mm versus nine grid intervals = 22.86 mm. Form terminal ends gently by about 0.13 mm per side when centered; do not force the holder. Dry fit before populating. T2.5A remains a provisional fuse choice pending measured behavior.
- Widen only the occupied terminal/Q201 holes to 1.3 mm, F201/D201/D202 holes to 1.5 mm; see pin-map.csv. Existing pad diameter remains 2.2 mm (nominal annulus 0.35 mm at 1.5-mm drill). Inspect for pad damage and support heavy parts/wires mechanically. Drilling/fit is not physically verified.
- U201 is specifically ST TL431CZ: REF1, anode2, cathode3. The wide TO-92 row requires spreading leads to 2.54-mm spacing. Do not substitute another TL431 package/pinout without checking its datasheet.
- U202 is LM393 in DIP8, notch toward the upper board edge. Verify pin1 before insertion. Q201 is IRLZ44NPBF: gate1, drain2, source3. Its metal tab is drain; insulate mounting hardware/heatsink as required. Other package variants require review.
- D201 SR540 and D202 1N5822: stripe/cathode = pin1, anode = pin2. The DO-201AD envelopes and 15.24-mm formed spacing are proposed; verify actual diameters and leads.
- R201..R207 use 10.16-mm axial spacing and a 6.3x2.5-mm body envelope; select ratings from the circuit design. R208 is external and is not an axial board resistor.
- Optional M1..M4 mounting reserves are drawn, not factory holes. Check actual mounting clearances before drilling. Leave room above the 20-mm capacitor, fuse cover and screw terminals.

Local capacitor/holder footprints are derived from KiCad standard geometry or explicit manufacturer dimensions. The capacitor lead spacing and fuse terminal spacing adaptations are documented; library licensing follows the KiCad Libraries License with its electronic-design exception. The two-pin holder geometry is original project material.

## Connections and assembly order

1. Print at actual size, check the ruler, board orientation and parts against their envelopes. Dry fit the fuse holder, terminals, capacitors and MOSFET before drilling or soldering.
2. Fit low components, DIP socket if used, then taller components. Keep fuse removed, both supplies disconnected and external cables disconnected during continuity work.
3. Install the load wires from PDF page3 / CSV group `load`. Use insulated conductors sized for the measured load and dump currents, reinforced at pad nodes. A drawn line indicates endpoints, not a bare solder bridge. Do not rely on 35-um isolated pad copper or a small component lead as a power bus. BV/BG/BD are junction pads only, not extra purchased parts.
4. Install control wires from page4 / group `control`. GS provides quiet ground; join it at Q201 source as listed. R207 returns directly to that source. Keep reference/sense wiring away from dump-current wiring. Crossings are insulated passovers, never implicit junctions.
5. Connect external switch/resistor harnesses. SW201 terminal1 is fused input, terminal2 goes to D201 anode. R208 terminal1 is SERVO_VS, terminal2 is MOSFET drain. The resistor has no polarity. Check actual switch ON contact pair before attachment.
6. Inspect diode/capacitor polarity, transistor/reference pinout, resistance between rails and every listed endpoint with power off. Do not treat the check record as a substitute for these measurements.
7. Follow the existing staged [commissioning plan](../pico-servo-system/discrete-shunt-design.md) before attaching a servo. Normal rail target is 5.0-5.3 V. The dump nominal thresholds are not guaranteed maximum transient voltages. Startup, PSU interaction, clamp behavior, fuse selection and heating remain unverified.

J201: pin1 PSU positive, pin2 ground. J202: pin1 SERVO_VS, pin2 ground. Test points: TP201 rail, TP202 ground, TP203 reference, TP204 sense, TP205 gate, TP206 drain. Support cables so screw-terminal forces cannot peel pads. The external resistor may dissipate substantial power; its nameplate rating assumes appropriate cooling.

## Review boundaries

The exported board is checked against a fresh revision-C netlist: references, values, nets, pad positions, drill sizes, body envelopes, distinct occupied holes, wire-graph completeness, polarities and quiet-ground topology. These are artifact checks, not hardware tests. Wire routes are endpoint guides; actual routing, clearances, joints, transient response and thermal behavior require manual review and bench measurements. No firmware changes or firmware tests were made.

All layout files are a consistent snapshot. Moving a footprint in KiCad does not update the CSV/PDF/SVG/model automatically. Update and recheck every artifact before assembly. Do not update this board from the whole three-sheet schematic without explicitly filtering the supply sheet: the other sheets belong to separate physical boards.
