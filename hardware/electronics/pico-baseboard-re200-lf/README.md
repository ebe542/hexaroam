# Pico baseboard on Roth RE200-LF

Revision P1, 2026-10-27: unbuilt placement and hand-wiring proposal for the controller assembly of the [revision C schematic](../pico-servo-system/README.md). This folder contains the Pico baseboard only, not the servo-output module or motor supply.

## Files

- [KiCad board](pico-baseboard-re200-lf.kicad_pcb) and [project](pico-baseboard-re200-lf.kicad_pro).
- [Seven-page assembly PDF](assembly-guide.pdf), A4 landscape, drawn at 1:1.
- [Component-side SVG](placement-top.svg) and [mirrored solder-side SVG](placement-bottom.svg).
- Solder-side wire groups: [ground/power](wiring-ground-power.svg), [port A](wiring-port-a.svg), [port B](wiring-port-b.svg), [port C](wiring-port-c.svg), [UART/expansion](wiring-other.svg). Hover a wire in a browser to highlight it and display its ID/net.
- [Complete wire endpoint list](wiring.csv), [physical pin map](pin-map.csv), [assembly parts](assembly-bom.csv).
- [Layout model](layout-model.json) and [verification record](layout-check.json).

The PDF, SVG and CSV files are a snapshot of the same model as the KiCad board. Moving a footprint in KiCad does not automatically update these documents. Reconcile all views and rerun the connection/geometry review before using a changed layout. Recorded hashes identify the checked snapshot.

## Board, coordinate system and views

Use the existing RE200-LF: FR4, 1.5-mm thickness, single-sided solder pads, 61 columns x 38 rows, 2.54-mm pitch, 2.2-mm pad diameter and nominal 1.0-mm holes. The manufacturer calls it 100 x 160 mm; its drawing dimensions are 160.15 x 100.20 mm. The illustrated first hole is modeled at x=3.89, y=3.07 mm from the drawing's top-left corner. Check actual board tolerances and orientation before relying on edge registration.

All IDs are defined looking at the COMPONENT side, with the copper underneath and the Pico USB facing left:

```text
C01-R01 = first column, first row (top left)
x = 3.89 + (column - 1) * 2.54 mm
y = 3.07 + (row - 1) * 2.54 mm
```

These are our coordinate labels, not a promise that the manufacturer's printed numbering has the same viewing orientation. In the solder-side drawings the board is flipped horizontally, like turning a page: USB is on the right, row 1 remains at the top, column 61 is on the left. Physical hole IDs never change. Do not mirror an already mirrored solder-side PDF again.

Print at Actual size / 100%, with all Fit/Shrink options disabled. Measure the 50-mm scale bar and align three widely separated existing holes with the paper before assembly. The PDF is not an instruction to drill all the existing holes.

## Placement and polarity

| Item | Physical position / orientation |
| --- | --- |
| U1 lower socket | C01..C20, R21; Pico pins 1..20 from left to right |
| U1 upper socket | C01..C20, R14; Pico pins 40..21 from left to right |
| Socket row separation | 17.78 mm; two individual 1x20 female headers |
| J1 | Pin 1 positive: C12-R04; pin 2 GND: C10-R04 |
| D2 | 1N5819 cathode/band (pin 1): C16-R08; anode (pin 2): C20-R08 |
| J2 port A | Pin 1 C53-R04; GP0..GP5 on odd pins; even pins GND |
| J3 port B | Pin 1 C53-R15; GP6..GP11 on odd pins; even pins GND |
| J4 port C | Pin 1 C53-R26; GP12..GP17 on odd pins; even pins GND |
| IDC key notches | Face toward the Pico/odd-pin side, left in component view, right in solder view |
| J5 UART | Pins 1..3 C06..C08, R32: TX / RX / GND; 3.3-V TTL |
| J6 expansion | Pins 1..8 C24..C31, R33: GP18, GP19, GP22, GP26, GP27, GP28, 3V3, GND |
| TP1 / TP2 | VSYS: C24-R06 / USB VBUS: C24-R10 |
| TP3 / TP4 / TP5 | GP0: C03-R26 / GP1: C07-R26 / GND: C12-R26 |

Confirm the molded pin-1 mark and key of the actual IDC connectors; pin numbers, not drawing resemblance, define the wiring. Standard footprint body dimensions are reference envelopes. The actual sockets, IDC cable plugs/latches and terminal must fit without contacting the Pico or neighboring parts. Preserve access above USB, BOOTSEL and the Pico SWD pads.

## Mechanical changes on the existing board

The reference J1 is a Phoenix MKDS 1,5/2-5,08 style footprint, 5.08-mm pitch. Its 0.9-mm square pins require 1.3-mm holes. ONLY its two occupied holes, C12-R04 and C10-R04, need widening for this reference part. Remove burrs, inspect the remaining pad rings and support the terminal body when tightening wires. A different terminal needs a fresh pin-size/body check; 5.00-mm pitch is not 5.08 mm. The matching reference part is Phoenix Contact 1715721, listed at Reichelt as MKDS 1,5 2 5,08. A different seller's compatible part still needs a dimension check.

D2 uses the existing 1.0-mm perfboard holes in this layout; verify that the delivered diode leads fit freely. The generic manufacturing footprint's larger drill allowance is not a requirement to force or automatically re-drill the board.

Optional M3 mounting reserves are C03-R03, C59-R03, C03-R36 and C59-R36. Circles show 3.2-mm holes and a 4-mm-radius keep-clear area; they are drawing guides, not existing holes. Decide spacer/washer arrangement and drill only if needed. Keep all nearby pads unused, inspect for damaged copper and leave wiring clear of mounting hardware. The schematic does not determine chassis-hole requirements.

