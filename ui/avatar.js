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

// Playback state. The blendshape track arrives as a single batch of frames,
// each with a time_code (seconds). We buffer them and play them back over
// time in the render loop, so the face animates instead of freezing on the
// final frame.
let blendShapeNames = null;     // ARKit names from the track header
let frameQueue = [];            // [{t, values}] pending playback
let playbackStart = 0;          // performance.now() ms when playback began
let playing = false;

async function init(canvasEl) {
  canvas = canvasEl;

  renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;

  scene = new THREE.Scene();

  // Placeholder framing; frameHead() repositions this after the model loads.
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

  // Keep the drawing buffer synced with the canvas's real display size.
  // The canvas starts display:none, so a one-shot resize() reads 0×0 and
  // falls back to the default 300×150 buffer. A ResizeObserver fires the
  // moment the canvas becomes visible with its true size.
  const resizeObserver = new ResizeObserver((entries) => {
    for (const entry of entries) {
      const w = entry.contentRect.width;
      const h = entry.contentRect.height;
      if (w > 0 && h > 0) {
        renderer.setSize(w, h, false);
        camera.aspect = w / h;
        camera.updateProjectionMatrix();
      }
    }
  });
  resizeObserver.observe(canvas);

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

    frameHead();

    ready = true;
    console.log('[avatar] VRM loaded,', validNames.size, 'expressions');
    window.dispatchEvent(new CustomEvent('avatar-ready'));
  } catch (e) {
    loadError = e;
    console.error('[avatar] VRM load failed:', e);
  }

  animate();
}

// Auto-frame the camera on the model's head, so we don't guess coordinates.
function frameHead() {
  const box = new THREE.Box3().setFromObject(vrm.scene);
  const size = box.getSize(new THREE.Vector3());
  const center = box.getCenter(new THREE.Vector3());

  // The head is the top ~30% of the model.
  const headCenter = new THREE.Vector3(
    center.x,
    center.y + size.y * 0.35,
    center.z
  );
  const headHeight = Math.max(size.y * 0.3, 0.2);

  const fov = THREE.MathUtils.degToRad(camera.fov);
  const dist = (headHeight / 2) / Math.tan(fov / 2);
  camera.position.set(headCenter.x, headCenter.y, headCenter.z + dist);
  camera.lookAt(headCenter);

  console.log(
    '[avatar] framed head: center',
    headCenter.toArray().map((v) => v.toFixed(2)),
    'dist', dist.toFixed(2)
  );
}

function animate() {
  requestAnimationFrame(animate);
  if (!vrm) return;
  const delta = clock.getDelta();

  // Play back the buffered track over time. Each frame carries a time_code
  // (seconds); we show the pose whose time_code is closest to the elapsed
  // playback time, so the face animates in sync with the audio instead of
  // snapping to the final frame.
  if (playing && frameQueue.length > 0) {
    const elapsed = (performance.now() - playbackStart) / 1000;
    let current = null;
    for (const f of frameQueue) {
      if (f.t <= elapsed) current = f;
      else break;
    }
    if (current) applyValues(current.values);
    if (elapsed >= frameQueue[frameQueue.length - 1].t) {
      playing = false;
      frameQueue = [];
      reset();
    }
  }

  vrm.update(delta);
  renderer.render(scene, camera);
}

function applyValues(values) {
  if (!vrm || !ready || !blendShapeNames) return;
  const n = Math.min(blendShapeNames.length, values.length);
  for (let i = 0; i < n; i++) {
    const name = blendShapeNames[i];
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

// Buffer a full track (names + frames) and start playback.
function enqueueFrames(names, frames) {
  if (!frames || frames.length === 0) return;
  blendShapeNames = names;
  frameQueue = frames;
  playbackStart = performance.now();
  playing = true;
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
  enqueueFrames,
  reset,
  get ready() { return ready; },
  get error() { return loadError; },
};

// Kick off rendering. Module scripts run after the DOM is parsed, so the
// canvas exists here. This is what actually starts the renderer.
const avatarCanvasEl = document.getElementById('avatar-canvas');
if (avatarCanvasEl) {
  init(avatarCanvasEl);
} else {
  console.error('[avatar] canvas #avatar-canvas not found');
}
