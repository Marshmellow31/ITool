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
Copy only; no device deletion or writes. Keep HEIC, MOV, embedded metadata and Live Photo resources. No conversion. Store personal media only locally. Do not imply cloud-only resources are locally available. File integrity and speed take priority over optional features.

## Evidence
The Rust CLI successfully exported seven resources matching the 3uTools baseline SHA-256 hashes. First measured end-to-end copy and verification: 185252866 bytes in 11.1781255 seconds. This is not a timed comparison with 3uTools.

## Product principles
Preserve bytes. Make selection and progress legible. Report incomplete transfers honestly. Never overwrite existing media.