## Hand wiring

All 61 entries in wiring.csv are separate insulated-wire connections. Each row gives component pin and physical hole at both ends. straight_mm is the geometric endpoint distance; cut_hint_mm is that distance plus approximately 20 mm, rounded up, not a measured routed length. Select a practical path and provide enough slack without obstructing sockets or mounting points.

Lines crossing in the SVG/PDF or Cmts.User layer do NOT form junctions. Do not reproduce crossings using bare wire. Only the named endpoints are electrically joined. Solder wire ends to the rear pad/component tail rather than attempting to push several wires through a hole already occupied by a connector pin. No pad has more than three wire ends in the proposed tree; check the real joint can be made cleanly and supported.

Use appropriately sized insulated hookup wire and protect it from sharp leads; keep power/ground mechanically secure. Each IDC's six ground pins are connected. The board carries Pico power and logic only: NO servo-positive rail or motor return current runs through it.

The exported KiCad 10 schematic includes all seven physical Pico GND pins (3, 8, 13, 18, 23, 28, 38). All seven are wired to the common baseboard ground, so the ground network is complete even with the Pico removed. AGND/pin 33, RUN, ADC_VREF and 3V3_EN are not externally wired. Grey pads identify the unused pins.

## Using the KiCad board

Open the .kicad_pcb directly, or use the separate project. The original three-sheet schematic models three physical assemblies, so do not blindly run Update PCB from Schematic from that full system into this controller-only board. There is no duplicated schematic in this folder.

- F.Fab / F.SilkS show component bodies/reference marks.
- Dwgs.User shows the existing perfboard grid, coordinates and optional mounting reserves.
- Cmts.User shows endpoint guide segments for insulated wires. Hide it for a clean placement view.
- There are NO copper tracks or routed jumpers. Ratsnest/unconnected indications are expected until physical hand wiring; this is not a claim of passing production PCB DRC.

KiCad represents the electrical through-hole component pads using a standard THT model with rings on both copper layers. Those front rings are virtual for this guide. The real RE200-LF is single-sided and not plated through. The thousands of unused solder pads/holes are drawing geometry, not a complete fabrication stackup. Do not order PCBs or drill files from this template. Its purpose is editable placement and hand-wiring documentation for an already manufactured board.

The local HexaRoam_Perfboard.pretty footprint retains only the 40 main THT pads of the standard Pico module footprint; SMD/castellation and Pico-local mechanical/SWD pads are not carrier socket holes. It models two 1x20 sockets rather than soldering the Pico directly to the baseboard. Its pin numbering matches the Pico module.

## Verification and owner-performed assembly checks

The saved KiCad board was independently reloaded and checked against the fresh revision C schematic netlist: 13 components, 96 physical pins and 61 wires. Every source root-sheet pin maps to exactly one physical hole. All occupied pads lie on the declared grid without overlap, the component envelope rectangles are separated and inside the board, and every connected net has a complete, non-shorting wire tree. Source/export hashes are in layout-check.json. This is document/geometry verification, not firmware testing or a hardware pass.

Suggested manual sequence for the owner:

1. Print at 100% and check registration; dry-fit sockets, diode and connectors. Check polarity/keying. Decide the terminal/mounting drilling before fitting parts.
2. Fit and solder sockets with the Pico removed. Solder the other parts, then work through the ground/power, A, B, C and other wire tables. Keep a paper checklist per wire.
3. With no power, Pico or external cables attached, check every listed connection and inspect for solder bridges. Verify separate positive nets: external +5V reaches only D2 anode; D2 cathode reaches VSYS; VBUS is not wired directly to either positive net. Use diode mode for D2; capacitor/semiconductor behavior after fitting Pico changes simple resistance readings.
4. Check supply polarity and ground arrangement with the existing USB/UART equipment. Insert the Pico only after inspecting the bare baseboard. Bring up external Pico power and verify VSYS; then repeat the established blink/UART/unloaded-signal checks. Servo power remains on the separate power/output assemblies.

## Reference documents and licensing

- [Roth RE200-LF manufacturer drawing, 072025](https://www.roth-elektronik.com/pdf/en_RE200-LF.pdf). An unmodified local copy and its source/hash are archived under ignored local-data/perfboard/; third-party terms apply.
- [Pico datasheet](../../references/raspberry-pi-pico/pico-datasheet.pdf), mechanical dimensions and external VSYS supply.
- [Reference terminal dimensions](https://www.phoenixcontact.com/gb/products/1715721/pdf): MKDS 1,5/2-5,08, 5.08-mm pitch, 0.9x0.9-mm pins, 1.3-mm holes.
- [Reichelt reference terminal listing](https://www.reichelt.com/de/en/shop/product/pcb_terminal_2-pin_spacing_5_08_mm_0_14_-_1_5_mm_-440491), checked 2026-10-27. Manufacturer/distributor PDF downloads returned HTTP 403 during local archiving; the primary dimensions and source links are recorded here and under local-data/perfboard/reference-terminal-notes.md. The Roth board PDF is archived locally.
- Standard footprint geometry comes from the installed KiCad 10.0 libraries. The socketed Pico derivative retains the [KiCad Libraries License and electronic-design exception](https://gitlab.com/kicad/libraries/kicad-footprints/-/blob/master/LICENSE.md). Project-specific placement, wire data and explanatory documents are HexaRoam material under the repository MIT license; upstream footprint and datasheet rights remain separate.
