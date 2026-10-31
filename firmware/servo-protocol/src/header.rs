//! Four-byte protocol header, independent of register access rules.

pub const HEADER_SIZE: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    Read = 1,
    Write = 2,
    Set = 3,
    Clear = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderError {
    UnknownOpcode(u8),
}

impl Opcode {
    pub fn from_byte(value: u8) -> Result<Self, HeaderError> {
        match value {
            1 => Ok(Self::Read),
            2 => Ok(Self::Write),
            3 => Ok(Self::Set),
            4 => Ok(Self::Clear),
            other => Err(HeaderError::UnknownOpcode(other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub opcode: Opcode,
    pub index: u8,
    pub length: u16,
}

impl Header {
    pub fn decode(bytes: &[u8; HEADER_SIZE]) -> Result<Self, HeaderError> {
        Ok(Self {
            opcode: Opcode::from_byte(bytes[0])?,
            index: bytes[1],
            length: u16::from_le_bytes([bytes[2], bytes[3]]),
        })
    }

    pub fn encode(&self) -> [u8; HEADER_SIZE] {
        let length = self.length.to_le_bytes();

        [self.opcode as u8, self.index, length[0], length[1]]
    }
}

#[cfg(test)]
#[path = "test_header.rs"]
mod tests;
