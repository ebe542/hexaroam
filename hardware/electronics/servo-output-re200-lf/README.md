# Six-channel servo-output module on RE200-LF

P1, 2026-10-27: unbuilt hand-wiring layout for page 2 of the [revision C system schematic](../pico-servo-system/README.md). All six channels are placed; only channel 0 is initially populated and connected. The present power system remains a ONE-servo proposal.

## Review files

- [Editable KiCad board](servo-output-re200-lf.kicad_pcb), [project](servo-output-re200-lf.kicad_pro).
- [Assembly PDF, eleven pages at 1:1](assembly-guide.pdf). Pages 1-4 cover the first assembly; pages 5-11 are future/DNP reservations.
- [Component view](placement-top.svg), [mirrored solder view](placement-bottom.svg).
- First wiring: [power/ground](wiring-initial-power.svg), [channel 0](wiring-channel-0.svg), and the [32-wire CSV](wiring-initial.csv).
- [Complete 142-wire CSV](wiring.csv), [pin map](pin-map.csv), [assembly BOM](assembly-bom.csv).
- [Layout model](layout-model.json), [connection/geometry record](layout-check.json).

The extra twelve BVx/BGx locations are existing solder pads used as wire junctions, not twelve purchased components. The complete layout has 71 schematic components, 168 source pins and these twelve additional wire-anchor pads. Native KiCad DNP marks include the 55 reserved channel parts and ten reserved wire anchors. Initial assembly has 16 electronic/connector parts, BV0/BG0 and 32 wire connections.

## Board coordinates and printing

Use Roth RE200-LF, single-sided FR4, 1.5-mm thickness, 61 columns by 38 rows, 2.54-mm pitch and 1.0-mm existing holes. The nominal board size is 100x160 mm; the manufacturer drawing gives 160.15x100.20 mm. Our coordinate origin is the drawing's first hole at x=3.89, y=3.07 mm, viewed from the component side with copper underneath:

```text
C01-R01 = top-left hole
x = 3.89 + (column - 1) * 2.54 mm
y = 3.07 + (row - 1) * 2.54 mm
```

The solder view is mirrored horizontally: column 61 at left, column 1 at right, row 1 still at top. Hole IDs refer to the same physical holes in every view, independent of manufacturer printing. Never mirror the solder-side PDF a second time.

Print at Actual size / 100%, without Fit/Shrink. Check the 50-mm line and three separated existing holes against the real board before assembly. Dimensions and connector bodies still need a dry fit; do not treat the drawings as a production PCB or drill every existing hole.

## First assembly and connector polarity

Fit common J101, J102, C100, TP101/TP102 and channel 0: Q101/Q102, R101..R107, C101 and J110. Leave all other channel parts and BV1..BV5/BG1..BG5 unpopulated/unwired. The optional extension requires a fresh power-system review.

| Item | Assignment / physical location |
| --- | --- |
| J101 | Keyed 2x6 IDC input, pin 1 C07-R04; odd pins S0..S5, all even pins GND |
| IDC key notch | Toward smaller column numbers in component view; opposite side in mirrored solder view |
| J102 | Separate servo power: pin 1 positive at C20-R04, pin 2 GND at C18-R04 |
| J110..J115 | Each: pin 1 signal, pin 2 positive, pin 3 GND |
| C100 | Positive pin 1 C26-R07, negative pin 2 C28-R07 |
| TP101 | Buffered channel-0 signal, C22-R22 |
| TP102 | GND, C14-R04 |
| BV0 / BG0 | Dedicated existing-pad junctions: positive C05-R22 / ground C07-R22 |

Servo headers are horizontal three-pin rows. In component view the pins run 1,2,3 toward larger column numbers; the solder view reverses their visual order. Verify the servo plug and the actual wire colors before insertion. Confirm the molded pin-1/key marks on the actual IDC parts, and test a straight-through cable end to end with power off.

