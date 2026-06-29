//! A non‑reference‑counted, non‑drop‑customisable opaque pointer.

use core::{alloc::Layout, cmp::max, marker::PhantomData};

#[repr(C)]
#[doc(hidden)]
struct Meta {
    size: u32,
    align: u16,
    at: *mut u8,
    #[cfg(debug_assertions)]
    align_t: u16,
}

/// An opaque owning pointer to a value of an erased type.
///
/// `Box<H>` behaves like `std::boxed::Box` but does not carry the concrete type
/// in its signature. It is parameterised by a compile‑time hash `H` that
/// identifies the stored type.
///
/// The underlying allocation is freed when the `Box` goes out of scope. The
/// stored value’s `Drop` implementation is called automatically (unless the
/// type does not implement `Drop`).
///
/// # Creation
///
/// Use `Box::new(value)` to create a new handle. The hash is inferred from the
/// macro `Box!(Type)`, but you can also write the type directly with an explicit
/// constant.
///
/// # Downcasting
///
/// Use `downcast<T>` or `downcast_mut<T>` to recover a reference to the original
/// type. This is safe at runtime (with debug assertions) as long as the hash
/// matches.
///
/// # Safety
///
/// The hash constant `H` must uniquely identify the stored type; otherwise
/// downcasting may lead to undefined behaviour. The `from_raw` and `to_raw`
/// methods are unsafe because they allow you to bypass the type system.
///
/// # Example
///
/// ```
/// use nopaque::Box;
///
/// # macro_rules! hash { ($s:literal) => { 123 } }  // dummy for example
/// let b = Box::new(42i32);
/// let val: &i32 = b.downcast::<i32>();
/// assert_eq!(*val, 42);
/// ```
#[repr(transparent)]
pub struct Box<const _T: usize, Tx = ()> (
    #[doc(hidden)] *const (), /* points to HdlMeta.data */
    PhantomData<Tx>,
);

impl<const _T: usize, Tx> core::fmt::Debug for Box<_T, Tx> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("Box![{:p}]", self.0))
    }
}

impl<const _T: usize, Tx> Drop for Box<_T, Tx> {
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

// constructors
impl<const _T: usize, Tx> Box<_T, Tx> {
    /// Creates a new `Box` containing the given value.
    ///
    /// The hash constant `_T` is typically provided by the `Box!` macro.
    pub fn new<T>(t: T) -> Self {
        let align = max(align_of::<T>(), 8);
        let padding = (align - size_of::<Meta>() % align) % align;
        let size = padding + size_of::<T>();
        let layout = unsafe { Layout::from_size_align_unchecked(size, align) };
        let addr = unsafe { alloc::alloc::alloc(layout) };
        let meta = unsafe { ((addr as usize + padding) as *mut Meta).as_mut_unchecked() };
        let data =
            unsafe { ((addr as usize + padding + size_of::<Meta>()) as *mut T).as_mut_unchecked() };
        meta.at = addr;
        meta.align = align as u16;
        #[cfg(debug_assertions)]
        {
            meta.align_t = align_of::<T>() as u16;
        }
        *data = t;
        Self((addr as usize + padding + size_of::<Meta>()) as *mut (), PhantomData)
    }
}

// transformers
impl<const _T: usize, Tx> Box<_T, Tx> {
    /// Converts the handle to a raw `usize` pointer.
    ///
    /// # Safety
    ///
    /// The returned value is only valid for the lifetime of this handle. It can
    /// be passed across an FFI boundary and later recovered with `from_raw`,
    /// but you must ensure that the handle is not dropped while the raw pointer
    /// is in use.
    #[inline(always)]
    pub unsafe fn to_raw(&self) -> usize {
        self.0 as _
    }

    /// Reconstructs a `Box` from a raw pointer previously obtained by `to_raw`.
    ///
    /// # Safety
    ///
    /// The `addr` must point to a valid handle that was created by a `Box` of
    /// the same hash constant `_T`. The handle must not have been dropped yet.
    #[inline(always)]
    pub unsafe fn from_raw(addr: usize) -> Self {
        Self(addr as _, PhantomData)
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

/// Macro to construct a `Box` type with the hash of the given identifier.
///
/// The macro expands to `Box<{ hash!(stringify!($($x)+)) }>`. The string is
/// the concatenation of the tokens passed to the macro.
///
/// # Example
///
/// ```
/// use nopaque::Box;
///
/// # macro_rules! hash { ($s:literal) => { 123 } }  // dummy
/// type MyBox = Box!(MyStruct);
/// ```
pub macro Box {
    ($($x:tt)+) => { Box<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }> },
    (@$($x:tt)+) => { Box<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
}

pub macro boxed {
    ($($x:tt)+) => { Box::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }> },
    (@$($x:tt)+) => { Box::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
}
