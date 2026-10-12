# Hardware reference documents

These unmodified documents were downloaded from Raspberry Pi on 2026-10-11 for the Raspberry Pi Pico / RP2040 used by HexaRoam.

## Document inventory

| Document | Local file | Publisher | Version / build | Pages | Official source |
| --- | --- | --- | --- | --- | --- |
| Raspberry Pi Pico Datasheet | [pico-datasheet.pdf](raspberry-pi-pico/pico-datasheet.pdf) | Raspberry Pi Ltd | Release 21; 2026-07-03; d4e0f1799616 | 31 | [Download](https://datasheets.raspberrypi.com/pico/pico-datasheet.pdf) |
| Getting Started with Raspberry Pi Pico-series | [getting-started-with-pico.pdf](raspberry-pi-pico/getting-started-with-pico.pdf) | Raspberry Pi Ltd | Release 25; 2026-10-01; 409579e6f75f | 36 | [Download](https://datasheets.raspberrypi.com/pico/getting-started-with-pico.pdf) |
| RP2040 Datasheet | [rp2040-datasheet.pdf](rp2040/rp2040-datasheet.pdf) | Raspberry Pi Ltd | 2025-02-20; 3184e62-clean | 642 | [Download](https://datasheets.raspberrypi.com/rp2040/rp2040-datasheet.pdf) |

Version dates above are the build dates inside the downloaded PDFs, not the download date. Upstream URLs may later serve different revisions.

## Licensing and attribution

All three PDFs identify their documentation license as [Creative Commons Attribution-NoDerivatives 4.0 International](https://creativecommons.org/licenses/by-nd/4.0/) ([legal code](https://creativecommons.org/licenses/by-nd/4.0/legalcode.en)). They are redistributed without modifications, with their original copyright, licensing, and disclaimer notices intact. They are not covered by HexaRoam's MIT License.

Pico Datasheet and Getting Started: copyright 2022-2026 Raspberry Pi Ltd.
RP2040 Datasheet: copyright 2020-2025 Raspberry Pi Ltd; the document also includes notices for Synopsys and Arm, retained in the PDF.

Keep annotations, translations, and project-specific explanations in separate project documents. Do not publish edited PDF versions under this license. Inclusion does not imply endorsement by Raspberry Pi.

## SHA-256 checksums

The accompanying [SHA256SUMS](SHA256SUMS) identifies the exact downloaded bytes. From Git Bash at the repository root:

```bash
(cd hardware/references && sha256sum -c SHA256SUMS)
```

These checksums identify the archived snapshot; they are locally calculated, not publisher-supplied authenticity signatures.

## Reading order

1. Pico Datasheet: board pinout, power connections, and schematic.
2. Getting Started: boot mode, USB programming, and debug connections. It describes C/C++ tooling; it is not our Rust setup guide.
3. RP2040 Datasheet: GPIO, clocks, PWM, and later PIO.

Board selection: the existing Raspberry Pi Pico with RP2040, without wireless connectivity. The Rust target is `thumbv6m-none-eabi`; its installation and firmware setup are performed by the project owner.

## Updating references

Download only relevant official documents. Before committing an update, inspect its license and version, retain applicable notices, and update this inventory and SHA256SUMS together. Review document changes before applying them to a built hardware revision.

Servo documentation will be added after selecting the actual servo model. Rust dependency documentation will be recorded separately with the versions selected for implementation.
