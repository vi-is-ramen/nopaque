#![feature(const_trait_impl, decl_macro)]
#![cfg_attr(feature = "std", no_std)]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

// #[allow(nonstandard_style)]#[doc=include_str!("../USAGE.md")]pub mod Usage_Guide{}

mod edrop;
pub use edrop::*;

mod boxed;
pub use boxed::*;

mod boxed_drop;
pub use boxed_drop::*;

mod arc_drop;
pub use arc_drop::*;

mod rc_drop;
pub use rc_drop::*;

mod arc;
pub use arc::*;

mod rc;
pub use rc::*;

pub macro hash($s:expr) {{
    const fn fnv1a64(data: &[u8]) -> u64 {
        const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
        const FNV_PRIME: u64 = 0x100000001b3;
        let mut hash = FNV_OFFSET_BASIS;
        let mut i = 0;
        while i < data.len() {
            hash ^= data[i] as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
            i += 1;
        }
        hash
    }
    fnv1a64($s)
}}

#[cfg(test)]
mod tests {
    mod boxed;
    mod box_drop;
    mod rc;
    mod rc_drop;
    mod arc;
    mod arc_drop;
}
