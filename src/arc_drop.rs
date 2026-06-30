use core::{alloc::Layout, cmp::max, marker::PhantomData, sync::atomic::AtomicU16};
use core::mem::size_of;
use core::mem::align_of;
use core::clone::Clone;
use core::marker::{Sync, Send};

use crate::ExplicitDrop;

/// Metadata for `ArcDrop`.
#[repr(C)]
#[doc(hidden)]
struct Meta {
    refc: AtomicU16,
    size: u32,
    align: u16,
    at: *mut u8,
    drop: fn(&mut ()),
}

/// Atomic reference‑counted pointer with explicit drop semantics.
///
/// This is the `Drop` variant of [`Arc`]: when the last reference is dropped,
/// the stored drop function (from `ExplicitDrop` or custom) is called before
/// deallocation.
///
/// It is `Send` and `Sync` when `Tx` is `Send` and `Sync`.
///
/// # Provider / Consumer Forms
///
/// The [`ArcDrop!`] and [`arc_drop!`] macros provide two forms:
/// - `ArcDrop!(&MyType)` → `Tx = MyType` (provider, can dereference).
/// - `ArcDrop!(MyType)`   → `Tx = ()`     (consumer, opaque; `MyType` is just a token).
///
/// # Examples
///
/// ```
/// # use nopaque::{ArcDrop, arc_drop, ExplicitDrop};
/// use std::thread;
///
/// struct MyResource;
///
/// impl ExplicitDrop for MyResource {
///     fn drop(&mut self) {
///         println!("Cleaning up resource");
///     }
/// }
///
/// let a = <arc_drop!(&MyResource)>::new(MyResource);
/// let a2 = a.clone();
///
/// thread::spawn(move || {
///     let _ = a2; // drops in the thread
/// }).join().unwrap();
/// // The resource is dropped when the last reference (a) goes out of scope.
/// ```
#[repr(transparent)]
pub struct ArcDrop<const _T: usize, Tx>(
    #[doc(hidden)] usize,
    PhantomData<Tx>,
);

unsafe impl<const _T: usize, Tx: Send> Send for ArcDrop<_T, Tx> {}
unsafe impl<const _T: usize, Tx: Sync> Sync for ArcDrop<_T, Tx> {}

impl<const _T: usize, Tx> core::fmt::Debug for ArcDrop<_T, Tx> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("ArcDrop![{:p}]", self.0 as *const ()))
    }
}

impl<const _T: usize, Tx> Clone for ArcDrop<_T, Tx> {
    #[inline(always)]
    fn clone(&self) -> Self {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        meta.refc.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
        Self(self.0, PhantomData)
    }
}

impl<const _T: usize, Tx> core::ops::Drop for ArcDrop<_T, Tx> {
    #[inline(always)]
    fn drop(&mut self) {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        let old = meta.refc.fetch_sub(1, core::sync::atomic::Ordering::AcqRel);

        if old == 1 {
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

impl<const _T: usize, Tx> ArcDrop<_T, Tx> {
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    fn meta(&self) -> &mut Meta {
        unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() }
    }
}

impl<const _T: usize, Tx> ArcDrop<_T, Tx> {
    /// Creates a new `ArcDrop` using the `ExplicitDrop` implementation of `T`.
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
        meta.refc.store(1, core::sync::atomic::Ordering::Relaxed);
        meta.drop = crate::edrop::call_explicit_drop::<T>;
        *data = t;
        Self(addr as usize + padding + size_of::<Meta>(), PhantomData)
    }

    /// Creates a new `ArcDrop` with a user‑provided drop function.
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
        meta.refc.store(1, core::sync::atomic::Ordering::Relaxed);
        *data = t;
        Self(addr as usize + padding + size_of::<Meta>(), PhantomData)
    }
}

impl<const _T: usize, Tx> core::ops::Deref for ArcDrop<_T, Tx> {
    type Target = Tx;

    fn deref(&self) -> &Self::Target {
        unsafe { (self.0 as *const Tx).as_ref_unchecked() }
    }
}

impl<const _T: usize, Tx> ArcDrop<_T, Tx> {
    /// Loads the current reference count atomically.
    #[inline(always)]
    pub unsafe fn rc_load(&self) -> usize {
        self.meta().refc.load(core::sync::atomic::Ordering::Relaxed) as _
    }

    /// Stores a new reference count atomically.
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

/// Creates an `ArcDrop` type with the correct `_T` hash (no turbofish).
///
/// Two forms:
/// - `ArcDrop!(&MyType)` → provider (Tx = MyType)
/// - `ArcDrop!(MyType)`   → consumer (Tx = (), token need not be defined)
pub macro ArcDrop {
    (&$($x:tt)+) => { ArcDrop<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { ArcDrop<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}

/// Alias with turbofish syntax.
pub macro arc_drop {
    (&$($x:tt)+) => { ArcDrop::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { ArcDrop::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}
