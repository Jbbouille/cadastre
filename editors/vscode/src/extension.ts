// Cadastre for VS Code: starts `cadastre lsp` for .ca, .cav and .cac files, previews and builds views.

import * as fs from "node:fs";
import * as path from "node:path";

import * as vscode from "vscode";
import { LanguageClient, LanguageClientOptions, ServerOptions } from "vscode-languageclient/node";

import { build } from "./build";
import { Preview, projectOf } from "./preview";

let client: LanguageClient | undefined;
// The `cadastre` executable shipped in the extension of a release, if any.
let bundled: string | undefined;

// The results of the checks shown on demand.
const checks = vscode.window.createOutputChannel("Cadastre checks");

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  bundled = bundledServer(context.extensionPath);
  context.subscriptions.push(
    vscode.commands.registerCommand("cadastre.restartServer", async () => {
      await stop();
      await start();
    }),
    vscode.commands.registerCommand("cadastre.previewView", async (uri?: string, view?: string) => {
      const target = await viewTarget(uri, view);
      if (target) {
        Preview.show(context, () => client, target.uri, target.view, () => build(serverCommand(), projectOf(target.uri), target.view, client));
      }
    }),
    // The result of a check, as text in the output panel.
    vscode.commands.registerCommand("cadastre.showCheck", async (uri: string, check: string) => {
      if (!client) {
        void vscode.window.showWarningMessage("Cadastre: the language server is not running.");
        return;
      }
      const report = await client.sendRequest<string>("cadastre/check", { uri, view: check });
      checks.clear();
      checks.append(report);
      checks.show(true);
    }),
    vscode.commands.registerCommand("cadastre.buildView", async (uri?: string, view?: string) => {
      const target = await viewTarget(uri, view);
      if (target) await build(serverCommand(), projectOf(target.uri), target.view, client);
    }),
    vscode.commands.registerCommand("cadastre.buildViews", async (resource?: vscode.Uri) => {
      const uri = resource?.toString() ?? currentUri();
      if (uri) await build(serverCommand(), projectOf(uri), undefined, client);
      else void vscode.window.showWarningMessage("Cadastre: open a .ca, .cav or .cac file, or a folder.");
    }),
    vscode.commands.registerCommand("cadastre.exportModel", async () => {
      const uri = currentUri();
      if (!uri || !client) {
        void vscode.window.showWarningMessage("Cadastre: open a .ca, .cav or .cac file of the project to export.");
        return;
      }
      const choice = await vscode.window.showQuickPick(
        [
          { label: "ArchiMate", description: "Archi, Sparx EA (Open Exchange Format)", format: "archimate", extension: "archimate.xml" },
          { label: "GraphML", description: "yEd, Gephi, Cytoscape, NetworkX", format: "graphml", extension: "graphml" },
          { label: "JSON", description: "the model, for scripts", format: "json", extension: "json" },
        ],
        { placeHolder: "Export the whole registry as…" },
      );
      if (!choice) return;
      const content = await client.sendRequest<string | null>("cadastre/export", { uri, format: choice.format });
      if (content === null) {
        void vscode.window.showErrorMessage("Cadastre: fix the errors of the project to export it.");
        return;
      }
      const folder = vscode.workspace.workspaceFolders?.[0]?.uri;
      const target = await vscode.window.showSaveDialog({
        defaultUri: folder ? vscode.Uri.joinPath(folder, `model.${choice.extension}`) : undefined,
      });
      if (target) await vscode.workspace.fs.writeFile(target, new TextEncoder().encode(content));
    }),
    vscode.workspace.onDidChangeTextDocument((event) => {
      if (event.document.languageId === "cadastre" && event.contentChanges.length > 0) Preview.refreshAll();
    }),
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration("cadastre.server.path")) {
        await stop();
        await start();
      }
    }),
  );
  await start();
}

export async function deactivate(): Promise<void> {
  await stop();
}

// A file of the current project: the active Cadastre editor, otherwise the first workspace folder.
function currentUri(): string | undefined {
  const editor = vscode.window.activeTextEditor;
  if (editor?.document.languageId === "cadastre") return editor.document.uri.toString();
  return vscode.workspace.workspaceFolders?.[0]?.uri.toString();
}

// The view to preview or build: given by a code lens, or picked among the views of the project.
async function viewTarget(uri?: string, view?: string): Promise<{ uri: string; view: string } | undefined> {
  uri ??= currentUri();
  if (!uri) {
    void vscode.window.showWarningMessage("Cadastre: open a .ca, .cav or .cac file, or a folder.");
    return undefined;
  }
  if (view) return { uri, view };
  if (!client) {
    void vscode.window.showWarningMessage("Cadastre: the language server is not running.");
    return undefined;
  }
  const views = await client.sendRequest<string[]>("cadastre/views", { uri });
  if (views.length === 0) {
    void vscode.window.showWarningMessage("Cadastre: this project has no view (declare one in a .cav file).");
    return undefined;
  }
  const picked = views.length === 1 ? views[0] : await vscode.window.showQuickPick(views, { placeHolder: "View" });
  return picked ? { uri, view: picked } : undefined;
}

// The executable: the `cadastre.server.path` setting, then the `CADASTRE_SERVER`
// environment variable (used when developing the extension), then the executable shipped in
// the extension, then `cadastre` from the PATH.
function serverCommand(): string {
  const configured = vscode.workspace.getConfiguration("cadastre").get<string>("server.path");
  return configured || process.env.CADASTRE_SERVER || bundled || "cadastre";
}

function bundledServer(extensionPath: string): string | undefined {
  const file = path.join(extensionPath, "bin", process.platform === "win32" ? "cadastre.exe" : "cadastre");
  if (!fs.existsSync(file)) return undefined;
  // A .vsix does not keep file modes.
  if (process.platform !== "win32") {
    try {
      fs.chmodSync(file, 0o755);
    } catch {
      // Read-only installation: the mode may already be right.
    }
  }
  return file;
}

async function start(): Promise<void> {
  const command = serverCommand();
  const serverOptions: ServerOptions = { command, args: ["lsp"] };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "cadastre" }],
    initializationOptions: vscode.workspace.getConfiguration("cadastre"),
    synchronize: { configurationSection: "cadastre" },
  };
  client = new LanguageClient("cadastre", "Cadastre", serverOptions, clientOptions);
  try {
    await client.start();
  } catch (error) {
    client = undefined;
    const action = await vscode.window.showErrorMessage(
      `Cadastre: cannot start the language server \`${command} lsp\` (${error}). ` +
        "Install `cadastre` on the PATH or set `cadastre.server.path`. Highlighting still works without it.",
      "Open settings",
    );
    if (action) {
      await vscode.commands.executeCommand("workbench.action.openSettings", "cadastre.server.path");
    }
  }
}

async function stop(): Promise<void> {
  const running = client;
  client = undefined;
  await running?.stop();
}
