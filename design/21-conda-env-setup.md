# 21 — Conda Install & Virtual Env Setup (amd64 + arm64)

> How to install Miniconda and create virtual environments for **both** Intel (x86_64 / amd64)
> and Apple Silicon (arm64) on macOS. Written after we discovered the existing miniconda
> install was the Intel build running under Rosetta 2.

---

## 0. The one fact that explains everything

Apple Silicon Macs can run **both** architectures:

| Architecture | Meaning | Runs on Apple Silicon? |
|---|---|---|
| `arm64` | Native Apple Silicon | Yes, natively |
| `x86_64` (amd64) | Intel | Yes, **only via Rosetta 2 translation** |

Rosetta 2 is Apple's translator. It silently runs Intel binaries on Apple Silicon. This is why
an Intel conda install "works fine" on an M-series Mac — until you hit a package that ships an
**arm64-only native library** (like `moonshine-voice`'s `libmoonshine.dylib`), which Rosetta
cannot bridge.

**The root cause of a wrong-architecture install is almost always a terminal running under Rosetta.**

---

## 1. Detect what you actually have

Run these in the terminal you use for conda:

```bash
# What does the OS report this process as?
uname -m
#   arm64   → native Apple Silicon
#   x86_64  → this shell is under Rosetta (or you're on an Intel Mac)

# Is THIS shell being translated by Rosetta?
sysctl -n sysctl.proc_translated
#   1 → yes, under Rosetta
#   0 → no, native

# What architecture is the conda python binary?
file $(which python)
#   "Mach-O 64-bit executable arm64"   → native
#   "Mach-O 64-bit executable x86_64"  → Intel build
```

**Rule of thumb:** if `uname -m` says `x86_64` on an M-series Mac, your *terminal* is under
Rosetta, not your hardware. Fix the terminal first (below), then reinstall.

---

## 2. Fix a Rosetta terminal (iTerm / Terminal)

1. Quit the terminal app completely.
2. In Finder → Applications, right-click the app → **Get Info**.
3. **Uncheck** "Open using Rosetta".
4. Reopen the terminal.
5. Verify: `uname -m` must now print `arm64`.

> If you *want* an Intel terminal (rare — only for running legacy Intel-only tools), check the
> box instead. But for a native conda, leave it unchecked.

---

## 3. Install native arm64 Miniconda (recommended on Apple Silicon)

```bash
# 1. Confirm native shell
uname -m          # must print arm64

# 2. Download the Apple Silicon installer
curl -O https://repo.anaconda.com/miniconda/Miniconda3-latest-MacOSX-arm64.sh

# 3. Run it
bash Miniconda3-latest-MacOSX-arm64.sh

# 4. Verify
file $(which python)   # must print "arm64"
```

---

## 4. Install Intel (amd64) Miniconda (only if you need it)

```bash
# 1. Confirm you're in a Rosetta shell (or on an Intel Mac)
uname -m          # must print x86_64

# 2. Download the Intel installer
curl -O https://repo.anaconda.com/miniconda/Miniconda3-latest-MacOSX-x86_64.sh

# 3. Run it
bash Miniconda3-latest-MacOSX-x86_64.sh

# 4. Verify
file $(which python)   # must print "x86_64"
```

---

## 5. Create a virtual environment for a specific architecture

The key is the `CONDA_SUBDIR` environment variable, which overrides the default subdir for a
single command.

### arm64 (Apple Silicon) env

```bash
CONDA_SUBDIR=osx-arm64 conda create -n someenv python=3.13
```

### amd64 (Intel) env

```bash
CONDA_SUBDIR=osx-64 conda create -n someenv python=3.13
```

### Verify the env's architecture

```bash
conda activate someenv
python -c "import platform; print(platform.machine())"
#   arm64   → native
#   x86_64  → Intel
```

---

## 6. Lock the subdir so future installs stay correct

`CONDA_SUBDIR` only applies to the single command it prefixes. Without locking, a later
`conda install` into that env can drift back to the default architecture.

```bash
conda activate someenv
conda config --env --set subdir osx-arm64   # or osx-64 for Intel
```

After this, every `conda install` into `someenv` stays on the locked architecture without
needing the `CONDA_SUBDIR=` prefix.

---

## 7. Quick reference

| Goal | Command |
|---|---|
| Detect shell arch | `uname -m` |
| Detect Rosetta | `sysctl -n sysctl.proc_translated` |
| Detect python arch | `file $(which python)` |
| Native arm64 install | `curl -O ...MacOSX-arm64.sh && bash ...` |
| Intel install | `curl -O ...MacOSX-x86_64.sh && bash ...` |
| arm64 env | `CONDA_SUBDIR=osx-arm64 conda create -n X python=3.13` |
| amd64 env | `CONDA_SUBDIR=osx-64 conda create -n X python=3.13` |
| Lock env arch | `conda config --env --set subdir osx-arm64` |

---

## 8. What this means for Prosopon (current state)

- The existing `~/miniconda` is the **Intel (x86_64)** build, running under Rosetta.
- `prosopon` env = x86_64 → used for the **wake word** (openwakeword / onnxruntime, which ship Intel builds).
- `prosopon-arm64` env = native arm64 → used for **STT** (moonshine-voice, which ships an arm64-only dylib).
- Long-term: migrate to a native arm64 miniconda and consolidate both sidecars into one arm64 env.

---

## §9 — Verification note (2026-09-13)

All four core steps in this doc were run end-to-end on this machine (Ki11ers-MacBook-Pro, macOS 26.2, Apple Silicon) and confirmed live:

1. `CONDA_SUBDIR=osx-arm64 conda create -n skye-test-arm64 python=3.13` → `Mach-O 64-bit executable arm64`, `platform.machine(): arm64` ✅
2. `conda config --env --set subdir osx-arm64` → `conda config --env --show subdir` returns `osx-arm64` ✅
3. Locked env stays arm64 on later installs (no `CONDA_SUBDIR` prefix needed): `conda install numpy` → `Mach-O 64-bit bundle arm64` ✅
4. `CONDA_SUBDIR=osx-64 conda create -n skye-test-x86 python=3.13` → `Mach-O 64-bit executable x86_64` ✅

Throwaway envs (`skye-test-arm64`, `skye-test-x86`) were removed after verification. The lock in step 2 is what makes the recipe ergonomic — without it, every future install would need the `CONDA_SUBDIR` prefix or the env silently drifts back to x86_64.
