//! A non‑atomic reference‑counted opaque pointer with custom drop.

use core::{alloc::Layout, cmp::max, marker::PhantomData, ptr::addr_of};

use crate::ExplicitDrop;

#[repr(C)]
#[doc(hidden)]
struct Meta {
    refc: u16,
    size: u32,
    align: u16,
    at: *mut u8,
    #[cfg(debug_assertions)]
    align_t: u16,
    drop: fn(*const ()),
}

/// A non‑atomic reference‑counted pointer with a custom destructor.
///
/// This is the single‑threaded counterpart of `ArcDrop`. See `Rc` and `BoxDrop`
/// for details.
#[repr(transparent)]
pub struct RcDrop<const _T: usize, Tx = ()> (
    #[doc(hidden)] *const (), /* points to HdlMeta.data */
    PhantomData<Tx>
);

impl<const _T: usize, Tx> core::fmt::Debug for RcDrop<_T, Tx> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("ArcDrop![{:p}]", self.0))
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

impl<const _T: usize, Tx> Drop for RcDrop<_T, Tx> {
    #[inline(always)]
    fn drop(&mut self) {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        meta.refc -= 1;

        if meta.refc == 0 {
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
impl<const _T: usize, Tx> RcDrop<_T, Tx> {
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    fn meta(&self) -> &mut Meta {
        unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() }
    }
}

// constructors
impl<const _T: usize, Tx> RcDrop<_T, Tx> {
    /// Creates a new `RcDrop` from a value that implements `ExplicitDrop`.
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

    /// Creates a new `RcDrop` with a custom drop function.
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
impl<const _T: usize, Tx> RcDrop<_T, Tx> {
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

impl<const _T: usize, Tx> core::ops::Deref for RcDrop<_T, Tx> {
    type Target = Tx;

    fn deref(&self) -> &Self::Target {
        unsafe { (self.0 as *const Tx).as_ref_unchecked() }
    }
}

// direct RC interaction, ACTUALLY UNSAFE
impl<const _T: usize, Tx> RcDrop<_T, Tx> {
    /// Reads the current reference count (non‑atomic).
    #[inline(always)]
    pub unsafe fn rc_load(&self) -> usize {
        self.meta().refc as _
    }

    /// Overwrites the reference count.
    #[inline(always)]
    pub unsafe fn rc_store(&self, v: usize) {
        self.meta().refc = v as _;
    }

    /// Adds to the reference count.
    #[inline(always)]
    pub unsafe fn rc_add(&self, v: usize) -> usize {
        let rv = self.meta().refc;
        self.meta().refc += v as u16;
        rv as _
    }

    /// Subtracts from the reference count.
    #[inline(always)]
    pub unsafe fn rc_sub(&self, v: usize) -> usize {
        let rv = self.meta().refc;
        self.meta().refc -= v as u16;
        rv as _
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

/// Macro to construct an `RcDrop` type.
pub macro RcDrop($($x:tt)+) { RcDrop<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }> }

/// Type alias macro for `RcDrop`.
pub macro rc_drop($($x:tt)+) { <RcDrop::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }>> }
