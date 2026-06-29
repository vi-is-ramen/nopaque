#![feature(const_trait_impl, decl_macro)]
#![cfg_attr(feature = "std", no_std)]

//! # nopaque – Type‑Safe Opaque Pointers for ABI Boundaries
//!
//! This crate provides smart pointer types (`Box`, `Rc`, `Arc`) that are **opaque**
//! and **type‑guarded** by a compile‑time hash. They are designed to be safely
//! passed across foreign function interfaces (e.g., C ABIs) where the concrete
//! type cannot be revealed.
//!
//! ## Core Concepts
//!
//! - **Type guarding**: Each pointer carries a `const _T: usize` parameter that
//!   is a 64‑bit FNV‑1a hash of the type name (or a custom string). This
//!   prevents accidental type confusion when handles are passed between
//!   different modules or languages.
//! - **ABI‑compatible layout**: All pointer types are `#[repr(transparent)]` and
//!   store a single `usize` (the raw address). Metadata (size, alignment, drop
//!   function, reference count) is placed **before** the pointed‑to data in a
//!   separate allocation. This layout is stable and can be reconstructed on the
//!   foreign side.
//! - **Memory management**: `Box` is unique ownership, `Rc` and `Arc` provide
//!   non‑atomic and atomic reference counting respectively. The `Drop` variants
//!   (`BoxDrop`, `RcDrop`, `ArcDrop`) let you specify explicit destructor logic
//!   via the [`ExplicitDrop`] trait or a custom function pointer.
//!
//! ## Provider / Consumer Pattern
//!
//! The macros have two forms:
//! - **With `&`**: `Box!(&MyType)` → inner type `Tx = MyType` (provider knows the type,
//!   can dereference). The hash is derived from `MyType`.
//! - **Without `&`**: `Box!(MyType)` → inner type `Tx = ()` (consumer receives an opaque
//!   handle, cannot dereference). The hash is derived from `MyType`. **The token `MyType`
//!   does not need to be a defined type**; it is only used for its name.
//!
//! In both cases the hash matches, so the types are compatible across the boundary.
//!
//! Two macro aliases are provided: `Box!` (type‑level) and `boxed!` (turbofish style).
//! Both expand to the same underlying `Box` type, but `boxed!` uses `::<>` syntax
//! which may be preferred in certain contexts.
//!
//! ## Examples
//!
//! Provider side (knows the type):
//! ```
//! # use nopaque::{Box, boxed};
//! struct Person { name: String, age: u8 }
//! type PersonHandle = Box!(&Person);
//!
//! let handle = PersonHandle::new(Person { name: "Alice".into(), age: 30 });
//! assert_eq!(handle.name, "Alice");  // can dereference
//! ```
//!
//! Consumer side (opaque):
//! ```
//! # use nopaque::{Box, boxed};
//! type PersonHandle = Box!(Person);  // Person need not be defined anywhere
//!
//! fn consume(handle: PersonHandle) {
//!     // Cannot access fields; handle is just an opaque token.
//!     // The drop will free the memory correctly.
//! }
//! ```

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;
#[macro_use] extern crate core;

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

/// Computes a 64‑bit FNV‑1a hash of a byte slice at compile time.
///
/// This macro is used internally to produce the `_T` const parameter from a
/// type name or a custom string. The hash is **not** cryptographically secure
/// but serves as a unique discriminant for type‑safety.
///
/// # Usage
///
/// ```
/// # use nopaque::hash;
/// const HASH: usize = hash!(b"MyType") as usize;
/// ```
///
/// It is not intended for direct use; the pointer macros call it automatically.
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
