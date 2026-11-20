# VRM fixtures

Public VRM files the importer's tests read, each unchanged from its source except where a conversion is named.

- `crimsom-0.x.vrm`: "Crimsom", avatar 001 of 100Avatars by Polygonal Mind, CC0
  (https://github.com/kourtincat/100Avatars, `100Avatars_001/100Avatars_001_Crimsom.vrm`).
- `crimsom-1.0.vrm`: the same model converted to VRM 1.0 with the VRM Add-on for Blender
  4.10.2 in Blender 5.0.1, CC0.
- `godette-0.x.vrm`: "Low-Poly Godette", original model by SirRichard94
  (https://github.com/SirRichard94/low-poly-godette), VRM adaptation by Lyuma, licensed
  under Creative Commons Attribution 3.0 Unported (CC BY 3.0,
  https://creativecommons.org/licenses/by/3.0/). From https://github.com/V-Sekai/godot-vrm,
  `vrm_samples/Godette_vrm_v4.vrm`.
- `twist-1.0.vrm`: "VRM1_Constraint_Twist_Sample" by pixiv Inc., licensed under the VRM Public
  License 1.0 (https://vrm.dev/licenses/1.0/; its VRM metadata allows redistribution and modified
  redistribution, credit unnecessary). From https://github.com/vrm-c/vrm-specification at commit
  `94e82dd346fa6cf0337c4421728640e5252dd38e`,
  `samples/VRM1_Constraint_Twist_Sample/vrm/VRM1_Constraint_Twist_Sample.vrm`.

`godette-0.x.springs.tsv` and `twist-1.0.springs.tsv` are `vrm-reference/run.sh` recordings of
three-vrm moving those files' spring bones.
