//! A non‑reference‑counted opaque pointer with custom drop support.

use core::{alloc::Layout, cmp::max, ptr::addr_of};

use crate::ExplicitDrop;

#[repr(C)]
#[doc(hidden)]
struct Meta {
    size: u32,
    align: u16,
    at: *mut u8,
    #[cfg(debug_assertions)]
    align_t: u16,
    drop: fn(*const ()),
}

/// An opaque owning pointer that calls a custom destructor.
///
/// `BoxDrop` behaves like `Box` but additionally stores a function pointer that
/// will be invoked when the handle is dropped. This is useful when the stored
/// type does not implement `Drop` (e.g. a C‑style struct) or when you need to
/// perform additional cleanup.
///
/// The stored type must either implement `ExplicitDrop` (then the `drop` method
/// is used) or you must provide a custom `drop` function via
/// `new_with_drop`.
///
/// # Example
///
/// ```
/// use nopaque::{BoxDrop, ExplicitDrop};
///
/// # macro_rules! hash { ($s:literal) => { 123 } }  // dummy
/// struct Resource { fd: i32 }
///
/// impl ExplicitDrop for Resource {
///     fn drop(&mut self) {
///         unsafe { libc::close(self.fd) };
///     }
/// }
///
/// let b = BoxDrop::new(Resource { fd: 42 });
/// // The destructor will be called when `b` goes out of scope.
/// ```
#[repr(transparent)]
pub struct BoxDrop<const _T: usize>(#[doc(hidden)] *const () /* points to HdlMeta.data */);

impl<const _T: usize> core::fmt::Debug for BoxDrop<_T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("BoxDrop![{:p}]", self.0))
    }
}

impl<const _T: usize> Drop for BoxDrop<_T> {
    #[inline(always)]
    fn drop(&mut self) {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        (meta.drop)(self.0);
        let layout =
            unsafe { Layout::from_size_align_unchecked(meta.size as usize, meta.align as usize) };
        unsafe {
            alloc::alloc::dealloc(meta.at, layout);
        }
    }
}

// internal methods
impl<const _T: usize> BoxDrop<_T> {
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    fn meta(&self) -> &mut Meta {
        unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() }
    }
}

// constructors
impl<const _T: usize> BoxDrop<_T> {
    /// Creates a new `BoxDrop` from a value that implements `ExplicitDrop`.
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
        Self((addr as usize + padding + size_of::<Meta>()) as *mut ())
    }

    /// Creates a new `BoxDrop` with a custom drop function.
    ///
    /// The `drop` function receives a raw pointer to the stored value as its
    /// argument. It must not attempt to deallocate the memory – that is handled
    /// automatically.
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
        Self((addr as usize + padding + size_of::<Meta>()) as *mut ())
    }
}

// transformers
impl<const _T: usize> BoxDrop<_T> {
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

    /// Downcast to a mutable reference. See `Box::downcast_mut`.
    #[inline(always)]
    pub fn downcast_mut<T>(&mut self) -> &mut T {
        let meta = self.meta();
        #[cfg(debug_assertions)]
        {
            debug_assert!(meta.align_t as usize == align_of::<T>());
            debug_assert!(
                meta.at as usize - meta.size as usize + self.0 as usize == size_of::<T>()
            );
        };
        unsafe { (self.0 as *mut T).as_mut_unchecked() }
    }

    /// Converts to a raw pointer. See `Box::to_raw`.
    #[inline(always)]
    pub unsafe fn to_raw(&self) -> usize {
        self.0 as _
    }

    /// Reconstructs from a raw pointer. See `Box::from_raw`.
    #[inline(always)]
    pub unsafe fn from_raw(addr: usize) -> Self {
        Self(addr as _)
    }
}

/// Macro to construct a `BoxDrop` type with the hash of the given identifier.
pub macro BoxDrop($($x:tt)+) { BoxDrop<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }> }
