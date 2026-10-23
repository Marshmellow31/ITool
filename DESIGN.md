# ITool desktop design

The user's primary requirements are original-file export, speed, and visible thumbnails. Optional layout decisions were delegated. The interface follows a photographic contact sheet: an orderly light workspace, image-led selection, a persistent transfer strip, and quiet controls.

The grounded directions considered were a Windows file manager, an archival inventory, a photographic contact sheet, a transfer console, a film editing bin, a camera import panel, and an image proofing board. The contact sheet is candidate three, matching the required direction seed. It provides direct recognition of the selected media while keeping filename and type legible.

The warm consumer-app challenger adds comfortable controls but loses density and media visibility; keep action clarity. The transit-map challenger obscures the media, but contributes consistent status semantics. Experimental barcode graphics do not help select personal photos; retain their disciplined alignment rather than their decoration. Product clarity governs this utility.

Use a neutral light canvas, white previews, dark readable text, and a restrained green action accent. Avoid decorative cards unrelated to the media. The Windows frame and folder picker retain platform behavior. Show loading, missing preview, no-device, failed scan, cancelled transfer, and completed transfer states explicitly.

The first screen is a local-only browser with a thumbnail grid, filename search, selection controls, and a destination picker. The footer owns export progress and cancellation. Preserve selection across pages. Load only the current page's thumbnails; no full-resolution decode is needed for browsing. Preview availability is separate from original-file availability.


## Editor workflow refinement

Keep the contact sheet and restrained green action accent. A persistent four-step guide names Connect & scan, Choose media, Review export, and Transfer & verify. Browsing offers a compact details view and adjustable grid density. Group and order are separate controls; groups remain intact under size/name ordering and within a month filter. Selection is a library-wide basket, with explicit shown-only actions and a selected-only filter.

Export review is a separate, scrollable window for the destination, layout, resource list, and collision feedback before Start transfer. Each export creates a new folder and preserves names and Live Photo resources. Date labels disclose their filesystem source. A scanned-library status must not imply a currently connected device. Completion and interruptions link to the actual output folder. Demo previews must label synthetic data and disable device actions.


## Control hierarchy and themes

Use Windows' installed Segoe UI where available, with egui's fallback elsewhere. Solid green belongs to the next transfer action. Secondary actions have a visible border; selected filters and view switches use an accent border and tinted fill. Clear selection is a quiet red action. Connection and demo status are plain text. Search, view settings, and selection actions occupy separate rows. Keep label/value pairs inside each dropdown so they cannot wrap apart.

Light and dark palettes share semantic roles: text, muted text, panel, sidebar, canvas, border, accent, tint, warning, and danger. Theme every app surface, including thumbnails' surrounding chrome and export review; preserve original preview pixels. Keep the local theme preference across launches. The native folder picker follows Windows rather than the custom app palette.
