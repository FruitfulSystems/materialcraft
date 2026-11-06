# Gaps

> **Last reviewed:** 2026-11-06 · **Last updated:** 2026-11-06 · **Change:** major (lights, shaders, material common values and view-state files closed most of the 3D view gap; node shortcuts closed part of the keymap gap) · **Target:** the professional node-based material workflow

Every known shortfall, most important first. Pick from the top.

| # | Gap | Evidence | Impact | Estimate | Parity doc |
|---:|---|---|---|---:|---|
| 1 | Node library breadth | 227 nodes vs. ~400 in the reference library | Recipes that need scan processing, more grunges, splines or SDF shapes | 30 h | target-app-parity (Node library) |
| 2 | GPU 3D view | CPU ray caster with lights, shaders and displacement (a tessellation factor scales the triangle budget) | Large meshes and high sample counts are slow; no path tracing | 12 h | target-app-parity (3D view) |
| 3 | Mesh-based generators and bakers | None | Workflows that start from a mesh | 15 h | target-app-parity (Node library) |
| 4 | Portable materials over MCP | `.mcmat` export runs from the app and the CLI; the MCP workspace sandbox refuses `file.*` commands and has no `material_export` tool | Sandboxed agents can't hand a material to TextureCraft | 1 h | target-app-parity (Files) |
| 5 | Function loops and reusable functions | Functions live on one parameter or processor | Iterative effects; sharing logic between nodes | 5 h | target-app-parity (Function graphs) |
| 6 | Input visibility conditions, per-instance output sizes | Not implemented | Organizing large graphs' inputs | 4 h | target-app-parity (Graph parameters) |
| 7 | `ui.screenshot` times out on this test machine | The window-capture callback never fires here; `view.screenshot` and the offscreen snapshot test work | Agents bridged to the app see views but not the whole window | 1 h | — |
| 8 | Translations and a menu keymap editor | English only; node creation shortcuts are editable, menu shortcuts are fixed | Non-English users; custom menu shortcuts | 7 h | localization-parity |
| 9 | Minimap and multi-graph tabs | One graph at a time, no overview | Navigating big graphs | 5 h | target-app-parity (Graph editing, UI and UX) |
| 10 | First signed release | Workflows copied, not run | Downloads | 3 h | target-app-parity (Platforms) |

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-06 | major | 3D view lights, shaders, common values, camera and renderer pages; node creation shortcuts; linked bitmaps and the dependency manager; plugins |
| 2026-11-02 | minor | Portable `.mcmat` materials for TextureCraft closed gap 4; what's left is an MCP tool |
| 2026-11-02 | major | Closed the function editor, dynamic parameters, FX-Map/Text/SVG, presets and groups, browser file I/O and accessible names; re-ranked |
| 2026-11-02 | major | First list |
