// The composer of views in a panel: `cadastre serve --edit` started on the project, on a port of
// its own, and its page of the composer shown in the panel. The same page as on the site of the
// project: a view written by taking words among those proposed, saved into its `.cav` file.

import { ChildProcess, spawn } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

import * as vscode from "vscode";

interface Server {
  process: ChildProcess;
  // Where it listens: `http://127.0.0.1:53211/`.
  url: Promise<string>;
  panels: number;
}

// The server of each project with a composer open; stopped with its last panel.
const servers = new Map<string, Server>();

function serve(command: string, project: string, playground: string | undefined): Server {
  const existing = servers.get(project);
  if (existing) return existing;
  // Any free port: the server says which on its first line.
  const args = ["serve", project, "--edit", "--port", "0"];
  if (playground) args.push("--playground", playground);
  const child = spawn(command, args, { stdio: ["ignore", "pipe", "pipe"] });
  const url = new Promise<string>((resolve, reject) => {
    let out = "";
    let errors = "";
    child.stdout?.on("data", (chunk: Buffer) => {
      out += chunk.toString();
      const address = out.match(/http:\/\/[^\s/]+\//);
      if (address) resolve(address[0]);
    });
    child.stderr?.on("data", (chunk: Buffer) => {
      errors += chunk.toString();
    });
    child.on("error", reject);
    child.on("exit", (code) => {
      servers.delete(project);
      reject(new Error(errors.trim() || `cadastre serve stopped (${code})`));
    });
  });
  const server = { process: child, url, panels: 0 };
  servers.set(project, server);
  return server;
}

function release(project: string, server: Server): void {
  if (server.panels > 0) return;
  server.process.kill();
  servers.delete(project);
}

// Shows the composer on a view of the project, or on a new one.
export async function compose(context: vscode.ExtensionContext, command: string, project: string, view?: string): Promise<void> {
  // The page shipped with the extension of a release; otherwise the one `cadastre` finds by
  // itself (beside its executable, `CADASTRE_PLAYGROUND`).
  const shipped = path.join(context.extensionPath, "playground");
  const server = serve(command, project, fs.existsSync(path.join(shipped, "compose.html")) ? shipped : undefined);
  let base: string;
  try {
    base = await server.url;
    const page = await fetch(`${base}playground/compose.html`);
    if (!page.ok) {
      throw new Error("the composer was not found: it comes with the releases of Cadastre, in the playground folder beside the executable (or set CADASTRE_PLAYGROUND)");
    }
  } catch (error) {
    release(project, server);
    void vscode.window.showErrorMessage(`Cadastre: ${error instanceof Error ? error.message : String(error)}`);
    return;
  }
  // The address as the window reaches it: forwarded when the project is on another machine.
  const external = (await vscode.env.asExternalUri(vscode.Uri.parse(base))).toString(true).replace(/\/?$/, "/");
  const target = `${external}playground/compose.html#${view ? `view=${encodeURIComponent(view)}` : "new"}`;
  const panel = vscode.window.createWebviewPanel(
    "cadastre.composer",
    view ? `Compose: ${view}` : "Compose a view",
    { viewColumn: vscode.ViewColumn.Beside, preserveFocus: false },
    { enableScripts: true, retainContextWhenHidden: true },
  );
  panel.iconPath = new vscode.ThemeIcon("edit");
  panel.webview.html = `<!doctype html>
<html>
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; frame-src http: https:; style-src 'unsafe-inline'">
<style>html, body, iframe { margin: 0; padding: 0; width: 100%; height: 100%; border: 0; } body { overflow: hidden; }</style>
</head>
<body><iframe src="${target.replace(/"/g, "&quot;")}" allow="clipboard-read; clipboard-write"></iframe></body>
</html>`;
  server.panels++;
  panel.onDidDispose(() => {
    server.panels--;
    release(project, server);
  });
}

// Stops every server started for a composer: the extension is deactivated.
export function stopComposers(): void {
  for (const server of servers.values()) server.process.kill();
  servers.clear();
}
