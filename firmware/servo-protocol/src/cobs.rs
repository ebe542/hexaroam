//! COBS encoding and decoding without frame delimiters.
//!
//! Functions return the number of bytes written.
//! Bytes beyond the returned length remain unchanged on success.
//! Output contents are unspecified after an error.
//!
//! Encoding always emits a final code block, including after a full
//! 254-byte nonzero block. Decoding also accepts the shorter form.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CobsError {
    OutputTooSmall,
    InvalidEncoding,
}

pub fn encode(input: &[u8], output: &mut [u8]) -> Result<usize, CobsError> {
    if output.is_empty() {
        return Err(CobsError::OutputTooSmall);
    }

    let mut code_index = 0;
    let mut write_index = 1;
    let mut code = 1_u8;

    for &byte in input {
        if byte == 0 {
            output[code_index] = code;

            // Reserve the next block's code byte.
            if write_index >= output.len() {
                return Err(CobsError::OutputTooSmall);
            }

            code_index = write_index;
            write_index += 1;
            code = 1;
        } else {
            let destination = output
                .get_mut(write_index)
                .ok_or(CobsError::OutputTooSmall)?;

            *destination = byte;
            write_index += 1;
            code += 1;

            if code == 255 {
                output[code_index] = code;

                // A full block does not imply a decoded zero.
                // Always reserve a final block, even at end of input.
                if write_index >= output.len() {
                    return Err(CobsError::OutputTooSmall);
                }

                code_index = write_index;
                write_index += 1;
                code = 1;
            }
        }
    }

    output[code_index] = code;

    Ok(write_index)
}

pub fn decode(input: &[u8], output: &mut [u8]) -> Result<usize, CobsError> {
    if input.is_empty() {
        return Err(CobsError::InvalidEncoding);
    }

    let mut read_index: usize = 0;
    let mut write_index: usize = 0;

    while read_index < input.len() {
        let code = input[read_index];

        if code == 0 {
            return Err(CobsError::InvalidEncoding);
        }

        read_index += 1;

        let block_length = usize::from(code) - 1;
        let read_end = read_index
            .checked_add(block_length)
            .ok_or(CobsError::InvalidEncoding)?;

        let block = input
            .get(read_index..read_end)
            .ok_or(CobsError::InvalidEncoding)?;

        if block.contains(&0) {
            return Err(CobsError::InvalidEncoding);
        }

        let write_end = write_index
            .checked_add(block_length)
            .ok_or(CobsError::OutputTooSmall)?;

        let destination = output
            .get_mut(write_index..write_end)
            .ok_or(CobsError::OutputTooSmall)?;

        destination.copy_from_slice(block);

        read_index = read_end;
        write_index = write_end;

        // Short blocks imply a zero only when another block follows.
        if code != 255 && read_index < input.len() {
            let destination = output
                .get_mut(write_index)
                .ok_or(CobsError::OutputTooSmall)?;

            *destination = 0;
            write_index += 1;
        }
    }

    Ok(write_index)
}

#[cfg(test)]
#[path = "test_cobs.rs"]
mod tests;
