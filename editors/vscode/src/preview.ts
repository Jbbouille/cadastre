// Live preview of a view: rendered by the language server from the open documents, laid
// out by a WebAssembly build of Graphviz (no `dot` needed), with the icons of the kinds; Mermaid
// drawn in the panel, Markdown rendered by VS Code.

import * as vscode from "vscode";
import { LanguageClient } from "vscode-languageclient/node";

import { layoutOf, svgOf } from "./graphviz";
import { previewHtml } from "./previewHtml";

export interface Rendering {
  view: string;
  // An `svg` output is DOT from the server; the panel receives the SVG instead. A `dot` output
  // keeps its DOT, laid out as `svg`; a `markdown` one gets its `html`. A `drawio` one comes with
  // the DOT of the view (`dot`): its file is laid out by Graphviz, and its diagram shown (`svg`).
  outputs: { format: string; content: string; svg?: string; html?: string; dot?: string }[];
  messages: string[];
  ok: boolean;
}

export class Preview {
  private static readonly panels = new Map<string, Preview>();

  private timer: NodeJS.Timeout | undefined;

  // Shows the preview of a view, reusing its panel when it is already open.
  static show(context: vscode.ExtensionContext, client: () => LanguageClient | undefined, uri: string, view: string, onBuild: () => void): void {
    const key = `${projectOf(uri)}#${view}`;
    const media = vscode.Uri.joinPath(context.extensionUri, "media");
    const existing = Preview.panels.get(key);
    if (existing) {
      existing.panel.reveal(undefined, true);
      existing.update();
      return;
    }
    const panel = vscode.window.createWebviewPanel(
      "cadastre.preview",
      `View: ${view}`,
      { viewColumn: vscode.ViewColumn.Beside, preserveFocus: true },
      { enableScripts: true, retainContextWhenHidden: true, localResourceRoots: [media] },
    );
    panel.iconPath = new vscode.ThemeIcon("type-hierarchy");
    const preview = new Preview(panel, client, uri, view);
    panel.webview.html = previewHtml(view, {
      mermaid: panel.webview.asWebviewUri(vscode.Uri.joinPath(media, "mermaid.js")).toString(),
      cspSource: panel.webview.cspSource,
    });
    panel.webview.onDidReceiveMessage((message) => {
      if (message?.type === "ready") preview.update();
      if (message?.type === "build") onBuild();
      if (message?.type === "open" && typeof message.url === "string") open(message.url);
      if (message?.type === "save" && typeof message.content === "string") {
        void save(view, message.content, message.format === "xlsx" ? "xlsx" : "drawio");
      }
    });
    panel.onDidDispose(() => Preview.panels.delete(key));
    Preview.panels.set(key, preview);
  }

  // Re-renders every open preview, shortly after the last edit.
  static refreshAll(): void {
    for (const preview of Preview.panels.values()) preview.schedule();
  }

  private constructor(
    private readonly panel: vscode.WebviewPanel,
    private readonly client: () => LanguageClient | undefined,
    private readonly uri: string,
    private readonly view: string,
  ) {}

  private schedule(): void {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.update(), 300);
  }

  private async update(): Promise<void> {
    const client = this.client();
    if (!client) {
      void this.panel.webview.postMessage({ type: "render", view: this.view, outputs: [], messages: ["The Cadastre language server is not running."], ok: false });
      return;
    }
    try {
      const rendering = await client.sendRequest<Rendering>("cadastre/render", { uri: this.uri, view: this.view });
      for (const output of rendering.outputs) {
        // `layout: elk`: already drawn by the compiler.
        if (output.format === "svg" && output.content.trimStart().startsWith("<svg")) {
          continue;
        } else if (output.format === "svg") {
          try {
            output.content = await svgOf(output.content, client);
          } catch (error) {
            output.content = "";
            rendering.messages.push(`Graphviz: ${error}`);
          }
        } else if (output.format === "dot") {
          try {
            output.svg = await svgOf(output.content, client);
          } catch (error) {
            rendering.messages.push(`Graphviz: ${error}`);
          }
        } else if (output.format === "drawio" && output.dot) {
          try {
            const layout = await layoutOf(output.dot);
            output.content =
              (await client.sendRequest<string | null>("cadastre/drawio", { uri: this.uri, view: this.view, layout })) ??
              output.content;
            output.svg = await svgOf(output.dot, client);
          } catch (error) {
            rendering.messages.push(`Graphviz: ${error}`);
          }
          delete output.dot;
        } else if (output.format === "markdown") {
          try {
            output.html = await vscode.commands.executeCommand<string>("markdown.api.render", output.content);
          } catch {
            // Without the Markdown extension of VS Code, the source.
          }
        }
      }
      void this.panel.webview.postMessage({ type: "render", ...rendering });
    } catch (error) {
      void this.panel.webview.postMessage({ type: "render", view: this.view, outputs: [], messages: [String(error)], ok: false });
    }
  }
}

// Saves the draw.io file or the Excel workbook (in base64) of a view where the user chooses,
// beside the project by default.
async function save(view: string, content: string, format: "drawio" | "xlsx"): Promise<void> {
  const folder = vscode.workspace.workspaceFolders?.[0]?.uri;
  const target = await vscode.window.showSaveDialog({
    defaultUri: folder ? vscode.Uri.joinPath(folder, `${view}.${format}`) : undefined,
    filters: format === "xlsx" ? { Excel: ["xlsx"] } : { "draw.io": ["drawio"] },
  });
  if (!target) return;
  const bytes = format === "xlsx" ? Buffer.from(content, "base64") : new TextEncoder().encode(content);
  await vscode.workspace.fs.writeFile(target, bytes);
}

// Opens a link of a rendering (the url or console of a resource) outside VS Code: a web page, or
// the client the system associates with ssh, rdp, vnc…; never a file nor a command.
const OPENED = ["http:", "https:", "ssh:", "sftp:", "ftp:", "rdp:", "vnc:", "telnet:", "mailto:"];

function open(url: string): void {
  try {
    const uri = vscode.Uri.parse(url, true);
    if (OPENED.includes(`${uri.scheme}:`)) void vscode.env.openExternal(uri);
  } catch {
    // Not a URL.
  }
}

// The project of a file, as the language server sees it: its workspace folder, or its directory.
export function projectOf(uri: string): string {
  const parsed = vscode.Uri.parse(uri);
  const folder = vscode.workspace.getWorkspaceFolder(parsed);
  return (folder?.uri ?? vscode.Uri.joinPath(parsed, "..")).fsPath;
}
