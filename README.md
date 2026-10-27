# ITool

A native Windows app and CLI for copying accessible iPhone photos, videos and Live Photo resources over USB. Original files are copied without conversion. Optional compatible copies are additional files.

**Development build:** the guided UI and roadmap features below are implemented in source. Compilation is distinct from tests and physical-device results. This work does not publish a new GitHub release or replace the existing 0.2.0 release.

## Screenshots

Actual screenshots of the Windows development build in demo mode. The library uses synthetic sample photos; export counts and rates are simulated. Select an image to view it at full size.

### 1. Connect

Connect your iPhone, unlock it and confirm Trust before scanning.

| Light mode | Dark mode |
| --- | --- |
| ![1. Connect in light mode](docs/screenshots/connect-light.png) | ![1. Connect in dark mode](docs/screenshots/connect-dark.png) |

### 2. Choose media

Browse the library, filter and sort media, and review your selection.

| Light mode | Dark mode |
| --- | --- |
| ![2. Choose media in light mode](docs/screenshots/gallery-light.png) | ![2. Choose media in dark mode](docs/screenshots/gallery-dark.png) |

### 3. Export

Name your export folder, choose how files are organized, and preview the destination.

| Light mode | Dark mode |
| --- | --- |
| ![3. Export in light mode](docs/screenshots/export-light.png) | ![3. Export in dark mode](docs/screenshots/export-dark.png) |

### 4. Results

Review the export summary and transfer details. This screenshot shows simulated demo results.

| Light mode | Dark mode |
| --- | --- |
| ![4. Results in light mode](docs/screenshots/results-light.png) | ![4. Results in dark mode](docs/screenshots/results-dark.png) |

## Guided workflow

1. **Connect:** connect one iPhone using a data-capable cable, unlock it, confirm Trust, then choose **Scan iPhone**. Setup help is available on this screen.
2. **Choose media:** use the grid or details list, search, media-type/month filters, available Albums/Favorites, and Sort/View options. Click, Shift-click, drag or select a group. Select/invert shown items and review the selection basket. Selection survives filters. Ctrl+A selects shown items; Escape clears selection outside text inputs.
3. **Export:** choose an existing save location, edit the new folder name, and preview its contents. Choose flat, day, month, media-type or DCIM source folders. Existing output folders and duplicate relative names block export. Change the name or choose Source folders to resolve conflicts. Gallery grouping and output layout are independent.
4. **Results:** see progress, cancel, inspect copied/skipped/converted resource counts and optional-step notes, and open the output folder. Cancellation preserves completed originals.

Neutral light/dark palettes use a blue accent. Appearance is saved in `%APPDATA%\ITool\preferences.json`. The desktop executable suppresses its console window; the separate CLI retains its console.

## Only copy new files

Enable **Only copy new files** on Export. Local history lives in `.itool-history` inside the save location. Reuse that parent for future batches, with a new batch folder name each time.

History is separated by a SHA-256 hash of the device identifier. A skip requires matching device/source path, size, filesystem creation/modification dates, and a saved original that still matches its stored SHA-256. Missing, modified or unknown saved copies are copied again. If one selected Live Photo resource needs copying, both are copied into the new batch. Requested conversion/timestamp options also affect skip eligibility.

This is metadata-based incremental detection with local-copy verification, not a new source digest. Source content changed without changing those metadata values cannot be detected. Moving/deleting history loses skip knowledge; older exports without history are copied again. This does not resume partial files or synchronize deletions. History contains private paths; keep it private.

## Additional JPEG and MP4 copies

Under **More options**, request JPEG copies of HEIC/HEIF and/or H.264/AAC MP4 copies of SDR MOV/M4V. Originals remain intact. Copies use names such as `IMG_1023.HEIC.jpg` and `IMG_1023.MOV.mp4` to distinguish them from originals.

