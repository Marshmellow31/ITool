# ITool future scope

Reviewed 27 October 2026 against source at `a25c9a4` and the saved local 3uTools static-analysis artifacts. This is a planning document; none of the proposed features is implemented by this review.

**Recommendation-not-fact:** prioritize a dependable, read-only media archive workflow: preflight → recoverable export → archive verification → richer resource handling. Broader device utilities can follow as separate, opt-in modules.

## Evidence and limits

- **Verified** means directly visible in the inspected source or saved static-analysis output, with that scope stated. It does not mean physical-device validation.
- **Historical** means a recorded earlier result, not repeated in this review.
- **Needs-confirmation** means feasibility, device access, schema compatibility or behavior remains unproven.
- **Recommendation-not-fact** marks proposed design and priority.

### What the 3uTools decompiled output actually shows

The review covers saved Ghidra output for selected functions in `idm_fm.dll`, plus PE imports/exports and string anchors for other modules. It is not a complete decompilation or runtime trace of 3uTools. The recorded `idm_fm.dll` SHA-256 is `93a1dff3a1598ff3c26e857f0ba0223182a82175d5ed12adce165af5fcd5f464`; the installed binary was not rehashed in this review.

| Evidence | Verified static observation | Implication and limit |
| --- | --- | --- |
| [File2PC wrapper](../analysis/rea/fm-File2PC-0.json), address `0x180008ac0` | Initializes transfer state and calls the overload at `0x1800088a0`. | A concrete export entry point is available for inspection. |
| [File2PC implementation](../analysis/rea/fm-File2PC-1.json), address `0x1800088a0` | Opens a source through a helper, opens the PC destination with `wb+`, repeatedly reads and writes, invokes a progress callback, checks a cancellation flag, and handles read/write errors. | Supports progress, cancellation and actionable errors as useful product ideas. This function alone does not establish retries, resume, no-overwrite behavior, hashing or parallel transfers. ITool should retain its own stronger publication/integrity rules. |
| [Open helper](../analysis/rea/fm-open-helper.json), address `0x180001dd0`; [read helper](../analysis/rea/fm-read-helper.json), address `0x180001fa0` | Builds requests with operation values `0xd` and `0xf`; both use a critical section. File2PC passes open mode `1`. | These are consistent with AFC file-open/read operations. Protocol attribution is an inference; locking here does not prove all of 3uTools is single-threaded. |
| Constructor at `0x180008030` in local `analysis/private/usb3-constructor.json` | Stores `0x400000` in the field used as File2PC's read-buffer size. | A 4 MiB constructor default is visible. It does not establish the active size in every path, a USB link speed or a performance advantage. The raw constructor artifact is local and ignored by Git. |
| [AFC strings](../analysis/rea/fm-afc-string.json) | Contains `com.apple.afc` and `com.apple.afc2`. | String presence is not proof that a service is used or accessible. ITool should continue using ordinary trusted-device access. |
| [PE inventory](../analysis/pe-inventory.json) | `libphoto.dll` exposes `ios_photo_get_list` and `ios_photo_get_thumb`; the inventory also contains Photos.sqlite/WAL anchors, FileMgr directory/stat/size symbols, battery/diagnostics imports, and backup/contact-related imports. | Identifies research targets for metadata, browsing and optional utilities. Symbols do not demonstrate implementation semantics or compatibility on a particular iPhone. |

The [older findings](../analysis/FINDINGS.md) are a historical snapshot: their statement that Ghidra was pending predates the saved decompilation above. Their pending UI/features and repository status also do not describe the current source.

### Current ITool baseline

**Verified in source:** guided selection/export, thumbnail previews with an in-memory cache, grouped still/MOV resources, organized named batches, completed-file checkpoints, received-stream SHA-256 plus local reread, source metadata checks, cancellation/timeouts, device matching for desktop exports, checked incremental history, optional compatible copies, destination capture timestamps, and conditional Albums/Favorites. See [lib.rs](../src/lib.rs), [workflow.rs](../src/workflow.rs), [library.rs](../src/library.rs) and [desktop](../src/bin/itool-desktop.rs).

These are not new roadmap additions. Their remaining device/media/interruption validation is recorded in [VALIDATION_HANDOFF.md](VALIDATION_HANDOFF.md). Incremental history is not partial-file resume; the current Live Photo grouping uses matching filename stems, not verified asset identifiers. The existing CLI baseline verifier expects flat files, so a general grouped-archive verifier is still useful.

**Historical:** the earlier seven-resource Rust export matched a 3uTools sample byte-for-byte. This does not validate later features or establish a speed comparison.

## Proposed additions, in delivery order

All rows below are **recommendation-not-fact**. Effort is relative, not a delivery estimate. No feature is guaranteed safe merely because device access is read-only: local disk loss, privacy, stale metadata and decoder failures still need controls.

