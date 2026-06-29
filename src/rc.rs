//! A non‑atomic reference‑counted opaque pointer (single‑threaded).

use core::{alloc::Layout, cmp::max, sync::atomic::AtomicU16};

#[repr(C)]
#[doc(hidden)]
struct Meta {
    refc: AtomicU16,
    size: u32,
    align: u16,
    at: *mut u8,
    #[cfg(debug_assertions)]
    align_t: u16,
}

/// A non‑atomic reference‑counted pointer to an erased type.
///
/// `Rc` is analogous to `std::rc::Rc` but with type erasure. It uses a
/// non‑atomic `u16` reference count and is **not** thread‑safe. It is suitable
/// for single‑threaded scenarios where you need shared ownership.
///
/// See `Arc` for the atomic version.
#[repr(transparent)]
pub struct Rc<const _T: usize>(#[doc(hidden)] *const () /* points to HdlMeta.data */);

impl<const _T: usize> core::fmt::Debug for Rc<_T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("Arc![{:p}]", self.0))
    }
}

impl<const _T: usize> Clone for Rc<_T> {
    #[inline(always)]
    fn clone(&self) -> Self {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        meta.refc.fetch_add(1, core::sync::atomic::Ordering::AcqRel);
        Self(self.0)
    }
}

impl<const _T: usize> Drop for Rc<_T> {
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

// internal methods
impl<const _T: usize> Rc<_T> {
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    fn meta(&self) -> &mut Meta {
        unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() }
    }
}

// constructors
impl<const _T: usize> Rc<_T> {
    /// Creates a new `Rc` containing the given value.
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
        Self((addr as usize + padding + size_of::<Meta>()) as *mut ())
    }
}

// transformers
impl<const _T: usize> Rc<_T> {
    /// Downcast to an immutable reference. See `Box::downcast`.
    #[inline(always)]
    pub fn downcast<T>(&self) -> &T {
        let meta = self.meta();
        #[cfg(debug_assertions)]
        {
            debug_assert!(meta.align_t as usize == align_of::<T>());
            debug_assert!(
                meta.at as usize - meta.size as usize + self.0 as usize == size_of::<T>()
            );
        };
        unsafe { (self.0 as *const T).as_ref_unchecked() }
    }

    /// Converts to a raw pointer. See `Box::to_raw`.
    #[inline(always)]
    pub unsafe fn to_raw(&self) -> usize {
        self.0 as _
    }

    /// Reconstructs from a raw pointer, **incrementing** the reference count.
    /// See `Arc::from_raw`.
    #[inline(always)]
    pub unsafe fn from_raw(addr: usize) -> Self {
        Self(addr as _).clone()
    }
}

// direct RC interaction, ACTUALLY UNSAFE
impl<const _T: usize> Rc<_T> {
    /// Reads the current reference count (non‑atomic).
    #[inline(always)]
    pub unsafe fn rc_load(&self) -> usize {
        self.meta().refc.load(core::sync::atomic::Ordering::Relaxed) as _
    }

    /// Overwrites the reference count.
    #[inline(always)]
    pub unsafe fn rc_store(&self, v: usize) {
        self.meta()
            .refc
            .store(v as _, core::sync::atomic::Ordering::Release)
    }

    /// Adds to the reference count.
    #[inline(always)]
    pub unsafe fn rc_add(&self, v: usize) -> usize {
        self.meta()
            .refc
            .fetch_add(v as _, core::sync::atomic::Ordering::AcqRel) as _
    }

    /// Subtracts from the reference count.
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

/// Macro to construct an `Rc` type with the hash of the given identifier.
pub macro Rc($($x:tt)+) { Rc<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }> }

/// Type alias macro for `Rc`.
pub macro rc($($x:tt)+) { <Rc::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }>> }
