use core::{alloc::Layout, cmp::max, marker::PhantomData};
use core::mem::size_of;
use core::mem::align_of;
use core::clone::Clone;

use crate::ExplicitDrop;

/// Metadata for `RcDrop`.
#[repr(C)]
#[doc(hidden)]
struct Meta {
    refc: u16,
    size: u32,
    align: u16,
    at: *mut u8,
    drop: fn(&mut ()),
}

/// A reference‑counted pointer with explicit drop semantics (non‑atomic).
///
/// This is the `Drop` variant of [`Rc`]: when the last reference is dropped,
/// the stored drop function (either from `ExplicitDrop` or a custom function)
/// is called on the value before deallocation.
///
/// # Thread Safety
///
/// Like `Rc`, this type is not `Send` or `Sync`.
///
/// # Provider / Consumer Forms
///
/// The [`RcDrop!`] and [`rc_drop!`] macros provide two forms:
/// - `RcDrop!(&MyType)` → `Tx = MyType` (provider, can dereference).
/// - `RcDrop!(MyType)`   → `Tx = ()`     (consumer, opaque; `MyType` is just a token).
///
/// # Examples
///
/// ```
/// # use nopaque::{RcDrop, rc_drop, ExplicitDrop};
/// struct MyResource;
///
/// impl ExplicitDrop for MyResource {
///     fn drop(&mut self) {
///         println!("Releasing resource");
///     }
/// }
///
/// let r = <rc_drop!(&MyResource)>::new(MyResource);
/// let r2 = r.clone();
/// // both r and r2 share ownership; when both are dropped,
/// // `MyResource::drop` is called once.
/// ```
#[repr(transparent)]
pub struct RcDrop<const _T: usize, Tx>(
    #[doc(hidden)] usize,
    PhantomData<Tx>,
);

impl<const _T: usize, Tx> core::fmt::Debug for RcDrop<_T, Tx> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("RcDrop![{:p}]", self.0 as *const ()))
    }
}

impl<const _T: usize, Tx> Clone for RcDrop<_T, Tx> {
    #[inline(always)]
    fn clone(&self) -> Self {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        meta.refc += 1;
        Self(self.0, PhantomData)
    }
}

impl<const _T: usize, Tx> core::ops::Drop for RcDrop<_T, Tx> {
    #[inline(always)]
    fn drop(&mut self) {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        meta.refc -= 1;

        if meta.refc == 0 {
            (meta.drop)(unsafe { (self.0 as *mut ()).as_mut_unchecked() });
            let layout = unsafe {
                Layout::from_size_align_unchecked(meta.size as usize, meta.align as usize)
            };
            unsafe {
                alloc::alloc::dealloc(meta.at, layout);
            }
        }
    }
}

impl<const _T: usize, Tx> RcDrop<_T, Tx> {
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    fn meta(&self) -> &mut Meta {
        unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() }
    }
}

impl<const _T: usize, Tx> RcDrop<_T, Tx> {
    /// Creates a new `RcDrop` using the `ExplicitDrop` implementation of `T`.
    pub fn new<T: ExplicitDrop>(t: T) -> Self {
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
        meta.drop = crate::edrop::call_explicit_drop::<T>;
        *data = t;
        Self(addr as usize + padding + size_of::<Meta>(), PhantomData)
    }

    /// Creates a new `RcDrop` with a user‑provided drop function.
    pub fn new_with_drop<T>(t: T, drop: fn(&mut ())) -> Self {
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
        meta.drop = drop;
        meta.refc = 1;
        *data = t;
        Self(addr as usize + padding + size_of::<Meta>(), PhantomData)
    }
}

impl<const _T: usize, Tx> core::ops::Deref for RcDrop<_T, Tx> {
    type Target = Tx;

    fn deref(&self) -> &Self::Target {
        unsafe { (self.0 as *const Tx).as_ref_unchecked() }
    }
}

impl<const _T: usize, Tx> RcDrop<_T, Tx> {
    /// Loads the current reference count.
    ///
    /// # Safety
    ///
    /// Unsynchronized; see [`Rc::rc_load`] for details.
    #[inline(always)]
    pub unsafe fn rc_load(&self) -> usize {
        self.meta().refc as _
    }

    /// Stores a new reference count.
    ///
    /// # Safety
    ///
    /// See [`Rc::rc_store`].
    #[inline(always)]
    pub unsafe fn rc_store(&self, v: usize) {
        self.meta().refc = v as _;
    }

    /// Adds `v` to the reference count and returns the previous value.
    ///
    /// # Safety
    ///
    /// See [`Rc::rc_add`].
    #[inline(always)]
    pub unsafe fn rc_add(&self, v: usize) -> usize {
        let rv = self.meta().refc;
        self.meta().refc += v as u16;
        rv as _
    }

    /// Subtracts `v` from the reference count and returns the previous value.
    ///
    /// # Safety
    ///
    /// See [`Rc::rc_sub`].
    #[inline(always)]
    pub unsafe fn rc_sub(&self, v: usize) -> usize {
        let rv = self.meta().refc;
        self.meta().refc -= v as u16;
        rv as _
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

/// Creates an `RcDrop` type with the correct `_T` hash (no turbofish).
///
/// Two forms:
/// - `RcDrop!(&MyType)` → provider (Tx = MyType)
/// - `RcDrop!(MyType)`   → consumer (Tx = (), token need not be defined)
pub macro RcDrop {
    (&$($x:tt)+) => { RcDrop<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { RcDrop<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}

/// Alias with turbofish syntax.
pub macro rc_drop {
    (&$($x:tt)+) => { RcDrop::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { RcDrop::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}
