# Fix the rodio line (remove bad symphonia-ogg/opus features) and add ogg + opus.
p = 'client-core/Cargo.toml'
s = open(p).read()
old = 'rodio = { version = "0.20", features = ["symphonia-ogg", "symphonia-opus"] }\n'
new = 'rodio = "0.20"\nogg = "0.9"\nopus = "0.4"\n'
assert old in s, "bad rodio line not found"
s = s.replace(old, new, 1)
open(p, 'w').write(s)
print("Cargo.toml fixed")
