pub use num_bigint::BigUint;
use num_traits::Num;
use std::fmt::Display;

//export some more funcs
pub use franklin_crypto::plonk::circuit::bigint::bigint::{biguint_to_fe, fe_to_biguint};

/// convert a hex integer representation ("0x...") to decimal representation
pub fn repr_to_big<T: Display>(r: T) -> String {
    BigUint::from_str_radix(&format!("{}", r)[2..], 16)
        .unwrap()
        .to_str_radix(10)
}
