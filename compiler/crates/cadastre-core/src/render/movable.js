// A view to move by hand, drawn by Cytoscape.js from the elements of `render::cytoscape`: its
// nodes and frames dragged (a frame with its content), its edges bent (cytoscape-edge-editing:
// drag an edge, or right click to add or remove a bend). Nothing goes back to the files.
//
// Shared by the playground, the site and the VS Code preview; it needs `cytoscape`, `Konva`,
// cytoscape-context-menus and cytoscape-edge-editing loaded first. Copied in `web/movable.js`.
(function (global) {
  const FONT = "Helvetica, Arial, sans-serif";
  const STYLE = [
    { selector: "node", style: {
      "shape": "data(shape)", "width": "data(width)", "height": "data(height)",
      "label": "data(label)", "text-wrap": "wrap", "text-valign": "center", "text-halign": "center",
      "font-family": FONT, "font-size": 10, "color": "data(text)",
      "background-color": "data(fill)", "border-width": 1, "border-color": "data(color)", "border-style": "data(line)",
    } },
    { selector: "node[icon]", style: {
      "background-image": "data(icon)", "background-width": 14, "background-height": 14,
      "background-position-x": 8, "background-position-y": "50%", "background-clip": "none", "text-margin-x": 9,
    } },
    { selector: "node[?frame]", style: {
      "shape": "rectangle", "text-valign": "top", "text-halign": "center", "color": "#333",
      "font-size": 11, "background-opacity": 0, "padding": 12,
    } },
    { selector: "node[?frame][?rounded]", style: { "shape": "round-rectangle" } },
    { selector: "node[?frame][?filled]", style: { "background-color": "data(fill)", "background-opacity": 1 } },
    { selector: "node[?frame][icon]", style: { "background-position-x": 6, "background-position-y": 6, "text-margin-x": 0 } },
    { selector: "edge", style: {
      "curve-style": "bezier", "width": 1.2, "line-color": "data(color)", "line-style": "data(line)",
      "target-arrow-color": "data(color)", "source-arrow-color": "data(color)", "arrow-scale": 0.8,
      "label": "data(label)", "font-size": 9, "text-wrap": "wrap", "text-max-width": 140, "color": "data(color)",
      "font-family": FONT, "text-background-color": "#fff", "text-background-opacity": 1, "text-background-padding": 1,
    } },
    { selector: "edge[head != 'none']", style: { "target-arrow-shape": "triangle" } },
    { selector: "edge[head = 'hollow']", style: { "target-arrow-fill": "hollow" } },
    { selector: "edge[tail != 'none']", style: { "source-arrow-shape": "triangle" } },
    { selector: "edge[tail = 'hollow']", style: { "source-arrow-fill": "hollow" } },
    { selector: ":selected", style: { "overlay-color": "#2563eb", "overlay-opacity": 0.12 } },
  ];

  const WORDS = {
    en: { add: "Add a bend", remove: "Remove the bend", removeAll: "Remove all the bends" },
    fr: { add: "Ajouter un coude", remove: "Retirer le coude", removeAll: "Retirer tous les coudes" },
  };

  // Where the nodes are, by qualified name, and the bends of the edges, by their ends and label:
  // what a new drawing of the same view keeps (`show(…, { kept })`).
  function kept(cy) {
    const nodes = {};
    for (const node of cy.nodes("[name][!frame]")) nodes[node.data("name")] = { ...node.position() };
    const edges = {};
    for (const edge of cy.edges()) {
      const bends = cy.edgeEditing("get").getAnchorsAsArray(edge);
      if (bends?.length) edges[edgeKey(edge)] = positions(bends);
    }
    return { nodes, edges };
  }

  const edgeKey = (edge) => `${edge.source().data("name") ?? edge.source().id()}>${edge.target().data("name") ?? edge.target().id()}|${edge.data("label")}`;
  const positions = (flat) => Array.from({ length: flat.length / 2 }, (_, i) => ({ x: flat[2 * i], y: flat[2 * i + 1] }));

  // Draws `elements` ({ nodes, edges }) in `box`; returns the Cytoscape instance.
  function show(box, elements, options = {}) {
    const words = WORDS[options.language] ?? WORDS.en;
    // Empty first: cytoscape-edge-editing reads its options in the style of the edges.
    const cy = cytoscape({ container: box, style: STYLE, wheelSensitivity: 0.2 });
    cy.edgeEditing({
      initAnchorsAutomatically: true,
      bendCornersIsRoundFunction: () => true,
      handleReconnectEdge: false,
      enableCreateAnchorOnDrag: true,
      addBendMenuItemTitle: words.add,
      removeBendMenuItemTitle: words.remove,
      removeAllBendMenuItemTitle: words.removeAll,
      addControlMenuItemTitle: false, removeControlMenuItemTitle: false, removeAllControlMenuItemTitle: false,
    });
    const previous = options.kept;
    cy.add([
      ...elements.nodes.map((e) => {
        const at = previous && e.data.name && previous.nodes[e.data.name];
        return { group: "nodes", ...e, ...(at ? { position: at } : {}) };
      }),
      ...elements.edges.map((e) => ({ group: "edges", ...e })),
    ]);
    for (const edge of cy.edges()) {
      const bends = previous?.edges[edgeKey(edge)];
      if (bends) edge.data("bendPointPositions", bends);
      if (edge.data("bendPointPositions") || edge.source().same(edge.target())) continue;
      // A straight edge gets a bend at its middle, still straight: the extension bends by
      // dragging only an edge that has one.
      const [a, b] = [edge.source().position(), edge.target().position()];
      edge.data("bendPointPositions", [{ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }]);
    }
    cy.edgeEditing("get").initAnchorPoints(cy.edges());
    cy.style().update();
    // An edge Cytoscape cannot draw (a bend inside one of its ends: it finds no point where the
    // line leaves the node) is skipped without a word: it is drawn straight instead, or, when it
    // still is not (between a frame and a node it holds, which Cytoscape draws without bends
    // only), as a curve. Never lost.
    const unseen = (edges) => edges.filter((edge) => !edge.source().same(edge.target()) && !Number.isFinite(edge.sourceEndpoint().x));
    const lost = unseen(cy.edges());
    if (lost.length) {
      for (const edge of lost) {
        const [a, b] = [edge.source().position(), edge.target().position()];
        edge.data("bendPointPositions", [{ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }]);
      }
      cy.edgeEditing("get").initAnchorPoints(lost);
      cy.style().update();
      for (const edge of unseen(lost)) edge.style("curve-style", "bezier");
      console.warn(`cadastre: ${lost.length} edge(s) redrawn, their bends inside their ends`);
    }
    if (options.viewport) cy.viewport(options.viewport);
    else cy.fit(undefined, 20);
    return cy;
  }

  // The drawing as a PNG, as a data URL.
  const png = (cy) => cy.png({ full: true, scale: 2, bg: "#ffffff" });

  global.cadastreMovable = { show, kept, png };
})(globalThis);
