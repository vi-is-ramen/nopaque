# nopaque

**Type‑Safe Opaque Pointers for ABI Boundaries**

`nopaque` provides smart pointers (`Box`, `Rc`, `Arc`) that are **opaque** and **type‑guarded** by a compile‑time hash. They are designed to be safely passed across foreign function interfaces (e.g., C ABIs) where the concrete type must remain hidden.

## The Problem

When writing libraries that expose a C API, you often need to return opaque handles to internal data structures. However, Rust’s type system cannot enforce that a handle returned from one function isn’t accidentally used with another function expecting a different type. `nopaque` solves this by embedding a unique hash of the type name into the handle’s type itself, preventing mismatches at compile time.

## How It Works

- Each pointer is `#[repr(transparent)]` and stores a single `usize` address, making it ABI‑compatible with `void*` or `uintptr_t`.
- Metadata (size, alignment, drop function, reference count) is stored **just before** the actual data.
- The `_T` const parameter is a 64‑bit FNV‑1a hash of the type name (or a custom token). This hash is part of the type, so mixing handles of different types is impossible without an explicit cast.

## Perfomance

| Criterio | Comment |
|----------|---------|
| Allocation | O(1) with a bit of mathematical operations for Layout.
| Deallocation | O(1) with 2 direct memory accesses (to obtain memory block address and it's alignment which required by `alloc::alloc::deallocate`).
| Dereferencing | O(1) with no overhead, equals to simple `unsafe { ( X as *const Tx ).as_ref_unchecked() }`, but still safe.
| Copying & movement | 8-size, 8-align, so it's equal to `usize`.
| Memory | 24-32 bytes for metadata + 0x-4x of `Tx`'s native alignment.
| Cache | metadata and data are packed to each other, both correctly aligned, so compiler can optimize code which uses nopaque quite effectively.

> **NOTE:**
> Each value in table above are presented for each individual nopacue pointer.

## Provider / Consumer Pattern

The macros have two forms to support two roles:

| Role | Macro Form | Inner Type `Tx` | Dereference? |
|------|------------|-----------------|--------------|
| **Provider** (knows the type) | `Box!(&MyType)` | `MyType` | Yes (`Deref`) |
| **Consumer** (only opaque handle) | `Box!(MyType)` | `()` | No (cannot access) |

The hash is derived from the token `MyType` in both cases, so the types match across the boundary. The token **need not be a defined type** on the consumer side—it is only used for its name.

## Provided Types

| Type | Ownership | Reference Counting | Drop Behaviour |
|------|-----------|-------------------|----------------|
| `Box` | Unique | No | Implicit `drop_in_place` |
| `BoxDrop` | Unique | No | Explicit (`ExplicitDrop` trait or custom function) |
| `Rc` | Shared | Non‑atomic | Implicit `drop_in_place` |
| `RcDrop` | Shared | Non‑atomic | Explicit |
| `Arc` | Shared | Atomic (thread‑safe) | Implicit `drop_in_place` |
| `ArcDrop` | Shared | Atomic (thread‑safe) | Explicit |

All types are `Send`/`Sync` as appropriate.

## Examples

### Provider Side (defines the type)

```rust
use nopaque::{Box, boxed};

struct Person {
    name: String,
    age: u8,
}

// Create a type alias for the opaque handle.
type PersonHandle = Box!(&Person);

// Create a new instance.
let handle = PersonHandle::new(Person {
    name: "Alice".into(),
    age: 30,
});

// Dereference is allowed because we are the provider.
assert_eq!(handle.name, "Alice");
```

### Consumer Side (only sees an opaque handle)

```rust
use nopaque::Box;

// The token `Person` does not need to be defined!
type PersonHandle = Box!(Person);

// The consumer cannot access fields, only pass the handle around.
fn process(handle: PersonHandle) {
    // handle is just an opaque token; no fields are accessible.
    // Memory will be freed correctly when handle is dropped.
}
```

### Using Explicit Drop

```rust
use nopaque::{BoxDrop, ExplicitDrop};

struct Resource {
    id: u32,
}

impl ExplicitDrop for Resource {
    fn drop(&mut self) {
        println!("Releasing resource {}", self.id);
    }
}

let b = <boxed_drop!(&Resource)>::new(Resource { id: 42 });
// When `b` goes out of scope, `Resource::drop` is called.
```

## Features

- `default = ["std"]` – when disabled, the crate operates in `no_std` mode (requires `alloc`).

## License

This crate is dual‑licensed under the MIT license and the Apache License (Version 2.0). See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.
