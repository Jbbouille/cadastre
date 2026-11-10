// Zoom and move around the drawing of a view: Ctrl + wheel (or a pinch) zooms around the cursor,
// the wheel alone too where the drawing is the whole page (`wheel: true`); dragging the
// background moves it, as do the arrow keys; buttons zoom in, out, fit the whole drawing, show
// it at its actual size and on the whole screen. It acts on the `viewBox` of the inline SVG: the
// links keep working, a drag does not open the link it starts on, and printing gives the whole
// drawing. The tooltips become bubbles shown at once, the edges are easier to point at, and a
// node pointed at lights up with its edges and neighbours, the rest faded. No dependency.
//
// Shared by the playground, the site and the VS Code preview. Copied in `web/zoom.js`.
(function (global) {
  const WORDS = {
    en: { in: "Zoom in", out: "Zoom out", fit: "Fit", actual: "1:1", full: "Full screen", hint: "Ctrl + wheel to zoom, drag to move" },
    fr: { in: "Zoomer", out: "Dézoomer", fit: "Ajuster", actual: "1:1", full: "Plein écran", hint: "Ctrl + molette pour zoomer, glisser pour déplacer" },
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
.cz-box:fullscreen { background: Canvas; }
.cz-tip { position: fixed; z-index: 1000; max-width: 420px; padding: 6px 9px; border-radius: 6px; pointer-events: none;
  background: #1f2328; color: #f6f8fa; font: 12px/1.45 system-ui, sans-serif; box-shadow: 0 4px 14px rgba(0,0,0,.25); }
.cz-tip > div { white-space: pre-wrap; overflow-wrap: anywhere; }
.cz-tip > .cz-tip-head { font-weight: 600; }
svg .cz-hit { stroke: transparent; stroke-width: 12px; fill: none; vector-effect: non-scaling-stroke; stroke-dasharray: none; }
svg g.node, svg g.edge { transition: opacity .12s; }
svg.cz-focus g.node:not(.cz-lit), svg.cz-focus g.edge:not(.cz-lit) { opacity: .18; }
@media print { .cz-bar { display: none; } .cz-box { height: auto !important; overflow: visible; } }
`;
  const XLINK = "http://www.w3.org/1999/xlink";

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

  // The tooltips of a drawing as bubbles, shown at once and set out (the first line, what is
  // pointed at, in bold) rather than those of the browser, a second later; the edges easier to
  // point at; in a drawing of Graphviz, a node or an edge pointed at lit with what it touches,
  // the rest faded. Returns the function to call with the element under the pointer (or null).
  function explain(svg, doc) {
    const graphviz = !!svg.querySelector("g.graph");
    const nodes = new Map();
    const edgesOf = new Map();
    const endsOf = new Map();
    const own = (g) => [...g.children].find((c) => c.tagName.toLowerCase() === "title");
    // What the browser would show by itself: the title of the drawing, of its items.
    for (const title of svg.querySelectorAll(":scope > title, g.graph > title")) title.remove();
    for (const g of svg.querySelectorAll("g")) {
      const title = own(g);
      let tip = "";
      if (graphviz) {
        if (!g.matches(".node, .edge, .cluster")) continue;
        const key = title ? title.textContent : "";
        if (title) title.remove();
        // What the item is, for the host: `n12` a node, `n3->n5` an edge, `e3` the label of one.
        if (key) g.dataset.key = key;
        for (const a of g.querySelectorAll("a")) {
          tip = tip || a.getAttributeNS(XLINK, "title") || a.getAttribute("xlink:title") || a.getAttribute("title") || "";
          a.removeAttributeNS(XLINK, "title");
          a.removeAttribute("xlink:title");
          a.removeAttribute("title");
        }
        if (g.matches(".node")) {
          nodes.set(key, g);
        } else if (g.matches(".edge")) {
          endsOf.set(g, key.split(/->|--/).map((end) => end.split(":")[0].trim()));
        }
      } else if (title) {
        tip = title.textContent;
        title.remove();
      }
      if (tip) g.dataset.tip = tip;
      // An edge pointed at from a few pixels away rather than exactly on its line.
      if (g.matches(".edge")) {
        const path = g.querySelector("path, polyline");
        if (path) {
          const hit = path.cloneNode(false);
          hit.removeAttribute("id");
          hit.setAttribute("class", "cz-hit");
          path.before(hit);
        }
      }
    }
    for (const [edge, ends] of endsOf) {
      for (const end of ends) {
        if (!edgesOf.has(end)) edgesOf.set(end, []);
        edgesOf.get(end).push(edge);
      }
    }
    const keyOf = new Map([...nodes].map(([key, g]) => [g, key]));
    // What a node or an edge touches: its edges and the nodes at their other ends, through the
    // labels drawn as points of the edges (`e3`), which split an edge in two.
    const label = (key) => /^e\d+$/.test(key);
    function around(keys, edges = []) {
      const items = new Set(edges);
      const seen = new Set(keys);
      const pending = [...keys];
      for (const edge of edges) for (const end of endsOf.get(edge)) pending.push(end);
      while (pending.length) {
        const key = pending.pop();
        if (nodes.has(key)) items.add(nodes.get(key));
        if (!label(key) && !keys.includes(key)) continue;
        seen.add(key);
        for (const edge of edgesOf.get(key) ?? []) {
          items.add(edge);
          for (const end of endsOf.get(edge)) if (!seen.has(end)) pending.push(end);
        }
      }
      return items.size > 1 ? [...items] : [];
    }

    let bubble = doc.getElementById("cz-tip");
    if (!bubble) {
      bubble = doc.createElement("div");
      bubble.id = "cz-tip";
      bubble.className = "cz-tip";
      bubble.hidden = true;
      doc.body.append(bubble);
    }
    const lit = [];
    function light(items) {
      for (const item of lit) item.classList.remove("cz-lit");
      lit.length = 0;
      svg.classList.toggle("cz-focus", items.length > 0);
      for (const item of items) {
        item.classList.add("cz-lit");
        lit.push(item);
      }
    }
    let current = null;
    return function point(target, x, y) {
      const item = target && target.closest ? target.closest("g[data-tip], g.node, g.edge") : null;
      if (item !== current) {
        current = item;
        const tip = item && item.dataset.tip;
        bubble.hidden = !tip;
        if (tip) {
          bubble.replaceChildren(...tip.split("\n").map((line, i) => {
            const div = doc.createElement("div");
            div.textContent = line;
            if (i === 0) div.className = "cz-tip-head";
            return div;
          }));
        }
        let items = [];
        if (item && keyOf.has(item)) {
          items = around([keyOf.get(item)]);
        } else if (item && endsOf.has(item)) {
          items = around([], [item]);
        }
        light(items);
      }
      if (!bubble.hidden) {
        const view = doc.documentElement;
        const r = bubble.getBoundingClientRect();
        const left = x + 14 + r.width > view.clientWidth ? Math.max(4, x - 14 - r.width) : x + 14;
        const top = y + 18 + r.height > view.clientHeight ? Math.max(4, y - 10 - r.height) : y + 18;
        bubble.style.left = `${left}px`;
        bubble.style.top = `${top}px`;
      }
    };
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

    // The bubble of what is pointed at, and what it touches lit.
    const point = explain(svg, doc);
    container.addEventListener("pointermove", (event) => {
      if (event.pointerType === "touch") return;
      point(drag && dragged ? null : event.target, event.clientX, event.clientY);
    });
    container.addEventListener("pointerleave", () => point(null, 0, 0));
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
    // The drawing on the whole screen, fitted to it; Escape brings it back.
    if (doc.fullscreenEnabled && container.requestFullscreen) {
      bar.append(button("⛶", words.full, () => {
        if (doc.fullscreenElement === container) doc.exitFullscreen();
        else container.requestFullscreen().catch(() => {});
      }));
      doc.addEventListener("fullscreenchange", () => {
        if (doc.fullscreenElement === container || !doc.fullscreenElement) global.requestAnimationFrame(fit);
      });
    }
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
