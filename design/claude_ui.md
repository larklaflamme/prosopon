Tauri + Rust + WebView is a solid choice for this — probably the best default for a cross-platform AI client UI in 2026. Here's my thinking:

**Why it fits well**

- **Binary size and resource usage**: Tauri uses the OS's native WebView (WebView2 on Windows, WKWebView on macOS, WebKitGTK on Linux) instead of bundling Chromium like Electron does. For an AI client that's likely already juggling memory-hungry things (model contexts, streaming responses, maybe local embeddings or even local inference), not carrying an extra ~150MB Chromium runtime per app instance is a real win.
- **Rust backend**: If you're doing anything beyond just calling a hosted API — local vector stores, file indexing, token counting, process management for local models (llama.cpp, ONNX runtime, etc.), streaming SSE/websocket handling — Rust gives you performance and safety guarantees that a Node/Electron backend won't, and it's a natural fit given you already write Rust.
- **IPC model**: Tauri's command/event system (invoke from JS, emit from Rust) is clean for the typical AI-app pattern of streaming tokens back to the UI. You can push incremental updates via events rather than polling.
- **Security posture**: Tauri's permission/capability system (in v2) is more locked-down by default than Electron, which matters if your app is going to hold API keys or touch the filesystem.

**Where it gets harder**

