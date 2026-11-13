# Opaque-background audit — 1 November 2026

Scope: source review of the unified Swift app's floating controls, sheets, decision
surfaces, reading and writing content after D's burgundy-glass update. This is not
an assertion that every system-managed presentation was inspected at runtime.
Only D's Column A navigation-preview fix is implemented in this slice.

## Implemented correction

D's preview previously filled its rounded shape with burgundy at 55% opacity,
allowing the workspace beneath to compete with its controls. The renamed
`InterfaceDNavigationPreviewSurface` now fills that shape with unmodified opaque
burgundy. The rim remains decorative. Shape, content, hit regions, hovered-icon
selection, keyboard opening and dismissal retain their existing owners.
The same surface serves all five navigation areas, including compact layouts.
No whole-window or foreground opacity is changed.

## Recommendations

| Surface | Source finding | Recommendation |
| --- | --- | --- |
| Column A navigation preview | D's custom overlay had a translucent fill; B and C already use opaque canvas fills. | Mandatory opaque interior. Fixed in D. |
| Link and picture editing popovers | Shared ComposerBodyEditor uses native popovers without an explicit opaque content fill. | Require opaque content backing for the editable address, picture description and controls. Native presentation must be checked; source alone does not establish bleed-through. Any D adoption should be explicit through its existing opt-in composer style. |
| Charge review and request terms | D's directory approach card uses thinMaterial; the detailed conditional-charge summary adds only quaternary backing. Request review also uses sheets. | Require opaque backing behind charge terms, deadlines and authorization controls. The surrounding window may remain glass. |
| Permission, disclosure and account decisions | Contact permission, alias publication and enrollment use native sheets; destructive/permission actions use native confirmation dialogs. The shared external-picture approval sheet has no explicit opaque content fill. | Require stable opaque content backing for forms and explanations, especially destination, disclosed resources and consequences. Native dialogs/menus remain system-managed; inspect before deciding whether an additional app-owned surface is needed. Do not replace native interaction just to force a color. |
| Saved drafts and referenced selections | Draft lists/editors and referenced-source text use sheets without an explicit opaque app-owned content fill. | High-priority candidates for opaque content backing. Exact source text and unsaved work need stable contrast. Do not change reading or source authority. |
| Native/plain-text correspondence and Column B source helper | D's native reader uses a 48%-opacity reading backing; the shared helper's plain text has no dedicated opaque reading surface. | Recommend an opaque reading area for sustained text or exact quotation/selection. Preserve surrounding glass and source coordinates. |
| Recipient suggestions and search results | Recipient choices are inline in a VStack, not a floating overlay. D's mail results occupy the main workspace. | No overlap-based requirement. Consider opaque local backing for exact names/addresses and result passages if desktop contrast competes; avoid converting the entire workspace automatically. |
| Writing canvases and original HTML | Standard D composer CSS/native canvas and request/follow-up TextEditors already have opaque burgundy fills. Original HTML has an opaque white presentation backing. | Retain these opaque content areas. Do not recolor sender-authored HTML or serialize app appearance into writing. |
| Sign-in cancellation control | D's custom top overlay uses regularMaterial in a capsule. | Candidate for an opaque control backing: its action must stay identifiable over arbitrary content. |
| Interface A navigation preview | A uses regularMaterial plus an 82% color overlay, with an opaque Reduce Transparency fallback. | Candidate for the same always-opaque preview rule if the owner extends the convention to A. Left unchanged. B/C already satisfy the app-owned opaque-fill requirement. |

## Boundary and validation

Opaque means the content backdrop occludes everything beneath it; a high-opacity
wash or blur is not an equivalent guarantee. Decorative rims and the surrounding
window can remain glass. The existing opaque Reduce Transparency/Increased Contrast
fallback does not establish ordinary-mode opacity.

Any later changes to shared composer/EmailReader sheets should use explicit
per-presentation inputs with existing defaults preserved. D-local sheets can own
their backing directly. No global material, NSWindow alpha or A/B/C styling change
is needed for this audit's recommendations.

This slice uses the canonical signed Apple build and running navigation-preview
inspection, plus closed-app account/signing/preference preservation. No new test
cases or unrelated Rust/PostgreSQL suites are needed for this fill-only change.
Other candidates are source findings awaiting their own implementation and runtime
inspection, including native sheets/popovers and iOS/accessibility combinations.
