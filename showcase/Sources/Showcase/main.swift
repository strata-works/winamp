import CCarapace

let v = carapace_abi_version()
let major = v >> 16
print("[showcase] carapace ABI \(major).\(v & 0xFFFF)")
precondition(major == 3, "expected carapace ABI major 3, got \(major)")
print("[showcase] linkage OK")