The input interface matches all three Pico ports. On port A local S0..S5 correspond to GP0..GP5, on B to GP6..GP11, on C to GP12..GP17. This one module layout is reusable; channels do not contain leg/joint assignments or a second controller.

## Measured and assumed component geometry

The owner measured the existing 470-uF/35-V C100 as diameter 10 mm, height 20 mm, 5.00-mm lead pitch. This layout uses adjacent-grid spacing 5.08 mm; gently form the flexible leads by the total 0.08-mm difference without pushing sideways on the capacitor seal. Observe polarity. Reserve clearance above its 20-mm body.

100-nF capacitor C101 uses a compact radial/disc reference envelope approximately 5mm wide by 2.5mm thick. Its reference 2.50-mm lead pitch is gently formed to 2.54 mm in the local footprint. Verify actual purchased capacitors fit; do not replace them with larger bodies without checking the model.

The BC547B footprint uses a wide inline TO-92 lead row, 2.54-mm spacing. The selected purchasing reference is Diotec BC547B (Reichelt BC 547B DIO); its checked manufacturer drawing defines pins 1/2/3 as collector/base/emitter. The flat-body edge is drawn in the placement view. Identify the manufacturer/package of the supplied transistors and check its pinout before soldering; do not guess from a different transistor or view direction. Spread leads gently away from the body as needed, and keep some lead length for forming. The modeled TO-92 body reserve is 5.2x5.2 mm.

Resistors use a DIN0207 reference body 6.3x2.5mm with 10.16-mm hole spacing; metal-film parts >=0.25W with bodies fitting that envelope are suitable references. The schematic values remain unchanged: 4.7k input/interstage, 1k collector pull-ups, 100k pull-downs, 180-ohm output series resistor. Dry-fit the actual parts.

J102 uses the same Phoenix MKDS 1,5/2-5,08 reference style as the Pico supply terminal, part 1715721. Enlarge ONLY C20-R04 and C18-R04 from 1.0 to 1.3 mm if using that part. Inspect pad rings and support the body during screw tightening. A terminal's headline current rating is not a rating of this entire handmade board.

Optional M3 reserves are C03-R03, C59-R03, C03-R36 and C59-R36. The drawn 3.2-mm holes do not exist yet. Leave the marked 4-mm-radius areas and nearby pads clear; decide real spacers/washers before drilling and inspect for damaged copper.

## Power wiring and initial-versus-future wires

The wire tables distinguish logic connections from role=load connections (marked * in the PDF). Load wires directly connect J102, C100, the BV/BG junctions and servo power/return pins. Motor supply current must not pass through resistor or transistor leads, or return through the IDC cable. Choose reinforced conductors appropriate for measured single-servo current and provide strain relief. Join wire ends directly at each solder junction rather than relying on a narrow solder bridge or forcing several wires into a hole.

BVx/BGx are already existing unused solder eyes; no extra header is required. A continuous suitably reinforced conductor with soldered branch taps can implement successive load-wire segments. Protect the insulated main conductor from component tails. The drawings show which endpoints connect, not a fully qualified physical wire route. There are no electrical connections at crossing lines. Keep exposed ends short and separated; crossings require intact insulation. Up to three wire ends are allocated to any one junction in the complete proposed tree.

The initial 32-wire graph is independently connected and uses ONLY fitted parts and BV0/BG0. It does not require any DNP resistor/servo pad as a feed-through. The full graph has 142 wires; its other 110 entries are future reservations. In particular, do not add all motor-power branches and then connect six servos to the present supply. Six-channel supply/return conductors, distribution, fuse, transient absorption and connector selection require current/thermal measurements and sizing first.

The buffer pulls signal high toward SERVO_VS. This remains the approximately 5-V bench proposal. Increasing motor supply voltage requires checking permissible signal voltage as well. The servo input thresholds and real loaded edge/pulse behavior are not established by the layout. Use the scope at the actual servo connector before attaching a motor.

## Manual build/check sequence

