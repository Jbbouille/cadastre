// DOT → SVG with a WebAssembly build of Graphviz, in the extension host: no `dot` needed.
// The icons of the kinds are then drawn by the language server (`cadastre/icons`).

import type { Viz } from "@viz-js/viz";
import { LanguageClient } from "vscode-languageclient/node";

let viz: Promise<Viz> | undefined;

// The layout Graphviz computes for a DOT (`-Tjson`), for the draw.io export.
export async function layoutOf(dot: string): Promise<string> {
  viz ??= (require("@viz-js/viz") as typeof import("@viz-js/viz")).instance();
  return (await viz).renderString(dot, { format: "json" });
}

export async function svgOf(dot: string, client: LanguageClient | undefined): Promise<string> {
  viz ??= (require("@viz-js/viz") as typeof import("@viz-js/viz")).instance();
  const svg = (await viz).renderString(dot, { format: "svg" });
  if (!client) return svg;
  try {
    return await client.sendRequest<string>("cadastre/icons", { svg });
  } catch {
    return svg;
  }
}
