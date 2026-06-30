use core::{alloc::Layout, cmp::max, marker::PhantomData, sync::atomic::AtomicU16};
use core::mem::size_of;

use crate::call_implicit_drop;

/// Metadata for atomic reference‑counted pointers.
#[repr(C)]
#[doc(hidden)]
struct Meta {
    refc: AtomicU16,
    size: u32,
    align: u16,
    at: *mut u8,
    drop: fn(&mut ()),
}

/// A type‑guarded atomic reference‑counted pointer.
///
/// `Arc<_T, Tx>` is the atomic (thread‑safe) counterpart of [`Rc`]. It uses
/// `AtomicU16` for the reference count, making it `Send` and `Sync` as long as
/// `Tx` is `Send` and `Sync`.
///
/// All other properties (type‑guarding via `_T`, `repr(transparent)`, memory
/// layout `[ padding ] [ Meta ] [ Tx ]`) are identical to `Rc`.
///
/// # Provider / Consumer Forms
///
/// The [`Arc!`] and [`arc!`] macros provide two forms:
/// - `Arc!(&MyType)` → `Tx = MyType` (provider, can dereference).
/// - `Arc!(MyType)`   → `Tx = ()`     (consumer, opaque; `MyType` is just a token).
///
/// # Examples
///
/// ```
/// # use nopaque::{Arc, arc};
/// use std::thread;
/// 
/// type Shared = i32;
///
/// let a = <arc!(&Shared)>::new(42);
/// let a2 = a.clone();
///
/// thread::spawn(move || {
///     assert_eq!(*a2, 42);
/// }).join().unwrap();
/// ```
#[repr(transparent)]
pub struct Arc<const _T: usize, Tx>(
    #[doc(hidden)] usize,
    PhantomData<Tx>,
);

unsafe impl<const _T: usize, Tx: Send> Send for Arc<_T, Tx> {}
unsafe impl<const _T: usize, Tx: Sync> Sync for Arc<_T, Tx> {}

impl<const _T: usize, Tx> core::fmt::Debug for Arc<_T, Tx> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("Arc![{:p}]", self.0 as *const ()))
    }
}

impl<const _T: usize, Tx> Clone for Arc<_T, Tx> {
    #[inline(always)]
    fn clone(&self) -> Self {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        meta.refc.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
        Self(self.0, PhantomData)
    }
}

impl<const _T: usize, Tx> core::ops::Drop for Arc<_T, Tx> {
    #[inline(always)]
    fn drop(&mut self) {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        let old = meta.refc.fetch_sub(1, core::sync::atomic::Ordering::AcqRel);

        if old == 1 {
            let layout = unsafe {
                Layout::from_size_align_unchecked(meta.size as usize, meta.align as usize)
            };
            unsafe {
                alloc::alloc::dealloc(meta.at, layout);
            }
        }
    }
}

impl<const _T: usize, Tx> Arc<_T, Tx> {
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    fn meta(&self) -> &mut Meta {
        unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() }
    }
}

impl<const _T: usize, Tx> Arc<_T, Tx> {
    /// Creates a new `Arc` with initial reference count 1.
    ///
    /// The drop function is the implicit `drop_in_place` for `T`.
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
        meta.refc.store(1, core::sync::atomic::Ordering::Relaxed);
        *data = t;
        Self(addr as usize + padding + size_of::<Meta>(), PhantomData)
    }
}

impl<const _T: usize, Tx> core::ops::Deref for Arc<_T, Tx> {
    type Target = Tx;

    fn deref(&self) -> &Self::Target {
        unsafe { (self.0 as *const Tx).as_ref_unchecked() }
    }
}

impl<const _T: usize, Tx> Arc<_T, Tx> {
    /// Loads the current reference count atomically.
    ///
    /// # Safety
    ///
    /// Provided for debugging; misuse can lead to data races if not synchronized.
    #[inline(always)]
    pub unsafe fn rc_load(&self) -> usize {
        self.meta().refc.load(core::sync::atomic::Ordering::Relaxed) as _
    }

    /// Stores a new reference count atomically.
    ///
    /// # Safety
    ///
    /// Use with extreme care.
    #[inline(always)]
    pub unsafe fn rc_store(&self, v: usize) {
        self.meta()
            .refc
            .store(v as _, core::sync::atomic::Ordering::Release)
    }

    /// Atomically adds `v` to the reference count and returns the previous value.
    #[inline(always)]
    pub unsafe fn rc_add(&self, v: usize) -> usize {
        self.meta()
            .refc
            .fetch_add(v as _, core::sync::atomic::Ordering::AcqRel) as _
    }

    /// Atomically subtracts `v` from the reference count and returns the previous value.
    #[inline(always)]
    pub unsafe fn rc_sub(&self, v: usize) -> usize {
        self.meta()
            .refc
            .fetch_sub(v as _, core::sync::atomic::Ordering::AcqRel) as _
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

/// Creates an `Arc` type with the correct `_T` hash (no turbofish).
///
/// Two forms:
/// - `Arc!(&MyType)` → provider (Tx = MyType)
/// - `Arc!(MyType)`   → consumer (Tx = (), token need not be defined)
pub macro Arc {
    (&$($x:tt)+) => { Arc<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { Arc<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}

/// Alias with turbofish syntax.
pub macro arc {
    (&$($x:tt)+) => { Arc::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { Arc::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}