Local **FFmpeg** is required; MP4 conversion also requires **ffprobe**. The app looks in a `tools` folder beside the executable, then on PATH. Settings shows availability. ITool does not download these tools or redistribute them in its portable package. See [FFmpeg downloads](https://ffmpeg.org/download.html).

Codec support depends on the installed build. Decode/encode failures become notes while preserving originals. HDR videos and videos with missing color-transfer metadata are skipped for conversion. Compatible copies can change compression, color and metadata; they are not archival substitutes. Converted pairs are not guaranteed to reimport as Apple Live Photos.

## Capture timestamps

Enable **Use embedded capture dates for saved file timestamps**. ITool reads EXIF DateTimeOriginal/OffsetTimeOriginal, including supported HEIF images, and can use ffprobe for video/container creation metadata. On Windows it sets destination creation/modification times without changing file contents.

A date without an offset uses the PC timezone and produces a note. Missing, ambiguous or unreadable dates leave destination timestamps unchanged and are reported. Video container creation time is not necessarily camera capture time. Gallery dates and day/month folders still use device filesystem dates, independently of this option.

## Albums and Favorites

The scan attempts a bounded read-only copy of `/PhotoData/Photos.sqlite` and its WAL when exposed by iOS. SQLite runs only on a private temporary local snapshot, discarded afterward. The reader supports `ZASSET`/`ZGENERICASSET`, Favorite flags and discoverable album join tables. Snapshots whose source metadata changes during copying or which fail consistency checks are rejected.

iOS can deny access and its private schema can change. Unsupported filters are disabled and availability is explained. Only metadata mapped to accessible DCIM originals is shown. DCIM folders are not Photos albums; cloud-only originals are not fetched. No jailbreak, database writes to the device or authentication bypass is used. Physical-phone access and schema compatibility remain unverified.

## Folder and integrity behavior

- Each batch creates the exact new folder shown on Export. Original names and grouped Live Photo resources are retained.
- Use an **NTFS** save location. Final publication uses hard links to avoid overwriting. exFAT/FAT destinations are not supported by this implementation.
- `completed.json` checkpoints originals copied in this batch. Unfinished files retain `.partial` and are not treated as complete.
- `transfer.json` records original evidence, skipped paths, compatible copies, options and notes. An all-skipped batch may contain reports without new originals.
- Original verification compares received-stream SHA-256 with a destination reread and checks source size/mtime. It is not an independent device-side digest.
- A different connected iPhone from the scanned device blocks desktop export until rescanning.
- Device access initiated by this app is read-only. Reports/history contain private paths and should remain private.
- The transfer default is **4 MiB**. USB 3 performance and improvement over smaller buffers require hardware benchmarks; no 10 Gbit/s throughput claim is made.

## Requirements

Windows 10/11 x64, Apple Mobile Device Support through Apple Devices or iTunes, one trusted USB-connected iPhone and an NTFS save location. Originals must be available through the device media service; thumbnails do not prove original availability. Optional FFmpeg/ffprobe enables compatible copies and video timestamps. EXIF parsing is built in.

Start `itool-desktop.exe` for the guided interface or `itool.exe` for CLI workflows.

## CLI examples

```powershell
.\itool.exe devices
.\itool.exe list /DCIM
.\itool.exe find IMG_3119.HEIC IMG_3119.MOV
.\itool.exe library
.\itool.exe library --favorites
.\itool.exe library --album "Travel"

# Destination is a NEW directory inside an existing parent
.\itool.exe export --dest D:\Photos\Batch-1 /DCIM/123APPLE/IMG_3119.HEIC /DCIM/123APPLE/IMG_3119.MOV
.\itool.exe export --dest D:\Photos\Batch-2 --new-only --capture-timestamps --jpeg-copies --mp4-copies /DCIM/123APPLE/IMG_3119.HEIC /DCIM/123APPLE/IMG_3119.MOV

# Override transfer chunks: 64–8192 KiB, default 4096
.\itool.exe export --dest D:\Photos\Batch-3 --chunk-kib 4096 /DCIM/123APPLE/IMG_3119.MOV
.\itool.exe verify --baseline .\manifest.json --dir D:\Photos\Batch-1
```

Include both still and clip paths for Live Photos through the exact-path CLI. Desktop selection expands both automatically. The existing baseline verifier expects flat original files.

## Build and sample screens

Rust edition 2024. Build optimized Windows binaries:

```powershell
cargo build --release --features desktop --bins
```

Device-free sample mode simulates the guided flow without connecting to an iPhone or copying files. Opening synthetic output paths is disabled:

```powershell
.\target\release\itool-desktop.exe --demo
.\target\release\itool-desktop.exe --demo --demo-connect
.\target\release\itool-desktop.exe --demo --demo-review
.\target\release\itool-desktop.exe --demo --demo-results
# Add --demo-light or --demo-dark to choose the theme, or --demo-list for details view.
```

Tests, when authorized, run separately with `cargo test --all-targets --features desktop`. Builds and sample screens do not establish physical-device behavior.

## Roadmap status

- [x] Organized exports and editable named-batch folders (implemented in source).
- [x] Incremental exports with checked local history (implemented; device validation pending).
- [x] Optional compatible copies (implemented; FFmpeg/codecs required; media validation pending).
- [x] Capture timestamps (implemented with fallbacks; media/NTFS validation pending).
- [x] Conditional Albums/Favorites (implemented; physical-phone access/schema support unverified).
- [ ] Device, interruption and long-batch validation of this development build.
- [ ] Measured USB 3 benchmarks.
- [ ] Reliable cloud-only original retrieval and broader Photos-schema coverage.
- [ ] Byte-offset resume for partial files.
- [ ] Verified HDR-to-SDR tone mapping.

Historical evidence: an earlier CLI copied seven resources totaling 185,252,866 bytes in 11.178 seconds, matching a 3uTools sample byte-for-byte. The 3uTools batch was not timed. This does not validate the new UI or roadmap features.

## References and license

EXIF parsing uses [kamadak-exif](https://docs.rs/kamadak-exif/latest/exif/). Photos schema discovery is informed by [iLEAPP's Photos metadata parser](https://github.com/abrignoni/iLEAPP/blob/main/scripts/artifacts/photosMetadata.py); this does not establish AFC database accessibility on a particular iPhone.

MIT licensed. Apple and iPhone are trademarks of Apple Inc. ITool is independent.
