# Changelog

## 0.2.0 - 2026-10-24

- Add saved light/dark themes and clear action/filter/selection styling.
- Add guided export review, details view, thumbnail sizing, latest-day filtering, and selection review tools.
- Add filename/size ordering and media-type/source-folder grouping; preserve groups and range-selection anchors while reordering.
- Add flat, day, month, media-type, and source-folder export layouts with collision validation.
- Record completed files and final transfer evidence; expose interrupted exports.
- Fix cancelled thumbnail queues leaving stale loading states.

Validation: 12 automated tests, optimized Windows desktop/CLI builds, and synthetic-media light/dark UI checks. Fresh physical-device transfer verification remains pending. Dates use device filesystem creation time, not guaranteed EXIF capture time.
