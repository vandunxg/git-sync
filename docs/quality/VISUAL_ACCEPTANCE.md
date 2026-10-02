# Visual fidelity gate

## Two different artifacts

1. **Design reference**: exported from the owner-reviewed corrected design. It expresses
   expected composition, density, typography, spacing, icons and approved deviations.
2. **Implementation golden**: rendered by the app in a pinned environment and independently
   accepted after comparison. It is used for regression testing, not to invent the design.

Never create both from current implementation output and call that fidelity validation.
Figma Refined V2 is a historical reference. Dot replacements were requested but the last
write was blocked. Do not claim those corrections were applied or that an old outer-bounds
check established internal correctness. Current BASELINES.json intentionally has no approved
images or hashes. check_visual_baselines.py reports BLOCKED until real artifacts exist.

## Freeze procedure

For each UI-01..UI-10 and implemented subview, capture corrected source at 1440x900.
Record source revision/time and pending DD-* corrections. Obtain actual owner review,
record its evidence, then register the source path/hash. Image paths are relative to the
blueprint root. Keep design exports distinct from implementation outputs. UI-10 is a state
catalog, not a production navigation route. Folder/review steps, detail tabs and settings
subviews absent from the original frame must have their own state-specific captures.

Next render the implementation using the fixture seed, locale, Geist build and pinned
renderer. Record the implementation commit, viewport, DPR, theme and renderer. Compare side
by side and with a difference image. Review at native size, not only a contact sheet.
Only after independent acceptance register the implementation golden. Approval metadata is
an audit record, not cryptographic proof that a named human approved it; preserve the actual
review evidence. Implementers cannot approve themselves.

## Required checks

- Correct screen hierarchy and labels; no invented cards, charts, settings or capabilities.
- Genuine semantic icons, consistent stroke/size, no text-dot substitute for state; brand
  marks from verified assets, not a guessed SVG. Text labels remain visible.
- No sibling intersection that obscures content, no clipped text/descenders, no fixed-height
  container hiding wrapped content, no hidden focused control.
- Tables have aligned columns and real loading/empty/error/partial states; long names and
  paths wrap/truncate intentionally with accessible full information.
- Buttons have valid actions or an explicit unavailable explanation; no inert false promise.
- Dialog focus trap, Escape/back/cancel and return-focus; keyboard-only navigation; meaningful
  accessible labels and sufficient contrast. Color is not the only status distinction.
- Repeat at 1280x800 and 1024x768, supported fractional scaling, zoom and long-content fixtures.
  These are desktop targets, not an invented mobile product.
- Verify real Tauri/WebKitGTK layouts and Linux dialogs/tray behavior. Chromium passing alone
  does not establish native equivalence.

Pixel-diff tolerance must be recorded per pinned renderer after a reviewed calibration;
there is no magic universal threshold. Do not raise the tolerance to hide a geometry/icon
regression. Ignore only explicitly approved dynamic regions, never the entire screen.
Intentional UI changes require a change record and renewed approval, not --update-snapshots
as an automatic repair.

## Gate result

Missing source, approval, required subview, renderer proof or hash mismatch => BLOCKED.
A known rendering/accessibility mismatch => CHANGES_REQUIRED. A mere screenshot file or a
passing baseline-file integrity script does not mean visual acceptance passed. P03-P07 can
continue on stable contracts while the visual gate is blocked; P08/P10 cannot claim UI DONE.
