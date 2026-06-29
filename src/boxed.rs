use core::{alloc::Layout, cmp::max, marker::PhantomData};

#[repr(C)]
#[doc(hidden)]
struct Meta {
    size: u32,
    align: u16,
    at: *mut u8,
    #[cfg(debug_assertions)]
    align_t: u16,
}

#[repr(transparent)]
pub struct Box<const _T: usize, Tx> (
    #[doc(hidden)] usize,
    PhantomData<Tx>,
);

impl<const _T: usize, Tx> core::fmt::Debug for Box<_T, Tx> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("Box![{:p}]", self.0 as *const ()))
    }
}

impl<const _T: usize, Tx> Drop for Box<_T, Tx> {
    #[inline(always)]
    fn drop(&mut self) {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };

        let layout =
            unsafe { Layout::from_size_align_unchecked(meta.size as usize, meta.align as usize) };
        unsafe {
            alloc::alloc::dealloc(meta.at, layout);
        }
    }
}

impl<const _T: usize, Tx> Box<_T, Tx> {
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
        #[cfg(debug_assertions)]
        {
            meta.align_t = align_of::<T>() as u16;
        }
        *data = t;
        Self(addr as usize + padding + size_of::<Meta>(), PhantomData)
    }
}

impl<const _T: usize, Tx> core::ops::Deref for Box<_T, Tx> {
    type Target = Tx;

    fn deref(&self) -> &Self::Target {
        unsafe { (self.0 as *const Tx).as_ref_unchecked() }
    }
}

impl<const _T: usize, Tx> core::ops::DerefMut for Box<_T, Tx> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { (self.0 as *mut Tx).as_mut_unchecked() }
    }
}

pub macro Box {
    (&$($x:tt)+) => { Box<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { Box<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}

pub macro boxed {
    (&$($x:tt)+) => { Box::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { Box::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}
