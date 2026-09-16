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

// Synthetic blink state. Audio2Face does not emit blink weights for our
// clips, so we drive a natural periodic blink ourselves. Humans blink every
// ~3-5s regardless of speech; this keeps the face from reading as dead.
const BLINK_DURATION = 0.28;    // seconds for a full close-open cycle
let blinkTimer = 2.0;           // seconds until the next blink starts
let blinkPhase = 0;             // 0..1 progress through the current blink
let blinking = false;

function nextBlinkDelay() {
  return 3.0 + Math.random() * 2.0; // 3-5s with jitter
}

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

  scene.add(new THREE.AmbientLight(0xffffff, 0.45));
  const key = new THREE.DirectionalLight(0xffffff, 0.8);
  key.position.set(1, 2, 1.5);
  scene.add(key);
  const rim = new THREE.DirectionalLight(0xffffff, 0.3);
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

    colorizeAvatar();
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

// Recolor the avatar's materials by name. VRoid models name their materials
// by part (hair, face, eye, lip, ...). We log every material name once so we
// can see exactly what this model exposes, then tint the ones we recognise.
function colorizeAvatar() {
  if (!vrm) return;
  const logged = new Set();
  const rules = [
    // order matters: first match wins
    { match: ['mouth'], color: 0xd32f2f },      // FaceMouth -> red lips
    { match: ['iris'], color: 0x3a6ea5 },       // EyeIris -> blue
    { match: ['hair'], color: 0x6b4a2f },       // Hair, HairBack -> warm brown
    { match: ['tops'], color: 0xd98ca0 },       // Tops_01_CLOTH -> dusty rose dress
    { match: ['bottoms'], color: 0xc97a90 },    // Bottoms_01_CLOTH -> deeper rose
    { match: ['shoes'], color: 0x4a4a5a },      // Shoes_01_CLOTH -> dark slate
    { match: ['body'], color: 0xf5c8a8 },       // Body_00_SKIN -> skin
    { match: ['skin'], color: 0xf5c8a8 },       // Face_00_SKIN -> skin
  ];
  vrm.scene.traverse((obj) => {
    if (!obj.isMesh) return;
    const mats = Array.isArray(obj.material) ? obj.material : [obj.material];
    for (const mat of mats) {
      if (!mat) continue;
      const name = (mat.name || '').toLowerCase();
      if (name.includes('outline')) continue; // keep black toon outline crisp
      if (!logged.has(name)) {
        logged.add(name);
        console.log('[avatar] material:', mat.name || '(unnamed)');
      }
      for (const rule of rules) {
        if (rule.match.some((m) => name.includes(m))) {
          if (mat.color) {
            mat.color.setHex(rule.color);
            mat.needsUpdate = true;
          }
          break;
        }
      }
    }
  });
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

// Idle motion state. The VRM has a humanoid rig (head, neck, chest, hips, ...).
// We drive a gentle procedural idle -- breathing + head sway + body bob -- so the
// avatar reads as alive even when it isn't speaking. Rotations are applied on the
// *normalized* bone nodes (delta from rest pose), so they compose cleanly with the
// blendshape-driven face and never accumulate.
let idleTime = 0;
let armAxisLogged = false;
let fingerDebugLogged = false;
// Angle (radians) to lower each upper arm from the T-pose rest. ~80deg so
// the arms hang naturally rather than rigidly straight down.
const ARM_DOWN = 1.4;
// Resting smile: a gentle, always-on corner-of-mouth lift so the face reads
// warm instead of blank. Applied as a FLOOR (never lowers A2F's own smile),
// so lip-sync and any future emotion signal can still push it higher.
const RESTING_SMILE = 0.22;
// Subtle shoulder + hand idle amplitudes (radians). Kept small so the motion
// reads as "settling/alive" rather than fidgety. All rotations are deltas from
// the rest pose (normalized bones), so they never accumulate or drift.
const SHOULDER_SWAY = 0.06;
const WRIST_SWAY = 0.12;
const FINGER_CURL = 0.15;

function updateIdleMotion(delta) {
  if (!vrm || !vrm.humanoid) return;
  idleTime += delta;
  const t = idleTime;

  // Breathing: subtle chest rotation (inhale = slight lean back).
  const chest = vrm.humanoid.getNormalizedBoneNode('chest');
  if (chest) {
    chest.rotation.x = Math.sin(t * 1.6) * 0.03;
  }

  // NOTE: body bob removed. The normalized rig's *position* is the bone's
  // actual local position (not a delta), so setting hips.position.y to a small
  // value dragged the whole model down ~1 unit out of view. If we want bob
  // back, we must ADD to the stored rest position, never SET it.

  // Head sway: slow yaw + a touch of roll, out of phase so it doesn't look
  // mechanical.
  const head = vrm.humanoid.getNormalizedBoneNode('head');
  if (head) {
    head.rotation.y = Math.sin(t * 0.5) * 0.06;          // slow yaw (turn)
    head.rotation.z = Math.sin(t * 0.31 + 1.3) * 0.03;   // roll (tilt)
    head.rotation.x = Math.sin(t * 0.23 + 0.8) * 0.04;   // pitch (gentle nod)
  }

  // Arms: lower from T-pose (out along X) to a natural resting pose, plus a
  // gentle sway. Rotation is around the upper arm's local Z axis; left and
  // right get opposite signs. The earlier "disappearing avatar" was NOT the
  // arms -- it was the hips.position.y bug (now removed). The arm rotation
  // itself was correct.
  try {
    const rightUpperArm = vrm.humanoid.getNormalizedBoneNode('rightUpperArm');
    if (rightUpperArm) {
      rightUpperArm.rotation.z = -ARM_DOWN + Math.sin(t * 0.4) * 0.04;
    }
    const leftUpperArm = vrm.humanoid.getNormalizedBoneNode('leftUpperArm');
    if (leftUpperArm) {
      leftUpperArm.rotation.z = ARM_DOWN + Math.sin(t * 0.4 + 1.7) * 0.04;
    }
  } catch (e) {
    console.error('[avatar] arm motion failed:', e);
  }

  // Shoulders: a barely-there settle/shrug so the torso doesn't read as rigid.
  // Rotation around the shoulder's local X (forward/back) + Z (up/down shrug).
  try {
    const rightShoulder = vrm.humanoid.getNormalizedBoneNode('rightShoulder');
    const leftShoulder = vrm.humanoid.getNormalizedBoneNode('leftShoulder');
    if (rightShoulder) {
      rightShoulder.rotation.x = Math.sin(t * 0.7) * SHOULDER_SWAY;
      rightShoulder.rotation.z = Math.sin(t * 0.5 + 1.0) * SHOULDER_SWAY * 0.7;
    }
    if (leftShoulder) {
      leftShoulder.rotation.x = Math.sin(t * 0.7 + 1.7) * SHOULDER_SWAY;
      leftShoulder.rotation.z = Math.sin(t * 0.5 + 2.4) * SHOULDER_SWAY * 0.7;
    }
  } catch (e) {
    console.error('[avatar] shoulder motion failed:', e);
  }

  // Hands: gentle wrist sway so the hanging hands don't look frozen.
  try {
    const rightHand = vrm.humanoid.getNormalizedBoneNode('rightHand');
    const leftHand = vrm.humanoid.getNormalizedBoneNode('leftHand');
    if (rightHand) {
      rightHand.rotation.z = Math.sin(t * 0.6 + 0.5) * WRIST_SWAY;
    }
    if (leftHand) {
      leftHand.rotation.z = Math.sin(t * 0.6 + 2.1) * WRIST_SWAY;
    }
  } catch (e) {
    console.error('[avatar] hand motion failed:', e);
  }

  // Fingers: a slow, gentle curl/uncurl (all fingers together) so the hands
  // breathe. Proximal + intermediate bones only; distal follows naturally.
  try {
    const fingerBones = [
      'leftThumbProximal', 'leftThumbIntermediate',
      'leftIndexProximal', 'leftIndexIntermediate',
      'leftMiddleProximal', 'leftMiddleIntermediate',
      'leftRingProximal', 'leftRingIntermediate',
      'leftLittleProximal', 'leftLittleIntermediate',
      'rightThumbProximal', 'rightThumbIntermediate',
      'rightIndexProximal', 'rightIndexIntermediate',
      'rightMiddleProximal', 'rightMiddleIntermediate',
      'rightRingProximal', 'rightRingIntermediate',
      'rightLittleProximal', 'rightLittleIntermediate',
    ];
    const curl = Math.sin(t * 0.8) * FINGER_CURL;
    let found = 0;
    for (const name of fingerBones) {
      const bone = vrm.humanoid.getNormalizedBoneNode(name);
      if (bone) { bone.rotation.x = curl; found += 1; }
    }
    if (!fingerDebugLogged) {
      fingerDebugLogged = true;
      console.log('[avatar] finger bones found:', found, 'of', fingerBones.length);
    }
  } catch (e) {
    console.error('[avatar] finger motion failed:', e);
  }

  // One-time debug: log the arm bones' world positions + rest rotations so we
  // can confirm the rotation axis is correct (arms should hang DOWN, not up).
  if (!armAxisLogged) {
    armAxisLogged = true;
    const r = vrm.humanoid.getNormalizedBoneNode('rightUpperArm');
    const l = vrm.humanoid.getNormalizedBoneNode('leftUpperArm');
    if (r && l) {
      console.log('[avatar] rightUpperArm world pos:', r.getWorldPosition(new THREE.Vector3()).toArray());
      console.log('[avatar] leftUpperArm  world pos:', l.getWorldPosition(new THREE.Vector3()).toArray());
      console.log('[avatar] rightUpperArm rest rot:', r.rotation.toArray());
      console.log('[avatar] leftUpperArm  rest rot:', l.rotation.toArray());
    }
  }
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

  // Synthetic blink, applied after the track so it wins over A2F's (zero)
  // blink weights.
  updateBlink(delta);

  // Resting smile floor (always-on warmth, never fights lip-sync).
  updateRestingSmile();

  // Gentle procedural idle motion (breathing + head sway + body bob).
  updateIdleMotion(delta);

  vrm.update(delta);
  renderer.render(scene, camera);
}

function updateRestingSmile() {
  if (!vrm || !ready) return;
  // Floor, not override: only lift the corners if A2F hasn't already.
  for (const name of ['MouthSmileLeft', 'MouthSmileRight']) {
    if (!validNames.has(name)) continue;
    const cur = vrm.expressionManager.getValue(name) || 0;
    if (cur < RESTING_SMILE) {
      vrm.expressionManager.setValue(name, RESTING_SMILE);
    }
  }
}

function updateBlink(delta) {
  if (!ready) return;
  if (blinking) {
    blinkPhase += delta / BLINK_DURATION;
    if (blinkPhase >= 1) {
      blinking = false;
      blinkPhase = 0;
      blinkTimer = nextBlinkDelay();
      setBlink(0);
    } else {
      // triangle wave: 0 -> 1 (closed) -> 0 (open)
      const w = blinkPhase < 0.5 ? blinkPhase * 2 : (1 - blinkPhase) * 2;
      setBlink(w);
    }
  } else {
    blinkTimer -= delta;
    if (blinkTimer <= 0) {
      blinking = true;
      blinkPhase = 0;
    }
  }
}

function setBlink(w) {
  if (!vrm) return;
  if (validNames.has('EyeBlinkLeft')) {
    vrm.expressionManager.setValue('EyeBlinkLeft', w);
    appliedNames.add('EyeBlinkLeft');
  }
  if (validNames.has('EyeBlinkRight')) {
    vrm.expressionManager.setValue('EyeBlinkRight', w);
    appliedNames.add('EyeBlinkRight');
  }
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
