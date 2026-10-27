# ITool

## Platform
Windows desktop (native Rust application).

## Stack
Rust, idevice for USB/AFC, egui/eframe for the desktop interface. The user requested Rust on Windows and delegated optional layout choices.

## Users and purpose
Harshil wants to export iPhone photos and videos to Windows quickly, preserving the original files. Thumbnails are explicitly required in the first interface.

## Operating context
A trusted iPhone is connected by USB to this Windows PC. Apple Mobile Device Support is already installed. The baseline consists of one Live Photo and five videos manually exported by 3uTools.

## Capabilities and constraints
Copy only; no device deletion or writes. Keep original HEIC/MOV bytes, embedded metadata and Live Photo resources. Optional conversion creates additional copies and never replaces originals. Store personal media locally. Do not imply cloud-only resources are available. File integrity and clear outcomes take priority over optional features.

## Evidence
The Rust CLI successfully exported seven resources matching the 3uTools baseline SHA-256 hashes. First measured end-to-end copy and verification: 185252866 bytes in 11.1781255 seconds. This is not a timed comparison with 3uTools.

## Product principles
Preserve bytes. Make selection and progress legible. Report incomplete transfers honestly. Never overwrite existing media.


## Daily editor workflow

Use Connect → Choose media → Export → Results. Browse in grid/details, search/filter/group/sort and keep a selection basket. Export to an editable named new folder with a tree preview and output layout choices. Local checked history enables new-files-only exports; optional EXIF/container capture timestamps and additional compatible copies are available. Albums/Favorites are conditional on readable supported Photos metadata. Gallery dates and folder dates still use device filesystem dates. Byte-offset resume and cloud-only original retrieval remain unsupported.

## Current development evidence

The guided UI and roadmap implementations are development changes. Windows compilation is separate from visual, codec, timestamp, device-metadata and transfer validation. Tests and device operations remain paused in this session. Preserve existing release identities until a release is explicitly requested.
