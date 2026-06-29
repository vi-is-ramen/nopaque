use core::{alloc::Layout, cmp::max, marker::PhantomData, ptr::addr_of};

use crate::ExplicitDrop;

/// Metadata stored before the actual data.
///
/// The layout in memory is: `[ padding ] [ Meta ] [ Tx ]`.
/// The padding ensures `Meta` is aligned to at least 8 bytes.
#[repr(C)]
#[doc(hidden)]
struct Meta {
    size: u32,
    align: u16,
    at: *mut u8,
    drop: fn(&mut ()),
}

/// A uniquely‑owned pointer that calls an explicit destructor before deallocation.
///
/// `BoxDrop<_T, Tx>` is identical to [`Box`] except that when the last reference
/// is dropped, the `drop` function stored in the metadata is called on the
/// pointed‑to value **before** the memory is freed. This allows for custom
/// destruction logic, typically via the [`ExplicitDrop`] trait or a user‑supplied
/// function.
///
/// # Type Parameters
///
/// See [`Box`] for the meaning of `_T` and `Tx`.
///
/// # Memory Layout
///
/// Identical to [`Box`]: `[ padding ] [ Meta ] [ Tx ]`.
///
/// # Provider / Consumer Forms
///
/// The [`BoxDrop!`] and [`boxed_drop!`] macros provide two forms:
/// - `BoxDrop!(&MyType)` → `Tx = MyType` (provider, can dereference). Requires
///   `MyType` to be a concrete type in scope.
/// - `BoxDrop!(MyType)`   → `Tx = ()`     (consumer, opaque). `MyType` is only
///   a token used for hashing; it does **not** need to be defined.
///
/// # Examples
///
/// Using [`ExplicitDrop`]:
/// ```
/// # use nopaque::{BoxDrop, boxed_drop, ExplicitDrop};
/// struct MyResource;
///
/// impl ExplicitDrop for MyResource {
///     fn drop(&mut self) {
///         println!("Cleaning up resource");
///     }
/// }
///
/// let b = <boxed_drop!(&MyResource)>::new(MyResource);
/// // When `b` goes out of scope, `MyResource::drop` is called.
/// ```
///
/// Using a custom function:
/// ```
/// # use nopaque::{BoxDrop, boxed_drop};
/// static mut DROPPED: bool = false;
///
/// fn my_drop(_ptr: &mut u32) {
///     unsafe { DROPPED = true; }
/// }
/// 
/// type Custom = u32;
///
/// let b = <boxed_drop!(&Custom)>::new_with_drop(42u32, my_drop);
/// // When `b` is dropped, `my_drop` is called.
/// ```
///
/// Consumer (opaque):
/// ```
/// # use nopaque::{BoxDrop, boxed_drop};
/// type Handle = BoxDrop!(MyStruct);   // MyStruct is just a token
/// ```
#[repr(transparent)]
pub struct BoxDrop<const _T: usize, Tx>(
    #[doc(hidden)] usize,
    PhantomData<Tx>,
);

impl<const _T: usize, Tx> core::fmt::Debug for BoxDrop<_T, Tx> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_fmt(format_args!("BoxDrop![{:p}]", self.0 as *const ()))
    }
}

impl<const _T: usize, Tx> Drop for BoxDrop<_T, Tx> {
    #[inline(always)]
    fn drop(&mut self) {
        let meta =
            unsafe { ((self.0 as usize - size_of::<Meta>()) as *mut Meta).as_mut_unchecked() };
        (meta.drop)(unsafe { (self.0 as *mut ()).as_mut_unchecked() });
        let layout =
            unsafe { Layout::from_size_align_unchecked(meta.size as usize, meta.align as usize) };
        unsafe {
            alloc::alloc::dealloc(meta.at, layout);
        }
    }
}

impl<const _T: usize, Tx> BoxDrop<_T, Tx> {
    /// Creates a new `BoxDrop` using the `ExplicitDrop` implementation of `T`.
    ///
    /// The drop function stored in the metadata will be
    /// `crate::edrop::call_explicit_drop::<T>`, which calls `<T as ExplicitDrop>::drop`.
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
        meta.drop = crate::edrop::call_explicit_drop::<T>;
        *data = t;
        Self(addr as usize + padding + size_of::<Meta>(), PhantomData)
    }

    /// Creates a new `BoxDrop` with a user‑provided drop function.
    ///
    /// The function `drop` will be called with a mutable pointer to the value
    /// just before deallocation.
    pub fn new_with_drop<T>(t: T, drop: fn(&mut T)) -> Self {
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
        meta.drop = *unsafe { (addr_of!(drop) as *const () as *const fn(&mut ())).as_ref_unchecked() };
        *data = t;
        Self(addr as usize + padding + size_of::<Meta>(), PhantomData)
    }
}

impl<const _T: usize, Tx> core::ops::Deref for BoxDrop<_T, Tx> {
    type Target = Tx;

    fn deref(&self) -> &Self::Target {
        unsafe { (self.0 as *const Tx).as_ref_unchecked() }
    }
}

impl<const _T: usize, Tx> core::ops::DerefMut for BoxDrop<_T, Tx> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { (self.0 as *mut Tx).as_mut_unchecked() }
    }
}

/// Creates a `BoxDrop` type with the correct `_T` hash (no turbofish).
///
/// This macro expands directly to `BoxDrop<{hash}, ...>`. It has two forms:
/// - **With `&`**: `BoxDrop!(&MyType)` → `Tx = MyType`, hash from `MyType`.
///   Use on the provider side; requires `MyType` to be a type.
/// - **Without `&`**: `BoxDrop!(MyType)` → `Tx = ()`, hash from `MyType`.
///   Use on the consumer side; `MyType` is only a token—it need not be defined.
///
/// The hash is derived from the token sequence (`MyType`). The `&` is
/// a syntactic marker that tells the macro to use that token as the inner type.
pub macro BoxDrop {
    (&$($x:tt)+) => { BoxDrop<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { BoxDrop<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}

/// Alias for the `BoxDrop` macro using turbofish syntax.
///
/// This macro expands to `BoxDrop::<{hash}, ...>`, equivalent to the type
/// produced by `BoxDrop!` but with `::<>`. The same two‑form semantics apply.
pub macro boxed_drop {
    (&$($x:tt)+) => { BoxDrop::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, $($x)+> },
    ($($x:tt)+) => { BoxDrop::<{ crate::hash!(stringify!($($x)+).as_bytes()) as usize }, ()> },
}
