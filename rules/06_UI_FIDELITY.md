# R-UI - fidelity and expectation contract

Do not interpret "clean" as empty wireframes, or "more visual" as adding charts.
Retain useful density, meaningful grouping, readable hierarchy and an obvious next
action. The latest owner corrections are part of the design, not optional style ideas.

## No uncontrolled visual invention

Use [UI_CONTRACT](../docs/ui/UI_CONTRACT.md), the screen's spec and an approved baseline.
The initial dark generated screenshots and Clean UI page are rejected references.
Refined V2 is the starting layout only; remaining dot markers must become semantic SVG
icons. Preserve content and workflows unless a change record explicitly alters them.

Do not introduce a hero section, gradients, glassmorphism, colorful full-width status
panels, decorative chart libraries, health scores, a new dashboard grid or extra cards.
Do not remove required labels/details merely to make the screen look sparse.

## Icons

Use the explicit semantic registry. Verify exports from the installed lucide-react
version. No Unicode bullet/circle/ellipsis stand-ins for status or action icons; no
hand-drawn SVG approximation when an official icon exists. Text punctuation is allowed;
a separator dot in a sentence is not a status marker. GitHub/GitLab brand marks come
from verified brand assets, not an invented animal silhouette or an outdated Lucide
brand export. Do not ship local font files in this documentation package.

## Approval and evidence

Capture original/reference and implementation separately at the same viewport/state.
Register approval and hashes; never self-approve. Test internal sibling intersections,
text clipping, dialog boundaries and hit targets as well as outer screen bounds.
Compare at 1440x900 and the specified smaller/wider sizes. Scroll only intended regions.
Reflow or local scrolling must not hide the primary action or keyboard focus.

A screenshot test is insufficient without actual keyboard/flow tests and targeted
native Tauri verification. Missing baseline => BLOCKED visual sign-off. It is permitted
to build a clearly labeled candidate implementation, not to call it design-perfect.
