/// A trait for types that need explicit destruction logic.
///
/// Types that implement `ExplicitDrop` can be stored in the `*Drop` variants
/// (`BoxDrop`, `ArcDrop`, `RcDrop`). The `drop` method will be called when the
/// handle is destroyed, exactly once per allocation.
///
/// # Example
///
/// ```ignore
/// use nopaque::ExplicitDrop;
///
/// struct MyResource {
///     fd: i32,
/// }
///
/// impl ExplicitDrop for MyResource {
///     fn drop(&mut self) {
///         unsafe { libc::close(self.fd) };
///     }
/// }
/// ```
pub const trait ExplicitDrop {
    fn drop(&mut self);
}
