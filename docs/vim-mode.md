# Vim mode

Sonora's Vim mode is a keyboard-navigation option, not a Vim text editor. It is tracked by
[issue #263](https://github.com/nolight132/sonora/issues/263), under the global shortcut work in
[issue #10](https://github.com/nolight132/sonora/issues/10).

Enable it in **Settings > General > Vim mode**. The setting is saved with the rest of Sonora's
preferences and is disabled by default.

## Current keys

| Key | Action |
| --- | --- |
| `j` | Select the next item in the active surface; otherwise scroll down |
| `k` | Select the previous item in the active surface; otherwise scroll up |
| `Ctrl-E` / `Ctrl-Y` | Move the selection down/up one row and reveal it |
| `Ctrl-D` / `Ctrl-U` | Move the selection down/up roughly half a viewport and reveal it |
| `Enter` | Enter a table/card surface; activate the selected item on the next press |
| `Ctrl-W h/j/k/l` | Move focus to the nearest visible pane in that direction |
| `h` | Scroll the active horizontal shelf left |
| `l` | Scroll the active horizontal shelf right |
| `gg` | Select and reveal the first item in the focused surface |
| `G` | Select and reveal the last item in the focused surface |
| `/` on Home | Open and focus global Search |
| `/` on a filtered screen | Focus the current screen's local filter |
| `/` on a screen without a filter | Do nothing |

`Alt-Left` and `Alt-Right` remain history navigation. `Enter`, `Escape`, `f`, Space, arrows and
Tab keep their existing meanings. If there is no matching scroll target, a motion does nothing.
On a page, `j/k` select the first/last item in the active table or card surface immediately.
Selection and viewport stay synchronized: selection uses minimal edge scrolling rather than centering,
and `gg/G` move both to the corresponding boundary. Table listings and queue rows reconcile a selected
item after native scrolling when it leaves the viewport; sidebar rows have no stable item geometry and
remain scroll-only for pointer scrolling. Virtualized shelves reveal their section first, then the
selected card after the rail mounts; pages without a selectable surface remain scroll-only.

In Search, press `Escape` after typing to move from the query input to the results. Then `j/k`
move through results, `h/l` switch result columns, and `Enter` plays a song or opens its artist,
album, or playlist. `Escape` on a selected result returns to the input.

In the left sidebar, clicking a row focuses the sidebar and selects that row. `j/k` traverse the
visible entries, including children of expanded Library, Local Music, and Settings groups. `Enter`
expands or collapses a group, or navigates a selected child. Navigating a child returns focus to the
content pane; use `Ctrl-W h` to return to the sidebar.

For horizontal shelves, the last shelf interacted with wins. Before any shelf interaction, the
first visible shelf is used. Text inputs, menus, forms and dialogs keep their existing keyboard
behavior. Pane switching currently targets the content, left sidebar, and right queue/lyrics panel;
the player bar is not a focus pane yet. Queue rows and sidebar entries support selection and Enter;
lyrics remain scroll-only.

## Focus boundaries

Vim bindings are active only inside Sonora's browsing shells. They are excluded while a text
`Input`, menu, form, or dialog has focus; Search's query input is excluded until `Escape` moves
focus to its results. A surface is selected only in the active content pane; otherwise `j/k` scroll
the page.

## Follow-up ideas

- Add viewport-to-cursor reconciliation for sidebar rows if stable item geometry becomes available.
- Add `[` and `]` for previous/next page landmarks or sections.
- Add `Ctrl-B` and `Ctrl-F` for full-page scrolling.
- Add `zt`, `zz` and `zb` for positioning a selected table row.
- Define normal/insert modes before changing text-editing behavior.
- Add scoped hint labels for visible actions without conflicting with fullscreen.
- Revisit more motions such as `0`, `$`, `H`, `M` and `L` once page landmarks are defined.