- **WebView inconsistency**: This is the real tradeoff. WebKitGTK on Linux lags behind Chrome/Safari in feature support and has historically been the source of most cross-platform bugs (rendering quirks, missing CSS features, sometimes even crashes on certain distros). If your UI leans on cutting-edge web APIs or heavy canvas/WebGL work (e.g., for image/video preview, since that's in your wheelhouse), test on Linux early rather than late.
- **Native module ecosystem**: Anything you'd normally reach for in the Node ecosystem (certain audio/video codecs, some ML tooling) will need a Rust equivalent or an FFI bridge. Usually fine, occasionally a time sink.
- **Debugging the WebView**: Each platform's WebView has different devtools quirks — slightly more friction than Chromium's uniform devtools in Electron.

**A couple of things worth deciding early**

- Frontend framework: Tauri is UI-framework-agnostic, so whether you use React, Svelte, Solid, or plain web components is orthogonal — worth picking based on how reactive/complex your streaming UI needs to be.
- If you expect heavy local inference (not just calling a remote API), consider whether Rust-side you'll want `candle` (Rust-native ML) vs shelling out to `llama.cpp`/Python sidecar — this affects how much logic lives in Rust vs a companion process.

If your app is primarily a thin client hitting an API with some local state/caching, Tauri is close to a no-brainer. If it's going to do substantial local ML work, it's still a good choice, but you'll want to nail down the local-inference architecture (in-process Rust crate vs. sidecar process) before you get deep into UI work, since that shapes how your IPC layer needs to look.

---

Good question to check rather than assume — turns out I actually have a concrete answer. Claude Desktop is built using Electron, which combines a web app with an instance of Chromium in a cross-platform executable. So the comparison is pretty direct, and interestingly it cuts in favor of your instinct, not against it.

**The tradeoff Anthropic made, and why**

The Electron choice comes with the exact downside you'd expect: each open instance runs its own Chromium, which can strain system resources and slow things down or cause crashes. That's the cost people pay for Claude Desktop today.

Why did they choose it anyway? Mostly organizational leverage, not a technical argument for Electron over Tauri. Reporting on this decision notes the tradeoff explicitly: shipping one Electron app that covers Windows, Mac, and Linux lets you reuse existing web app code, whereas going fully native multiplies the surface area for bugs and support three-fold across separate codebases. A Claude Code team member weighed in on this directly, pointing out the irony that Claude spent $20k on an agent swarm implementing a C-compiler in Rust, but desktop Claude is still an Electron app — the point being that "we could technically build it" isn't the bottleneck; ongoing maintenance and support burden across three platforms is. For a company shipping a single flagship product to a huge user base with a large existing web codebase to reuse, that calculus makes sense even if it's not the most elegant technical answer.

**Where this actually differs from your situation**

That reasoning doesn't necessarily transfer to you:

- Anthropic already had a mature web app (claude.ai) to wrap. If you're building fresh, you don't get an Electron "reuse the web app for free" discount — you're writing frontend code regardless of shell.
- You already know Rust, which removes the biggest practical cost of Tauri (a second language/toolchain to maintain).
- You're one team building one app, not weighing 3x support surface across a large org with existing Electron ops tooling (crash reporting, update infra, etc. already built around Electron).

So ironically, the tool you're leaning toward (Tauri) is the one people building *lighter, more resource-conscious* Claude-like clients reach for specifically to avoid the problem Anthropic's own desktop app has. There's a concrete existing example of this: a Linux-focused open source Claude client explicitly rejects the Electron model, built as a lightweight native Claude AI desktop client using Tauri v2 and Svelte 5, using the system's WebKitGTK for rendering to keep the binary small and memory usage low rather than wrapping Electron or repackaging a Windows build. It's a good reference for feature scope too — it includes streaming chat with markdown and syntax highlighting, a conversation sidebar, provider/model/system-prompt settings with MCP server config, an artifacts panel with multiple renderers, and side-by-side model comparison, which maps closely to what an AI client UI needs regardless of which shell you pick.

**Bottom line**

Anthropic's Electron choice isn't evidence that Tauri is wrong for an AI client — it's more a story about the cost of retrofitting a framework decision onto an existing large web codebase and org. For a from-scratch build where you already write Rust, Tauri remains the better default, and it's already being used by others building this exact category of app.

---

Here's what I found — and it's worth being upfront that there isn't much public technical documentation from Anthropic itself; most of what's discoverable is external commentary and inference, not an official architecture writeup.

**Confirmed**

Claude Desktop is built with Electron, which combines a web app with a bundled Chromium instance in a cross-platform executable.

**Why, according to outside commentary (not Anthropic's own statement)**

The recurring explanation across the pieces I found is organizational, not technical: Electron lets a team reuse an existing web codebase and ship one app across Windows, Mac, and Linux rather than maintaining three native codebases, which triples the bug/support surface. One widely-discussed post frames this as almost ironic — Anthropic showcases agentic coding wins like a Rust C-compiler built by an agent swarm, yet desktop Claude runs on Electron, a comparatively "un-agentic," conventional choice. A Claude Code team member (Boris Cherney) responded to that critique on Hacker News, agreeing that native is technically reachable now with coding agents, but arguing the real cost is the "last 10%" — messy edge cases, three-fold support surface across platforms, and ongoing maintenance — which coding agents don't eliminate.

**What I couldn't verify**

I didn't find anything from Anthropic on:
- Their actual main/renderer/preload process split or IPC channel design
- Specific security hardening choices (context isolation, CSP, sandboxing config) — the results I found for those topics are generic third-party Electron best-practice guides, not Anthropic-specific
- How streaming responses are piped from network to renderer
- Their auto-update, code-signing, or packaging setup

So the honest picture is: **confirmed framework, inferred rationale, no public deep-dive on internals.** If you want, I can look specifically for any Anthropic engineering blog posts or conference talks that might cover Claude Desktop's architecture directly, rather than relying on secondhand commentary — want me to dig further there?

---

ChatGPT on Claude UI client:

Anthropic has disclosed more about Claude Desktop’s agent architecture than about its UI framework.

The important conclusion is:

> Claude Desktop is not just a thin chat wrapper around a WebView. It is a desktop capability broker connected to cloud services, local tools, MCP extensions, and isolated execution environments.

Anthropic has not publicly confirmed whether the UI is built with Electron, Tauri, React, native frameworks, or another internal stack.

## What Claude Desktop supports

Claude Desktop currently brings together:

- Claude chat
- Cowork
- Claude Code
- Local files and folders
- Local MCP servers
- Desktop extensions
- Built-in browser access
- Computer interaction
- Cloud and local task execution
- Cross-device account synchronization

The desktop app is available for macOS, Windows, and Linux. Linux support is currently described as beta, with Ubuntu and Debian x64/arm64 support. [Claude download page](https://claude.com/download)

## The architecture Anthropic has disclosed

Anthropic describes two broad execution modes.

### Cloud session

```text
Claude cloud session
        │
        │ Anthropic-brokered connection
        ▼
Claude Desktop on the user’s machine
        │
        ├── Selected local folders
        ├── Browser
        └── Approved local capabilities
```

The cloud agent runs on Anthropic’s infrastructure. When it needs something on the user’s computer, the request passes through the Claude Desktop app. Local access is limited to folders the user has connected, and each local tool call is checked against permissions. [Claude Cowork architecture overview](https://support.claude.com/en/articles/14479294-claude-cowork-architecture-overview)

### Local session

```text
Claude Desktop
├── Native local agent loop
│   ├── Conversation handling
│   ├── File reads/writes
│   ├── Web fetches
│   └── Local MCP servers
│
└── Isolated Linux VM
    ├── Shell commands
    ├── Generated code
    └── Code execution
```

In local sessions, the agent loop runs natively on the device, but shell commands and generated code run inside a dedicated Linux virtual machine. On macOS this uses Apple’s Virtualization Framework; on Windows it uses Hyper-V. [Claude Cowork architecture overview](https://support.claude.com/en/articles/14479294-claude-cowork-architecture-overview)

That is a major architectural distinction from a typical Tauri application:

```text
Typical Tauri:
Frontend → Rust command → host operating system

Claude-style agent client:
Frontend → local agent/runtime → policy layer → isolated VM or approved host capability
```

## Why the VM matters

Anthropic explicitly designed the VM so that Claude’s code execution does not automatically receive unrestricted access to the host computer.

The VM has:

- Its own Linux kernel
- Its own filesystem
- Its own process table
- Restricted mounted workspace folders
- Network egress controls
- Per-session user isolation
- Credentials kept outside the guest

Anthropic describes this as a security boundary because an agent may encounter malicious instructions in files, web pages, repositories, or tool responses. [How Anthropic contains Claude](https://www.anthropic.com/engineering/how-we-contain-claude)

This is probably the largest difference between Claude Desktop and the architecture most developers initially imagine for an AI client.

## MCP and desktop extensions

Claude Desktop uses MCP as a major extension mechanism.

Local desktop extensions can run:

- Node.js servers
- Python servers
- Native binaries

Claude Desktop includes a built-in Node.js environment for extensions. Sensitive configuration values are encrypted using the operating system’s secure storage:

- Keychain on macOS
- Credential Manager on Windows
- The Linux distribution’s keychain manager on Linux

[Anthropic local MCP documentation](https://support.claude.com/en/articles/10949351-getting-started-with-local-mcp-servers-on-claude-desktop)

Anthropic separates:

```text
Remote MCP connector:
Anthropic cloud → remote MCP server

Local desktop extension:
Claude Desktop → local MCP server
```

Remote connectors can work across web, mobile, and desktop. Local extensions are available only where local execution is possible, primarily Desktop and Claude Code. [Anthropic connector guidance](https://support.claude.com/en/articles/11725091-when-to-use-desktop-and-web-connectors)

## Permission architecture

Claude’s permission model is layered:

```text
Workspace/admin permissions
        ↓
Account and connector permissions
        ↓
Connected folders
        ↓
Tool-specific authorization
        ↓
VM or host execution boundary
```

For enterprise deployments, administrators can control:

- Whether local MCP is enabled
- Whether desktop extensions are allowed
- Which workspace folders can be mounted
- Whether Claude Code is enabled
- Whether Cowork is enabled
- Whether automatic updates are allowed

[Anthropic enterprise configuration](https://support.claude.com/en/articles/12622667-enterprise-configuration-for-claude-desktop)

## What is not publicly known

Anthropic has not publicly specified:

- The frontend framework
- Whether Claude Desktop uses Electron, Tauri, or a custom shell
- Whether the UI is React, native, or hybrid
- The internal IPC protocol between UI and runtime
- The exact local database technology
- The full update/signing architecture
- How the app coordinates all desktop processes internally

So it would be incorrect to say “Claude Desktop is built with Tauri” or “Claude Desktop is Electron” without additional evidence.

## Comparison with your planned client

Your proposed application could begin as:

```text
Web UI
   ↓
Tauri
   ↓
Rust commands
   ↓
AI backend
```

A Claude-like architecture would eventually look more like:

```text
Web UI
   ↓
Desktop shell
   ↓
Local runtime / capability broker
   ├── Permission manager
   ├── File access
   ├── MCP host
   ├── Browser integration
   ├── Process manager
   └── VM manager
          ↓
      Isolated execution environment

Cloud backend
   ├── Model calls
   ├── Task orchestration
   ├── Account data
   ├── Remote connectors
   └── Synchronization
```

## What I would borrow for your project

For a first version, I would copy these Claude design ideas:

1. Make the frontend mostly unprivileged.
2. Put native access behind explicit commands.
3. Allow users to connect specific folders instead of exposing the entire filesystem.
4. Treat local tools and remote tools as separate categories.
5. Store credentials in the OS keychain.
6. Design MCP-style tools with schemas and explicit permissions.
7. Run generated code in a sandbox or VM.
8. Keep cloud conversations separate from local workspace state.
9. Add an audit log for tool calls.
10. Ask for confirmation before destructive actions.

I would not build the VM first unless your product executes arbitrary code or manipulates the user’s computer. Start with Tauri plus Rust and a tightly scoped permission layer. Add isolated execution once your product actually needs agents to run shell commands or modify projects.

The conceptual difference is:

```text
Tauri is primarily a cross-platform application shell.

Claude Desktop is a cross-platform application shell
plus an agent runtime, tool system, permission broker,
and isolated execution platform.
```

---
Response from Antigravity:

Used tool: search_web

Anthropic took a different architectural approach with the **Claude Desktop client** compared to OpenAI’s initial native Swift/AppKit build. Claude Desktop was engineered primarily as a **developer-centric, cross-platform powerhouse** designed around **Electron** and their open **Model Context Protocol (MCP)**.

---

### 1. Core Shell & Platform Architecture

* **Electron + React/TypeScript:**
  * Claude Desktop wraps Anthropic’s web interface (`claude.ai`) inside an **Electron** shell (Chromium + Node.js). 
  * This allows a single codebase to serve macOS and Windows simultaneously, maintaining near-instant feature parity (e.g., Projects, Artifacts, Thinking mode) across the web and desktop.
* **Native Helpers (Swift, Rust, & Go):**
  * While the UI layer is web-based, Anthropic uses native compiled modules and background daemons for performance-critical OS tasks:
    * Global shortcut interception and window positioning.
    * Process management for local tools.
    * Native virtualization hooks on macOS.

---

### 2. Model Context Protocol (MCP) Host Architecture

The defining technical feature of the Claude Desktop client is that it acts as a **native MCP Client/Host**. It connects Claude directly to local development tools, databases, and local file systems.

```
┌─────────────────────────────────────────────────────────┐
│                   Claude Desktop UI                     │
│                    (Electron/React)                     │
└────────────────────────────┬────────────────────────────┘
                             │ IPC
┌────────────────────────────▼────────────────────────────┐
│                  Electron Main Process                  │
│                     (MCP Host Core)                     │
└──────┬─────────────────────┬─────────────────────┬──────┘
       │ stdio               │ stdio               │ SSE / HTTP
┌──────▼──────┐       ┌──────▼──────┐       ┌──────▼──────┐
│  Local Files │       │  PostgreSQL │       │ Remote APIs │
│  MCP Server │       │  MCP Server │       │  (e.g. Git) │
│ (Node/Python)│      │  (Docker/uv)│       │             │
└─────────────┘       └─────────────┘       └─────────────┘
```

* **Configuration Management (`claude_desktop_config.json`):**
  * The app exposes a central configuration file stored in:
    * **macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`
    * **Windows:** `%APPDATA%\Claude\claude_desktop_config.json`
* **Local Process Spawning (`stdio` transport):**
  * When Claude Desktop starts, the Electron main process parses the JSON configuration and spawns local sub-processes via standard command-line tools (`npx`, `uvx`, `python`, `docker`).
  * Communication runs over **JSON-RPC 2.0 via standard input/output (`stdio`)**.
* **Tool Discovery & Execution:**
  * The desktop client queries connected MCP servers for their available tools and resources at runtime, serializes them into tool-use schemas, and attaches them to the Anthropic Messages API request.
  * When Claude invokes a tool, the desktop client prompts the user for permission and executes the command locally, returning the result back to the model.

---

### 3. Sandboxed Artifacts Rendering

Claude’s interactive Artifacts (React components, SVGs, HTML/JS apps, Mermaid diagrams) require safe client-side execution:

* **Multi-Layered Sandboxing:**
  * Artifacts execute within isolated `<iframe>` elements configured with strict Content Security Policies (CSP) and stripped of Node.js integration (`nodeIntegration: false`, `contextIsolation: true`).
* **In-Browser Transpilation:**
  * For live React previews, code is transpiled dynamically on the client (using Babel/esbuild in Web Workers) to render interactive components without compiling to disk.

---

### 4. Agentic Capabilities & Virtualization ("Computer Use")

For agentic workflows where Claude navigates the desktop or runs code in an isolated environment:

* **OS Virtualization Frameworks:**
  * Rather than giving raw, unconstrained access to the host machine, advanced agentic features use sandboxed virtual environments (e.g., leveraging Apple’s `Virtualization.framework` on macOS or lightweight Linux containers).
* **Input & Display Bridges:**
  * Captures screen buffer state via OS display APIs and dispatches synthesized mouse coordinates and keystrokes inside the sandboxed target application.

---

### Summary: ChatGPT vs. Claude Desktop Implementations

| Feature / Dimension      | **ChatGPT Desktop**                                      | **Claude Desktop**                                        |
| :----------------------- | :------------------------------------------------------- | :-------------------------------------------------------- |
| **Primary Framework**    | Native **Swift/AppKit** (macOS) / Electron (Windows)     | **Electron** (Cross-platform macOS & Windows)             |
| **Design Philosophy**    | Native OS citizen, Spotlight-like companion, voice-first | Power-user workspace, extensible tool orchestrator        |
| **Extensibility**        | Built-in Accessibility API scraping ("Work with Apps")   | **Model Context Protocol (MCP)** via local `stdio`/SSE    |
| **Voice & Multimodal**   | Realtime WebRTC / WebSockets audio engine                | Text/Image streaming over SSE                             |
| **Local Tool Execution** | Server-side interpreter / direct OS Accessibility hooks  | Spawns local CLI servers (`npx`, `uvx`, `python`) via MCP |

---
From Antigravity:

To build a cross-platform desktop client that replicates **Claude Desktop** (including MCP integration, sandboxed Artifacts, global hotkeys, and per-platform native builds), there are two primary industry-standard routes:

1. **The Modern & Lightweight Route: Tauri 2.0 (Rust + Web Frontend)** *(Recommended)*
2. **The Battle-Tested / Exact Replica Route: Electron (TypeScript + Node.js)**

Here is the complete technology blueprint broken down by layer.

---

### 1. Framework Comparison: Tauri 2 vs. Electron

| Dimension            | **Option A: Tauri 2.0 (Rust)** *(Recommended)*     | **Option B: Electron (Node.js)** *(Claude's Stack)*               |
| :------------------- | :------------------------------------------------- | :---------------------------------------------------------------- |
| **Memory Footprint** | ~30–60 MB idle (Uses OS WebView)                   | ~150–300 MB idle (Bundles Chromium)                               |
| **Installer Size**   | ~10–25 MB                                          | ~80–130 MB                                                        |
| **MCP Execution**    | Managed via Rust `tokio::process` / sub-processes  | Managed via Node.js `child_process` / `@modelcontextprotocol/sdk` |
| **Security Model**   | Strict Rust-enforced capabilities & IPC boundaries | Configurable via Electron `contextIsolation` & CSP                |
| **Web Tech**         | HTML / CSS / React / Vue / Svelte                  | HTML / CSS / React / Next.js                                      |

---

### 2. Full Technology Stack by Architectural Layer

#### A. Frontend & UI Layer (Shared across all platforms)
* **Framework:** **React 19** or **Vite + React + TypeScript**
* **Styling & Components:** **Tailwind CSS** + **shadcn/ui** or **Radix UI** (provides accessible, themeable desktop-like UI components).
* **Code & Markdown Rendering:**
  * `react-markdown` + `remark-gfm` + `rehype-highlight` (or `shiki` for VS Code-quality syntax highlighting).
  * `@monaco-editor/react` or `CodeMirror 6` for interactive code blocks and diff viewers.
* **State & Data Management:** **Zustand** (lightweight client state) + **TanStack Query** (API and tool state).

---

#### B. MCP (Model Context Protocol) Host Engine
Replicating Claude’s tool ecosystem requires an MCP Client that spawns local CLI processes:

* **If using Tauri (Rust backend):**
  * Use **`tokio::process::Command`** to spawn `stdio` MCP processes (e.g., `npx`, `uvx`, `python`).
  * Use Rust MCP SDKs (e.g. `rmcp` or custom JSON-RPC over `tokio` streams).
  * Parse config from OS standard paths (`directories-rs` crate).
* **If using Electron (Node backend):**
  * Use the official **`@modelcontextprotocol/sdk`** (`Client` and `StdioClientTransport`).
  * Read and write `config.json` via Node `fs/promises`.

```json
// Example: config.json location per platform
// macOS:   ~/Library/Application Support/YourApp/config.json
// Windows: %APPDATA%\YourApp\config.json
// Linux:   ~/.config/YourApp/config.json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/dir"]
    }
  }
}
```

---

#### C. Sandboxed Artifacts & Preview Engine
To safely render HTML, React, SVGs, and Mermaid diagrams without allowing malicious scripts access to local system APIs:

* **Sandboxed `<iframe>` Container:**
  * Configure attributes: `sandbox="allow-scripts"` (omit `allow-same-origin` to isolate cookies, local storage, and parent window tokens).
  * Strict **Content Security Policy (CSP)** restricting network fetches and script domains.
* **In-Browser Dynamic Compilers:**
  * **React / TSX Preview:** Use `@babel/standalone` or `esbuild-wasm` executing inside a **Web Worker** to transpile TSX to executable JS on the fly.
  * **Diagrams:** Use `mermaid.js` for dynamic diagram compilation.

---

#### D. OS-Level Integrations & Native Plugins

| Feature                          | Tauri 2 Plugin                        | Electron Native Module                       |
| :------------------------------- | :------------------------------------ | :------------------------------------------- |
| **Global Shortcuts**             | `tauri-plugin-global-shortcut`        | `electron.globalShortcut`                    |
| **System Tray & Menus**          | `tauri-plugin-tray`                   | `electron.Tray` & `Menu`                     |
| **Secure Storage (Tokens)**      | `tauri-plugin-stronghold` / `keyring` | `keytar` / `safeStorage` (DPAPI/Keychain)    |
| **Window State (Position/Size)** | `tauri-plugin-window-state`           | `electron-window-state`                      |
| **Auto Updates**                 | `tauri-plugin-updater`                | `electron-updater` / `update.electronjs.org` |

---

### 3. Packaging, Signing & Per-Platform Builds

To produce signed, production-ready binaries for each target operating system:

```
                  ┌──────────────────────────────┐
                  │    GitHub Actions Matrix     │
                  └──────────────┬───────────────┘
          ┌──────────────────────┼──────────────────────┐
          ▼                      ▼                      ▼
    [ macOS Runner ]     [ Windows Runner ]      [ Linux Runner ]
          │                      │                      │
   • Apple Dev ID         • Authenticode Cert    • GPG Signing
   • codesign             • signtool / Azure     • AppImage / Flatpak
   • notarytool           • NSIS / MSI           • .deb / .rpm
          │                      │                      │
          ▼                      ▼                      ▼
      .dmg / .zip            .msi / .exe            .AppImage / .deb
```

#### Platform Targets:

1. **macOS (Apple Silicon & Intel Universal Builds):**
   * **Artifacts:** `.dmg`, `.app`, or `.pkg`.
   * **Tooling:** `cargo tauri build --target universal-apple-darwin` or `electron-builder --mac`.
   * **Requirements:** Apple Developer Certificate (`Developer ID Application`) for `codesign` and Apple `notarytool` for macOS Gatekeeper verification.

2. **Windows (x64 / ARM64):**
   * **Artifacts:** `.msi` (enterprise-ready via WiX) or `.exe` (via NSIS installer).
   * **Tooling:** `cargo tauri build --target x86_64-pc-windows-msvc` or `electron-builder --win`.
   * **Requirements:** Microsoft Authenticode Signing Certificate (EV or Azure Trusted Signing) to eliminate Windows SmartScreen warnings.

3. **Linux:**
   * **Artifacts:** `.AppImage` (universal), `.deb` (Debian/Ubuntu), `.rpm` (Fedora/RHEL), or `Flatpak`.
   * **Tooling:** Built natively on Ubuntu runner (`cargo tauri build` / `electron-builder --linux`).

---

### 4. Recommended Starter Blueprint

If you want to start building today with modern architecture:

1. **Initialize Project:**
   ```bash
   # Initialize a Tauri 2 + React + TypeScript workspace
   npm create tauri-app@latest my-ai-client -- --template react-ts
   ```
2. **Add MCP Capabilities:** Implement a lightweight MCP client manager in Rust (`src-tauri/src/mcp.rs`) handling JSON-RPC over `tokio::process::Command` stdio pipes.
3. **Connect Frontend to LLM:** Use the **Vercel AI SDK** (`ai/react`) or direct streaming clients to consume Anthropic/OpenAI SSE streams.
4. **Setup CI/CD:** Use the official `tauri-apps/tauri-action` or `electron-builder` workflow templates on GitHub Actions to auto-build and sign on tag releases.