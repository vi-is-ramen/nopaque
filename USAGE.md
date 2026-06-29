# `nopaque` — Type‑Safe Opaque Pointers for ABIs, the User Guide

**`nopaque`** is a Rust crate that provides smart pointers with erased concrete types, designed specifically for building **type‑safe dynamic APIs and ABIs**.  
It allows you to pass opaque handles across library boundaries (e.g., plugins, kernel modules, or FFI) while preserving the ability to recover the original type later, **without** sacrificing compile‑time safety.

---

## Why `nopaque`?

When building dynamic modules or plugins, you often need to:

- Hide internal data structures from the caller.
- Allow the caller to hold handles to objects it never directly inspects.
- Provide functions that accept or return such handles in a type‑safe way.

The classical approach uses `*mut c_void` or `usize` – but that throws away all type information and forces **every** interaction to be `unsafe`.  
With `nopaque` you can:

- Keep the type **information** (as a compile‑time hash) in the pointer type itself.
- **Downcast** the handle back to the concrete type when you know what it is.
- Rely on the Rust compiler to prevent mismatched types, because the hash is part of the type signature.

This makes your API both **opaque** (the caller sees only the handle type) and **type‑safe** (the compiler guarantees that only the right operations are performed on the right handles).

---

## Core Concepts

### Type Erasure via Compile‑Time Hash

Every `nopaque` pointer is parameterised by a `const _T: usize` – a **hash** that uniquely identifies the stored type.  
The hash is computed from the type’s name (or any string) using a `hash!` macro that **you** provide.  
For example:

```rust
// Your hash macro – must produce a `usize` constant
macro_rules! hash {
    ($s:literal) => {{
        const H: usize = my_const_hash($s.as_bytes());
        H
    }};
}
```

The same type name (string) must yield the **same hash** in all compilation units that share the ABI.

### Metadata Layout

Each allocation holds:

- A **header** (`Meta`) containing:
  - the allocation’s original address and layout,
  - a reference count (for `Arc`/`Rc` variants),
  - an optional `drop` function pointer,
  - (debug) alignment and size checks.
- The **value** itself, aligned to at least 8 bytes.

The user‑facing handle is a **pointer to the value**, with the header located just before it.  
Downcasting simply computes the offset and casts the pointer – all checks are performed at compile time (or in debug builds).

### Pointer Kinds

| Type        | Reference Counting | Custom Drop | Thread‑Safe | Downcast `mut` |
|-------------|---------------------|-------------|-------------|----------------|
| `Box`       | no                  | no          | –           | yes            |
| `BoxDrop`   | no                  | yes         | –           | yes            |
| `Rc`        | non‑atomic          | no          | no          | **no**         |
| `RcDrop`    | non‑atomic          | yes         | no          | **no**         |
| `Arc`       | atomic              | no          | yes         | **no**         |
| `ArcDrop`   | atomic              | yes         | yes         | **no**         |

> **Note:** For reference‑counted types (`Rc`, `Arc`, `RcDrop`, `ArcDrop`), **mutable downcasting (`downcast_mut`) is not provided**.  
> This is intentional: shared ownership does not grant exclusive access.  
> If you need mutable access, use `Box` or `BoxDrop` (unique ownership) or consider interior mutability (`RefCell`, `Mutex`, etc.).

---

## Getting Started

### Add `nopaque` to your `Cargo.toml`

```toml
[dependencies]
nopaque = { git = "..." }   # or use a registry version when available
```

The crate is `no_std` by default (with `alloc`). Enable the `std` feature if you need standard library integration (it is enabled by default).

### Define the `hash!` Macro

`nopaque` does **not** provide a hashing implementation – it leaves the choice to you.  
You must define a macro named `hash!` that takes a string literal and expands to a `usize` constant.

