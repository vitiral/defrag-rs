//! contains the public object, the Box
use super::types::*;

#[derive(Copy, Clone)]
pub struct Rc {
    _block: block,
}

impl core::default::Default for Rc {
    fn default() -> Rc {
        Rc {
            _block: NULL_BLOCK
        }
    }
}
