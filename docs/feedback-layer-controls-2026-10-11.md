# Facebook feedback on layer control visibility

Source: [Gilles Lecoq's post in Beam Bench Laser Software](https://www.facebook.com/groups/beambench/permalink/1512008437335375/),
opened from the supplied Facebook share link on 2026-10-11. The public post
displayed no comments. The original French refers to Output, Show and Air
Assist for a layer. Facebook's English translation mistranslates the layer
reference. The author asks for more noticeable icon colors and possible
grouping/alignment before turning on the laser, and reports no other problem.

This is a usability request. There is no reported connection failure,
incorrect output, or failure to save these settings. The post gives no app
version, theme, screenshot, machine model or multi-sub-layer configuration.
The findings below describe the current checkout, which matches the controls
named in the post. No application code or Facebook content was changed.

## Confirmed implementation

| Control | Location and behavior | Visible state |
| --- | --- | --- |
| Output | `LayerList.tsx:264`; changes the layer's `enabled` flag, which the planner uses to include/exclude the layer | Zap/ZapOff and shared active/inactive styling |
| Show | `LayerList.tsx:276`; changes layer canvas visibility. It does not disable layer output. Individually hidden objects are a separate job-filtering property. | Eye/EyeOff and shared active/inactive styling |
| Air Assist | `SubLayerStack.tsx:521`; changes the current cut entry's `air_assist` flag. Output generation selects matching cut-entry IDs and uses the machine profile's configured commands. | The Wind icon is identical in both states; only styling changes |

All three use `IconToggleButton.tsx`. Its buttons are 28 by 28 CSS pixels with
16-pixel icons. Active uses the theme accent, a 15% accent background and a 50%
accent border. Inactive uses muted text on the normal background. They have
accessible labels, `aria-pressed`, native button semantics and a focus ring.

Output and Show share the layer summary row. Air Assist appears below speed,
power and passes in the cut-entry editor. With multiple cut entries, only the
selected entry's editor is expanded. The three settings cannot all be scanned
as one group, and Air Assist for other entries requires selecting them.

There is no supported per-icon color preference. Display Settings has system,
light and dark appearance choices. The layer's Color picker changes its palette
color, not these control colors.

## Scoped technical audit

This is a source-based audit of these controls, not a score for the entire
application or a live desktop accessibility certification.

| Dimension | Score | Evidence |
| --- | --- | --- |
| Accessibility | 3/4 | Labels, pressed states and focus handling exist. Air Assist lacks a visible non-color state cue. The icon/background contrast passes the 3:1 non-text threshold in the calculated theme combinations. |
| Performance | 3/4 | Reuses the existing small SVG/button component with CSS color transitions; no new dependencies or continuous animation. Live rendering performance was not measured. |
| Responsive design | 2/4 | Compact desktop controls, but the summary row does not wrap and the Layers panel permits a 200-pixel floating width. Verify narrow widths and translated labels before adding a third item. |
| Theming | 4/4 | All toggle colors use existing theme tokens and switch between light and dark. Individual color customization is absent. |
| Anti-patterns | 4/4 | The control scope uses the established app vocabulary, simple icons and restrained state colors. No decorative UI pattern needs replacing. |
| Total | 16/20 | Existing controls are usable; state visibility and grouping deserve a focused improvement. |

Contrast was calculated from the committed CSS tokens and alpha compositing,
using the panel and alternate background as representative parents. The active
icon is approximately 7.42–8.06:1 in dark mode and 3.90–4.19:1 in light mode.
Inactive icons are approximately 7.25:1 and 6.00:1 respectively. The active
background differs from the surrounding surface by only about 1.23–1.34:1.
This supports the concern about weak visual emphasis without claiming that
the icon itself fails contrast. No screenshot from the author's machine was
available, and their exact background/theme is unknown.

## Prioritized findings and proposed release change

1. **P2: Air Assist is harder to recognize at a glance.** Its glyph and label
   stay the same across states. Add a visible On/Off state, a check/slash cue,
   and a stronger filled active treatment. Keep the accessible pressed state.
   A shared state treatment is preferable to adding separate user color pickers
   for every icon. Use existing theme tokens and check both themes.
2. **P2: Related checks are separated.** Put Output, Show and Air Assist in one
   aligned, labeled group for a normal layer with one cut entry. For multiple
   entries, expose Air Assist state beside each entry's selector or header.
   Preserve per-entry settings rather than replacing them with one layer flag.
3. **P2: Grouping could obscure different meanings.** Keep Show identified as
   canvas visibility and Output as job inclusion. Air Assist is a requested job
   setting, not confirmation that a physical pump is running. Preserve the
   machine profile's command behavior and the existing job-generation logic.

The smaller change is to improve shared on/off recognition and align the
single-entry controls, with compact state indicators for multiple entries.
Check 200-pixel floating panels, normal docked panels, wide layouts, French and
longer labels, keyboard operation, tool layers, and mixed Air Assist settings.
Do not add homing, pump commands, laser output or controller writes to a UI
styling change.

Suggested implementation order is `$impeccable colorize` for the state
treatment, `$impeccable layout` for grouping, then `$impeccable polish` and a
scoped re-audit. Future design work can optionally use `$impeccable init` to
record product/design context; its absence does not block this improvement.

## Verification

- `layerList.test.tsx`: 35 tests pass, including pressed-eye behavior,
  visibility commands, error handling and tool-layer control exclusions.
- `cutSettingsEditor.test.tsx` and `m4Workflow.test.tsx`: 36 tests pass, covering
  cut-entry editing, stacks and copied settings.
- `cargo test -p beambench-grbl air_assist_`: four tests pass, including
  per-entry emission, configured commands, dwell and turning air off before
  subsequent travel.

No laser was connected or operated. Existing unit tests establish the settings
and output behavior; they do not measure how quickly a person notices a state.
No reply, reaction or other communication was posted to Facebook.