A simple example using [Fnv](https://docs.rs/fnv) or [xxhash](https://docs.rs/xxhash-rust) in a const context:

```rust
// In your crate root
const fn const_hash(s: &[u8]) -> usize {
    // Implement a const‑friendly hash (e.g., FNV‑1a)
    let mut h = 0x811c9dc5;
    let mut i = 0;
    while i < s.len() {
        h ^= s[i] as u32;
        h = h.wrapping_mul(0x01000193);
        i += 1;
    }
    h as usize
}

macro_rules! hash {
    ($s:literal) => {{
        const H: usize = const_hash($s.as_bytes());
        H
    }};
}
```

> **Critical:** The hash **must** be stable across all compilation units that exchange opaque handles.  
> Use the same hashing algorithm and the same string representation for each type.

### Using the Macros

For every pointer type, there is a macro that expands to the full type with the hash:

- `Box!(Type)` -> `Box<{ hash!("Type") }>`
- `Arc!(Type)` -> `Arc<{ hash!("Type") }>`
- `Rc!(Type)` -> `Rc<{ hash!("Type") }>`
- … and so on.

Lowercase variants (`rc!`, `arc!`, etc.) are also provided as type aliases for convenience.

**Example:**

```rust
use nopaque::Arc;

type DeviceHandle = Arc!(Device);   // expands to Arc<{ hash!("Device") }>
```

---

## Basic Usage

### Creating an Opaque Handle

```rust
use nopaque::Box;

let handle = Box::<0xdfefef86444098f>::new(42u32);
```

### Downcasting

When you know the original type, you can downcast to a reference:

```rust
let value: &u32 = handle.downcast::<u32>();
assert_eq!(*value, 42);
```

For unique pointers (`Box`, `BoxDrop`), you can also get a mutable reference:

```rust
let mut handle: Box![opaque string] = Box::new("hello".to_string());
let s: &mut String = handle.downcast_mut::<String>();
s.push_str(" world");
```

### Passing Across an ABI Boundary

Suppose you want to export a function that returns an opaque handle to a kernel object, and import it in a module.

**Kernel side (exports):**

```rust
// kernel/lib.rs
#[no_mangle]
pub extern "Rust" fn ke_device_new(name: &str) -> Arc!(Device) {
    Arc::new(Device::new(name))
}
```

**Module side (imports):**

```rust
// module/lib.rs
extern "Rust" {
    fn ke_device_new(name: &str) -> Arc!(Device);
}

fn use_device() {
    let dev = ke_device_new("eth0");
    // We can downcast only if we know the type, but we might just pass it around.
    // The type is still part of the signature, so the compiler enforces consistency.
}
```

The module never sees the definition of `Device` – it only knows the handle type `Arc!(Device)`. Yet the compiler ensures that this handle cannot be accidentally used as a handle for something else (e.g., `Arc!(Network)`), because the hash differs.

---

## Reference‑Counted Handles

`Arc` and `Rc` allow shared ownership. Cloning increments the reference count, and the allocation is freed when the last clone is dropped.

```rust
use nopaque::Arc;

let a = Arc::new(vec![1, 2, 3]);
let b = a.clone();   // refcount becomes 2
drop(a);             // refcount becomes 1
// b still owns the data
```

Because reference‑counted types are shared, they only provide **immutable** downcasting (`downcast`).  
If you need to mutate the contained value, consider:

- Using `Box` (unique ownership) instead.
- Wrapping the value in `RefCell` or `Mutex` and downcasting to that wrapper.
- Using `Arc::get_mut` (which returns `Option<&mut T>` if the refcount is 1) – but `nopaque` does not provide that directly; you would need to use raw pointer manipulation.

---

## Custom Destructors

The `*Drop` variants (`BoxDrop`, `ArcDrop`, `RcDrop`) let you control how the value is destroyed.

### The `ExplicitDrop` Trait

For types that implement `ExplicitDrop`, you can create a handle with `new`:

```rust
use nopaque::{BoxDrop, ExplicitDrop};

struct Resource {
    fd: i32,
}

impl ExplicitDrop for Resource {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}

let handle = BoxDrop::new(Resource { fd: 42 });
// When `handle` is dropped, `Resource::drop` will be called.
```

### Custom `drop` Function

You can also provide a raw function pointer that receives the value’s address:

```rust
fn custom_drop(ptr: *const ()) {
    // ptr points to the stored value
    let res = unsafe { &*(ptr as *const Resource) };
    unsafe { libc::close(res.fd) };
}

let handle = BoxDrop::new_with_drop(Resource { fd: 42 }, custom_drop);
```

This is useful when the type does not implement `Drop` (e.g., a C structure), or when you need to call a specific deinitialisation routine.

---

## Use Case: Kernel Module Interface (KMI)

Imagine you are building a kernel that loads **dynamic modules** (`.so` files) that run in ring 0.  
The kernel exposes a set of **stable functions** to modules – constructors for various kernel objects.  
Modules can call these functions to obtain opaque handles to kernel resources, but they never see the actual kernel types.

### Kernel Side

The kernel defines the public API with `nopaque` handles:

```rust
// kernel/api.rs
use nopaque::{Arc, KeStr};   // KeStr is a kernel‑specific string type

#[no_mangle]
pub extern "Rust" fn KeVtDeviceNew(name: KeStr) -> Arc!(Device) {
    // Actually create the device inside the kernel
    let dev = crate::dev::Device::new(name);
    Arc::new(dev)
}

#[no_mangle]
pub extern "Rust" fn KeVtDeviceGetName(dev: &Arc!(Device)) -> KeStr {
    // Downcast to access the device's fields
    let dev_ref: &Device = dev.downcast();
    dev_ref.name().clone()
}
```

The kernel has full knowledge of the `Device` type, but the function signatures only expose `Arc!(Device)` – an opaque handle.

### Module Side

The module **does not** have the definition of `Device`. It only has the **declaration** of the external functions:

```rust
// module/lib.rs
extern "Rust" {
    fn KeVtDeviceNew(name: KeStr) -> Arc!(Device);
    fn KeVtDeviceGetName(dev: &Arc!(Device)) -> KeStr;
}

fn setup() {
    let dev = unsafe { KeVtDeviceNew("eth0".into()) };
    let name = unsafe { KeVtDeviceGetName(&dev) };
    // The module can call kernel functions that accept opaque handles,
    // but it cannot downcast `dev` to `Device` because it doesn't know the type.
    // That is fine – it only uses the handle as an opaque token.
}
```

### Why This Works

- Both kernel and module **must** use the **same** `hash!` macro and the **same** string `"Device"` to compute the type parameter.  
  If they differ, the module will fail to link (or worse, cause UB) – but that’s a deliberate design choice: the ABI contract includes the hashing scheme.

- The module cannot accidentally treat a `Arc!(Device)` as a `Arc!(Network)` – the compiler will reject it because the hash constants differ.

- If the module wants to inspect the device, it must call a kernel function (like `KeVtDeviceGetName`) that performs the downcast inside the kernel – where it is safe.

### Downcasting in Modules

In general, a module **must not** downcast a handle it did not construct, because it does not have the concrete type.  
The only safe downcasts are on handles that the module itself created (e.g., with `Box::new` or `Arc::new`).

---

## Safety Notes

### Hash Collisions

If two different types produce the same hash, then a downcast from one to the other would be accepted (even with debug checks, because the size and alignment might coincidentally match) and lead to undefined behaviour.

**Solution:** Use a strong hash function (e.g., 64‑bit FNV or xxhash) and ensure the string includes the full path (e.g., `"my_crate::Device"`) to minimise collision risk. The probability is negligible for practical purposes.

### Raw Pointer Conversions

The `to_raw` and `from_raw` methods are `unsafe`:

- `to_raw` returns the raw address of the handle.
- `from_raw` **increments** the reference count for `Arc`/`Rc` variants (to account for the new handle), and for `Box` it simply reconstructs the handle.

You must ensure that the raw address is valid and that the handle has not been dropped.  
In shared‑ownership scenarios, the refcount must be managed correctly – otherwise you may leak memory or double‑free.

### Reference Count Manipulation

The `rc_*` methods (e.g., `rc_load`, `rc_store`, `rc_add`) are `unsafe` because they allow direct manipulation of the refcount.  
Misuse can easily break the memory safety guarantees. Only use them if you are absolutely certain of what you are doing (e.g., when integrating with a foreign reference‑counting system).

### Downcasting Invariants

When you call `downcast<T>`, the crate assumes that:

- The hash `_T` matches the hash of `T`.
- The alignment of the stored type equals `align_of::<T>()`.
- The size matches.

In debug builds, these are asserted. In release builds, they are **not** checked – the compiler trusts the hash.  
Therefore, if the hash is wrong, you get undefined behaviour.

---

## Advanced Topics

### Using `to_raw` / `from_raw` Across an ABI

You can convert a handle to a raw `usize` and pass it through C‑style FFI, then reconstruct it on the other side:

```rust
// Sender
let handle = Arc::new(MyType);
let raw = unsafe { handle.to_raw() };
// pass `raw` across FFI as `usize`

// Receiver
let handle = unsafe { Arc::<{ hash!("MyType") }>::from_raw(raw) };
// Now you have a new handle with an incremented refcount.
```

This is useful for integrating with languages that only understand integers.

### Custom Allocators

The crate uses the global allocator (`alloc::alloc`). If you need a custom allocator, you can modify the source or submit a feature request.

---

## Conclusion

`nopaque` gives you the best of both worlds: **opaque** handles that hide implementation details, and **type‑safe** interactions that let the compiler catch mismatches.  
Whether you are building kernel modules, plugin systems, or any dynamic library interface, `nopaque` can make your API safer and more ergonomic.

Start using it today – just define your `hash!` macro, choose the appropriate pointer kind, and enjoy compile‑time‑checked opaque pointers across ABI boundaries.
