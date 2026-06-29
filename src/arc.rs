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

impl<const _T: usize, Tx> Arc<_T, Tx> {
    #[inline(always)]
    #[allow(clippy::mut_from_ref)]
    fn meta(&self) -> &mut Meta {
        unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() }
    }
}

impl<const _T: usize, Tx> Arc<_T, Tx> {
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
        meta.refc.store(1, core::sync::atomic::Ordering::Relaxed);
        #[cfg(debug_assertions)]
        {
            meta.align_t = align_of::<T>() as u16;
        }
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
    #[inline(always)]
    pub unsafe fn rc_load(&self) -> usize {
        self.meta().refc.load(core::sync::atomic::Ordering::Relaxed) as _
    }

    #[inline(always)]
    pub unsafe fn rc_store(&self, v: usize) {
        self.meta()
            .refc
            .store(v as _, core::sync::atomic::Ordering::Release)
    }

    #[inline(always)]
    pub unsafe fn rc_add(&self, v: usize) -> usize {
        self.meta()
            .refc
            .fetch_add(v as _, core::sync::atomic::Ordering::AcqRel) as _
    }

    #[inline(always)]
    pub unsafe fn rc_sub(&self, v: usize) -> usize {
        self.meta()
            .refc
            .fetch_sub(v as _, core::sync::atomic::Ordering::AcqRel) as _
    }

    #[inline(always)]
    pub unsafe fn rc_inc(&self) -> usize {
        unsafe { self.rc_add(1) }
    }

    #[inline(always)]
    pub unsafe fn rc_dec(&self) -> usize {
        unsafe { self.rc_sub(1) }
    }
}

pub macro Arc {
    (&$($x:tt)+) => { Arc<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { Arc<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}

pub macro arc {
    (&$($x:tt)+) => { Arc::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { Arc::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}
