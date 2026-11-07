# UI parity drop (re-based on main after the primitive rewrite)

What changed, per area of the reference screenshots. Everything is wired to the engine; no empty settings.

## Re-port onto main (2026-11-06)
- Built on `origin/main` (1a468e7, PR #125). Everything from the 123-improving-ui drop was re-applied in the new
  layout: one type per file, new engine commands in their own module (`engine/src/resource_cmds.rs`, registered by
  one line in `commands.rs`), 3D view settings in `engine/src/view3d_settings.rs`, the 3D view split into
  `ui-egui/src/view3d/{actions,menus,overlays,settings,side_strip}.rs`, the explorer actions in `explorer/{actions,package_menu}.rs`,
  the new-graph dialog in `new_graph_dialog.rs`, graph toolbar tools in `graph/toolbar_tools.rs`, auto conversion in `graph/auto_convert.rs`.
- Upstream features kept as they are: the material-preview assignment (View in 3D view, graph drop onto the 3D view,
  `view.3d.outputs`, `view.3d.clear`), the Material assignment list now lives at the bottom of the 3D view's Materials menu;
  the function and FX-Map pattern editors; `snapToGrid` from `prefs.snap_to_grid`.
- `.mcraft` is gone upstream, so everything here is SBS only: linked bitmaps are written as `<filepath>` without `binembedded`,
  plugins load `.sbs` packages, the Plugin Manager installs `.sbs` files.
- Library: the reference's generator/filter groups are no longer Rust nodes ("SBS library nodes belong in SBS graphs"), so
  the library shows the 26 atomic nodes, graph items, the 85 function ops (added through the function editor), the package's
  graphs by kind, and the graphs of loaded plugin packages filed under each graph's `attributes.category` (else "PBR materials ▸ Plugins").
  The remaining reference groups stay listed, greyed, with a 0 count.

## Menu pass (2026-11-06, late)
- Library: the "SDF functions" group is now "MCF functions"; empty reference groups explain on hover that library
  packages (Tools ▸ Plugin Manager) fill them.
- Help menu points at getartcraft.com: Documentation/website → the MaterialCraft app page, Tutorials → /tutorials,
  What's new → /news, System requirements → /faq, Support and feedback → /support, Discord; release notes and bug
  reports stay on GitHub. Home and About carry the same links.
- Home screen: Samples shows the starter templates and the graphs of loaded library packages (click imports the graph).
- Graph toolbar quick-insert row now has every atomic node (added SVG, Text, Gradient Dynamic, Motion Blur,
  Value Processor, Input Color, Input Value).

## Menus (File / Edit / Tools / Windows / Help)
- File: New ▸ Graph / Empty package, Open, Recent, Save, Save as, Save a copy as, Export with dependencies, Import bitmap, Link bitmap, Export outputs, Export material, Send to TextureCraft, Close, Quit.
- Tools: Switch engine (F9: CPU thread count, rayon pool), Cancel rendering, Suspend engine, Clear cache, Reload resources, Plugin Manager.
- Windows: panels, Main toolbar toggle, Dependency Manager, maximize/unmaximize (Shift+Space), reset layout.
- Help: docs, tutorials, release notes, requirements, automation, feedback, bug report, export log, about. No vendor names, no Partners.
- Status bar: "MC Engine: CPU (n threads)".

## Explorer
- Package context menu: New ▸, Import ▸ (Bitmap, Graph from a package), Link ▸ Bitmap, Reload, Save/Save as/Save a copy as, Export with dependencies, Send to ▸ TextureCraft, Copy/Paste (graphs), Close package, Compute Outputs, Show in Explorer, Dependency Manager.
- Graph context menu: Open (Return), Copy, Paste, Remove (Del), Rename (F2, in place), View Outputs In 3D View, Compute Outputs, Export outputs.
- Resources: Relink, Add a Bitmap node, Remove. Linked bitmaps show a link icon.
- Default graph name is Material_Graph.
- Not done: "Open dependencies" (we have no package-to-package links to open).

## Library
- Tree: All nodes, Favorites (persisted, right-click a tile), Graph items (Comment, Frame, Pin + inputs/outputs/dot), Atomic nodes, FxMap nodes, Function nodes ▸ (groups), Texture generators ▸, Filters ▸, Material filters ▸ (ours + planned empty groups greyed with 0), Mesh-based generators, Spline & Path tools, SDF functions (planned, greyed), Functions (function graphs of the package), PBR materials ▸ This package / Plugins, Values ▸ Constant.
- Collapsible groups with counts, search, icon/list view, tile size slider.
- Function nodes drop into the open function editor; plugin graphs are copied into the package on drop (file.importGraph).

## New graph dialog
- Category dropdown, group headings, ⓘ template info, width/height with a link toggle, "Create graph in".

## Graph toolbar
- Row 1: frame all, 1:1, screenshot (PNG), graph info popup, find node, thumbnails, cook times, straight/curved wires, grid, snap, compute, clean up; right: Stack checkbox + align buttons, Parent size with link and reset.
- Row 2: atomic quick-insert, comment, dot, frame, pin.

## 3D view
- Menus: Scene (11 scenes incl. Plane hi-res, Cloth, Rounded cylinder, Sphere 2 tiles; Display UVs in 2D View; Load/Save state file; Save as default; Reset), Materials ▸ Default (Edit properties, 9 shaders, reset common/material, create graph), Lights, Camera (Frame F, viewpoints, Save Render Alt+S, Copy Render, Reset Position R), Environment, Display (wireframe, axis, backface culling, bounding box, grid, light gizmos, resolution/stats/FPS overlays), Renderer (Edit settings).
- "Edit properties" pages in the Properties panel: Camera (focal length, projection, background type/colour, render resolution, clip planes), Lights (ambient + 2 point lights with colour strips), Environment (enabled, exposure, rotation dial/turns/degrees, environment preview), Renderer (resolution, scale, samples, AO rays), Material (OpenPBR sections: UVs, Base, Specular, Transmission, Subsurface, Coat, Fuzz, Emission, Thin-film, Geometry, Height; each with the "use map" checkbox, sRGB/Float/HSV colour strips).
- Left tool strip: frame, property pages, display toggles.
- Renderer: point lights with shadows, ambient, env on/off, 9 shading models, common values, wireframe, backface culling, clip planes, background colour/environment, FOV/ortho, tessellation factor.
- Engine: `view.3d` takes nested objects (camera/lights/renderer/display/material), `view.3d.reset {what}`, `view.3d.state`, `view.3d.loadState`.

## Preferences (Edit ▸ Preferences)
- Tabs General / Projects / Graph / 3D View / Bakers / Library / Shortcuts with OK / Cancel / Apply.
- General: recent count, history size (session.settings), language (English for now), appearance, invert zoom, save/export path mode + fixed folder, temp (backup) folder, image cache budget, cook threads, auto backup interval + versions (rotating files), cooking size limit (caps previews and exports).
- Projects: plugins folder, backups folder, default 3D state (save/clear), default lighting state, library view, preview size, 3D pan limit/speed.
- Graph: Tab menu, drag-create, 3D on open, thumbnails, 2D on open, auto display new node, auto colour/grayscale conversion node, grid/snap/wires/timings.
- 3D View: environment hidden by default, viewport scaling, sample count.
- Bakers: baking threads. Library: rebuild thumbnails, clear cache. Shortcuts: node creation shortcuts (click a field, press keys).
- Skipped on purpose: UDIM, .sbsar publishing.

## Plugin Manager (Tools ▸ Plugin Manager)
- Filter, Plugins/Loaded table, Browse (opens the plugins folder), Install (copies .sbs files in), Refresh.
- A plugin = a package file or a folder with package files + optional plugin.json {name, version, author, description} in Documents/MaterialCraft/plugins. Loaded plugins' graphs show in the library under PBR materials ▸ Plugins.

## Engine commands added
file.linkBitmap, file.reloadResources, file.deleteResource, file.importGraph, file.packageGraphs, file.saveCopy, file.exportWithDependencies, graph.add, session.settings, view.3d.reset {what}, view.3d.state, view.3d.loadState; `view.3d` nested objects. Resources can be linked (SBS keeps relative links).

## Checks run here
cargo fmt --all, cargo check --workspace --all-targets, cargo clippy --workspace --all-targets -- -D warnings, cargo xtask layers,
cargo test --workspace --release --no-fail-fast. `cargo xtask wasm` could not run in this container (the wasm target cannot be
downloaded here); please run it on your side.
