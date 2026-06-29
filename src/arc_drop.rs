//! An atomic reference‑counted opaque pointer with custom drop.

use core::{alloc::Layout, cmp::max, marker::PhantomData, ptr::addr_of, sync::atomic::AtomicU16};

use crate::ExplicitDrop;

#[repr(C)]
#[doc(hidden)]
struct Meta {
    refc: AtomicU16,
    size: u32,
    align: u16,
    at: *mut u8,
    #[cfg(debug_assertions)]
    align_t: u16,
    drop: fn(*const ()),
}

/// An atomic reference‑counted pointer with a custom destructor.
///
/// This combines the features of `Arc` and `BoxDrop`. It is thread‑safe and
/// calls a stored drop function when the last reference is released.
///
/// See `Arc` and `BoxDrop` for details.
#[repr(transparent)]
pub struct ArcDrop<const _T: usize, Tx = ()>(
    #[doc(hidden)] *const (), /* points to HdlMeta.data */
    PhantomData<Tx>,
);

impl<const _T: usize, Tx> core::fmt::Debug for ArcDrop<_T, Tx> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("ArcDrop![{:p}]", self.0))
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

impl<const _T: usize, Tx> Drop for ArcDrop<_T, Tx> {
    #[inline(always)]
    fn drop(&mut self) {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        let old = meta.refc.fetch_sub(1, core::sync::atomic::Ordering::AcqRel);

        if old == 1 {
            (meta.drop)(self.0);
            let layout = unsafe {
                Layout::from_size_align_unchecked(meta.size as usize, meta.align as usize)
            };
            unsafe {
                alloc::alloc::dealloc(meta.at, layout);
            }
        }
    }
}

// internal methods
impl<const _T: usize, Tx> ArcDrop<_T, Tx> {
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    fn meta(&self) -> &mut Meta {
        unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() }
    }
}

// constructors
impl<const _T: usize, Tx> ArcDrop<_T, Tx> {
    /// Creates a new `ArcDrop` from a value that implements `ExplicitDrop`.
    pub fn new<T: ExplicitDrop>(t: T) -> Self {
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
        let d = <T as ExplicitDrop>::drop;
        meta.drop = *unsafe { (addr_of!(d) as *const fn(*const ())).as_ref_unchecked() };
        #[cfg(debug_assertions)]
        {
            meta.align_t = align_of::<T>() as u16;
        }
        *data = t;
        Self((addr as usize + padding + size_of::<Meta>()) as *mut (), PhantomData)
    }

    /// Creates a new `ArcDrop` with a custom drop function.
    pub fn new_with_drop<T>(t: T, drop: fn(*const ())) -> Self {
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
        meta.drop = drop;
        #[cfg(debug_assertions)]
        {
            meta.align_t = align_of::<T>() as u16;
        }
        *data = t;
        Self((addr as usize + padding + size_of::<Meta>()) as *mut (), PhantomData)
    }
}

// transformers
impl<const _T: usize, Tx> ArcDrop<_T, Tx> {
    /// Converts to a raw pointer. See `Box::to_raw`.
    #[inline(always)]
    pub unsafe fn to_raw(&self) -> usize {
        self.0 as _
    }

    /// Reconstructs from a raw pointer, **incrementing** the reference count.
    #[inline(always)]
    pub unsafe fn from_raw(addr: usize) -> Self {
        Self(addr as _, PhantomData).clone()
    }
}

impl<const _T: usize, Tx> core::ops::Deref for ArcDrop<_T, Tx> {
    type Target = Tx;

    fn deref(&self) -> &Self::Target {
        unsafe { (self.0 as *const Tx).as_ref_unchecked() }
    }
}

// direct RC interaction, ACTUALLY UNSAFE
impl<const _T: usize, Tx> ArcDrop<_T, Tx> {
    /// Reads the current reference count. See `Arc::rc_load`.
    #[inline(always)]
    pub unsafe fn rc_load(&self) -> usize {
        self.meta().refc.load(core::sync::atomic::Ordering::Relaxed) as _
    }

    /// Overwrites the reference count. See `Arc::rc_store`.
    #[inline(always)]
    pub unsafe fn rc_store(&self, v: usize) {
        self.meta()
            .refc
            .store(v as _, core::sync::atomic::Ordering::Release)
    }

    /// Adds to the reference count. See `Arc::rc_add`.
    #[inline(always)]
    pub unsafe fn rc_add(&self, v: usize) -> usize {
        self.meta()
            .refc
            .fetch_add(v as _, core::sync::atomic::Ordering::AcqRel) as _
    }

    /// Subtracts from the reference count. See `Arc::rc_sub`.
    #[inline(always)]
    pub unsafe fn rc_sub(&self, v: usize) -> usize {
        self.meta()
            .refc
            .fetch_sub(v as _, core::sync::atomic::Ordering::AcqRel) as _
    }

    /// Increments the reference count.
    #[inline(always)]
    pub unsafe fn rc_inc(&self) -> usize {
        unsafe { self.rc_add(1) }
    }

    /// Decrements the reference count.
    #[inline(always)]
    pub unsafe fn rc_dec(&self) -> usize {
        unsafe { self.rc_sub(1) }
    }
}

/// Macro to construct an `ArcDrop` type.
pub macro ArcDrop {
    ($x:ty) => { ArcDrop<{ crate::hash!(stringify!($x).as_bytes()) as usize }> },
    (@$x:ty) => { ArcDrop<{ crate::hash!(stringify!($x).as_bytes()) as usize }, $x> },
}

pub macro arc_drop {
    ($x:ty) => { ArcDrop::<{ crate::hash!(stringify!($x).as_bytes()) as usize }> },
    (@$x:ty) => { ArcDrop::<{ crate::hash!(stringify!($x).as_bytes()) as usize }, $x> },
}
