// Prosopon frontend — the avatar, driven by the Rust state machine via Tauri events.
// Uses the global `window.__TAURI__` (withGlobalTauri: true), so no build step.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const orb = document.getElementById("orb");
const stateLabel = document.getElementById("state-label");
const statusDot = document.getElementById("status-dot");
const muteBtn = document.getElementById("btn-mute");
const connectBtn = document.getElementById("btn-connect");
const transcript = document.getElementById("transcript");
const logsPanel = document.getElementById("logs");
const logsBody = document.getElementById("logs-body");
const logsBtn = document.getElementById("btn-logs");
const clearLogsBtn = document.getElementById("btn-clear-logs");
const avatarCanvas = document.getElementById("avatar-canvas");

const MAX_LOG_LINES = 500;

// The VRM face (avatar.js) is ready once the model has loaded.
let avatarReady = false;

// The ARKit blendshape names, from the track header (persists across events).
let blendShapeNames = null;

const STATE_COLORS = {
  disconnected: "#4a4a55",
  idle: "#5b8def",
  awaking: "#8ab4ff",
  listening: "#3b82f6",
  thinking: "#f59e0b",
  speaking: "#14b8a6",
};

function applyState(state) {
  const s = state.state;
  orb.dataset.state = s;
  stateLabel.textContent = s;
  statusDot.style.background = STATE_COLORS[s] || "#4a4a55";
  orb.classList.toggle("is-muted", state.muted);
  muteBtn.classList.toggle("muted", state.muted);
  updateConnectButton(state);
  updateAvatarVisibility();
}

function updateConnectButton(state) {
  const connected = state.state !== "disconnected";
  connectBtn.textContent = connected ? "Disconnect" : "Connect";
  connectBtn.classList.toggle("connected", connected);
}

// Show the VRM face when connected and loaded; otherwise show the robot.
function updateAvatarVisibility() {
  const connected = orb.dataset.state !== "disconnected";
  const showAvatar = connected && avatarReady;
  avatarCanvas.classList.toggle("visible", showAvatar);
  orb.classList.toggle("hidden", showAvatar);
}

function addLine(speaker, text) {
  const hint = transcript.querySelector(".empty-hint");
  if (hint) hint.remove();

  const line = document.createElement("div");
  line.className = "line";
  const who = document.createElement("span");
  who.className = "speaker " + speaker;
  who.textContent = speaker === "lark" ? "Lark" : "Skye";
  line.appendChild(who);
  line.appendChild(document.createTextNode(text));
  transcript.appendChild(line);
  transcript.scrollTop = transcript.scrollHeight;
}

function addLog(entry) {
  const line = document.createElement("div");
  line.className = "log-line log-" + (entry.level || "info");
  const src = document.createElement("span");
  src.className = "log-source";
  src.textContent = "[" + (entry.source || "?") + "]";
  line.appendChild(src);
  line.appendChild(document.createTextNode(entry.message || ""));
  logsBody.appendChild(line);
  while (logsBody.children.length > MAX_LOG_LINES) {
    logsBody.removeChild(logsBody.firstChild);
  }
  logsBody.scrollTop = logsBody.scrollHeight;
}

async function doConnect() {
  console.log("[prosopon] connect clicked");
  connectBtn.disabled = true;
  connectBtn.textContent = "Connecting…";
  try {
    await invoke("connect_webrtc");
    console.log("[prosopon] connect_webrtc returned OK");
  } catch (e) {
    console.error("[prosopon] connect_webrtc failed:", e);
    addLine("sky", "connect failed: " + e);
  } finally {
    connectBtn.disabled = false;
  }
}

async function doDisconnect() {
  console.log("[prosopon] disconnect clicked");
  try {
    const state = await invoke("disconnect");
    applyState(state);
    console.log("[prosopon] disconnect OK, state =", state.state);
  } catch (e) {
    console.error("[prosopon] disconnect failed:", e);
  }
}

async function init() {
  // Initial state from the Rust side.
  try {
    const state = await invoke("get_state");
    applyState(state);
    console.log("[prosopon] initial state:", state);
  } catch (e) {
    console.error("[prosopon] get_state failed:", e);
  }

  // Backfill buffered logs, then stream live ones.
  try {
    const existing = await invoke("get_logs");
    for (const entry of existing) addLog(entry);
  } catch (e) {
    console.error("[prosopon] get_logs failed:", e);
  }
  await listen("log", (event) => addLog(event.payload));

  // Live state changes.
  await listen("state", (event) => {
    console.log("[prosopon] state event:", event.payload);
    applyState(event.payload);
  });

  // The VRM face finished loading (avatar.js dispatches this).
  window.addEventListener("avatar-ready", () => {
    avatarReady = true;
    console.log("[prosopon] avatar ready");
    updateAvatarVisibility();
  });

  // Blendshape track (avatar animation). The payload is NDJSON:
  //   line 1: {"header":{"blendShapes":["EyeBlinkLeft", ...]}}
  //   then:   {"t": <time_code>, "v": [<weights...>]}
  // We parse the header for names, then feed each frame's weights to the VRM.
  await listen("blendshapes", (event) => {
    const track = event.payload || "";
    console.log("[prosopon] blendshapes event, payload length:", track.length);
    const lines = track.split("\n").filter((l) => l.trim().length > 0);
    let frameCount = 0;
    for (const line of lines) {
      let obj;
      try {
        obj = JSON.parse(line);
      } catch (e) {
        continue;
      }
      if (obj.header && Array.isArray(obj.header.blendShapes)) {
        blendShapeNames = obj.header.blendShapes;
        console.log("[prosopon] blendshape header:", blendShapeNames.length, "shapes");
      } else if (obj.v && Array.isArray(obj.v) && blendShapeNames) {
        window.avatar?.applyFrame(blendShapeNames, obj.v);
        frameCount++;
      }
    }
    if (frameCount > 0) {
      console.log("[prosopon] blendshapes:", frameCount, "frames");
    }
  });

  // Connect / disconnect toggle.
  connectBtn.addEventListener("click", () => {
    const connected = orb.dataset.state !== "disconnected";
    if (connected) doDisconnect();
    else doConnect();
  });

  // Mute toggle.
  muteBtn.addEventListener("click", async () => {
    const next = !muteBtn.classList.contains("muted");
    try {
      const state = await invoke("set_muted", { muted: next });
      applyState(state);
    } catch (e) {
      console.error("[prosopon] set_muted failed:", e);
    }
  });

  // Logs panel toggle.
  logsBtn.addEventListener("click", () => {
    logsPanel.hidden = !logsPanel.hidden;
    logsBtn.classList.toggle("active", !logsPanel.hidden);
  });

  // Clear logs.
  clearLogsBtn.addEventListener("click", () => {
    logsBody.innerHTML = "";
  });

  // Window controls (frameless).
  document.getElementById("btn-minimize").addEventListener("click", () => {
    window.__TAURI__.window.getCurrentWindow().minimize();
  });
  // Close = hide to tray (wake word stays armed).
  document.getElementById("btn-close").addEventListener("click", () => {
    window.__TAURI__.window.getCurrentWindow().hide();
  });
}

init();
