# Motion

Everything that moves answers an action and takes 120 to 200 ms: the active section's line glides (`.tab-ind`: transform and width, 180 ms), a view fades in with a 4 px rise (`view-in`, 150 ms), a popover scales in (`pop-in`, 120 ms), the dock opens by height (`dock-open`, 160 ms), a toast comes in (`toast-in`, 160 ms) and goes out the way it came (`.toast.leaving`: it fades and folds its height away over 160 ms, so the others do not jump), rows ease on hover (120 ms). With `prefers-reduced-motion` none of it runs (durations are forced to 0.001 ms) and a toast leaves at once.

## Drive

`scripts/editor-verify drive motion` (`two_boards`). It asks the page which animations are running just after each action (`document.getAnimations()`), and checks their names and durations; the toast's exit; that an even row of the BOM changes on hover; and, with the media feature emulated, that no animation longer than 1 ms runs.

## Real evidence of smoothness (not in the recipe)

Freeze the page's animations with the DevTools protocol, then step them: `Animation.enable`, `Animation.setPlaybackRate 0` before the action, collect `Animation.animationStarted`, and `Animation.seekAnimations` to 12 times over 200 ms, taking a clipped screenshot at each; the frames go into one montage. To see a flash or a jump at the real frame rate, sample a computed style in `requestAnimationFrame` for 450 ms after the action. Round 3's montages and samples were taken this way: the line moves from tab to tab in the frames; a view's opacity runs 0.00, 0.20, 0.55, 0.73, 0.84, 0.91, 0.95, 0.98, 1.00 with no frame at full opacity before it; the dock's height runs 2, 49, 134, 179, 204, 220, 230, 236, 239, 240 px; a toast's opacity and max-height fall together in about 200 ms.

## Gotchas

- A CSS animation overrides a transition of the same property: a toast that is dismissed in the first 160 ms fades by its animation's end, not by the exit's transition. A recipe waits 300 ms before it dismisses.
- The dock opens by animating height, which reflows the view above it on every frame. It is smooth but not free; a view with a heavy canvas should not do it for long.
- A skeleton that shimmers in a section that is not on show is stopped (`.section.away .sk`).
