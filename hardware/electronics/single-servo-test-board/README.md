# Single-servo bench schematic

Revision A is an unbuilt one-servo prototype proposal for a Euro-format prototyping board. It contains the series fuse, power switch and SR540, the 1000-uF and ceramic rail capacitors, an external adjustable shunt module, the two-stage BC547B signal buffer, connectors, and four test points. There is no PCB layout or perfboard placement drawing.

## Files and opening

- `single-servo-test-board.kicad_pro`: open this project in KiCad.
- `single-servo-test-board.kicad_sch`: native editable schematic, saved by KiCad 10.0.7.
- `single-servo-test-board.pdf`: one A4 landscape sheet, directly exported by KiCad in black and white without interactive property popups.
- `HexaRoam.kicad_sym` and `sym-lib-table`: project-local external shunt-module symbol. Keep them beside the project for library editing and ERC. Symbols are also embedded in the schematic.
- `single-servo-test-board.net`: KiCad-exported XML netlist for connection review.
- `erc-report.json`: ERC result from KiCad 10.0.7.

See the [design and parts guide](../single-servo-test-board.md) for operating points, assembly guidance, and component requirements. J2 pins are 1=signal/orange, 2=positive/red, 3=ground/black. Verify physical connector and transistor orientation before soldering; schematic pin numbers are not a physical placement drawing.

## Verified on 2026-10-27

- KiCad 10.0.7 successfully loaded and resaved the native schematic.
- ERC returned zero errors and zero warnings using its standard configuration, without item-specific exclusions.
- All eleven exported nets were compared against the intended pin-to-pin connection sets, including D1 cathode, capacitor polarity, and Q1/Q2 collector/base/emitter connections.
- The 22-component schematic was exported to a one-page PDF, rendered, and visually inspected. Labels and ground routing were adjusted for readability.

These are file/connectivity checks. They do not establish clamp performance, power-supply compatibility under regeneration, actual servo input loading, component heating, or a validated physical build. Supply and fuse settings remain provisional as stated in the design guide.

## Re-export

Run from this directory. On Windows Git Bash, select the installed CLI explicitly if it is not on PATH:

```bash
KICAD_CLI="/c/Program Files/KiCad/10.0/bin/kicad-cli.exe"
```

On Ubuntu with KiCad installed, use `KICAD_CLI=kicad-cli` instead.

```bash
"$KICAD_CLI" sch erc --format json --exit-code-violations \
  --output erc-report.json single-servo-test-board.kicad_sch

"$KICAD_CLI" sch export pdf --black-and-white \
  --exclude-pdf-property-popups \
  --output single-servo-test-board.pdf single-servo-test-board.kicad_sch

"$KICAD_CLI" sch export netlist --format kicadxml \
  --output single-servo-test-board.net single-servo-test-board.kicad_sch
```

## Symbol provenance

Standard symbols were embedded from the locally installed official KiCad 10.0 symbol libraries: Device (R, C, C_Polarized, D_Schottky, Fuse), Switch (SW_SPST), Connector_Generic (Conn_01x02, Conn_01x03), Connector (TestPoint), and Transistor_BJT (BC547, with inherited Q_NPN_CBE graphics resolved). Q1/Q2 instance values are BC547B. The external shunt module symbol is original HexaRoam material under MIT.

The KiCad community library data is covered by the [KiCad Libraries License](https://gitlab.com/kicad/libraries/kicad-symbols/-/blob/master/LICENSE.md), CC-BY-SA 4.0 with its electronic-design exception. The standard symbols are embedded as part of this design; no standalone copy of the standard libraries is distributed here.
