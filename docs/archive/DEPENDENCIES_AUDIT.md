# Dependencies Audit - Legacy Audio Provisioner

**Date**: March 6, 2026
**Status**: ⚠️ INCOMPLETE - Critical deps missing

---

## Executive Summary

The integration tests I created **simulate functionality** without verifying the actual external dependencies required by the specification. This is **critically flawed** because:

1. ❌ Tests pass without actual software installed
2. ❌ Production code will fail at runtime
3. ❌ Real USB operations untested

---

## Rust Crate Dependencies

### Currently in `Cargo.toml` ✅
```toml
lazy_static = "1.4"      # Regex compilation
regex = "1.10"           # Filename sanitization
chrono = "0.4"           # Timestamps
hex = "0.4"              # Hex encoding
sha2 = "0.10"            # SHA256 checksums
anyhow = "1.0"           # Error handling
log = "0.4"              # Logging
env_logger = "0.11"      # Log configuration
walkdir = "2.4"          # File traversal
clap = "4.4"             # CLI parsing
serde = "1.0"            # Serialization
serde_json = "1.0"       # JSON handling
```

### Missing from `Cargo.toml` ❌

| Crate | Version | Use Case | Spec Requirement | Priority |
|-------|---------|----------|------------------|----------|
| `nix` | >=0.27 | Linux system calls (statvfs, mount detection) | R-04, R-05 | **HIGH** |
| `sysinfo` | >=0.30 | Cross-platform API for drives/mounts | R-04 | **HIGH** |
| `metaflac` | >=0.2 | ID3v2 tag reading/stripping | R-11 (R-06) | **HIGH** |
| `id3` | >=0.7 | Alternative ID3 tag library | R-11 | MEDIUM |
| `ffmpeg-sys` | >=5.0 | FFmpeg bindings (requires ffmpeg installed) | R-10, R-18 | **HIGH** |
| `ac-ffmpeg` | >=0.2 | High-level FFmpeg wrapper | R-10, R-18 | MEDIUM |
| `tempfile` | >=3.8 | ✅ In [dev-dependencies] for tests | Tests | LOW |

---

## System Executable Dependencies

These must be **installed on the system** and available in `$PATH`:

### FFmpeg (Audio Processing) ❌
**Status**: Not verified in tests
**Specification**: R-10, R-18 (Audio normalization)
**What we need**:
```bash
ffmpeg -i input.mp3 -acodec libmp3lame -ab 192k output.mp3
```
**Test Status**: SIMULATED ONLY
**Install**:
```bash
# Linux (Ubuntu/Debian)
sudo apt install ffmpeg

# macOS
brew install ffmpeg

# Verify
ffmpeg -version
```

### rsync or coreutils (Backup) ⚠️
**Status**: Partially implemented with `std::fs`
**Specification**: R-05 ("rsync o cp -p para el backup")
**What we need**:
```bash
rsync -av --progress source/ dest/
# or
cp -p source/ dest/
```
**Test Status**: Using `std::fs::copy()` instead (NOT rsync)
**Install**:
```bash
# rsync
sudo apt install rsync  # Linux
brew install rsync      # macOS

# cp is built-in
cp --version
```

### eject/udisksctl (Safe USB Eject) ⚠️
**Status**: Implemented as stubs
**Specification**: R-T5 (Verification and eject)
**What we need**:
```bash
# Linux - Option 1: udisksctl
udisksctl power-off -b /dev/sdb1

# Linux - Option 2: eject
eject /media/dev/6A08-0A02

# macOS
diskutil unmountDisk /Volumes/USB_NAME
```
**Test Status**: Implementation exists but not tested
**Install**:
```bash
# udisksctl comes with udisks2
sudo apt install udisks2      # Linux
brew install udisks2          # macOS (limited)

# eject
sudo apt install eject        # Linux
eject -V                       # Check
```

---

## System Library Dependencies

### statvfs() / statfs() ✅
**Status**: Part of POSIX/C stdlib
**Specification**: R-04, R-05 (check available disk space)
**What it does**: Get filesystem stats (free space, inode count)
**Available**: Linux (statfs), macOS (statfs)
**Rust Integration**: `nix::sys::statvfs::statvfs()`

**Currently Used**: ❌ NO - using dummy checks only
**Needed In Code**:
```rust
use nix::sys::statvfs::statvfs;

let stat = statvfs("/media/usb")?;
let free_bytes = stat.blocks_available() * stat.block_size();
```

---

## Dependency Matrix

| Component | Rust Crate | System Binary | Current Status | Test Validation |
|-----------|-----------|---------------|----|---|
| **R-03: Name Sanitization** | regex ✅ | — | ✅ DONE | ✅ YES |
| **R-04: Hardware Detection** | nix ❌ / sysinfo ❌ | lsblk, blkid ⚠️ | ⏳ PARTIAL | ❌ SIMULATED |
| **R-05: Backup** | walkdir ✅ | rsync ⚠️ / cp ⚠️ | ⚠️ PARTIAL | ❌ SIMULATED |
| **R-06: Normalization** | metaflac ❌ | ffmpeg ❌ | ⏳ STUB | ❌ NO |
| **R-07: Distribution** | — ✅ | — | ✅ DONE | ✅ YES |
| **R-T5: Verification** | — ✅ | eject ⚠️ / udisksctl ⚠️ | ⚠️ PARTIAL | ⚠️ LIMITED |
| **R-16: Checkpoints** | serde_json ✅ | — | ✅ DONE | ✅ YES |
| **R-17: Recovery** | — ✅ | — | ✅ DONE | ✅ YES |

