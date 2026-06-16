# Hey Verse — 3D asset pipeline

The game builds everything from primitives until real models exist, then
swaps them in **without code changes**. Drop files here and they're used:

| File | Replaces |
|---|---|
| `models/avatar.glb` | the primitive chibi robot (avatar.gd `_build_primitive`) |
| `models/world.glb` | the whole procedural world (home.gd `_build_world`) |

## Pipeline

Blender → export **glTF 2.0 (.glb)** → drop in `assets/models/` → Godot
auto-imports → toon materials applied.

## Export rules (Blender)

- Apply all transforms (Ctrl-A) before export; **1 unit = 1 metre**.
- Avatar: origin at the feet, facing **+Z**, ~1.8 m tall to match the camera.
- Use simple Principled materials with base color only — the game's look is
  flat toon; rough/metal maps are wasted bytes.
- Keep it phone-lite: avatar ≤ 8k tris, props ≤ 2k tris, world ≤ 60k tris
  total, textures ≤ 1024² (atlas where possible).
- Name avatar body meshes with a `Body` prefix if they should be tinted by
  the player's DID color (tinting hook lives in avatar.gd).

## Free prototyping content

CC0 packs (e.g. Kenney, Quaternius) are license-clean for placeholder props
while custom art is in progress. Keep everything shipped 100% original or CC0.
