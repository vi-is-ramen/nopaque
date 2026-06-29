use core::{alloc::Layout, cmp::max, marker::PhantomData};

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
    drop: fn(&mut ()),
}

#[repr(transparent)]
pub struct RcDrop<const _T: usize, Tx> (
    #[doc(hidden)] usize,
    PhantomData<Tx>
);

impl<const _T: usize, Tx> core::fmt::Debug for RcDrop<_T, Tx> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("ArcDrop![{:p}]", self.0 as *const ()))
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
        #[cfg(debug_assertions)]
        {
            meta.align_t = align_of::<T>() as u16;
        }
        *data = t;
        Self(addr as usize + padding + size_of::<Meta>(), PhantomData)
    }

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
        #[cfg(debug_assertions)]
        {
            meta.align_t = align_of::<T>() as u16;
        }
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
    #[inline(always)]
    pub unsafe fn rc_load(&self) -> usize {
        self.meta().refc as _
    }

    #[inline(always)]
    pub unsafe fn rc_store(&self, v: usize) {
        self.meta().refc = v as _;
    }

    #[inline(always)]
    pub unsafe fn rc_add(&self, v: usize) -> usize {
        let rv = self.meta().refc;
        self.meta().refc += v as u16;
        rv as _
    }

    #[inline(always)]
    pub unsafe fn rc_sub(&self, v: usize) -> usize {
        let rv = self.meta().refc;
        self.meta().refc -= v as u16;
        rv as _
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

pub macro RcDrop {
    (&$($x:tt)+) => { RcDrop<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { RcDrop<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}

pub macro rc_drop {
    (&$($x:tt)+) => { RcDrop::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { RcDrop::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}
