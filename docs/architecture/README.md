# Architecture maps

Start with [software layers](software-layers.md) to follow a command from an angle to GPIO pulses. Use [traits and implementations](interfaces.md) to find the contracts between layers and their concrete implementations.

These diagrams describe the source reviewed through 2026-10-30. They are maintained manually, render on GitHub through Mermaid, and link to the implementation. They show selected architectural relationships, not every Rust module, function, generic argument or test double. Update them when changing a layer boundary or implementation relationship.

| View | Question answered |
| --- | --- |
| [Software layers](software-layers.md) | Which layer handles angles, pulse widths, GPIO mapping, timing and hardware? |
| [Traits and implementations](interfaces.md) | Which type implements a contract, and which type consumes it? |
| [Project design](../architecture.md) | What is the intended robot architecture, including future work? |
| [Signal-output design](../servo-output-design.md) | How do frame replacement, DMA ownership and fault handling work? |
| [Firmware guide](../../firmware/README.md) | Which builds and hardware observations have been reported? |

Solid arrows in the layer diagrams describe the existing command/data path. Dashed arrows explicitly labelled future integration describe work that is not wired up yet. A trait diagram's dashed triangle arrow means implements, its solid triangle arrow means extends, and its labelled dependency arrow means uses. Hardware-test coverage does not imply that the same path runs in the normal firmware.

For API details, generate Rustdoc with `python tools/dev.py doc` and open `target/rustdoc/thumbv6m-none-eabi/doc/index.html`. Rustdoc supplements these diagrams with methods, fields and implementation details. Generated HTML remains under ignored target directories; these maintained architecture sources belong in docs.
