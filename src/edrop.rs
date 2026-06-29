pub const trait ExplicitDrop { fn drop(&mut self); }

pub(crate) fn call_explicit_drop<T: ExplicitDrop>(ptr: &mut ()) {
    <T as ExplicitDrop>::drop(unsafe { (core::ptr::addr_of!(*ptr) as *mut T).as_mut_unchecked() });
}
