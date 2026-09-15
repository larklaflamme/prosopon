import re

# 1. Cargo.toml — add rodio after cpal
p = 'client-core/Cargo.toml'
s = open(p).read()
old = 'cpal = "0.15"\n'
new = 'cpal = "0.15"\nrodio = { version = "0.20", features = ["symphonia-ogg", "symphonia-opus"] }\n'
assert old in s, "cpal line not found"
s = s.replace(old, new, 1)
open(p, 'w').write(s)
print("Cargo.toml patched")

# 2. client-core/src/lib.rs — add pub mod playback
p = 'client-core/src/lib.rs'
s = open(p).read()
old = 'pub mod webrtc_client;\n'
new = 'pub mod webrtc_client;\npub mod playback;\n'
assert old in s, "webrtc_client mod not found"
s = s.replace(old, new, 1)
open(p, 'w').write(s)
print("client-core lib.rs patched")

# 3. src-tauri/src/lib.rs — replace play_audio body
p = 'src-tauri/src/lib.rs'
s = open(p).read()
pattern = re.compile(r'fn play_audio\(app: &AppHandle, bytes: &\[u8\]\) \{.*?\n\}', re.DOTALL)
new_fn = '''fn play_audio(app: &AppHandle, bytes: &[u8]) {
    match prosopon_client_core::playback::play_ogg_opus(bytes) {
        Ok(()) => emit_log(
            app,
            "info",
            "playback",
            format!("played {} bytes of audio", bytes.len()),
        ),
        Err(e) => emit_log(app, "error", "playback", format!("playback failed: {e}")),
    }
}'''
s2, n = pattern.subn(new_fn, s, count=1)
assert n == 1, f"expected 1 replacement, got {n}"
open(p, 'w').write(s2)
print("tauri lib.rs patched")
