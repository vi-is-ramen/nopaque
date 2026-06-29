# `nopaque` – Type‑Safe Opaque Pointers

**`nopaque`** provides smart pointers (`Box`, `Arc`, `Rc`) that erase the concrete type of the stored value while retaining enough information to **safely downcast** later. It is designed for building **type‑safe dynamic APIs**, plugin systems, kernel module interfaces, or any FFI boundary where you need to pass opaque handles without losing compile‑time guarantees.

## Key Features

- **Type‑erased handles** – the pointer type carries only a compile‑time hash of the original type.
- **Safe downcasting** – recover `&T` or `&mut T` when you know the type.
- **Multiple ownership models**: unique (`Box`), reference‑counted (`Rc`/`Arc`), with or without custom drop support.
- **`no_std` compatible** – only depends on `alloc`.
- **Zero‑cost abstraction** – the hash is a const generic; downcasting is a simple pointer offset.

## Quick Example

```rust
use nopaque::Box;

const HASH: usize = 0x89df589df;

let handle = Box::<HASH>::new(42_u32);

// Downcast to the original type
let value: &u32 = handle.downcast();
assert_eq!(*value, 42);
```

```rust
use nopaque::Box;

macro_rules! hash { ($s:literal) => { ... } }

type BoxedMyType = Box![MyType];

let handle = BoxedMyType::new(42_u32);

// Downcast to the original type
let value: &u32 = handle.downcast();
assert_eq!(*value, 42);
```

### Kernel Module Interface (KMI)

In a kernel, you can expose constructors that return opaque handles:

```rust
// Kernel side
pub fn KeVtDeviceNew(name: KeStr) -> Arc!(Device) {
    Arc::new(crate::dev::Device::new(name))
}

// Module side (no Device definition needed)
extern "Rust" {
    fn KeVtDeviceNew(name: KeStr) -> Arc!(Device);
}
```

The module can call `KeVtDeviceNew` and use the handle safely – the compiler enforces that only functions accepting `Arc!(Device)` can be called with it. No `unsafe` downcasting is required unless you need to inspect the internals (which should be done by the kernel, not the module).

## Pointer Kinds

| Type        | Ref‑counted | Custom drop | `downcast_mut` |
|-------------|-------------|-------------|----------------|
| `Box`       | no          | no          | yes            |
| `BoxDrop`   | no          | yes         | yes            |
| `Rc`        | non‑atomic  | no          | **no**         |
| `RcDrop`    | non‑atomic  | yes         | **no**         |
| `Arc`       | atomic      | no          | **no**         |
| `ArcDrop`   | atomic      | yes         | **no**         |

> For shared ownership, mutable downcasting is intentionally omitted – use unique pointers or interior mutability.

---

## Safety & Requirements

- **Hash uniqueness** – you must provide a stable `hash!` macro that maps type names to `usize` constants. Collisions can lead to UB.
- **No reflection** – the crate does not store type IDs; it relies on the hash from the compile‑time string.
- **`unsafe` raw pointer conversions** – `to_raw`/`from_raw` are unsafe and must be used with care.

## More Information

- See [`USAGE.md`](USAGE.md) for detailed examples, use cases, and advanced topics.
- Read the [API documentation](https://docs.rs/nopaque) for all types and methods.

## License

This crate is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
