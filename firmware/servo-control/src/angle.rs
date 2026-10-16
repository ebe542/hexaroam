//! Angles represented in hundredths of a degree.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AngleError {
    Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Angle {
    centidegrees: i32,
}

impl Angle {
    pub const fn from_centidegrees(centidegrees: i32) -> Self {
        Self { centidegrees }
    }

    pub fn from_degrees(degrees: i32) -> Result<Self, AngleError> {
        let centidegrees = degrees
            .checked_mul(100)
            .ok_or(AngleError::Overflow)?;

        Ok(Self::from_centidegrees(centidegrees))
    }

    pub const fn as_centidegrees(self) -> i32 {
        self.centidegrees
    }
}

#[cfg(test)]
#[path = "test_angle.rs"]
mod tests;
