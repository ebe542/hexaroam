MEMORY {
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100

    /* Firmware excludes the final two 4-KiB flash sectors. */
    FLASH : ORIGIN = 0x10000100, LENGTH = 2048K - 8K - 0x100

    RAM : ORIGIN = 0x20000000, LENGTH = 256K

    /* Persistent calibration journal; no firmware sections go here. */
    CALIBRATION : ORIGIN = 0x101FE000, LENGTH = 8K
}

/* Address boundaries for the future storage driver.
 * These symbols do not create initialized data in the journal.
 */
__calibration_start = ORIGIN(CALIBRATION);
__calibration_end = ORIGIN(CALIBRATION) + LENGTH(CALIBRATION);

ASSERT(
    ORIGIN(FLASH) + LENGTH(FLASH) == ORIGIN(CALIBRATION),
    "Firmware flash must end at the calibration region"
);

ASSERT(
    (ORIGIN(CALIBRATION) % 4096) == 0,
    "Calibration region must start on a flash-sector boundary"
);

ASSERT(
    LENGTH(CALIBRATION) == 2 * 4096,
    "Calibration journal requires two flash sectors"
);

ASSERT(
    ORIGIN(CALIBRATION) + LENGTH(CALIBRATION) == 0x10200000,
    "Calibration region must end at the Pico flash boundary"
);

EXTERN(BOOT2_FIRMWARE)

SECTIONS {
    .boot2 ORIGIN(BOOT2) :
    {
        KEEP(*(.boot2));
    } > BOOT2
} INSERT BEFORE .text;
