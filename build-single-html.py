#!/usr/bin/env python3
"""Build a single self-contained HTML file (dist/fapiao-worker.html).

Bundles:
  - pkg/fapiao_worker_bg.wasm  -> base64 string, instantiated from bytes
  - pkg/fapiao_worker.js       -> inlined wasm-bindgen glue
  - static/ assets (src/srcset/href) -> base64 data URIs
  - external CSS (unpkg mvp.css) -> downloaded and inlined

Run `wasm-pack build --target web --release` first for a much smaller file.
"""

import base64
import mimetypes
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
DIST = ROOT / "dist"
OUT = DIST / "fapiao-worker.html"


def data_uri(path: Path) -> str:
    mime = mimetypes.guess_type(path.name)[0] or "application/octet-stream"
    b64 = base64.b64encode(path.read_bytes()).decode()
    return f"data:{mime};base64,{b64}"


def main() -> int:
    glue_path = ROOT / "pkg" / "fapiao_worker.js"
    wasm_path = ROOT / "pkg" / "fapiao_worker_bg.wasm"
    html_path = ROOT / "index.html"
    for p in (glue_path, wasm_path, html_path):
        if not p.exists():
            print(f"error: {p} not found — run `wasm-pack build --target web` first")
            return 1

    html = html_path.read_text()

    # --- 2. Inline static assets; drop references to missing files ---
    def inline_static(m: re.Match) -> str:
        attr, quote, rel = m.group(1), m.group(2), m.group(3)
        p = ROOT / rel
        if not p.exists():
            print(f"warning: {rel} not found, dropping the whole line")
            return f"__DROP_LINE__{attr}={quote}{rel}{quote}"
        return f"{attr}={quote}{data_uri(p)}{quote}"

    html = re.sub(r'(src|srcset|href)=("|\')(static/[^"\']+)\2', inline_static, html)
    html = re.sub(r"^.*__DROP_LINE__.*\n?", "", html, flags=re.MULTILINE)

    # --- 3. Inline external stylesheets ---
    def inline_css(m: re.Match) -> str:
        file = m.group(1)
        print(f"reading {file}")
        css = (ROOT / file).read_text()
        return f"<style>\n{css}\n</style>"

    html = re.sub(
        r'<link rel="stylesheet" href="([^"]+)">',
        inline_css,
        html,
    )

    # --- 4. Extract the app <script type="module"> and rewrite its imports ---
    m = re.search(r'<script type="module">\n?(.*?)\n?\s*</script>', html, re.DOTALL)
    if not m:
        print('error: no <script type="module"> block found in index.html')
        return 1
    app_js = m.group(1)
    app_js = re.sub(
        r'^\s*import init, \{ fill_claim_form \} from "\./pkg/fapiao_worker\.js";\n?',
        "",
        app_js,
        flags=re.MULTILINE,
    )
    app_js = app_js.replace(
        "await init();",
        "await __wbg_init({ module_or_path: __b64ToBytes(WASM_B64) });",
    )
    if "__wbg_init" not in app_js:
        print("error: could not rewrite the wasm init call in index.html")
        return 1

    glue_js = glue_path.read_text()
    wasm_b64 = base64.b64encode(wasm_path.read_bytes()).decode()

    bundled_js = f"""// ===== embedded wasm binary (base64) =====
const WASM_B64 = "{wasm_b64}";
function __b64ToBytes(b64) {{
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}}

// ===== wasm-bindgen glue =====
{glue_js}

// ===== app =====
{app_js}"""

    html = (
        html[: m.start()]
        + '<script type="module">\n'
        + bundled_js
        + "\n</script>"
        + html[m.end() :]
    )

    # --- 5. Sanity check: no leftover relative/external references ---
    leftovers = re.findall(
        r'(?:src|srcset|href)="(?!data:|#|https?://|mailto:)([^"]+)"', html
    )
    if leftovers:
        print(f"warning: unresolved references remain: {leftovers}")

    DIST.mkdir(exist_ok=True)
    OUT.write_text(html)
    size_mb = OUT.stat().st_size / 1_000_000
    print(f"wrote {OUT} ({size_mb:.1f} MB)")
    if size_mb > 10:
        print(
            "hint: run `wasm-pack build --target web --release` to shrink the wasm binary"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
