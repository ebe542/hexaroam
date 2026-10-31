//! Header plausibility based on configured register regions.

use crate::Header;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    ReadOnly,
    ReadWrite,
    ReadWriteBits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterRegion {
    pub first_index: u8,
    pub entries: u16,
    pub bytes_per_entry: u16,
    pub access: Access,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    InvalidMap,
    UnknownIndex,
    UnsupportedAccess,
    InvalidLength,
    OutOfBounds,
}

/// Checks map validity, index, access, length and region boundaries.
///
/// Does not inspect payload values or controller state.
pub fn validate_header(
    header: &Header,
    regions: &[RegisterRegion],
) -> Result<(), ValidationError> {
    // Use wider arithmetic: index 255 plus one entry ends at 256.
    validate_regions(regions)?;

    let requested_index = u32::from(header.index);

    let region = regions
        .iter()
        .find(|region| {
            let start = u32::from(region.first_index);
            let end = start + u32::from(region.entries);

            requested_index >= start && requested_index < end
        })
        .ok_or(ValidationError::UnknownIndex)?;

    let access_allowed = match region.access {
        Access::ReadOnly => matches!(header.opcode, crate::Opcode::Read),
        Access::ReadWrite => matches!(
            header.opcode,
            crate::Opcode::Read | crate::Opcode::Write
        ),
        Access::ReadWriteBits => true,
    };

    if !access_allowed {
        return Err(ValidationError::UnsupportedAccess);
    }

    if header.length == 0 || header.length % region.bytes_per_entry != 0 {
        return Err(ValidationError::InvalidLength);
    }

    let requested_entries = u32::from(header.length / region.bytes_per_entry);

    let offset = requested_index - u32::from(region.first_index);
    let remaining_entries = u32::from(region.entries) - offset;

    if requested_entries > remaining_entries {
        return Err(ValidationError::OutOfBounds);
    }

    Ok(())
}

pub(crate) fn validate_regions(
    regions: &[RegisterRegion],
) -> Result<(), ValidationError> {
    for (index, region) in regions.iter().enumerate() {
        let start = u32::from(region.first_index);
        let end = start + u32::from(region.entries);

        if region.entries == 0 || region.bytes_per_entry == 0 || end > 256 {
            return Err(ValidationError::InvalidMap);
        }

        for previous in &regions[..index] {
            let previous_start = u32::from(previous.first_index);
            let previous_end = previous_start + u32::from(previous.entries);

            if start < previous_end && previous_start < end {
                return Err(ValidationError::InvalidMap);
            }
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "test_validation.rs"]
mod tests;