1. Check the 1:1 print, actual connector bodies, polarity and capacitor fit. Decide the terminal/mounting drilling first. Fit only common parts and channel 0; fit no servo yet.
2. Work through wiring-initial.csv or PDF pages 3/4, marking each completed wire. Keep reinforced power paths separate from logic/signal wiring and mechanically supported.
3. With power, Pico and servo disconnected, verify all listed endpoint continuities and inspect solder joints. Check there is no unintended positive-to-ground short, no adjacent IDC pin bridge and no accidental connection to DNP pads. Account for C100 charging during resistance measurements.
4. Verify the IDC cable pin-for-pin. Establish common ground with the two supplies/USB/UART/scope before power. First power the module from an appropriately limited low-voltage source with servo unplugged; confirm polarity and rail voltage.
5. Feed the established unloaded Pico PWM into S0. Measure J110 pin 1 relative to pin 3: output amplitude, pulse width, edge shape and low behavior. Confirm channels 1..5 are still unpopulated. Actual servo commissioning waits for the separately verified power protection.

These are owner-performed checks. No firmware tests or hardware experiments were run as part of drawing this layout.

## KiCad and verification boundaries

Open this controller-independent module board directly or through its local project. It derives from only the system's output sheet; do not blindly import the complete three-sheet schematic into this one physical board. The schematic itself was not changed.

F.Fab/F.SilkS show component references; Dwgs.User shows the existing grid and optional mounting reserves; Cmts.User shows insulated-wire endpoint guides. There are no copper tracks. Native unconnected/DNP indicators are expected and do not constitute a passed production PCB DRC. Standard native THT pad front rings are virtual: the actual RE200-LF is single-sided and not plated through. Do not send this layout as fabrication/Gerber data.

The local capacitor footprints adapt standard-library pad pitch to hand-formed leads on the existing grid. The PDF, SVG, CSV and KiCad files form one checked snapshot; moving a footprint in KiCad does not update the other files automatically. Reconcile them before building a changed revision. layout-check.json records hashes of the checked artifacts.

Independent checks reload the native KiCad board, compare all 168 source pins and DNP states to the freshly exported schematic, verify unique grid positions and separated body envelopes, and check both initial/full wire graphs. Additional checks ensure dedicated load-only paths from each servo power/ground pin to J102 without IDC or small-component leads. These establish drawing consistency, not current capacity, motor safety, timing accuracy or physical fit.

## Sources and licensing

- [System schematic and values](../pico-servo-system/README.md), output sheet revision C.
- [Roth RE200-LF drawing](https://www.roth-elektronik.com/pdf/en_RE200-LF.pdf); unmodified local copy/source hash under ignored local-data/perfboard/.
- [Diotec BC546..BC549 datasheet](https://diotec.com/request/datasheet/bc546.pdf), version 2026-03-30: pin1 collector, pin2 base, pin3 emitter; body and lead forms reviewed. Unmodified local PDF/source hash under ignored local-data/servo-output/. [Reichelt BC 547B DIO](https://www.reichelt.de/de/de/shop/produkt/bipolartransistor_npn_45v_0_1a_0_5w_to-92-219082) is the purchasing reference. A differently numbered manufacturer variant requires rechecking the package orientation; the type name alone is insufficient.
- C100 geometry is the owner's measured information from 2026-10-27.
- [Reference terminal at Reichelt](https://www.reichelt.com/de/en/shop/product/pcb_terminal_2-pin_spacing_5_08_mm_0_14_-_1_5_mm_-440491), part 1715721; [manufacturer dimensions](https://www.phoenixcontact.com/gb/products/1715721/pdf).
- Standard/derived footprint geometry comes from installed KiCad 10.0 libraries under the [KiCad Libraries License and design exception](https://gitlab.com/kicad/libraries/kicad-footprints/-/blob/master/LICENSE.md). Project placement/wire data and explanations are under the repository MIT license; upstream footprint and datasheet rights remain separate.
