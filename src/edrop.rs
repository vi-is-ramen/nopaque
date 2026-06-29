//! Custom drop behaviour for types passed across ABI boundaries.
//!
//! This module defines the [`ExplicitDrop`] trait, which allows types to
//! provide a `drop` method that is called before deallocation when using
//! the `*Drop` pointer variants (`BoxDrop`, `RcDrop`, `ArcDrop`).

/// A trait for types that need explicit drop logic.
///
/// When a type implements `ExplicitDrop`, its `drop` method will be invoked
/// when the last reference to the object is released (for `BoxDrop`, `RcDrop`,
/// or `ArcDrop`). This is useful for FFI types that own resources that must
/// be manually released, or when you need to run code before the memory is
/// freed.
///
/// # Example
///
/// ```
/// use nopaque::{ExplicitDrop, boxed_drop};
///
/// struct Resource {
///     id: u32,
/// }
///
/// impl ExplicitDrop for Resource {
///     fn drop(&mut self) {
///         // e.g., close a file handle, release a foreign resource
///         println!("Releasing resource {}", self.id);
///     }
/// }
///
/// let b = <boxed_drop!(&Resource)>::new(Resource { id: 5 });
/// // When `b` goes out of scope, `Resource::drop` is called.
/// ```
pub const trait ExplicitDrop {
    /// Called just before the object’s memory is deallocated.
    ///
    /// This method is called **after** all other references are gone and
    /// the object is about to be freed. It should not be called manually.
    fn drop(&mut self);
}

/// Internal: calls `<T as ExplicitDrop>::drop`.
///
/// Used as a drop function pointer in the metadata.
#[doc(hidden)]
pub(crate) fn call_explicit_drop<T: ExplicitDrop>(ptr: &mut ()) {
    <T as ExplicitDrop>::drop(unsafe { (core::ptr::addr_of!(*ptr) as *mut T).as_mut_unchecked() });
}

/// Internal: calls `drop_in_place` for the type.
///
/// Used as a default drop function for non‑`ExplicitDrop` pointers.
#[doc(hidden)]
pub(crate) fn call_implicit_drop<T>(ptr: &mut ()) {
    unsafe {
        core::ptr::drop_in_place(core::ptr::addr_of_mut!(*ptr) as *mut T);
    }
}
