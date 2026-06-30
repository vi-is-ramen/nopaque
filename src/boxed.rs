use core::{alloc::Layout, cmp::max, marker::PhantomData};
use core::mem::size_of;

use crate::call_implicit_drop;

/// Metadata stored before the actual data.
///
/// The layout in memory is: `[ padding ] [ Meta ] [ Tx ]`.
/// The padding ensures `Meta` is aligned to at least 8 bytes.
#[repr(C)]
#[doc(hidden)]
struct Meta {
    size: u32,
    align: u16,
    at: *mut u8,
    drop: fn(&mut ()),
}

/// A uniquely‑owned, type‑guarded pointer that is safe to pass across ABI
/// boundaries.
///
/// `Box<_T, Tx>` is a `repr(transparent)` wrapper around a `usize` address.
/// The pointer points to a memory block that contains a `Meta` header followed
/// by the actual value of type `Tx`. The `_T` const parameter is a compile‑time
/// hash that acts as a type discriminator, preventing accidental mixing of
/// pointers for different types.
///
/// # Type Parameters
///
/// * `_T` – A `usize` hash of the type name (or a custom string). This is
///   typically supplied by the [`Box!`] or [`boxed!`] macro.
/// * `Tx` – The pointee type. This is the type that is actually stored and
///   can be accessed via `Deref` and `DerefMut`.
///
/// # Memory Layout
///
/// The allocated block has the following structure:
///
/// ```text
/// [ padding ] [ Meta ] [ Tx ]
/// ```
///
/// where `Meta` contains the total size, alignment, the original allocation
/// pointer, and a drop function pointer. The pointer returned (`self.0`)
/// points to the start of `Tx`, so the header is at a negative offset.
/// Padding ensures the `Meta` is aligned to at least 8 bytes.
///
/// # ABI Stability
///
/// Because `Box` is `repr(transparent)`, its layout is exactly that of a
/// single `usize`. This makes it suitable for passing as a `void*` or
/// `uintptr_t` across an FFI boundary. The foreign code can store the handle
/// and pass it back; the Rust code will reconstruct the `Box` from the address.
/// However, the foreign side **must not** try to dereference the pointer or
/// interpret the metadata; it is only an opaque handle.
///
/// # Safety
///
/// The type system prevents you from using a `Box` of one type as if it were
/// another, because the `_T` hash is part of the type. However, when
/// reconstructing a `Box` from a raw address (e.g., from FFI), you must ensure
/// that the address actually points to a valid block created by the same
/// `Box::new` (or equivalent) and that the hash matches. The crate provides
/// no runtime checking for that; it is the caller's responsibility.
///
/// # Provider / Consumer Forms
///
/// The [`Box!`] and [`boxed!`] macros provide two forms:
/// - `Box!(&MyType)` → `Tx = MyType` (provider, can dereference). Requires
///   `MyType` to be a concrete type in scope.
/// - `Box!(MyType)`   → `Tx = ()`     (consumer, opaque). `MyType` is only
///   a token used for hashing; it does **not** need to be defined.
///
/// # Examples
///
/// Provider:
/// ```
/// # use nopaque::{Box, boxed};
/// struct MyStruct { x: u32 }
///
/// let b: Box<_, MyStruct> = <boxed!(&MyStruct)>::new(MyStruct { x: 10 });
/// assert_eq!(b.x, 10);
/// ```
///
/// Consumer (the type `MyStruct` need not exist):
/// ```
/// # use nopaque::{Box, boxed};
/// type Handle = Box!(MyStruct);   // MyStruct is just a token for the hash
///
/// fn use_handle(h: Handle) {
///     // Cannot access fields because Tx = ()
/// }
/// ```
#[repr(transparent)]
pub struct Box<const _T: usize, Tx>(
    #[doc(hidden)] usize,
    PhantomData<Tx>,
);

impl<const _T: usize, Tx> core::fmt::Debug for Box<_T, Tx> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("Box![{:p}]", self.0 as *const ()))
    }
}

impl<const _T: usize, Tx> core::ops::Drop for Box<_T, Tx> {
    #[inline(always)]
    fn drop(&mut self) {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        let layout =
            unsafe { Layout::from_size_align_unchecked(meta.size as usize, meta.align as usize) };
        unsafe {
            alloc::alloc::dealloc(meta.at, layout);
        }
    }
}

impl<const _T: usize, Tx> Box<_T, Tx> {
    /// Creates a new `Box` owning the provided value.
    ///
    /// The value is moved into a freshly allocated memory block with
    /// appropriate alignment and padding for the metadata.
    ///
    /// # Panics
    ///
    /// This function will panic if the allocation fails (rare in practice).
    pub fn new<T>(t: T) -> Self {
        let align = max(align_of::<T>(), 8);
        let padding = (align - size_of::<Meta>() % align) % align;
        let size = padding + size_of::<Meta>() + size_of::<T>();
        let layout = unsafe { Layout::from_size_align_unchecked(size, align) };
        let addr = unsafe { alloc::alloc::alloc(layout) };
        let meta = unsafe { ((addr as usize + padding) as *mut Meta).as_mut_unchecked() };
        let data =
            unsafe { ((addr as usize + padding + size_of::<Meta>()) as *mut T).as_mut_unchecked() };
        meta.at = addr;
        meta.align = align as u16;
        meta.size = size as u32;
        meta.drop = call_implicit_drop::<T>;
        *data = t;
        Self(addr as usize + padding + size_of::<Meta>(), PhantomData)
    }
}

impl<const _T: usize, Tx> core::ops::Deref for Box<_T, Tx> {
    type Target = Tx;

    fn deref(&self) -> &Self::Target {
        unsafe { (self.0 as *const Tx).as_ref_unchecked() }
    }
}

impl<const _T: usize, Tx> core::ops::DerefMut for Box<_T, Tx> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { (self.0 as *mut Tx).as_mut_unchecked() }
    }
}

/// Creates a `Box` type with the correct `_T` hash (no turbofish).
///
/// This macro expands directly to `Box<{hash}, ...>`. It has two forms:
/// - **With `&`**: `Box!(&MyType)` → `Tx = MyType`, hash from `MyType`.
///   Use on the provider side; requires `MyType` to be a type.
/// - **Without `&`**: `Box!(MyType)` → `Tx = ()`, hash from `MyType`.
///   Use on the consumer side; `MyType` is only a token—it need not be defined.
///
/// The hash is derived from the token sequence (`MyType`). The `&` is
/// a syntactic marker that tells the macro to use that token as the inner type.
pub macro Box {
    (&$($x:tt)+) => { Box<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { Box<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}

/// Alias for the `Box` macro using turbofish syntax.
///
/// This macro expands to `Box::<{hash}, ...>`, which is equivalent to the
/// type produced by `Box!` but uses `::<>` for the const parameter.
/// The same two‑form semantics apply.
pub macro boxed {
    (&$($x:tt)+) => { Box::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { Box::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}
