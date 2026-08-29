# Floe performance contract

Phase 21A measures existing Floe code paths with a reproducible, opt-in release
harness. It is a regression gate, not a claim that every filesystem, desktop,
or machine will match this host. Every fixture is created below one
`tempfile` root and removed after the run; the harness never reads the user's
home directory, Trash, mounts, settings, caches, or data.

## Reproduce

Build and run the serial release harness from the repository root:

```bash
cargo test -p floe-app phase_21a_performance --release -- --ignored --nocapture --test-threads=1
```

The ordinary debug test suite excludes this test because creating and deleting
100,000 real entries is intentionally expensive. Each measured operation emits
a `PHASE21A_RESULT` line with its workload, elapsed time, budget, and status.
The final line reports `entries=100000 temporary_root=true status=pass`.

## Recorded host and method

The current evidence was recorded on 2026-08-29 with:

- Linux 7.2.0-1-cachyos, x86-64, Wayland session;
- AMD Ryzen 9 9950X3D, eight CPUs online to this session;
- Rust 1.98.0, Cargo 1.98.0, LLVM 22.1.8;
- `/tmp` on tmpfs; and
- an optimized Cargo release test process, one test thread.

Elapsed measurements use `std::time::Instant`. They include the production API
call but exclude fixture creation unless the row explicitly says fixture. The
Linux process peak memory procedure reads `VmHWM` from `/proc/self/status` after
all workloads. It therefore captures the whole test process high-water mark,
including the retained 100,000-entry model and GLib/image-library state; it is
not an allocation profile for an individual operation.

## Workloads, budgets, and current evidence

Budgets are deliberately conservative release regression ceilings. They are
not product promises for cold disks, encrypted storage, network filesystems, or
slower machines.

| Workload | Exact bounded fixture | Budget | Current |
| --- | --- | ---: | ---: |
| Fixture construction | 100,000 real empty regular files | 120,000 ms | 124 ms |
| Directory enumeration | 100,000 entries, production no-recursion enumerator and default name sort | 30,000 ms | 72 ms |
| Metadata sort | 100,000 loaded entries by size, descending | 5,000 ms | 1 ms |
| Quick filter | 100,000 loaded names, case-insensitive text, 100 matches | 5,000 ms | 1 ms |
| Filename search | 100,000 real entries, current folder, 100 matches | 30,000 ms | 67 ms |
| Thumbnail decode | 32 generated 512x512 PNGs to 192-pixel bounded output | 15,000 ms | 87 ms |
| Content search | 512 UTF-8 files, 16.7 MB total, one match each | 15,000 ms | 3 ms |
| Copy | One 32 MiB regular file, fail-if-exists, production copy engine | 20,000 ms | 5 ms |
| Checksum | SHA-256 over copied 32 MiB regular file | 20,000 ms | 49 ms |
| Duplicate scan | 128 64-KiB files in 64 exact duplicate pairs | 30,000 ms | 11 ms |
| Integrity save | Saved SHA-256 fingerprint of 32 MiB file | 20,000 ms | 49 ms |
| Integrity verify | Identity-bound fingerprint verification of same file | 20,000 ms | 49 ms |
| Advanced metadata | Word-count index and sort over 512 text files | 15,000 ms | 6 ms |

The observed process peak memory was **119,876 KiB** (about 117 MiB). The
release harness completed with every workload under budget.

## Measured optimization

The advanced metadata index previously counted words with
`split_whitespace()` and then made a second complete pass with `lines()`. A
CPU-bound comparison used the same 16,652,288-byte ASCII text buffer for eight
iterations of the Baseline and Current implementations:

| Text-fact implementation | Time |
| --- | ---: |
| Baseline: separate word and line passes | 64,108 us |
| Current: one ASCII pass | 32,212 us |

That is a 49.8% elapsed-time reduction on the recorded host. The optimization
is bounded to ASCII text, which is the common source/code/document fast path.
Unicode text retains the prior Unicode-aware implementation.

Correctness is enforced separately from the timing assertion. The regression
compares the Current result with Rust's existing `split_whitespace()` and
`lines()` semantics for empty text, trailing newlines, CRLF, bare carriage
returns, consecutive blank lines, every ASCII whitespace form (including
vertical tab), and Unicode whitespace/text. The file index still opens sources
no-follow, enforces its existing size and cumulative-read budgets, validates
source identity, preserves exact paths, and remains cancellable off GTK.

## Limitations

- `/tmp` is tmpfs on the recorded host, so filesystem latency and throughput
  are warm-memory results rather than cold SSD or rotational-disk evidence.
- CPU frequency, page-cache state, kernel load, filesystem, and release
  toolchain affect elapsed results. Re-record evidence when those materially
  change; do not silently relax budgets to hide a regression.
- `VmHWM` is Linux-specific and reports only the whole process peak memory. It
  does not attribute allocations to a workload or include compositor memory.
- The deterministic harness does not render or scroll GTK. Native 100,000-entry
  launch/liveness is a separate Wayland gate. On this host the disposable app
  loaded the folder, answered D-Bus Ping, quit normally with exit 0, and peaked
  at 114,296 KiB; the critical-free sub-gate is explicitly abandoned because
  the host AT-SPI socket refused connections even with a private bus address.
  Dogtail/pyatspi is also unavailable, so no semantic E2E claim is made.
- This phase does not benchmark startup, remote/network filesystems, MTP,
  compositor-specific integration, vaults, encryption, or other unimplemented
  capabilities.
