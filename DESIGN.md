---
name: ITool
description: A guided Windows desktop workspace for bringing iPhone originals home.
colors:
  action-blue: "#3B6AFA"
  light-canvas: "#FAFBFD"
  light-panel: "#FFFFFF"
  light-sidebar: "#FCFCFE"
  light-text: "#191D27"
  light-muted: "#596377"
  light-border: "#DDE2EC"
  light-selection: "#E8EDFF"
  dark-canvas: "#181E25"
  dark-panel: "#1C212A"
  dark-sidebar: "#1E242D"
  dark-text: "#F0F2F7"
  dark-muted: "#ABB5C7"
  dark-border: "#353E4E"
  dark-emphasis: "#829EFF"
  dark-selection: "#263150"
typography:
  body:
    fontFamily: "Segoe UI, egui proportional fallback"
    fontSize: "14px"
    fontWeight: 400
  headline:
    fontFamily: "Segoe UI Semibold, egui proportional fallback"
    fontSize: "28px"
    fontWeight: 600
  label:
    fontFamily: "Segoe UI, egui proportional fallback"
    fontSize: "12px"
    fontWeight: 400
rounded:
  control: "5px"
  primary: "6px"
  window: "8px"
spacing:
  inline: "12px"
  row: "8px"
  section: "24px"
components:
  button-primary:
    backgroundColor: "{colors.action-blue}"
    textColor: "{colors.light-panel}"
    rounded: "{rounded.primary}"
    height: "48px"
  button-secondary:
    rounded: "{rounded.control}"
    height: "36px"
---

# Design System: ITool

## Overview

The approved references in `design/mockups/` define the visual authority: a calm Windows utility with neutral light surfaces, graphite dark surfaces, blue actions, a photographic contact sheet and the paired-frame identity. Preserve the reference compositions for Connect, Choose media and Export when extending these screens.

This document records the native Rust egui implementation in `src/bin/itool-desktop.rs` and `src/bin/desktop/`. It describes implemented appearance and behavior, not a certificate of pixel-perfect fidelity. Sample scene compositions and badges differ from the mockups; no image pixel-diff measurement has established an exact match.

## Colors

Action blue anchors primary buttons in both themes. Dark emphasis blue is used for readable labels, icons and selection outlines on graphite. Selected controls combine a pale or slate-blue fill with a blue border. Neutral text, muted labels and thin dividers carry the rest of the hierarchy.

**The Blue Chrome Rule.** Use no green for application controls or success chrome. Preserve the original colors inside photographs. Errors use the existing warm red palette and warnings use amber; do not communicate state through color alone.

## Typography

Load Segoe UI for regular text and the actual Windows Segoe UI Semibold font for the named heading family, with egui proportional fallback when those files are unavailable. Connect and Export lead with semibold headlines (28px); Choose media uses a smaller heading (24px), shell identity uses 22px, and the general heading style uses 21px. Body and primary button text use 14px, secondary controls use 13px, and supporting labels use 12px. Keep headings concise and supporting copy subordinate.

## Layout

The borderless desktop window defaults to 940×664 with a 900×600 minimum. A custom header (60px high) combines the logo and name, four numbered steps, theme/Settings controls and minimize, maximize and close actions. Header background supports dragging and double-click maximization. Step navigation respects prerequisites and is disabled during active operations.

Use the existing 12px horizontal / 8px vertical rhythm and 24px section separation. Connect places instructions and its scan action beside the phone/cable illustration, then a three-part feature strip. Choose media has a fixed 188px sidebar, a gallery toolbar and a persistent selection footer. Gallery columns fit the available width; continuous galleries with up to eight items reduce card height when space is constrained. Larger libraries scroll with off-screen rows skipped.

Export places destination fields beside a bounded folder-tree preview. The columns stack below 760px of content width. The screen scrolls while Back and Start export remain in the footer. More options is collapsed initially so destination and organization choices stay prominent.

## Elevation & Depth

The main surfaces are flat, separated by thin borders, dividers and changes in neutral tone. Photographs provide the visual richness. Preserve this restrained layering instead of introducing decorative shadows or elevated dashboard cards.

## Shapes

Controls use compact rounded corners; primary actions and the folder preview use slightly softer corners. Icons are consistent thin geometric strokes. Gallery previews fill their image rectangles with a centered cover crop, preserving aspect ratio; labels sit below rather than across the photographs. Selection uses a blue outline, tint and check indicator.

The shell uses the approved paired-frame PNG from `design/mockups/itool-logo-concept.png`, cropped for display. `assets/itool.svg`, PNG and ICO remain the packaged identity assets for their existing roles; do not describe the shell logo as a substitute geometric redraw.

## Components

- **Header and navigation:** Four steps, Connect → Choose media → Export → Results, with blue current-step emphasis and quieter unavailable states. The custom window controls share the header's visual language.
- **Buttons and inputs:** Blue filled primary actions with white semibold labels; neutral outlined secondary controls; tinted blue hover/selected treatment. The Connect scan action is larger (330×56px). Destination and folder-name inputs remain editable, with Browse beside the destination.
- **Connect:** Three numbered USB/unlock/Trust instructions, Scan iPhone, expandable connection help and inline errors. The phone/cable artwork has a light treatment and a lighter-stroke dark treatment. A scanned-library message does not imply continuous device detection.
- **Gallery:** Media-type, month and conditional Favorites/Albums navigation; search, Grid/Details, Sort and View controls above the contact sheet. Retain range, marquee and group selection, thumbnail loading/failure/retry states, and the basket summary with Continue to export.
- **Export:** Save location, folder name and organization choices sit beside the tree/resource preview. Only copy new files, embedded capture timestamps and optional compatible copies live inside More options. Invalid destinations, conflicts and unavailable conversion tools receive explicit explanations.
- **Results:** Progress/cancellation and complete/interrupted states distinguish originals, skipped resources, compatible copies and notes. Open folder targets the actual export batch in normal operation.

Demo mode routes through the guided screens using sample media and simulated outcomes, without device operations or export file copies. Counts and rates in demo results are explicitly sample data; folder opening and rescanning are disabled there.

## Do's and Don'ts

- **Do** use the approved mockups as composition references and the implementation tokens above for new controls.
- **Do** preserve visible selection state, readable filename labels, scroll access and persistent footer actions at constrained sizes.
- **Do** treat `.impeccable/review/final/` as rendered demo screenshot evidence, checking that captures reflect the latest source before using them for comparison.
- **Don't** promote mockups or sample screenshots into proof of physical-device behavior, successful real transfers, timestamp fidelity or codec support.
- **Do** distinguish the successful local release build recorded in docs/BUILD_HANDOFF.md from automated tests and physical-device validation, which remain paused at the user's request.
- **Don't** claim exact mockup reproduction without measured evidence or replace blue chrome, the paired-frame logo or the guided four-step structure with a new visual direction.
