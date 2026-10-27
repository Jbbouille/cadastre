"""The C compiler cargo is given to build the WebAssembly module with zig (`zig cc`): the `cc`
crate asks for `--target=wasm32-unknown-unknown`, which zig calls `wasm32-freestanding`."""
import subprocess
import sys

args = [a for a in sys.argv[1:] if not a.startswith("--target=")]
sys.exit(subprocess.call(["zig", "cc", "-target", "wasm32-freestanding"] + args))
