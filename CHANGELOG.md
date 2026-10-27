# Changelog

## Unreleased development build — 2026-10-27

- Replace the single workspace with Connect, Choose media, Export and Results screens; neutral light/dark themes with blue actions and a paired-frame app icon.
- Add editable named-batch folders and a folder/resource preview; retain gallery selection, sorting, grouping and thumbnail loading.
- Add new-files-only export history with device/path/version separation and saved-file hash checks; preserve paired-resource skip decisions.
- Add optional embedded capture timestamps and additional JPEG/SDR H.264 MP4 copies using local FFmpeg tools. Originals remain untouched; optional failures are reported.
- Add conditional album/Favorites filtering from a bounded local Photos database/WAL snapshot with schema discovery and an unavailable state.
- Share roadmap options with the CLI; use 4 MiB default chunks and suppress the desktop/converter console windows.
- Replace unsupported README claims with implementation details and explicit evidence limits.
- Match the approved UI direction with a custom window header, photographic demo library, approved logo and consistent light/dark screens.
- Document all four main screens with eight actual demo screenshots in the README.

Optimized Windows builds and light/dark demo screenshot inspection are recorded in the build handoff. Automated tests, codec/date validation and physical-device behavior remain deferred. Existing published 0.2.0 assets remain unchanged.

## 0.2.0 - 2026-10-24

- Add saved light/dark themes and clear action/filter/selection styling.
- Add guided export review, details view, thumbnail sizing, latest-day filtering, and selection review tools.
- Add filename/size ordering and media-type/source-folder grouping; preserve groups and range-selection anchors while reordering.
- Add flat, day, month, media-type, and source-folder export layouts with collision validation.
- Record completed files and final transfer evidence; expose interrupted exports.
- Fix cancelled thumbnail queues leaving stale loading states.

Validation: 12 automated tests, optimized Windows desktop/CLI builds, and synthetic-media light/dark UI checks. Fresh physical-device transfer verification remains pending. Dates use device filesystem creation time, not guaranteed EXIF capture time.