| Priority / effort | Addition and user benefit | Safe implementation boundary | Required evidence before calling it supported |
| --- | --- | --- | --- |
| 1 / Small–medium | **Export preflight:** show required original bytes, available PC space, destination filesystem, write access, filename conflicts and optional-tool availability before starting. | Check the actual destination volume. Explain conversion space is an estimate; reserve working space and handle disk-full during the job. Use only an app-created local probe file. Keep NTFS as the current requirement. | Low space, unwritable folders, long/Unicode paths, removable targets and a destination that fills after preflight fail clearly without damaging completed files. |
| 2 / Medium | **Retry failed items / recover a batch:** reconnect and finish remaining originals without selecting everything again. | Persist an export plan and per-resource state. Revalidate the device and source, rehash completed files, and restart an unfinished file from byte zero first. Bound retries; stop on identity/source changes or permanent access errors. Mark a Live Photo group incomplete if either resource is missing. | Unplug/reconnect, app restart, changed source, wrong phone, cancellation, tampered checkpoint and unavailable original. Never overwrite a completed file or call an incomplete pair complete. |
| 3 / Medium | **Verify an archive later:** check a saved export without connecting the phone; show missing/modified originals and optional copies separately. | Read manifest-relative paths within the selected archive only; reject traversal and unsafe links. Rehash local files. Include grouped layouts and report schema versions. Hashes prove agreement with recorded hashes, not authenticity against a maliciously replaced manifest. | Deleted/modified files, broken reports, moved batches, traversal/link inputs and large grouped exports. Label this local archive verification, not independent device verification. |
| 4 / Medium | **Media details and archive metadata:** dimensions, duration, codecs, capture date/timezone, available album/favorite data; JSON/CSV sidecars. | Prefer local exported originals for EXIF/ffprobe work. Record provenance and unknown values. Keep GPS/location opt-in and local. Sidecars must not modify original media; escape CSV formula-like values. | Known media fixtures, malformed files, conflicting/missing dates, Unicode albums, denied metadata access and schema changes. |
| 5 / Medium–large | **More accurate Live Photo, RAW+JPEG and edit handling:** preserve accessible original resources and optional AAE/edit sidecars; distinguish original from rendered edits. | Use asset/resource identifiers where available, with an explicitly labeled filename fallback. Copy only accessible regular files; preserve names and pair relationships in the manifest. Do not execute adjustment instructions or replace originals with edited previews. | Same-stem false pairs, missing clips, RAW pairs, edited images, bursts and unsupported resource mappings. Rendering Apple's edits and reimporting Live Photos require separate proof. |
| 6 / Medium | **A faster large-library experience:** progressive catalog display, persistent preview cache, offline browsing of prior exports and richer date/type filters. | Existing in-memory thumbnails already work in source. Any persistent cache must be device-scoped, bounded, invalidatable and user-clearable; offer no persistent cache. Keep previews distinct from accessible originals. | Large synthetic libraries for responsiveness, then real-device mapping; stale cache, changed phone and cache cleanup. Photos and metadata remain local. |
| 7 / Medium–large | **Album-based output and export presets:** save common choices; optionally organize output by actual Photos albums. | Current day/month/type/source layouts and album filtering already exist. For album folders, define multiple-album membership, invalid/colliding names and unfiled assets. Prefer one physical original plus an album index first; independent duplicate copies can be an explicit option. Never claim DCIM folders are albums. | Multi-album assets, duplicate names, unavailable metadata, deterministic paths and a preview matching the resulting folder tree. |
| 8 / Large | **True pause/resume inside a large video:** continue from a verified byte offset. | Needs-confirmation. AFC exposes seek operations, but safe resume is additional work. Bind state to device/source identity and options; compare the existing partial prefix with a fresh source prefix before appending, then verify the complete stream and destination. Metadata-only matching is insufficient. Fall back to restarting the file on uncertainty. | Modified partials/sources, interruption at many offsets, files over 4 GiB, unsupported seek, app crashes and no-overwrite publication. Prefix checking costs time and cannot promise instant recovery. |
| 9 / Medium–large | **exFAT external-drive exports:** save directly to common portable SSDs. | Needs-confirmation. Current finalization depends on NTFS hard links. Design and validate an alternative that publishes without replacing an existing target; do not silently drop the integrity contract. Treat FAT32 file-size limits separately and do not silently split originals. | Name races, power loss, unplug, disk-full, interrupted publication, Unicode names and >4 GiB files on the actual target filesystems. |
| 10 / Medium–large | **Measured performance tuning:** benchmark presets and possibly overlap device reads with local hashing/writes. | Measure scan, read, disk reread and conversion separately. Compare existing 1/4/8 MiB settings first. Keep memory bounded, full verification and cancellation. Multiple AFC sessions are a later experiment; do not assume more connections are faster. | Repeated same-device/cable/port/destination trials, small and large files, unchanged hashes, bounded memory and clear benchmark context. No USB 3 claim from buffer size or throughput alone. |
| 11 / Large | **More compatible viewing copies:** optional HDR-to-SDR and local Live Photo viewing/export convenience. | Current compatible-copy support skips HDR/unknown transfer metadata. New tone mapping must create labeled derivatives, preserve originals and record settings. A playback-friendly pair or combined clip is not an Apple-reimportable Live Photo. | Known HDR formats, orientation, audio sync, clipping/color quality and multiple playback devices; unsupported input stays unsupported. |

