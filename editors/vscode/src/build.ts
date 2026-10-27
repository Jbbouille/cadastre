// `cadastre build`, run from the editor: the renderings are written as the command line does.

import { execFile } from "node:child_process";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import * as vscode from "vscode";
import { LanguageClient } from "vscode-languageclient/node";

import { svgOf } from "./graphviz";

let output: vscode.OutputChannel | undefined;

function channel(): vscode.OutputChannel {
  output ??= vscode.window.createOutputChannel("Cadastre build");
  return output;
}

// Runs `cadastre build <root> [--view <view>]` and reports the written files.
export async function build(command: string, root: string, view?: string, client?: LanguageClient): Promise<void> {
  const args = ["build", root];
  if (view) args.push("--view", view);
  if (vscode.workspace.getConfiguration("cadastre").get<boolean>("diagnostics.strict")) args.push("--strict");

  const log = channel();
  log.appendLine(`> ${command} ${args.join(" ")}`);
  const result = await vscode.window.withProgress(
    { location: vscode.ProgressLocation.Window, title: `Cadastre: building ${view ?? "all views"}` },
    () =>
      new Promise<{ code: number; stdout: string; stderr: string; error?: Error }>((resolve) => {
        execFile(command, args, { cwd: root, maxBuffer: 64 * 1024 * 1024 }, (error, stdout, stderr) => {
          const code = error && typeof error.code === "number" ? error.code : error ? -1 : 0;
          resolve({ code, stdout, stderr, error: code === -1 ? error ?? undefined : undefined });
        });
      }),
  );
  log.append(result.stdout);
  log.append(result.stderr);

  if (result.error) {
    void vscode.window.showErrorMessage(`Cadastre: cannot run \`${command}\` (${result.error.message}).`);
    return;
  }
  const written = result.stdout
    .split(/\r?\n/)
    .filter((line) => line.startsWith("wrote "))
    .map((line) => line.slice("wrote ".length).trim());
  if (result.code !== 0 || written.length === 0) {
    const reason = result.stdout.split(/\r?\n/).find((line) => /error\(s\)/.test(line)) ?? `exit code ${result.code}`;
    const action = await vscode.window.showErrorMessage(`Cadastre: build failed (${reason}).`, "Show output");
    if (action) log.show(true);
    return;
  }

  const files = written.map((file) => (path.isAbsolute(file) ? file : path.join(root, file)));
  let note = "";
  if (result.stderr.includes("Graphviz")) {
    // Without `dot`, the command writes the DOT source: the extension lays it out itself.
    const svgs = await layOut(files.filter((file) => file.endsWith(".dot")), log, client);
    files.push(...svgs);
    note = " (SVG laid out by the extension: Graphviz is not installed)";
  }
  const names = files.map((file) => path.basename(file)).join(", ");
  const action = await vscode.window.showInformationMessage(
    `Cadastre: wrote ${names}${note}.`,
    "Reveal in explorer",
    "Show output",
  );
  if (action === "Reveal in explorer") {
    await vscode.commands.executeCommand("revealFileInOS", vscode.Uri.file(files[0]));
  } else if (action === "Show output") {
    log.show(true);
  }
}

// Writes `<view>.svg` next to each `<view>.dot`, with a WebAssembly build of Graphviz.
async function layOut(dotFiles: string[], log: vscode.OutputChannel, client?: LanguageClient): Promise<string[]> {
  const written: string[] = [];
  for (const file of dotFiles) {
    const svgFile = file.replace(/\.dot$/, ".svg");
    try {
      const svg = await svgOf(await fs.readFile(file, "utf8"), client);
      await fs.writeFile(svgFile, svg);
      written.push(svgFile);
      log.appendLine(`laid out ${svgFile}`);
    } catch (error) {
      log.appendLine(`cannot lay out ${file}: ${error}`);
    }
  }
  return written;
}
