// Zoom and move around the drawing of a view: Ctrl + wheel (or a pinch) zooms around the cursor,
// the wheel alone too where the drawing is the whole page (`wheel: true`); dragging the
// background moves it, as do the arrow keys; buttons zoom in, out, fit the whole drawing and
// show it at its actual size. It acts on the `viewBox` of the inline SVG: the links and the
// tooltips keep working, a drag does not open the link it starts on, and printing gives the
// whole drawing. No dependency.
//
// Shared by the playground, the site and the VS Code preview. Copied in `web/zoom.js`.
(function (global) {
  const WORDS = {
    en: { in: "Zoom in", out: "Zoom out", fit: "Fit", actual: "1:1", hint: "Ctrl + wheel to zoom, drag to move" },
    fr: { in: "Zoomer", out: "Dézoomer", fit: "Ajuster", actual: "1:1", hint: "Ctrl + molette pour zoomer, glisser pour déplacer" },
  };
  // CSS pixels per unit of a drawing of Graphviz (points).
  const PX_PER_PT = 4 / 3;
  const STYLE = `
.cz-box { position: relative; overflow: hidden; touch-action: none; cursor: grab; outline: none; }
.cz-box.cz-moving { cursor: grabbing; }
.cz-box > svg { display: block; width: 100%; height: 100%; max-width: none; }
.cz-bar { position: absolute; top: 6px; right: 6px; display: flex; gap: 2px; z-index: 1; opacity: .85; }
.cz-bar:hover { opacity: 1; }
.cz-bar button { font: 12px/1 system-ui, sans-serif; min-width: 26px; height: 24px; padding: 0 6px; border: 1px solid #c8d1dc;
  background: #fff; color: #1f2328; border-radius: 4px; cursor: pointer; }
.cz-bar button:hover { background: #f3f6fb; }
@media print { .cz-bar { display: none; } .cz-box { height: auto !important; overflow: visible; } }
`;

  function addStyle(doc) {
    if (doc.getElementById("cadastre-zoom-style")) return;
    const style = doc.createElement("style");
    style.id = "cadastre-zoom-style";
    style.textContent = STYLE;
    doc.head.append(style);
  }

  // The viewBox of a drawing, from its attribute or its size.
  function initialBox(svg) {
    const box = svg.viewBox && svg.viewBox.baseVal;
    if (box && box.width > 0 && box.height > 0) return { x: box.x, y: box.y, w: box.width, h: box.height };
    const w = parseFloat(svg.getAttribute("width")) || 800;
    const h = parseFloat(svg.getAttribute("height")) || 600;
    return { x: 0, y: 0, w, h };
  }

  // Makes `container` (holding the inline `svg`) zoomable; returns { fit, actual, state }.
  // Options: `wheel` (the wheel alone zooms), `height` (CSS height of the box, otherwise that of
  // the drawing fitted to the width, at most 80% of the window), `state` (a state to restore,
  // from `state()`), `language`.
  function attach(container, options = {}) {
    const svg = container.querySelector("svg");
    if (!svg || container.classList.contains("cz-box")) return null;
    const doc = container.ownerDocument;
    addStyle(doc);
    const words = WORDS[options.language] ?? WORDS.en;
    const whole = initialBox(svg);
    container.classList.add("cz-box");
    container.tabIndex = 0;
    if (options.height) {
      container.style.height = options.height;
    } else {
      const width = container.clientWidth || whole.w * PX_PER_PT;
      const fitted = Math.min(whole.h * PX_PER_PT, (width / whole.w) * whole.h);
      container.style.height = `${Math.round(Math.min(fitted + 8, global.innerHeight * 0.8))}px`;
    }
    svg.removeAttribute("width");
    svg.removeAttribute("height");
    svg.setAttribute("preserveAspectRatio", "xMidYMid meet");

    let view = { ...whole };
    const apply = () => svg.setAttribute("viewBox", `${view.x} ${view.y} ${view.w} ${view.h}`);
    // Units of the drawing per CSS pixel, the drawing being fitted in the box ("meet").
    const unit = () => {
      const r = container.getBoundingClientRect();
      return Math.max(view.w / Math.max(r.width, 1), view.h / Math.max(r.height, 1));
    };
    // The point of the drawing under a point of the screen.
    const at = (clientX, clientY) => {
      const r = container.getBoundingClientRect();
      const u = unit();
      const shown = { w: r.width * u, h: r.height * u };
      return {
        x: view.x - (shown.w - view.w) / 2 + (clientX - r.left) * u,
        y: view.y - (shown.h - view.h) / 2 + (clientY - r.top) * u,
      };
    };
    const limits = () => {
      const r = container.getBoundingClientRect();
      // From the whole drawing, and a little more, to eight times its actual size.
      const largest = Math.max(whole.w, (whole.h * r.width) / Math.max(r.height, 1)) * 1.5;
      const smallest = (r.width / PX_PER_PT) / 8;
      return { largest, smallest };
    };
    // Zooms by `factor` (> 1: closer) around a point of the drawing.
    function zoom(factor, point) {
      const { largest, smallest } = limits();
      const w = Math.min(largest, Math.max(smallest, view.w / factor));
      const k = w / view.w;
      const p = point ?? { x: view.x + view.w / 2, y: view.y + view.h / 2 };
      view = { x: p.x - (p.x - view.x) * k, y: p.y - (p.y - view.y) * k, w, h: view.h * k };
      apply();
    }
    function fit() {
      view = { ...whole };
      apply();
    }
    function actual() {
      const r = container.getBoundingClientRect();
      const center = { x: view.x + view.w / 2, y: view.y + view.h / 2 };
      const w = r.width / PX_PER_PT;
      const h = r.height / PX_PER_PT;
      view = { x: center.x - w / 2, y: center.y - h / 2, w, h };
      apply();
    }

    container.addEventListener("wheel", (event) => {
      if (!options.wheel && !event.ctrlKey && !event.metaKey) return;
      event.preventDefault();
      const factor = Math.exp(-event.deltaY * (event.deltaMode === 1 ? 0.05 : 0.0015));
      zoom(factor, at(event.clientX, event.clientY));
    }, { passive: false });

    // Dragging the drawing moves it; a drag does not open the link it starts on.
    let drag = null;
    let dragged = false;
    container.addEventListener("pointerdown", (event) => {
      if (event.button !== 0 || event.target.closest(".cz-bar")) return;
      drag = { x: event.clientX, y: event.clientY, view: { ...view }, id: event.pointerId };
      dragged = false;
    });
    container.addEventListener("pointermove", (event) => {
      if (!drag || event.pointerId !== drag.id) return;
      const dx = event.clientX - drag.x;
      const dy = event.clientY - drag.y;
      if (!dragged && Math.hypot(dx, dy) < 4) return;
      if (!dragged) {
        dragged = true;
        container.classList.add("cz-moving");
        container.setPointerCapture(event.pointerId);
      }
      const u = unit();
      view = { ...drag.view, x: drag.view.x - dx * u, y: drag.view.y - dy * u };
      apply();
    });
    const end = () => {
      drag = null;
      container.classList.remove("cz-moving");
    };
    container.addEventListener("pointerup", end);
    container.addEventListener("pointercancel", end);
    container.addEventListener("click", (event) => {
      if (dragged) {
        event.preventDefault();
        event.stopPropagation();
        dragged = false;
      }
    }, true);

    container.addEventListener("keydown", (event) => {
      const step = 40 * unit();
      const moves = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] };
      if (moves[event.key]) {
        view = { ...view, x: view.x + moves[event.key][0], y: view.y + moves[event.key][1] };
        apply();
      } else if (event.key === "+" || event.key === "=") {
        zoom(1.25);
      } else if (event.key === "-") {
        zoom(0.8);
      } else if (event.key === "0") {
        fit();
      } else {
        return;
      }
      event.preventDefault();
    });

    const bar = doc.createElement("div");
    bar.className = "cz-bar";
    bar.title = words.hint;
    const button = (label, title, onclick) => {
      const b = doc.createElement("button");
      b.type = "button";
      b.textContent = label;
      b.title = title;
      b.onclick = (event) => {
        event.stopPropagation();
        onclick();
      };
      return b;
    };
    bar.append(
      button("+", words.in, () => zoom(1.25)),
      button("−", words.out, () => zoom(0.8)),
      button(words.fit, words.fit, fit),
      button(words.actual, words.actual, actual),
    );
    container.append(bar);

    // Printed whole.
    global.addEventListener("beforeprint", fit);

    if (options.state && options.state.w > 0) {
      view = { ...options.state };
    }
    apply();
    return { fit, actual, zoom, state: () => ({ ...view }) };
  }

  global.cadastreZoom = { attach };
})(globalThis);
