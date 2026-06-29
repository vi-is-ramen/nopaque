//! # nopaque – Type‑safe opaque pointers for ABIs
//! 
//! @[`Usage_Guide`]
//!
//! This crate provides smart pointers (`Box`, `Arc`, `Rc`) that store values
//! with their concrete type erased, but allow safe downcasting via a compile‑time
//! type hash. It is designed for building **type‑safe dynamic APIs** – for
//! example, plugin systems, add‑on modules, or any FFI boundary where you need
//! to pass opaque handles that can later be inspected and cast back to the
//! original type.
//!
//! ## Core idea
//!
//! Each handle is a pointer to a heap‑allocated block that stores the value
//! together with a small metadata header. The metadata contains:
//! - the original allocation’s address and layout,
//! - for reference‑counted handles, an atomic or non‑atomic reference count,
//! - optionally, a function pointer to a custom destructor.
//!
//! The handle itself is a zero‑sized type (ZST) with a const generic parameter
//! `_T: usize`. That parameter holds a hash of the type’s name (or any
//! arbitrary identifier). When you call `downcast<T>`, the handle checks that
//! the hash matches the hash of `T` (using debug assertions) and then casts the
//! pointer to `&T` or `&mut T`.
//!
//! This design allows you to pass handles across ABI boundaries without exposing
//! the concrete type, yet still recover the original type on the other side
//! – provided both sides agree on the hash.
//!
//! ## Provided pointer kinds
//!
//! | Type        | Reference counting | Custom drop support |
//! |-------------|---------------------|---------------------|
//! | `Box`       | no                  | no                  |
//! | `BoxDrop`   | no                  | yes                 |
//! | `Rc`        | yes (non‑atomic)    | no                  |
//! | `RcDrop`    | yes (non‑atomic)    | yes                 |
//! | `Arc`       | yes (atomic)        | no                  |
//! | `ArcDrop`   | yes (atomic)        | yes                 |
//!
//! The `Drop` variants require the stored type to implement the `ExplicitDrop`
//! trait, or you may provide a custom `drop` function pointer.
//!
//! ## Macros
//!
//! For each pointer type there is a macro that expands to the type with the
//! correct hash, e.g. `Box!(MyType)` -> `Box<{ hash!("MyType") }>`. These macros
//! are the recommended way to write the type in signatures.  
//! Additionally, lowercase macros like `box!` exist as type aliases for easier
//! writing in generic contexts (they expand to `Box::<{ hash!(...) }>`).
//!
//! ## Required `hash!` macro
//!
//! The crate expects a macro `hash!` to be in scope that computes a `usize`
//! constant from a string literal. **This macro is not provided** – you must
//! define it yourself. A typical implementation might use a const‑fn like
//! `xxhash` or `fnv`:
//!
//! ```ignore
//! macro_rules! hash {
//!     ($s:literal) => {{
//!         const H: usize = my_const_hash($s.as_bytes());
//!         H
//!     }};
//! }
//! ```
//!
//! The hash value must be stable across compilation units and across different
//! crates if you intend to share opaque handles across ABI boundaries.
//!
//! ## Feature flags
//!
//! - **`std`** (enabled by default): when enabled, the crate links to the
//!   standard library. When disabled, it operates in `no_std` mode and only
//!   depends on `alloc`.
//!
//! ## Safety
//!
//! The crate uses `unsafe` code to manage raw allocations and to cast pointers.
//! The main invariants are:
//!
//! - The const generic hash must uniquely identify the type that was originally
//!   stored. If two different types produce the same hash, downcasting will
//!   cause undefined behaviour.
//! - The `from_raw` functions assume that the raw address points to a valid
//!   handle that was created by the corresponding `to_raw` method of the same
//!   pointer kind, and that the handle’s reference count (if any) is correctly
//!   managed.
//! - For `Arc` and `Rc`, the reference count is accessed via `unsafe` methods;
//!   misusing them can cause leaks or double‑frees.
//!
//! Always read the documentation of each method carefully.

#![feature(const_trait_impl, decl_macro)]
#![cfg_attr(feature = "std", no_std)]

extern crate alloc;

#[allow(nonstandard_style)]#[doc=include_str!("../USAGE.md")]pub mod Usage_Guide{}

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
