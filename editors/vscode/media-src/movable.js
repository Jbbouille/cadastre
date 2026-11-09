// Cytoscape.js and its extensions for the views to move by hand in the preview panel, with the
// drawing shared with the playground and the site, in one script.
import cytoscape from "cytoscape";
import Konva from "konva";
import contextMenus from "cytoscape-context-menus";
import edgeEditing from "cytoscape-edge-editing";
import menus from "cytoscape-context-menus/cytoscape-context-menus.css";
import "../../../compiler/crates/cadastre-core/src/render/movable.js";

globalThis.Konva = Konva;
globalThis.cytoscape = cytoscape;
cytoscape.use(contextMenus);
// cytoscape-edge-editing registers itself with Konva as well.
edgeEditing(cytoscape, Konva);
document.head.append(Object.assign(document.createElement("style"), { textContent: menus }));
