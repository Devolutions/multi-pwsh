use std::error::Error;
use std::fmt;

use super::PdUChar;

// same definition as ffi::NulError and widestring::NulError<u16>
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct NulError(usize, Vec<PdUChar>);

impl NulError {
    pub fn new(nul_position: usize, data: Vec<PdUChar>) -> Self {
        Self(nul_position, data)
    }

    pub fn nul_position(&self) -> usize {
        self.0
    }

    pub fn into_vec(self) -> Vec<PdUChar> {
        self.1
    }
}

#[cfg(not(windows))]
impl From<std::ffi::NulError> for NulError {
    fn from(err: std::ffi::NulError) -> Self {
        Self::new(err.nul_position(), err.into_vec())
    }
}

#[cfg(windows)]
impl From<widestring::NulError<PdUChar>> for NulError {
    fn from(err: widestring::NulError<PdUChar>) -> Self {
        Self::new(err.nul_position(), err.into_vec())
    }
}

impl Error for NulError {
    fn description(&self) -> &str {
        "nul value found in data"
    }
}

impl fmt::Display for NulError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "nul byte found in provided data at position: {}", self.0)
    }
}

impl From<NulError> for Vec<PdUChar> {
    fn from(e: NulError) -> Vec<PdUChar> {
        e.into_vec()
    }
}
