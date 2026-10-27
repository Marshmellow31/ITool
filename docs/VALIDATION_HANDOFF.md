# Deferred validation

The user authorized implementation and Windows builds, while automated tests and physical-device operations remain paused. No new app launch or visual inspection was performed in this pass.

## When validation is authorized

1. Open `itool-desktop.exe --demo --demo-connect`, then sample gallery, `--demo-review` and `--demo-results`. Repeat with `--demo-dark` and at 900×600. Check visible controls, text fitting, keyboard focus, scrolling and theme consistency against design/mockups. Synthetic screens do not validate USB behavior.
2. Run `cargo test --all-targets --features desktop`. The existing suite covers prior validation and gallery helpers; it does not establish full coverage of the new modules.
3. Connect one owned, unlocked iPhone; confirm Trust and Scan. Compare available Favorites/Albums with Photos. Confirm unavailable database/schema states are clear rather than silently showing an empty collection.
4. Select a small still/video/Live Photo sample. Set a new editable folder name and inspect the output-tree preview. Use NTFS. Confirm original filenames, paired resources, report counts and byte hashes; test duplicate names from separate DCIM folders.
5. Export the same sample to another new batch under the same parent with Only copy new files. Confirm expected skips. Delete or modify a prior saved file, then retry and confirm recopy. Test a changed source signature and a different connected phone. Confirm paired resources are never split by skip decisions.
6. Enable capture dates for known offset/no-offset EXIF images and known videos. Compare NTFS creation/modification dates and unchanged original hashes. Missing/ambiguous capture metadata should produce notes without inventing dates.
7. With local FFmpeg/ffprobe, request compatible copies for supported HEIC/HEIF and SDR MOV/M4V, plus HDR and missing-color-metadata videos. Confirm playback, orientation, quality and original hashes. Confirm unsupported codecs/HDR yield notes and retained originals.
8. Check cancellation during original copying, history checking and conversion, plus unplug, disk-full, permissions and long batches. Completed originals should remain intact; partial files must not count as completed. Reusing valid history is not byte-offset resume.
9. Benchmark controlled large originals across 1/4/8 MiB settings on a verified USB 3 device/cable/port. Do not infer link speed from copy rate or claim 10 Gbit/s without measurement.

No release, Play upload, remote push or automatic device operation is part of this handoff.
