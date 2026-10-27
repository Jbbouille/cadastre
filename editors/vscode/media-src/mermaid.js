// Mermaid for the preview panel, with the ELK layout the Mermaid outputs ask for, in one script.
import mermaid from "mermaid";
import elk from "@mermaid-js/layout-elk";

mermaid.registerLayoutLoaders(elk);
globalThis.mermaid = mermaid;
