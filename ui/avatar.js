// avatar.js — three.js VRM face renderer for Prosopon.
// Loads the VRoid VRM and drives its ARKit blendshapes from the Audio2Face track.
//
// The track arrives via the Tauri "blendshapes" event as NDJSON:
//   line 1: {"header":{"blendShapes":["EyeBlinkLeft", ...]}}
//   then:   {"t": <time_code>, "v": [<weights...>]}
// The ARKit names map 1:1 to the VRM's blendShapeGroup names.

import * as THREE from 'three';
import { GLTFLoader } from 'three/addons/loaders/GLTFLoader.js';
import { VRMLoaderPlugin, VRMUtils } from '@pixiv/three-vrm';

let vrm = null;
let renderer = null;
let scene = null;
let camera = null;
let clock = null;
let canvas = null;
let ready = false;
let loadError = null;

const validNames = new Set();   // expression names present in the VRM
const appliedNames = new Set(); // names we've driven, for reset
const warnedNames = new Set();  // names that didn't match, warned once

async function init(canvasEl) {
  canvas = canvasEl;

  renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;

  scene = new THREE.Scene();

  // Frame the head. VRM is in meters; the head sits around y=1.5.
  camera = new THREE.PerspectiveCamera(30, 1, 0.01, 20);
  camera.position.set(0, 1.5, 0.55);
  camera.lookAt(0, 1.5, 0);

  scene.add(new THREE.AmbientLight(0xffffff, 0.9));
  const key = new THREE.DirectionalLight(0xffffff, 1.4);
  key.position.set(1, 2, 1.5);
  scene.add(key);
  const rim = new THREE.DirectionalLight(0xffffff, 0.5);
  rim.position.set(-1, 1, -1);
  scene.add(rim);

  clock = new THREE.Clock();

  resize();
  window.addEventListener('resize', resize);

  const loader = new GLTFLoader();
  loader.register((parser) => new VRMLoaderPlugin(parser));
  try {
    const gltf = await loader.loadAsync('models/VRoid_V110_Female_v1.1.3.vrm');
    vrm = gltf.userData.vrm;
    VRMUtils.rotateVRM0(vrm);
    scene.add(vrm.scene);
    for (const name of Object.keys(vrm.expressionManager.expressionMap)) {
      validNames.add(name);
    }
    ready = true;
    console.log('[avatar] VRM loaded,', validNames.size, 'expressions');
    window.dispatchEvent(new CustomEvent('avatar-ready'));
  } catch (e) {
    loadError = e;
    console.error('[avatar] VRM load failed:', e);
  }

  animate();
}

function resize() {
  if (!canvas || !renderer) return;
  const w = canvas.clientWidth || canvas.width;
  const h = canvas.clientHeight || canvas.height;
  renderer.setSize(w, h, false);
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
}

function animate() {
  requestAnimationFrame(animate);
  if (!vrm) return;
  const delta = clock.getDelta();
  vrm.update(delta);
  renderer.render(scene, camera);
}

function applyFrame(names, values) {
  if (!vrm || !ready) return;
  const n = Math.min(names.length, values.length);
  for (let i = 0; i < n; i++) {
    const name = names[i];
    const w = Math.max(0, Math.min(1, values[i]));
    if (validNames.has(name)) {
      vrm.expressionManager.setValue(name, w);
      appliedNames.add(name);
    } else if (!warnedNames.has(name)) {
      warnedNames.add(name);
      console.warn('[avatar] no expression for blendshape:', name);
    }
  }
}

function reset() {
  if (!vrm) return;
  for (const name of appliedNames) {
    vrm.expressionManager.setValue(name, 0);
  }
  appliedNames.clear();
}

window.avatar = {
  init,
  applyFrame,
  reset,
  get ready() { return ready; },
  get error() { return loadError; },
};