## Optional expansion beyond media

These are separate product directions, not commitments or capabilities established by the 3uTools symbols.

| Direction | Assessment | Bounded next step |
| --- | --- | --- |
| Device overview: model, iOS version, connection state, accessible-media storage | **Recommendation-not-fact; needs-confirmation** for individual fields. Useful and relatively contained. | Read only trusted services already available to the app. Distinguish AFC media-volume capacity from whole-device storage. Hide identifiers in screenshots/logs by default; use “unavailable” for denied fields. |
| Battery/diagnostic information | **Needs-confirmation.** Inventory imports identify a research lead, not trustworthy battery-health or component-authenticity results. | Investigate a small allowlist of read-only values. Present source/units and unknowns. Avoid “genuine parts,” repair-history and precise health claims without independently validated meaning. |
| Read-only accessible-file browser | **Recommendation-not-fact.** FileMgr listing/stat symbols suggest useful UX, while ITool already has CLI listing. | Start with media paths and explicit copy-out. No delete, rename, upload, device timestamp changes or implied access to every app sandbox. Add strict path/symlink rules. |
| Import a user-selected local Apple/iCloud export folder | **Recommendation-not-fact.** Can reuse local verification and organization without adding Apple-account handling. | Keep local-folder provenance separate from USB. Do not assume album/Live Photo metadata is complete or that filesystem placeholders are downloaded original bytes. |
| Contacts, messages or local-backup inspection | **Needs-confirmation; separate discovery project.** More sensitive data and a different access model. | Evaluate only explicit user-selected local backups first, with privacy controls and encrypted-backup requirements. The `cm_backup_contact` / backup imports are only names. Do not promise recovery, decryption or access to live device databases. |

## Deferred work and scope boundaries

| Area | Status and detailed boundary | Acceptance requirement / next step |
| --- | --- | --- |
| Broader Photos-schema coverage | **Recommendation-not-fact; needs-confirmation.** Extend the current conditional Albums/Favorites parser and asset-resource mapping only for observed, understood schemas. Keep reads on bounded, consistent local snapshots; no device database writes. Preserve unknown states instead of treating unsupported metadata as an empty album. | Build versioned fixtures from authorized, private samples; check album/favorite/resource mapping against Photos on each supported device/iOS combination. Reject inconsistent snapshots, unexpected joins and unmapped paths clearly. |
| Direct cloud-only original retrieval | **Needs-confirmation; deferred.** A thumbnail or database record does not prove original bytes are available through AFC. Direct Apple-account login/download automation is outside the current implementation plan. | First explain unavailable originals and offer Apple's supported download/export instructions. Any later retrieval proposal needs its own supported access method, authentication/privacy design and byte-integrity validation. Local-folder import is a separate, narrower proposal. |
| Device modification and bypass utilities | **Outside this read-only roadmap.** Device deletion/cleanup, two-way sync, Photos database writes, flashing/restoring firmware, jailbreak/AFC2 access, lock bypass, activation changes and backup-password manipulation conflict with the present product contract or require an independently scoped product and validation effort. | Do not add device-write APIs as a shortcut for media access or recovery. Keep read-only browsing/copy-out and explicit local archive operations as the product boundary. |

## Suggested milestones and acceptance gates

These names are planning labels, not version numbers or release dates. Complete and verify each milestone before starting the next ordered release.

1. **Reliability:** validate the existing build, then add preflight, recoverable batch plans and general archive verification. Gate: interruption/disk-full/wrong-device scenarios leave every completed original intact and all incomplete resources explicitly reported.
2. **Archive completeness:** media details/sidecars, stronger resource association, album indexes and presets. Gate: known asset fixtures and real-phone samples retain originals and relationships, with unsupported schemas clearly identified.
3. **Scale and destinations:** progressive scanning/cache, byte-offset resume and filesystem support, each delivered and verified separately. Gate: long-batch responsiveness, corruption/interruption checks and destination-specific behavior meet the documented contract.
4. **Optional utilities:** choose one device-information or local-import module after a separate feasibility review. Keep sensitive backups and device modification outside this sequence.

For each milestone, use offline fixtures and fault injection where useful, followed by explicitly authorized real-device validation. This planning review performs no tests, app launch, device connection, release or remote write.

## External feasibility references

- [libimobiledevice AFC API](https://docs.libimobiledevice.org/libimobiledevice/latest/afc_8h.html): read-only open, directory/stat/device information and seek primitives. This supports feasibility research, not compatibility guarantees for ITool's Rust implementation. The pinned local `idevice` 0.1.68 source also exposes AFC device information and seek-related methods; no device calls were made.
- [Microsoft CreateHardLinkW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createhardlinkw): documents the NTFS constraint behind the current publication design. exFAT support needs an alternative implementation.
- [Apple: Download iCloud photos and videos](https://support.apple.com/en-us/111762): describes supported original-download/export routes and iCloud for Windows. This does not establish direct AFC access to cloud-only media.
