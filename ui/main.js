// Prosopon frontend — the orb, driven by the Rust state machine via Tauri events.
// Uses the global `window.__TAURI__` (withGlobalTauri: true), so no build step.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const orb = document.getElementById("orb");
const stateLabel = document.getElementById("state-label");
const statusDot = document.getElementById("status-dot");
const muteBtn = document.getElementById("btn-mute");
const transcript = document.getElementById("transcript");

const STATE_COLORS = {
  disconnected: "#4a4a55",
  idle: "#5b8def",
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

async function init() {
  // Initial state from the Rust side.
  try {
    const state = await invoke("get_state");
    applyState(state);
  } catch (e) {
    console.error("get_state failed:", e);
  }

  // Live state changes.
  await listen("state", (event) => {
    applyState(event.payload);
  });

  // Mute toggle.
  muteBtn.addEventListener("click", async () => {
    const next = !muteBtn.classList.contains("muted");
    try {
      const state = await invoke("set_muted", { muted: next });
      applyState(state);
    } catch (e) {
      console.error("set_muted failed:", e);
    }
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
