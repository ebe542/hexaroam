# Power-card mechanical choices before layout

2026-10-27. The owner requested the fuse holder directly on the perfboard and the optional switch mounted separately on a small bracket. The switch and holder are not yet owned. These choices prepare the power-card layout; the [P1 power-card layout and assembly guide](../servo-power-re200-lf/README.md) are now available as an unbuilt proposal. Existing one-servo limits, provisional T2.5A fuse and unqualified startup/thermal behavior remain unchanged.

## Proposed procurement references

| Function | Selected reference | Source / mounting |
| --- | --- | --- |
| F201 holder | ESKA 503.400, Reichelt ESKA 503400 | Board-mounted open holder, 5x20mm, body 22x9x13.5mm; PCB terminal pitch 22.6mm; pins 0.4x1.2mm; manufacturer PCB holes 1.5mm |
| Holder cover | ESKA 503.310, Reichelt ESKA 503310 | Matching cover, separately purchased |
| F201 fuse | T2.5A, 5x20mm | Still provisional pending measured servo startup/current behavior; holder current rating is not the selected fuse value |
| SW201 | APEM 3631NF/2, Reichelt HS 3631 NF2 | Separate ON-OFF switch on bracket; manufacturer resistive rating 12A/28VDC; 6.35x0.8mm quick-connect/solder terminals |
| Switch harness | Two adequately rated insulated leads / matching insulated female quick-connects or soldered lugs with insulation | Match termination to the actual conductor and support the harness; do not infer capacitive/inrush performance solely from resistive rating |

## Perfboard fit decisions to apply

Nine 2.54-mm intervals give 22.86mm, not the holder's 22.60mm. Account explicitly for the 0.26mm total difference (0.13mm per side if centered). The lead length is 4.5mm; the diagram's rectangular pin size and hole clearance must be considered. Do not force a rigid holder into offset holes. A gently formed lead-end variant and enlarged existing holes can be proposed, but dry fit and pad-ring inspection are required before building. Record actual chosen holes/drills in the power-card guide. Do not silently treat the footprint as 22.86mm nominal.

The switch mounts off-board. Provide a labeled two-wire interface rather than placing a fictitious PCB switch footprint. The bracket requires the manufacturer's panel cutout (catalog reference 12.2mm diameter) and clearance for the 35x21.5mm body, lever and insulated terminals. The current APEM sheet limits panel thickness to 2mm with all supplied accessories; check the actual nut/washer arrangement. Verify which two terminals close in ON before wiring. This remains a functional power switch, not an emergency-stop system.

R208 remains external on a heatsink, with its own labeled cable interface. Keep the clamp loop short and arrange its return separately from quiet comparator/reference grounding. Physical routing still needs transient and thermal checks.

C201 was measured by the owner: diameter 10mm, height 20mm, lead spacing approximately 5mm. The layout uses gently formed 5.08mm spacing. C100 on the output module is also measured and incorporated. The power guide records 1.3mm holes for terminals/Q201 and 1.5mm for F201/D201/D202; fuse-holder spacing is gently adapted to 22.86mm, retaining the actual 2.2mm pad diameter.

## Sources and local archive

- [ESKA holder at Reichelt](https://www.reichelt.de/de/de/shop/produkt/sicherungshalter_offen_22x9mm_print-376895).
- [ESKA manufacturer drawing hosted by Reichelt](https://cdn-reichelt.de/documents/datenblatt/C400/503.400_503.500_503.310_503.350_2021_01.pdf), Jan 2021: exact pin spacing, pin section, recommended drill and cover variants.
- [APEM 3631NF/2 at Reichelt](https://www.reichelt.com/de/en/shop/product/lever_switch_3600nf_series_1x_on_-_off-105660).
- [APEM manufacturer datasheet](https://www.apem.com/storage/datasheets/files/toggle_switches_serie_3600NF/media/toggle-switches-serie-3600NF.pdf): explicit DC resistive rating and mechanical drawings.

Both manufacturer PDFs are archived unchanged under ignored local-data/power-layout-parts/ with source URLs and SHA-256 values. They are third-party material, outside the project's MIT license. Stock listings were checked on 2026-10-27 and can change; no purchase has been made.
