# Static Linking of Visual C++ Runtime

## Problem

By default, Rust binaries compiled with the MSVC toolchain on Windows dynamically link to the Visual C++ runtime library (`VCRUNTIME140.dll`). This means users need to have the **Microsoft Visual C++ Redistributable 2015-2022** installed on their system for the binary to run.

This creates a dependency that:
- Requires users to install additional software
- Complicates distribution via package managers (Scoop, WinGet, etc.)
- Can cause "DLL not found" errors on fresh Windows installations

## Solution: Hybrid Linking

We use the `static_vcruntime` crate which implements **hybrid linking**:

- ✅ **Statically links** the Visual C++ runtime (`VCRUNTIME140.dll`)
- ✅ **Dynamically links** the Universal C Runtime (UCRT) which ships with Windows 10/11

This approach gives us the best of both worlds:
- No vcredist2022 dependency required
- Still benefits from Windows Update security patches for UCRT
- Minimal binary size increase

## Implementation

### 1. Added `static_vcruntime` to `Cargo.toml`

```toml
[target.'cfg(windows)'.build-dependencies]
static_vcruntime = "2.0"
```

### 2. Created `build.rs`

```rust
fn main() {
    // Statically link the Visual C++ runtime on Windows to avoid requiring
    // vcredist2022 installation. This uses "hybrid linking" which statically
    // links vcruntime while dynamically linking the Universal C Runtime (ucrt)
    // that ships with Windows.
    #[cfg(windows)]
    static_vcruntime::metabuild();
}
```

## Verification

You can verify the binary's dependencies using:

```bash
cargo install cargo-binutils
cargo readobj --release --bin winstow -- --coff-imports
```

The output should show:
- ❌ **NO** `VCRUNTIME140.dll` import
- ✅ Only `api-ms-win-crt-*.dll` imports (UCRT - ships with Windows)
- ✅ Standard Windows system DLLs (`kernel32.dll`, `ntdll.dll`, etc.)

## Benefits

1. **No vcredist2022 dependency** - Users can run winstow without installing additional software
2. **Simpler package manager manifests** - No need to declare vcredist2022 as a dependency
3. **Better user experience** - Works out of the box on Windows 10/11
4. **Still gets security updates** - UCRT is updated via Windows Update
5. **Minimal size increase** - Binary remains small (~572 KB)

## References

- [static_vcruntime crate](https://crates.io/crates/static_vcruntime)
- [Rust Users Forum: Windows binaries VCRUNTIME140.DLL not found](https://users.rust-lang.org/t/windows-binaries-vcruntime140-dll-not-found-unless-crt-static/94517)
- [Microsoft: C++ binary compatibility 2015-2022](https://learn.microsoft.com/en-us/cpp/porting/binary-compat-2015-2017)

