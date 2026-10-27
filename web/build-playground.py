"""Builds a self-hosted playground: a folder to serve from any static web server, without access
to the internet.

    python web/build-playground.py --wasm <cadastre_wasm.wasm> --out <folder> [--viz <viz-standalone.js>] [--no-examples]

The folder holds the page, its JavaScript wrapper, the WebAssembly module, Graphviz for the browser
(downloaded once from the CDN the page uses, unless given with --viz) and the examples; without
them, it is the playground `cadastre serve` and `cadastre site` publish a project with.
"""

import argparse
import re
import shutil
import sys
import urllib.request
from pathlib import Path

WEB = Path(__file__).resolve().parent
REPOSITORY = WEB.parent

NOTICE = """Cadastre playground

Cadastre: MIT or Apache-2.0, at your option (LICENSE-MIT, LICENSE-APACHE).
Graphviz for the browser (viz-standalone.js): @viz-js/viz, MIT license, and Graphviz, Eclipse
Public License 1.0 (https://github.com/mdaines/viz-js, https://graphviz.org).
Icons: Lucide, ISC license (https://lucide.dev).

Serve this folder with any static web server (IIS, nginx, Apache, `python -m http.server`) and
open index.html. Nothing is sent anywhere: the files of a project are read in the browser.
"""


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--wasm", required=True, type=Path, help="the module built by cargo")
    parser.add_argument("--out", required=True, type=Path, help="the folder to write")
    parser.add_argument("--viz", type=Path, help="viz-standalone.js, instead of downloading it")
    parser.add_argument("--no-examples", action="store_true", help="without the examples")
    args = parser.parse_args()

    out = args.out
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)

    for name in ("index.html", "cadastre.js"):
        shutil.copy(WEB / name, out / name)
    shutil.copy(args.wasm, out / "cadastre_wasm.wasm")

    # Graphviz: the version the page loads from its CDN.
    if args.viz:
        shutil.copy(args.viz, out / "viz-standalone.js")
    else:
        url = re.search(r'const VIZ_CDN = "([^"]+)"', (WEB / "index.html").read_text(encoding="utf-8")).group(1)
        with urllib.request.urlopen(url) as response:
            (out / "viz-standalone.js").write_bytes(response.read())

    # The examples, as the page lists them.
    for project in [] if args.no_examples else sorted((REPOSITORY / "examples").iterdir()):
        if project.is_dir():
            target = out / "examples" / project.name
            target.mkdir(parents=True)
            for file in project.iterdir():
                if file.suffix in (".ca", ".cav", ".cac"):
                    shutil.copy(file, target / file.name)

    for name in ("LICENSE-MIT", "LICENSE-APACHE", "THIRD-PARTY-NOTICES.md"):
        shutil.copy(REPOSITORY / name, out / name)
    (out / "NOTICE.txt").write_text(NOTICE, encoding="utf-8")

    size = sum(f.stat().st_size for f in out.rglob("*") if f.is_file())
    print(f"playground written to {out} ({size / 1e6:.1f} MB)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