**Legend**: ✅ = Working | ⚠️ = Partial | ❌ = Missing | ⏳ = Stub

---

## Code TODOs Currently Not Addressed

### [src/hardware.rs:84](src/hardware.rs#L84)
```rust
// TODO: Obtener información del sistema de archivos usando statvfs
```
**Needed**: `nix` crate + `statvfs()` call

### [src/backup.rs:108](src/backup.rs#L108)
```rust
// TODO: Verificar espacio usando statvfs o similar
```
**Needed**: `nix` crate integration

### [src/normalizer.rs:80](src/normalizer.rs#L80)
```rust
// TODO: Usar metaflac para leer audio headers
```
**Needed**: `metaflac` crate + FFmpeg binary

---

## Reproducible Install Instructions

### Debian/Ubuntu
```bash
# Required Rust crates
cargo add nix --features default
cargo add metaflac

# System packages
sudo apt update
sudo apt install -y ffmpeg rsync eject udisks2

# Verify installations
ffmpeg -version
rsync --version
eject -V
udisksctl --version
```

### macOS
```bash
# Required Rust crates
cargo add nix --features default
cargo add metaflac

# System packages (via Homebrew)
brew install ffmpeg rsync util-linux udisks2

# Verify
ffmpeg -version
rsync --version
eject -V
diskutil info /
```

### Fedora/RHEL
```bash
# System packages
sudo dnf install -y ffmpeg rsync eject udisks2

# Rust crates same as above
cargo add nix --features default
cargo add metaflac
```

---

## Problem: Integration Tests Don't Validate Real Dependencies

### ❌ What tests currently do:
```rust
fn test_filename_sanitization() {
    // Tests the sanitize_filename() function - OK ✅
}

fn test_end_to_end_provisioning_workflow() {
    // Uses TempDir instead of real USB ❌
    // Simulates hardware detection ❌
    // Doesn't call ffmpeg ❌
    // Doesn't check statvfs ❌
}
```

### ✅ What tests SHOULD do:
```rust
#[test]
#[cfg(feature = "integration")]
fn test_binary_ffmpeg_installed() {
    let output = Command::new("ffmpeg")
        .arg("-version")
        .output()
        .expect("ffmpeg not found in PATH");
    assert!(output.status.success());
}

#[test]
fn test_nix_statvfs() {
    let stat = nix::sys::statvfs::statvfs("/tmp")
        .expect("statvfs failed");
    assert!(stat.blocks_available() > 0);
}
```

---

## Action Items - Phase 3

### Immediate (Required for functionality):
- [ ] Add `nix` crate to Cargo.toml
- [ ] Add `metaflac` crate to Cargo.toml
- [ ] Implement `test_ffmpeg_installed()` (system binary check)
- [ ] Implement real `statvfs()` in hardware.rs:84
- [ ] Implement real disk space check in backup.rs:108

### Medium Priority (for robustness):
- [ ] Add `sysinfo` as alternative to `nix` for cross-platform
- [ ] Create dependency verification script
- [ ] Add CI/CD checks for system dependencies
- [ ] Create install script (install-deps.sh)

### Nice to Have (polish):
- [ ] Add feature flags: `[features] full=["ffmpeg", "metaflac"]`
- [ ] Document system requirements in README
- [ ] Add pre-flight check command: `legacy-audio-provisioner --check-deps`

---

## Verification Checklist

Run this to validate your system has all dependencies:

```bash
#!/bin/bash
echo "=== Rust Crate Dependencies ==="
grep -A 20 '^\[dependencies\]' Cargo.toml

echo -e "\n=== System Binaries ==="
ffmpeg -version >/dev/null 2>&1 && echo "✅ ffmpeg" || echo "❌ ffmpeg"
rsync --version >/dev/null 2>&1 && echo "✅ rsync" || echo "❌ rsync"
eject -V >/dev/null 2>&1 && echo "✅ eject" || echo "❌ eject"
udisksctl --version >/dev/null 2>&1 && echo "✅ udisksctl" || echo "❌ udisksctl"

echo -e "\n=== System Library Support ==="
cargo build --features test 2>&1 | grep -E "(nix|statvfs)" && echo "✅ statvfs available" || echo "⚠️ Check nix crate"
```

---

## Conclusion

**The integration tests PASS but don't prove the system works.**

They validate:
- ✅ Business logic (filename sanitization, distribution)
- ✅ Data structures (checkpoints, volumes)

They DON'T validate:
- ❌ External dependencies installed
- ❌ System calls work (statvfs, mkdir, copy)
- ❌ FFmpeg integration
- ❌ Real USB operations
- ❌ Cross-platform compatibility

This is exactly what the user pointed out: I was "passing these by" (pasando de largo) the real requirements.

**Next step**: Add proper dependency checks to tests and implement the missing Cargo.toml entries.
