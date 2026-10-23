# Investigation findings

Evidence snapshot: 23 October 2026. Status terms distinguish direct evidence from proposed explanations and unfinished work.

## Verified: independent Rust USB export

The CLI communicates through Apple's USB service at `127.0.0.1:27015`, using the pinned `idevice` 0.1.68 crate and AFC. It opens device files read-only. The application copy path does not use 3uTools DLLs or re-encode media.

The exact seven device paths are recorded in [baseline-device-paths.txt](baseline-device-paths.txt). The sample contains one HEIC image, its associated MOV, and five other MOV files. Export results are recorded in [../baseline/rust-1024k-01/transfer.json](../baseline/rust-1024k-01/transfer.json); the reference hashes are in [../baseline/3utools-20261023-01-manifest.json](../baseline/3utools-20261023-01-manifest.json).

| Measurement | Result |
| --- | --- |
| Files | 7 |
| Bytes | 185,252,866 |
| Requested chunk size | 1024 KiB |
| Elapsed time | 11.1781255 s |
| Effective throughput | 15.805056 MiB/s |
| Comparison | All seven file sizes and SHA-256 digests match the 3uTools baseline |

Elapsed time includes destination disk rereads and transfer work after the AFC connection opens. Initial device connection/discovery is excluded. It is a single local measurement and cannot establish typical performance or superiority over 3uTools: the original 3uTools export was not timed reliably.

The exporter hashes bytes as received, rereads the local partial file, and checks that source size/mtime stayed stable. These checks detect several transfer and disk-write problems. They do not provide an independent device-side digest. Equality to the 3uTools baseline is the current strongest comparison evidence.

## Verified: static binary inventory

[pe-inventory.json](pe-inventory.json) contains the local PE inventory. A useful exported symbol is `FileMgrFile2PC`. These symbols and the earlier media-related strings identify promising analysis targets; they do not prove an executed call chain or the active transfer implementation.

The earlier inspection found references to AFC, DCIM, photo database queries, and thumbnail/edited resources. These are static clues. The independent Rust AFC export establishes that AFC can retrieve the selected sample, not that every 3uTools export uses this exact path.

## Verified: thumbnail discovery

The following thumbnail path was successfully listed on the test phone:

```text
/PhotoData/Thumbnails/V2/DCIM/123APPLE/IMG_3119.HEIC/5005.JPG
```

Listing establishes the path's accessibility for discovery. Thumbnail decoding, UI display, and reliable mapping across an entire library remain pending at this snapshot. Thumbnails are preview resources and must not substitute for originals during export.

## Inference: likely 3uTools architecture

Static references suggest a photo index supplies filenames and resource metadata, while a device file-transfer component retrieves the selected resources. AFC is a plausible transport for the selected media. Ghidra call-graph work or runtime tracing is still needed to attribute this behavior to a concrete 3uTools export path. Buffer sizing, parallelism and retry behavior have not been established.

## Pending: REA and Ghidra

REA 6.7 is installed under `.tools/rea`. The initial Ghidra provider attempt failed because the provider was not configured; its diagnostic is preserved in [rea-initial-provider-check.json](rea-initial-provider-check.json). [libphoto-artifact.json](libphoto-artifact.json) records the initial artifact investigation.

Ghidra 12.1.4 was being downloaded when this snapshot was written. Successful provider setup, decompilation and function tracing are pending. No completed REA/Ghidra decompilation is claimed.

## Pending: product work and validation

- Complete the Windows GUI and verify its actual launch, browsing and export behavior.
- Benchmark controlled repeat runs and multiple chunk sizes before selecting performance defaults.
- Test large videos, cancellation, disconnects, disk-full behavior and long batches.
- Add automatic Live Photo grouping and make original versus edited-resource selection explicit.
- Establish behavior for iCloud-only resources; report unavailable originals instead of silently exporting previews.
- Implement and verify reconnect/resume, import history, and collision-safe organization.
- Package a distributable app and document its Apple device-support requirements.

No Git repository or remote has been created; there are no project commits, pushes or PRs. This work remains a local prototype. Private sample media and device-derived records should remain local.
