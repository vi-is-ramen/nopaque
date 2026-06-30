use core::{alloc::Layout, cmp::max, marker::PhantomData};
use core::mem::size_of;
use core::mem::align_of;
use core::clone::Clone;

use crate::call_implicit_drop;

/// Metadata for reference‑counted pointers (non‑atomic refcount).
///
/// The layout in memory is: `[ padding ] [ Meta ] [ Tx ]`.
#[repr(C)]
#[doc(hidden)]
struct Meta {
    refc: u16,
    size: u32,
    align: u16,
    at: *mut u8,
    drop: fn(&mut ()),
}

/// A type‑guarded reference‑counted pointer with **non‑atomic** reference counting.
///
/// `Rc<_T, Tx>` provides shared ownership of a value of type `Tx`. The reference
/// count is stored in the metadata header and is **not** atomic, so `Rc` is
/// neither `Send` nor `Sync`. It is intended for use within a single thread.
///
/// Like [`Box`], `Rc` is `repr(transparent)` and can be passed as an opaque
/// handle across ABI boundaries. The type parameter `_T` is a hash to prevent
/// type confusion.
///
/// # Reference Counting
///
/// * `Clone` increases the reference count.
/// * `Drop` decreases it; when it reaches zero, the value is dropped via
///   the implicit `drop_in_place` (i.e., the normal `Drop` implementation of
///   `Tx`) and the memory is freed.
///
/// # Thread Safety
///
/// `Rc` is **not** thread‑safe. Use [`Arc`] for multi‑threaded scenarios.
///
/// # Provider / Consumer Forms
///
/// The [`Rc!`] and [`rc!`] macros provide two forms:
/// - `Rc!(&MyType)` → `Tx = MyType` (provider, can dereference). Requires
///   `MyType` to be a concrete type in scope.
/// - `Rc!(MyType)`   → `Tx = ()`     (consumer, opaque). `MyType` is only
///   a token used for hashing; it does **not** need to be defined.
///
/// # Examples
///
/// Provider:
/// ```
/// # use nopaque::{Rc, rc};
/// struct MyData { val: u32 }
///
/// let r: Rc<_, MyData> = <rc!(&MyData)>::new(MyData { val: 5 });
/// let r2 = r.clone();
/// assert_eq!(r.val, 5);
/// ```
///
/// Consumer (opaque):
/// ```
/// # use nopaque::{Rc, rc};
/// type Handle = Rc!(MyData);   // MyData is just a token
/// ```
#[repr(transparent)]
pub struct Rc<const _T: usize, Tx>(
    #[doc(hidden)] usize,
    PhantomData<Tx>,
);

impl<const _T: usize, Tx> core::fmt::Debug for Rc<_T, Tx> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("Rc![{:p}]", self.0 as *const ()))
    }
}

impl<const _T: usize, Tx> Clone for Rc<_T, Tx> {
    #[inline(always)]
    fn clone(&self) -> Self {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        meta.refc += 1;
        Self(self.0, PhantomData)
    }
}

impl<const _T: usize, Tx> core::ops::Drop for Rc<_T, Tx> {
    #[inline(always)]
    fn drop(&mut self) {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        meta.refc -= 1;

        if meta.refc == 0 {
            let layout = unsafe {
                Layout::from_size_align_unchecked(meta.size as usize, meta.align as usize)
            };
            unsafe {
                alloc::alloc::dealloc(meta.at, layout);
            }
        }
    }
}

impl<const _T: usize, Tx> Rc<_T, Tx> {
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    fn meta(&self) -> &mut Meta {
        unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() }
    }
}

impl<const _T: usize, Tx> Rc<_T, Tx> {
    /// Creates a new `Rc` with an initial reference count of 1.
    ///
    /// The drop function stored in metadata is `call_implicit_drop::<T>`,
    /// which calls `<T as Drop>::drop` when the count reaches zero.
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
        meta.refc = 1;
        meta.drop = call_implicit_drop::<T>;
        *data = t;
        Self(addr as usize + padding + size_of::<Meta>(), PhantomData)
    }
}

impl<const _T: usize, Tx> core::ops::Deref for Rc<_T, Tx> {
    type Target = Tx;

    fn deref(&self) -> &Self::Target {
        unsafe { (self.0 as *const Tx).as_ref_unchecked() }
    }
}

impl<const _T: usize, Tx> Rc<_T, Tx> {
    /// Loads the current reference count.
    ///
    /// # Safety
    ///
    /// This function allows unsynchronized access to the refcount; it is
    /// provided for debugging or when you have exclusive access to the `Rc`.
    /// In normal usage, you should not need to call this.
    #[inline(always)]
    pub unsafe fn rc_load(&self) -> usize {
        self.meta().refc as _
    }

    /// Stores a new reference count.
    ///
    /// # Safety
    ///
    /// This can easily corrupt the refcount if used incorrectly. Only use
    /// when you know exactly what you are doing.
    #[inline(always)]
    pub unsafe fn rc_store(&self, v: usize) {
        self.meta().refc = v as _
    }

    /// Adds `v` to the reference count and returns the previous value.
    ///
    /// # Safety
    ///
    /// Unsynchronized; only safe if you have exclusive access to all `Rc`
    /// handles.
    #[inline(always)]
    pub unsafe fn rc_add(&self, v: usize) -> usize {
        self.meta().refc += v as u16;
        self.meta().refc as usize - 1
    }

    /// Subtracts `v` from the reference count and returns the previous value.
    ///
    /// # Safety
    ///
    /// Unsynchronized; only safe if you have exclusive access to all `Rc`
    /// handles.
    #[inline(always)]
    pub unsafe fn rc_sub(&self, v: usize) -> usize {
        self.meta().refc -= v as u16;
        self.meta().refc as usize - 1
    }

    /// Increments the reference count by 1 and returns the previous value.
    #[inline(always)]
    pub unsafe fn rc_inc(&self) -> usize {
        unsafe { self.rc_add(1) }
    }

    /// Decrements the reference count by 1 and returns the previous value.
    #[inline(always)]
    pub unsafe fn rc_dec(&self) -> usize {
        unsafe { self.rc_sub(1) }
    }
}

/// Creates an `Rc` type with the correct `_T` hash (no turbofish).
///
/// This macro expands directly to `Rc<{hash}, ...>`. Two forms:
/// - **With `&`**: `Rc!(&MyType)` → `Tx = MyType`, hash from `MyType`.
/// - **Without `&`**: `Rc!(MyType)` → `Tx = ()`, hash from `MyType` (token need not be defined).
pub macro Rc {
    (&$($x:tt)+) => { Rc<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { Rc<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}

/// Alias for `Rc` macro with turbofish syntax.
pub macro rc {
    (&$($x:tt)+) => { Rc::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { Rc::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}
