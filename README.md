# Avatar Workshop

Portable Godot 4 avatar runtime components, licensed under MIT OR Apache-2.0.
Copy `addons/avatar_workshop/` into a Godot 4.6 project. No editor plugin activation,
native library, network access, file loading or third-party dependency is required.

## Expression mixer

Preload `res://addons/avatar_workshop/expression_mixer.gd` and call
`mix(weights, bindings)`. Both arrays have one entry per expression channel.
Each channel's bindings contain `[target, shape_index, influence]` triples;
`target` is a caller-owned identifier (including a mesh instance).
The result maps `[target, shape_index]` to the accumulated weight, clamped to
0–1 after all contributions. Zero-weight channels still emit zero for their
bound targets so callers can reset the previous frame. Inputs remain unchanged.
The caller validates channel lengths and binding triples when loading assets.
The mixer does not access nodes; the caller applies returned weights.

This first runtime component supports existing eye-expression integration.
Unity-package conversion, complete VRChat behavior, skinning policy and shader
translation are future work; this is not a complete VRChat avatar importer.

## Tests

Run `godot4 --headless --path . --script tests/expressions.gd`.
The seven checks cover shared targets, separate meshes, neutral reset, empty
input, clamping and reusable binding data. No asset downloads are needed.

## License

Original code is available under either [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option. Third-party code, if added later,
must retain its original license notices. This revision contains none.
