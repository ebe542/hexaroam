#![no_std]

//! Transport-independent framing for the HexaRoam register protocol.

mod header;

pub use header::{HEADER_SIZE, Header, HeaderError, Opcode};

mod validation;

pub use validation::{
    Access, RegisterRegion, ValidationError, validate_header,
};

mod frame;

pub use frame::{
    CRC_SIZE, Frame, FrameError, FrameKind, MAX_FRAME_SIZE, MAX_PAYLOAD_SIZE,
    decode_frame, encode_frame,
};

mod stream;

pub use stream::{ParsedFrame, ParserError, StreamParser};
