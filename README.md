# ITool — iPhone Original Media Exporter

An ultra-fast, 100% private, native Windows desktop application and CLI written in Rust for transferring original-quality photos, videos, and Live Photos from an iPhone to a Windows PC over USB.

[![GitHub Release](https://img.shields.io/github/v/release/Marshmellow31/ITool?color=166534&label=Release)](https://github.com/Marshmellow31/ITool/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%2010%20%2F%2011%20(x64)-informational)](#requirements)
[![Language](https://img.shields.io/badge/Language-Rust%202021-orange.svg)](https://www.rust-lang.org/)
[![Privacy](https://img.shields.io/badge/Privacy-100%25%20Offline%20%7C%20Zero%20Telemetry-success)](#why-itool-is-better-than-3utools--proprietary-chinese-apps)

---

## The Problem

Transferring photos and videos from an iPhone to a Windows PC has historically been a frustrating experience:
- **Windows File Explorer (MTP)** freezes, crashes midway on large transfers, frequently fails with *"The device is unreachable"*, and splits Live Photos into confusing disconnected files.
- **iCloud for Windows** compresses media, requires paid cloud storage, and is dependent on slow internet bandwidth.
- **Proprietary Chinese Utilities (such as 3uTools, iTools, Tenorshare)** are bloated, closed-source suites that notoriously collect device telemetry, install background services, inject ads, and risk privacy.

**ITool** solves this by providing a clean, standalone, high-performance native Windows app that communicates directly over USB with bit-for-bit cryptographic integrity.

---

## Key Features

### 📸 1. Bit-for-Bit Original Quality
- Transfers exact camera-original **HEIC, HEIF, JPEG, PNG, MOV, and MP4** files without re-encoding, compression, or downscaling.
- Preserves all original embedded EXIF metadata, HDR color spaces, GPS locations, and camera capture profiles.
- Validated with end-to-end byte-size and SHA-256 cryptographic hashes against verified baselines.

### 🔴 2. True Live Photo Preservation
- Intelligently detects and pairs Live Photo still images (`.HEIC` / `.JPG`) with their accompanying short video clips (`.MOV`).
- Exports both components together to ensure your Live Photo memories remain intact.

### ⚡ 3. High-Performance Virtualized Gallery
- Modern Windows 11 Fluent interface built in native Rust (`eframe` / `egui`).
- Smooth 60–144 FPS scrolling effortlessly handling **4,763+ media items** with zero UI freezing or memory bloat.
- Viewport-aware asynchronous thumbnail loading pipeline with LRU texture recycling.

### 🖱️ 4. Drag-to-Select Multi-Selection
- Intuitive **marquee / rubberband selection box**: drag across any photo grid area to select multiple photos and videos at once.
- Full modifier key support: hold `Shift` or `Ctrl` while dragging to add to your current selection.
- Automatic viewport edge scrolling while dragging across large collections.

### 📅 5. Chronological Months Overview
- Compact left sidebar grouping your media by month with exact item counts and human-readable storage sizes.
- Filter to a single month or view your entire library with a single click.

### 🔍 6. Instant Search, Filters & Sorting
- Filter instantly by **Photos**, **Videos**, or **Live Photos**.
- Real-time substring search by filename or folder.
- Grouping modes: **Month**, **Week**, **Date**, or continuous gallery.
- Sorting modes: **Newest First**, **Oldest First**, **Largest First**, and **Smallest First**.

### 💾 7. Reliable Export Dock
- Persistent bottom toolbar with one-click Windows destination folder picker.
- Live transfer progress displaying item counts, transferred megabytes, and real-time throughput (MB/s).
- Graceful cancellation support without locking the device or application.

### 💻 8. Scriptable Headless CLI
- Includes standalone `itool.exe` for headless terminal use, scheduled backup jobs, and automated workflows.

---

## Daily editor workflow

Use the **Dark mode / Light mode** button at the top right to switch the entire workspace. The preference is saved locally in `%APPDATA%\ITool\preferences.json` and reused on launch. Native Windows folder-picker appearance follows Windows settings.

1. **Connect & scan:** connect one iPhone by USB, unlock it, confirm Trust, and click **Scan iPhone**. A scanned library is a snapshot; reconnect the phone before copying.
2. **Choose media:** use filename/date/folder search, Photos / Videos / Live filters, the month sidebar, or **Latest day**. Latest day means the newest device file date in the scan, not necessarily today.
3. **Set your view:** choose **Grid** for thumbnails or **Details** for names, dates, sizes, types, and resource counts. Adjust thumbnail size. Sort by newest, oldest, largest, smallest, name A–Z, or name Z–A. Group by month, week, day, media type, source folder, or no grouping. The sort applies inside each group; groups appear in first-encounter order. Source folders are DCIM directories, not Photos albums.
4. **Select:** click individual items, Shift-click a range, drag across the grid, select a group, or use **Select shown**, **Invert shown**, and **Clear selection**. **Selected only** reviews your basket. Selection survives filters. Ctrl+A selects shown media; Escape clears selection when a text field or review window is not focused. Select shown and Invert shown affect the current filtered view only.
5. **Review export:** choose an existing parent folder and click **Review … items**. Review lists every original resource, including both Live Photo components and any selections hidden by filters. Choose one folder, folders by day/month, separate media-type folders, or source folders. Duplicate output names block transfer; source-folder organization resolves matching names in different DCIM folders.
6. **Transfer & verify:** click **Start transfer**. ITool creates the exact new folder shown in review, keeps original filenames, and copies the selected resources in group/sort order. Cancel preserves completed files. Open the resulting folder after completion, or use **Open incomplete export** after an interrupted transfer.

**Dates:** browsing and date-based export folders use device filesystem creation time. They are not an EXIF capture-date guarantee. Embedded metadata remains unchanged.

**Transfer evidence:** `completed.json` records successfully copied files after each file, with source paths, relative output paths, byte counts, and SHA-256 hashes. A successful batch also writes `transfer.json` with totals and verification details. Verification compares the received stream hash with a local destination reread and checks source size/mtime stability; it is not an independent source digest. These reports contain private media paths and stay in your local export folder. They do not implement automatic resume or incremental sync.

The export-folder choices are informed by [3uTools' documented Professional Export options](https://www.3u.com/3u). ITool keeps original resources together and never converts or writes media to the phone.

For a device-free UI preview, run `itool-desktop.exe --demo`. The sample media is synthetic, and device scanning and transfer are disabled. Add `--demo-list`, `--demo-review`, or `--demo-dark` to preview those surfaces and the dark palette. UI appearance and automated checks do not establish real-device transfer behavior.

---

## Why ITool is Better than 3uTools & Proprietary Chinese Apps

Popular third-party iOS management software (such as **3uTools**, **iTools**, **Tenorshare iCareFone**, etc.) has long dominated Windows iOS utilities, but they come with severe privacy, safety, and performance trade-offs.

Here is how ITool compares:

| Feature / Criteria | 3uTools & Chinese iOS Utilities | ITool (iPhone Original Media Exporter) |
| :--- | :--- | :--- |
| **Privacy & Telemetry** | ❌ **High risk.** Sends device identifiers (UDID, IMEI, serial), battery health, and telemetry to overseas servers. | ✅ **100% Offline.** Zero network requests, zero telemetry, zero analytics, zero data harvesting. |
| **Open Source** | ❌ **Closed-source proprietary binary.** Impossible to audit what code executes when your unlocked device is connected. | ✅ **100% Open Source Rust.** Full auditability, transparent codebase, and clean dependencies. |
| **System Footprint & Bloat** | ❌ **Heavy bloatware (200MB+ installer).** Installs background autorun daemons, adware popups, and virtual network adapters. | ✅ **Ultra-lightweight (~11 MB single binary).** Portable, zero installation required, no registry entries, no background daemons. |
| **Device Safety** | ⚠️ **Invasive write access.** Uses proprietary jailbreak/flashing daemons with write permissions on the device. | ✅ **Strictly Read-Only (`LFS_O_RDONLY`).** Connects via Apple's standard MobileDevice service. Impossible to delete or corrupt phone media. |
| **Media Preservation** | ⚠️ Often silently converts HEIC to compressed JPEG or strips paired Live Photo video tracks. | ✅ **Bit-for-Bit Original.** Never compresses or converts; pairs Live Photos automatically; verifies with SHA-256. |
| **UI Performance** | ❌ Laggy, web-view/Chromium or bloated MFC interfaces that stutter with thousands of files. | ✅ **Native GPU-accelerated Rust (`egui`).** Butter-smooth 60+ FPS virtualization across 4,700+ media files using ~60MB RAM. |

---

## Feature Requests — If You Request It, It Will Be Added! 🚀

ITool is being actively developed to become the fastest, most reliable, and most trustworthy open-source iPhone companion for Windows users.

**Have a feature in mind? Need a specific workflow?**
👉 **[Open a GitHub Issue](https://github.com/Marshmellow31/ITool/issues)** or reach out directly — **if a feature is requested, it will be prioritized and built!**

### Ideas on the Roadmap:
- [x] **Organized Exports**: Choose day, month, media-type, source-folder, or flat output in export review; see the date-source limitation above.
- [ ] **Incremental Sync**: One-click "Export New Photos Only" to skip media already backed up previously.
- [ ] **Selective Format Conversion**: Optional toggle to export converted JPEG/MP4 copies alongside the original HEIC/MOV.
- [ ] **Capture Timestamp Preservation**: Explicitly stamp NTFS file creation and modification timestamps from embedded EXIF metadata.
- [ ] **Album & Favorites Filtering**: Direct filtering by iOS Favorites and custom user albums.

---

## Download & Getting Started

### Requirements
- **Operating System**: Windows 10 or Windows 11 (64-bit).
- **Apple Mobile Device Support**: Provided automatically by the official **Apple Devices** app (from the Microsoft Store) or **iTunes for Windows**.
- **USB Connection**: Connect your iPhone with a standard Lightning or USB-C cable, unlock the screen, and tap **"Trust This Computer"** when prompted.

### Installation
1. Go to the latest **[GitHub Releases](https://github.com/Marshmellow31/ITool/releases)**.
2. Download `ITool-v0.2.0-windows-x64.zip` (or `itool-desktop.exe` directly).
3. Unzip anywhere and double-click `itool-desktop.exe` to run. No installation or admin privileges needed.

---

## Building from Source

If you prefer building directly from source:

```powershell
# Clone the repository
git clone https://github.com/Marshmellow31/ITool.git
cd ITool

# Run tests
cargo test --all-targets --features desktop

# Build optimized release binaries
cargo build --release --features desktop --bin itool-desktop
cargo build --release --bin itool

# Launch the desktop UI
.\target\release\itool-desktop.exe
```

---

## CLI Usage

For power users and automated backup scripts, `itool.exe` provides a clean command-line interface:

```powershell
# List connected Apple devices
.\target\release\itool.exe devices

# List DCIM directory contents
.\target\release\itool.exe list /DCIM

# Find specific photos or videos across DCIM folders
.\target\release\itool.exe find IMG_3119.HEIC IMG_3119.MOV

# Export media with bit-for-bit verification
.\target\release\itool.exe export --dest D:\Backups\iPhone2026 --chunk-kib 1024 /DCIM/123APPLE/IMG_3119.HEIC /DCIM/123APPLE/IMG_3119.MOV

# Verify exported files against a baseline manifest
.\target\release\itool.exe verify --baseline .\manifest.json --dir D:\Backups\iPhone2026
```

---

## License

This project is licensed under the [MIT License](LICENSE).
Apple, iPhone, Lightning, and Live Photos are trademarks of Apple Inc., registered in the U.S. and other countries. ITool is an independent open-source project.
