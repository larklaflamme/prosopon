// Prosopon frontend — the avatar, driven by the Rust state machine via Tauri events.
// Uses the global `window.__TAURI__` (withGlobalTauri: true), so no build step.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const orb = document.getElementById("orb");
const orbStage = document.querySelector(".orb-stage");
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
const levelCanvas = document.getElementById("level-canvas");

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
  orbStage.dataset.state = s;
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
  if (!transcript) return;
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

  // Level meter: user (mic) + agent (playback) levels over time.
  // Two stacked channels, each a dB grid with a scrolling waveform line.
  const levelCtx = levelCanvas.getContext("2d");
  const LEVEL_HISTORY = 160;
  const userHistory = new Array(LEVEL_HISTORY).fill(0);
  const agentHistory = new Array(LEVEL_HISTORY).fill(0);
  const LEVEL_LOW = 0.005;   // below -> yellow (too quiet)
  const LEVEL_HIGH = 0.05;   // above -> red (too loud)
  const DB_TOP = 0;          // top of grid (0 dBFS)
  const DB_BOTTOM = -80;     // bottom of grid
  const LABEL_GUTTER = 40;   // left margin for dB labels

  function rmsToDb(v) {
    if (v <= 0) return DB_BOTTOM;
    const db = 20 * Math.log10(v);
    return Math.max(DB_BOTTOM, Math.min(DB_TOP, db));
  }

  function levelColor(v) {
    if (v < LEVEL_LOW) return "#eab308"; // yellow
    if (v > LEVEL_HIGH) return "#ef4444"; // red
    return "#22c55e"; // green (sweet spot)
  }

  function sizeLevelCanvas() {
    const dpr = window.devicePixelRatio || 1;
    const w = levelCanvas.clientWidth;
    const h = levelCanvas.clientHeight;
    levelCanvas.width = Math.max(1, Math.floor(w * dpr));
    levelCanvas.height = Math.max(1, Math.floor(h * dpr));
    levelCtx.setTransform(dpr, 0, 0, dpr, 0, 0);
  }

  function drawGrid(w, y0, stripH) {
    levelCtx.font = "9px sans-serif";
    levelCtx.textBaseline = "middle";
    for (let db = DB_TOP; db >= DB_BOTTOM; db -= 20) {
      const y = y0 + ((DB_TOP - db) / (DB_TOP - DB_BOTTOM)) * stripH;
      levelCtx.strokeStyle = "rgba(255,255,255,0.08)";
      levelCtx.beginPath();
      levelCtx.moveTo(LABEL_GUTTER, y);
      levelCtx.lineTo(w, y);
      levelCtx.stroke();
      levelCtx.fillStyle = "rgba(255,255,255,0.4)";
      levelCtx.fillText(db + " dB", 4, y);
    }
    for (let i = 1; i < 4; i++) {
      const x = LABEL_GUTTER + ((w - LABEL_GUTTER) / 4) * i;
      levelCtx.strokeStyle = "rgba(255,255,255,0.04)";
      levelCtx.beginPath();
      levelCtx.moveTo(x, y0);
      levelCtx.lineTo(x, y0 + stripH);
      levelCtx.stroke();
    }
  }

  function drawWaveform(history, y0, stripH, fillColor) {
    const w = levelCanvas.clientWidth;
    const n = history.length;
    const plotW = w - LABEL_GUTTER;
    const stepX = plotW / (n - 1);
    const pts = [];
    for (let i = 0; i < n; i++) {
      const db = rmsToDb(history[i]);
      const y = y0 + ((DB_TOP - db) / (DB_TOP - DB_BOTTOM)) * stripH;
      pts.push([LABEL_GUTTER + i * stepX, y]);
    }
    // filled area under the line
    levelCtx.beginPath();
    levelCtx.moveTo(pts[0][0], y0 + stripH);
    for (const p of pts) levelCtx.lineTo(p[0], p[1]);
    levelCtx.lineTo(pts[n - 1][0], y0 + stripH);
    levelCtx.closePath();
    levelCtx.fillStyle = fillColor + "22";
    levelCtx.fill();
    // the line itself, colored by zone per segment
    levelCtx.lineWidth = 1.5;
    for (let i = 1; i < n; i++) {
      levelCtx.strokeStyle = levelColor(history[i]);
      levelCtx.beginPath();
      levelCtx.moveTo(pts[i - 1][0], pts[i - 1][1]);
      levelCtx.lineTo(pts[i][0], pts[i][1]);
      levelCtx.stroke();
    }
  }

  function drawLevels() {
    const w = levelCanvas.clientWidth;
    const h = levelCanvas.clientHeight;
    levelCtx.clearRect(0, 0, w, h);
    const stripH = h / 2;
    drawGrid(w, 0, stripH);
    drawGrid(w, stripH, stripH);
    drawWaveform(userHistory, 0, stripH, "#5b8def");
    drawWaveform(agentHistory, stripH, stripH, "#14b8a6");
    // channel labels
    levelCtx.fillStyle = "rgba(255,255,255,0.65)";
    levelCtx.font = "10px sans-serif";
    levelCtx.textBaseline = "alphabetic";
    levelCtx.fillText("you", LABEL_GUTTER + 4, stripH - 4);
    levelCtx.fillText("sky", LABEL_GUTTER + 4, h - 4);
    // live dB readouts
    const userDb = rmsToDb(userHistory[userHistory.length - 1]);
    const agentDb = rmsToDb(agentHistory[agentHistory.length - 1]);
    levelCtx.fillStyle = "rgba(255,255,255,0.5)";
    levelCtx.font = "10px sans-serif";
    levelCtx.textAlign = "right";
    levelCtx.fillText(userDb.toFixed(1) + " dB", w - 4, stripH - 4);
    levelCtx.fillText(agentDb.toFixed(1) + " dB", w - 4, h - 4);
    levelCtx.textAlign = "left";
  }

  sizeLevelCanvas();
  window.addEventListener("resize", () => { sizeLevelCanvas(); drawLevels(); });

  await listen("levels", (event) => {
    const { user, agent } = event.payload || {};
    userHistory.push(typeof user === "number" ? user : 0);
    userHistory.shift();
    agentHistory.push(typeof agent === "number" ? agent : 0);
    agentHistory.shift();
    drawLevels();
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
    let names = null;
    const frames = [];
    for (const line of lines) {
      let obj;
      try {
        obj = JSON.parse(line);
      } catch (e) {
        continue;
      }
      if (obj.header && Array.isArray(obj.header.blendShapes)) {
        names = obj.header.blendShapes;
        console.log("[prosopon] blendshape header:", names.length, "shapes");
      } else if (obj.v && Array.isArray(obj.v) && names) {
        frames.push({ t: typeof obj.t === "number" ? obj.t : 0, values: obj.v });
      }
    }
    if (names && frames.length > 0) {
      window.avatar?.enqueueFrames(names, frames);
      console.log("[prosopon] blendshapes:", frames.length, "frames queued");
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
