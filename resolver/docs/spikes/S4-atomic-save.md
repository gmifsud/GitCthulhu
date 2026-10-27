---
type: spike-report
spike: S4
title: Atomic save and external-change guard
status: partial
date: 2026-10-27
platforms_done: [linux-ext4]
platforms_pending: [windows-ntfs, macos-apfs, wsl-9p]
result: go-on-linux
tags: [cthulhu-resolve, phase-0, safety]
---

# S4 — Atomic save and external-change guard

**Verdict: Go on Linux/ext4. Windows, macOS and WSL `/mnt/c` are still pending**, because this sandbox only has Linux.

## Design under test (`AtomicFileWriter`)

1. **Fingerprint check.** Compare length and SHA-256 against the load fingerprint. Timestamps are recorded but not trusted. A mismatch means *ExternallyModified* and nothing is written.
2. **Backup.** On the first save, the current file is copied to the backup directory.
3. **Temp file.** `.<name>.cthulhu-<random>.tmp` is written beside the target with `CreateNew`, then `Flush(flushToDisk: true)`.
4. **Mode bits.** Unix mode bits are copied from the target.
5. **Replace.** Windows uses `File.Replace` (keeps ACL and attributes). POSIX uses `rename(2)` via `File.Move(overwrite)`.
6. **Directory flush.** POSIX `fsync` of the directory makes the rename durable.

Symlink targets are refused, because symlink conflicts are handled at file level.

## Evidence (`AtomicFileWriterTests`, all passing on Linux)

| Case | Expected | Result |
|---|---|---|
| Fault before the temp write, after it, or before replace | Target bytes unchanged; no temp file left | ✅ |
| Fault after replace | Target is the whole new file | ✅ |
| External write between load and save | Save blocked; external content kept | ✅ |
| File appeared since load (expected absent) | Save blocked | ✅ |
| Executable bit | Preserved across the replace | ✅ |
| Symlink target | Refused; link target untouched | ✅ |
| BOM, CRLF and no final newline | Written byte-exact | ✅ |
| Orphan cleanup | Removes only `*.cthulhu-*.tmp` older than the threshold | ✅ |

## Pending (needs the CI matrix or your machine)

| Platform | What to verify |
|---|---|
| Windows NTFS | `File.Replace` keeps the ACL and the read-only attribute; behaviour while VS/Rider holds the file open (sharing violation means *Failed*, not a torn file); long paths over 260 characters |
| macOS APFS | Directory `fsync` through `libc` resolves to libSystem; NFD file names round-trip |
| WSL `/mnt/c` (9P/drvfs) | Whether `rename` over an open file is atomic there **[unverified]**. Until measured, the spec's rule applies: the Windows build handles Windows paths and the Linux build handles Linux paths |
| Disk full | `ENOSPC`/`ERROR_DISK_FULL` maps to `DiskFull`. Classification is implemented but untested here (needs a small loop-mounted or ramdisk volume in CI) |
