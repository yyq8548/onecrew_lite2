# Unsafe Proof and Safe Abstractions

Use this pattern for an unsafe block, unsafe function or trait, raw pointer,
FFI call, or safe wrapper. Official authority: the Rust Reference on the
[unsafe keyword](https://doc.rust-lang.org/reference/unsafe-keyword.html),
[unsafety](https://doc.rust-lang.org/reference/unsafety.html), and
[undefined behavior](https://doc.rust-lang.org/reference/behavior-considered-undefined.html),
the Rust Book on [unsafe Rust](https://doc.rust-lang.org/book/ch20-01-unsafe-rust.html),
the standard library contract for
[`slice::from_raw_parts`](https://doc.rust-lang.org/stable/core/slice/fn.from_raw_parts.html),
and the Edition Guide on
[`unsafe_op_in_unsafe_fn`](https://doc.rust-lang.org/edition-guide/rust-2024/unsafe-op-in-unsafe-fn.html).

## Critical Rules

`unsafe` declares a proof obligation; it does not disable Rust's other checks
or prove soundness. Keep the block around only the unsafe operations, state
every relied-on fact in a nearby `SAFETY` comment, validate dynamic conditions
before entering, and contain the result behind a safe abstraction that
maintains aliasing, initialization, bounds, lifetime, ownership, and cleanup.

```rust
// ❌ BAD: broad unsafe region, implicit lifetime, and borrowed raw storage escapes.
unsafe fn read_native() -> &'static [u8] {
    let (ptr, len) = native_read();
    std::slice::from_raw_parts(ptr, len)
}

// ✅ GOOD: the wrapper validates, proves one operation, copies, and cleans up.
pub fn read_native() -> Result<Vec<u8>, NativeError> {
    let buffer = NativeBuffer::read()?;
    if buffer.len() > isize::MAX as usize {
        return Err(NativeError::BufferTooLarge);
    }
    // SAFETY: NativeBuffer guarantees a non-null, aligned pointer to `len`
    // initialized u8 values in one allocation; the checked size cannot wrap.
    // Its shared borrow permits no mutation, and Drop keeps the allocation
    // live until after this slice has been copied.
    Ok(unsafe {
        std::slice::from_raw_parts(buffer.as_ptr(), buffer.len())
    }.to_vec())
}
```

An `unsafe fn` still uses explicit unsafe blocks around its unsafe operations.
Document the caller's complete `# Safety` contract; do not let the function's
keyword blur which operations and facts require review.

## Quick Reference

| Proof area | Questions to answer |
|---|---|
| Pointer and bounds | Non-null when required, aligned, in-bounds, correct element count |
| Initialization | Bytes/values are initialized for the type being read |
| Aliasing and mutation | Shared and mutable references obey exclusivity for their lifetimes |
| Lifetime | Allocation and pointees outlive every reference derived from them |
| Ownership and cleanup | Exactly one owner frees/releases resources on every path |
| FFI status | Return status, buffer lengths, sentinels, and platform contracts are validated |
| Concurrency | Thread-safety claims and any `Send`/`Sync` implementation are proved |
| Safe surface | Safe callers cannot construct inputs that violate the invariant |

## Common Patterns

### Narrow proof adjacent to the operation

Put `SAFETY` immediately above the block or impl. Name the source of each fact:
validated length, owning handle, OS/API contract, lock, type invariant, or
construction rule. Avoid vague comments such as "pointer is valid" or
"required by FFI"; they restate the desired conclusion.

### Safe wrapper owns the full lifecycle

Validate inputs, call the unsafe primitive, convert to owned or lifetime-bound
safe data, and release native resources in one abstraction. A raw pointer or
borrowed slice must not outlive the allocation it depends on. Use guards when
multiple error paths need the same cleanup.

### Domain facts feed the reusable proof

The owner of an OS, cryptographic, protocol, or device contract supplies its
status, buffer, aliasing, lifetime, and cleanup facts. Unsafe review turns
those facts into one local proof and a safe abstraction; it must not invent or
silently change the underlying domain contract.

### No invented complete UB checklist

The Reference's undefined-behavior list is not exhaustive. Audit the concrete
unsafe operation and current official contract, then test safe observable
behavior. A passing test, Miri run, or `SAFETY` comment alone is not proof.

## Checklist

- [ ] Every unsafe operation has an explicit, narrow block.
- [ ] Every block or unsafe impl has a nearby complete `SAFETY` proof.
- [ ] Dynamic pointer, length, status, and ownership facts are checked first.
- [ ] The safe wrapper prevents callers from violating the invariant.
- [ ] Cleanup, error, early-return, and panic paths preserve one owner.
- [ ] Unsafe functions document every caller obligation under `# Safety`.
- [ ] OS, cryptographic, protocol, or device facts remain with their domain owner.
- [ ] Evidence is not overstated as soundness proof.
