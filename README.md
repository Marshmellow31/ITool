# ITool

A Windows Rust prototype for copying accessible iPhone photos and videos to a PC without re-encoding. The first target is reliable original-file export over USB.

**Verified on 23 October 2026:** seven exported files matched the user's 3uTools baseline by size and SHA-256. The Rust run copied 185,252,866 bytes in 11.1781255 seconds (15.805 MiB/s), including destination reread verification. This is one local run; 3uTools has no measured baseline time, so a speed advantage has not been established.

The CLI uses `idevice` 0.1.68 and Apple's local USB service at `127.0.0.1:27015`. It does not load 3uTools DLLs. The GUI is being developed; Ghidra analysis is pending. See [analysis/FINDINGS.md](analysis/FINDINGS.md) for evidence and boundaries.

## Build and first connection

Requirements: Windows, Rust with a working native build toolchain, Apple's device support/service, and one trusted USB-connected iPhone. Unlock the phone and accept its Trust prompt when needed. For final output, use an NTFS destination: the current publication method requires hard links.

Run from this project's directory in PowerShell:

```powershell
cargo build --release
.\target\release\itool.exe devices
.\target\release\itool.exe list /DCIM
```

`devices` prints connection types without device identifiers. Device export requires exactly one USB device. `list` reads directory names; it does not copy media.

## Find and export

Find known filenames in DCIM and its immediate child directories:

```powershell
.\target\release\itool.exe find IMG_3119.HEIC IMG_3119.MOV
```

Use the paths printed by `find`. The following example uses paths verified on the test phone; other phones can have different directory names:

```powershell
.\target\release\itool.exe export --dest .\baseline\rust-example-01 --chunk-kib 1024 /DCIM/123APPLE/IMG_3119.HEIC /DCIM/123APPLE/IMG_3119.MOV
```

The destination must not already exist, and its parent must exist. The exporter retains source filenames. It rejects unsafe Windows names, duplicate output names and reserved report names. Buffer requests are configurable from 64 to 8192 KiB; 1024 KiB is the default.

Each file is read using AFC's read-only open mode into a new `.partial` file. The exporter hashes the transfer, checks source size and modification time again, rereads and hashes the destination, then publishes the completed filename without replacing an existing file. A failed transfer retains its partial file. `completed.json` records completed files; `transfer.json` records a successful batch and timing.

The prototype does not resume partial files or retry automatically. Choose a new destination for a retry. It copies supplied paths individually; include both the image and video path when preserving a Live Photo pair. It does not yet select these automatically.

## Compare with the recorded baseline

```powershell
.\target\release\itool.exe verify --baseline .\baseline\3utools-20261023-01-manifest.json --dir .\baseline\rust-1024k-01
```

Verification checks size and SHA-256 for each manifest entry. It normalizes the recorded 3uTools date-prefixed names to their `IMG_...` filenames. It rejects empty or malformed manifests and duplicate normalized names. Extra destination files are not part of this comparison.

## Current scope and limitations

- The seven-file comparison proves equality with that 3uTools export. There is no independent device-side digest establishing every resource's original provenance.
- No conversion occurs in the Rust copy path. Embedded metadata stays inside the copied bytes; local filesystem creation/capture timestamps are not explicitly restored.
- Cloud-only originals, edited-version choice, album mapping, automatic Live Photo grouping, reconnect/resume, and large-library reliability remain pending.
- Timing includes file copy and disk verification after the AFC session opens. It excludes initial device connection and media discovery.
- Reads have a stall deadline; this is not a complete reconnect/recovery implementation.
- GUI completion, Ghidra decompilation, packaging, and distribution remain pending. No Git repository, commit, push, remote or PR has been created for this project.

## Development

```powershell
cargo test
```

Keep device operations read-only and keep private media, local manifests, and device information out of public commits. Changes to transfer behavior should preserve the completed-file comparison and add focused regression coverage for failure handling.
