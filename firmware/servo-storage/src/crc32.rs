//! CRC-32/ISO-HDLC for calibration records.

/// Calculates CRC-32/ISO-HDLC independently for each call.
pub trait Checksum {
    fn checksum(&mut self, data: &[u8]) -> u32;
}

pub struct SoftwareCrc32;

impl Checksum for SoftwareCrc32 {
    fn checksum(&mut self, data: &[u8]) -> u32 {
        crc32(data)
    }
}

pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;

    for &byte in data {
        crc ^= u32::from(byte);

        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }

    !crc
}

#[cfg(test)]
#[path = "test_crc32.rs"]
mod tests;
