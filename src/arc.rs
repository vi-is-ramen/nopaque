//! An atomic reference‑counted opaque pointer (thread‑safe).

use core::{alloc::Layout, cmp::max, marker::PhantomData, sync::atomic::AtomicU16};

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

/// An atomic reference‑counted pointer to an erased type.
///
/// `Arc` is analogous to `std::sync::Arc` but with the type erased. It uses
/// an atomic `u16` for the reference count, making it suitable for sharing
/// across threads.
///
/// The handle can be cloned, and the allocation is freed when the last clone
/// is dropped. Downcasting works the same as for `Box`.
///
/// # Example
///
/// ```
/// use nopaque::Arc;
/// use std::thread;
///
/// # macro_rules! hash { ($s:literal) => { 123 } }  // dummy
/// let a = Arc::new(vec![1, 2, 3]);
/// let a2 = a.clone();
///
/// thread::spawn(move || {
///     let v: &Vec<i32> = a2.downcast();
///     assert_eq!(v[0], 1);
/// }).join().unwrap();
/// ```
#[repr(transparent)]
pub struct Arc<const _T: usize, Tx = ()>(
    #[doc(hidden)] *const (), /* points to HdlMeta.data */
    PhantomData<Tx>,
);

impl<const _T: usize, Tx> core::fmt::Debug for Arc<_T, Tx> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("Arc![{:p}]", self.0))
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

impl<const _T: usize, Tx> Drop for Arc<_T, Tx> {
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
impl<const _T: usize, Tx> Arc<_T, Tx> {
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    fn meta(&self) -> &mut Meta {
        unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() }
    }
}

// constructors
impl<const _T: usize, Tx> Arc<_T, Tx> {
    /// Creates a new `Arc` containing the given value.
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

impl<const _T: usize, Tx> core::ops::Deref for Arc<_T, Tx> {
    type Target = Tx;

    fn deref(&self) -> &Self::Target {
        unsafe { (self.0 as *const Tx).as_ref_unchecked() }
    }
}

// direct RC interaction, ACTUALLY UNSAFE
impl<const _T: usize, Tx> Arc<_T, Tx> {
    /// Reads the current reference count.
    ///
    /// # Safety
    ///
    /// The count may be modified concurrently; this is only for debugging or
    /// exotic use cases.
    #[inline(always)]
    pub unsafe fn rc_load(&self) -> usize {
        self.meta().refc.load(core::sync::atomic::Ordering::Relaxed) as _
    }

    /// Overwrites the reference count.
    ///
    /// # Safety
    ///
    /// This can easily break the reference counting invariants. Only use if you
    /// know exactly what you are doing.
    #[inline(always)]
    pub unsafe fn rc_store(&self, v: usize) {
        self.meta()
            .refc
            .store(v as _, core::sync::atomic::Ordering::Release)
    }

    /// Atomically adds to the reference count and returns the previous value.
    #[inline(always)]
    pub unsafe fn rc_add(&self, v: usize) -> usize {
        self.meta()
            .refc
            .fetch_add(v as _, core::sync::atomic::Ordering::AcqRel) as _
    }

    /// Atomically subtracts from the reference count and returns the previous value.
    #[inline(always)]
    pub unsafe fn rc_sub(&self, v: usize) -> usize {
        self.meta()
            .refc
            .fetch_sub(v as _, core::sync::atomic::Ordering::AcqRel) as _
    }

    /// Increments the reference count by one.
    #[inline(always)]
    pub unsafe fn rc_inc(&self) -> usize {
        unsafe { self.rc_add(1) }
    }

    /// Decrements the reference count by one.
    #[inline(always)]
    pub unsafe fn rc_dec(&self) -> usize {
        unsafe { self.rc_sub(1) }
    }
}

/// Macro to construct an `Arc` type with the hash of the given identifier.
pub macro Arc($($x:tt)+) { Arc<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }> }

/// Type alias macro for `Arc` (useful in generic contexts).
pub macro arc($($x:tt)+) { <Arc::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }>> }
