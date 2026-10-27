# Electronics

Document controller pin assignments, wiring, connectors, power distribution, and revisions. Servo power must use a suitably dimensioned supply; document the common signal ground and verified voltages before commissioning.

## Current modular proposal: revision C

The [Pico / servo system](pico-servo-system/README.md) contains a socketed Pico baseboard with all 18 signal outputs, external Manson-to-VSYS supply protection, UART and expansion connectors, a six-channel output-module exemplar, and the separate one-servo bench power card with a discrete TL431/LM393/MOSFET energy dump.

- [KiCad project](pico-servo-system/pico-servo-system.kicad_pro)
- [Three-page PDF](pico-servo-system/pico-servo-system.pdf)
- [Placement concept](pico-servo-system/placement-concept.svg)
- [Parts list](pico-servo-system/bom.csv)

Only output channel 0 is initially populated. The remaining five channels reserve circuits and space; full six-/eighteen-servo power distribution still requires sizing and bench verification. No routed fabrication PCB is provided; the controller now has a separate perfboard assembly guide.

## Pico baseboard hand-wiring layout: RE200-LF

The [controller-only layout](pico-baseboard-re200-lf/README.md) provides an editable KiCad placement, a [seven-page 1:1 assembly guide](pico-baseboard-re200-lf/assembly-guide.pdf), mirrored solder-side wire groups, and CSV pin/wire lists for the existing Roth RE200-LF. It remains unbuilt; the output module has its own guide below; the power card also has its own guide below.

## Servo-output module hand-wiring layout: RE200-LF

The [six-channel output layout](servo-output-re200-lf/README.md) provides a [1:1 assembly guide](servo-output-re200-lf/assembly-guide.pdf), editable KiCad board, mirrored solder views and separate initial/future wire lists. Only channel 0 and common parts are fitted initially. The measured 470-uF capacitor is included; motor-current paths use dedicated wire junctions. Six-servo power capability remains unqualified.

## Protected-supply hand-wiring layout: RE200-LF

The [one-servo power layout](servo-power-re200-lf/README.md) provides an editable KiCad board, [four-page assembly guide](servo-power-re200-lf/assembly-guide.pdf), pin/wire lists and parts list. The board-mounted fuse holder is adapted explicitly to the grid; switch and cooled load resistor remain external. C201 uses the owner's measured dimensions. All three modular layouts are now documented as unbuilt proposals; supply commissioning remains required.

## Earlier one-servo proposal: revision A

The [original design and parts list](single-servo-test-board.md) and [standalone KiCad/PDF](single-servo-test-board/README.md) remain as the earlier circuit reference. Use revision C for the current architecture and connector references.
