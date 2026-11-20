// Records how three-vrm moves a VRM file's spring bones under a scripted motion of the avatar and its head.
//
// usage: node springs.mjs AVATAR.vrm > AVATAR.springs.tsv
//
// The avatar is placed facing +Z, a VRM 0.x one turned about Y as three-vrm's VRMUtils.rotateVRM0 turns it, and
// updated at 60 Hz by three-vrm, each joint's length taken to its child's current place. Rows, tab-separated:
//   tip    NODE LENGTH            a spring tip and the rest length of its path from its spring's root
//   frame  F  PX PY PZ  QX QY QZ QW  HX HY HZ HW
//                                  the avatar root in the world and the head's local rotation that frame
//   at     F  NODE  X Y Z         a tip's position in the avatar root's space after that frame's update
import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { VRMSpringBoneLoaderPlugin } from "@pixiv/three-vrm-springbone";
import { readFileSync } from "node:fs";

const RATE = 60;
const REST = 120;
const MOTION = 360;
const SETTLE = 180;

function glb(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const length = view.getUint32(12, true);
  const json = JSON.parse(new TextDecoder().decode(bytes.subarray(20, 20 + length)));
  const rest = bytes.subarray(20 + length);
  return { json, rest };
}

// Images need a browser to decode and the springs do not read materials, so both are left out.
function withoutMaterials(bytes) {
  const { json, rest } = glb(bytes);
  delete json.images;
  delete json.textures;
  delete json.samplers;
  delete json.extensionsRequired;
  if (json.materials) json.materials = json.materials.map(() => ({}));
  if (json.extensions?.VRM) delete json.extensions.VRM.materialProperties;
  let text = new TextEncoder().encode(JSON.stringify(json));
  const padded = new Uint8Array(Math.ceil(text.length / 4) * 4).fill(0x20);
  padded.set(text);
  const out = new Uint8Array(20 + padded.length + rest.length);
  const view = new DataView(out.buffer);
  out.set(bytes.subarray(0, 12));
  view.setUint32(8, out.length, true);
  view.setUint32(12, padded.length, true);
  out.set(bytes.subarray(16, 20), 16);
  out.set(padded, 20);
  out.set(rest, 20 + padded.length);
  return out;
}

function motion(f) {
  const t = Math.max(0, Math.min(f - REST, MOTION)) / RATE;
  const on = f > REST && f < REST + MOTION ? 1 : 0;
  const position = new THREE.Vector3(0.35 * Math.sin(2.1 * t) * on, 0.06 * Math.sin(5.3 * t) * on, 0.25 * Math.sin(1.4 * t + 0.5) * on - 0.25 * Math.sin(0.5) * on);
  const yaw = 0.7 * Math.sin(1.7 * t) * on;
  const nod = 0.35 * Math.sin(3.1 * t) * on;
  const shake = 0.45 * Math.sin(2.3 * t + 1.0) * on;
  return { position, rotation: new THREE.Quaternion().setFromEuler(new THREE.Euler(0, yaw, 0)), head: new THREE.Quaternion().setFromEuler(new THREE.Euler(nod, shake, 0, "XYZ")) };
}

function headNode(json) {
  const v1 = json.extensions?.VRMC_vrm?.humanoid?.humanBones?.head?.node;
  if (v1 != null) return v1;
  return json.extensions?.VRM?.humanoid?.humanBones?.find((b) => b.bone === "head")?.node;
}

function tips(json) {
  const children = (i) => json.nodes[i].children ?? [];
  const out = [];
  const springs = json.extensions?.VRMC_springBone?.springs;
  if (springs) {
    for (const s of springs) if (s.joints.length > 1) out.push({ root: s.joints[0].node, tip: s.joints[s.joints.length - 1].node, path: s.joints.map((j) => j.node) });
    return out;
  }
  for (const group of json.extensions?.VRM?.secondaryAnimation?.boneGroups ?? []) {
    for (const root of group.bones ?? []) {
      const walk = (n, path) => {
        if (children(n).length === 0) {
          if (path.length > 1) out.push({ root, tip: n, path });
          return;
        }
        for (const c of children(n)) walk(c, [...path, c]);
      };
      walk(root, [root]);
    }
  }
  return out;
}

const bytes = new Uint8Array(readFileSync(process.argv[2]));
const { json } = glb(bytes);
const v0 = !json.extensions?.VRMC_vrm;
const loader = new GLTFLoader();
loader.register((parser) => new VRMSpringBoneLoaderPlugin(parser));
const buffer = withoutMaterials(bytes);
const gltf = await loader.parseAsync(buffer.buffer.slice(buffer.byteOffset, buffer.byteOffset + buffer.byteLength), "");
const manager = gltf.userData.vrmSpringBoneManager;
// A joint measures its length to its child's world matrix, which three-vrm refreshes only after the joint's own
// update, so below a moving joint the length is taken against the child's last place. The VRM spring bone keeps
// its bone's length; bringing the child current first keeps it.
for (const joint of manager?.joints ?? []) {
  const update = joint.update.bind(joint);
  joint.update = (delta) => {
    joint.child?.updateWorldMatrix(false, false);
    update(delta);
  };
}
const nodes = await gltf.parser.getDependencies("node");
const scene = gltf.scene;
const facing = new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), v0 ? Math.PI : 0);
const head = headNode(json);
const headRest = head != null ? nodes[head].quaternion.clone() : null;
const chosen = tips(json);

scene.quaternion.copy(facing);
scene.updateMatrixWorld(true);
manager?.setInitState();

const lines = [];
const world = (n) => new THREE.Vector3().setFromMatrixPosition(nodes[n].matrixWorld);
for (const c of chosen) {
  let length = 0;
  for (let k = 1; k < c.path.length; k++) length += world(c.path[k]).distanceTo(world(c.path[k - 1]));
  lines.push(["tip", c.tip, length.toPrecision(9)].join("\t"));
}
const fmt = (x) => x.toPrecision(9);
for (let f = 0; f < REST + MOTION + SETTLE; f++) {
  const m = motion(f);
  scene.position.copy(m.position);
  scene.quaternion.copy(m.rotation).multiply(facing);
  let h = [0, 0, 0, 1];
  if (head != null) {
    nodes[head].quaternion.copy(headRest).multiply(m.head);
    h = nodes[head].quaternion.toArray();
  }
  scene.updateMatrixWorld(true);
  if (f > 0) manager?.update(1 / RATE);
  scene.updateMatrixWorld(true);
  const r = m.rotation.toArray();
  lines.push(["frame", f, ...m.position.toArray().map(fmt), ...r.map(fmt), ...h.map(fmt)].join("\t"));
  const inverse = new THREE.Matrix4().compose(m.position, m.rotation, new THREE.Vector3(1, 1, 1)).invert();
  for (const c of chosen) {
    const p = world(c.tip).applyMatrix4(inverse);
    lines.push(["at", f, c.tip, fmt(p.x), fmt(p.y), fmt(p.z)].join("\t"));
  }
}
process.stdout.write(lines.join("\n") + "\n");
