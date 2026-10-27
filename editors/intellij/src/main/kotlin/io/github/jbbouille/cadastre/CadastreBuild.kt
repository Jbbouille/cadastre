package io.github.jbbouille.cadastre

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.execution.process.CapturingProcessHandler
import com.intellij.execution.ui.ConsoleViewContentType
import com.intellij.ide.actions.RevealFileAction
import com.intellij.notification.NotificationAction
import com.intellij.notification.NotificationType
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.fileEditor.FileEditorManager
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.Task
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.LocalFileSystem
import java.nio.file.Path
import java.util.concurrent.TimeUnit

/** `cadastre build`, run from the IDE: the renderings are written as the command line does. */
object CadastreBuild {
    /** Runs `cadastre build <root> [--view <view>]` in the background and reports the written files. */
    fun run(project: Project, root: Path, view: String?) {
        val executable = CadastreSettings.get().executable()
            ?: return Cadastre.warn(project, "the cadastre executable was not found: install it on the PATH, or give its path in Settings > Tools > Cadastre.")
        val arguments = buildList {
            add("build")
            add(root.toString())
            if (view != null) addAll(listOf("--view", view))
            if (CadastreSettings.get().state.strict) add("--strict")
        }
        object : Task.Backgroundable(project, "Cadastre: building ${view ?: "all views"}", true) {
            override fun run(indicator: ProgressIndicator) {
                val commandLine = GeneralCommandLine(listOf(executable) + arguments)
                    .withWorkDirectory(root.toFile())
                    .withCharset(Charsets.UTF_8)
                val output = runCatching { CapturingProcessHandler(commandLine).runProcessWithProgressIndicator(indicator) }
                    .getOrElse { error ->
                        ApplicationManager.getApplication().invokeLater {
                            Cadastre.warn(project, "cannot run $executable (${error.message}).")
                        }
                        return
                    }
                val written = output.stdout.lines()
                    .filter { it.startsWith("wrote ") }
                    .map { root.resolve(it.removePrefix("wrote ").trim()) }
                LocalFileSystem.getInstance().refreshNioFiles(written)
                ApplicationManager.getApplication().invokeLater { report(project, executable, arguments, output, written) }
            }
        }.queue()
    }

    private fun report(
        project: Project,
        executable: String,
        arguments: List<String>,
        output: com.intellij.execution.process.ProcessOutput,
        written: List<Path>,
    ) {
        val console = CadastreConsole.get(project)
        val view = console.console(CadastreConsole.Tab.BUILD)
        view.clear()
        view.print("> $executable ${arguments.joinToString(" ")}\n", ConsoleViewContentType.SYSTEM_OUTPUT)
        view.print(output.stdout, ConsoleViewContentType.NORMAL_OUTPUT)
        view.print(output.stderr, ConsoleViewContentType.ERROR_OUTPUT)
        val showOutput = NotificationAction.createSimple("Show output") { console.reveal(CadastreConsole.Tab.BUILD) }
        if (output.exitCode != 0 || written.isEmpty()) {
            val reason = output.stdout.lines().firstOrNull { "error(s)" in it } ?: "exit code ${output.exitCode}"
            Cadastre.notification("build failed ($reason).", NotificationType.ERROR).addAction(showOutput).notify(project)
            return
        }
        // Without Graphviz, the command line writes the DOT source instead of the SVG.
        val note = if ("Graphviz" in output.stderr) " Graphviz is not installed: the DOT source was written instead of the SVG." else ""
        val names = written.joinToString(", ") { it.fileName.toString() }
        Cadastre.notification("wrote $names.$note", NotificationType.INFORMATION)
            .addAction(NotificationAction.createSimpleExpiring("Open") {
                LocalFileSystem.getInstance().findFileByNioFile(written[0])?.let { FileEditorManager.getInstance(project).openFile(it, true) }
            })
            .addAction(NotificationAction.createSimple("Reveal") { RevealFileAction.openFile(written[0]) })
            .addAction(showOutput)
            .notify(project)
    }
}

/** Graphviz (`dot` on the `PATH`), to lay out the DOT of the views as the command line does. */
object Graphviz {
    fun available(): Boolean = CadastreSettings.onPath("dot") != null

    /** The output of `dot -T<format>` for a DOT source; `null` without Graphviz or on failure. */
    fun render(dot: String, format: String): String? {
        val executable = CadastreSettings.onPath("dot") ?: return null
        return runCatching {
            val process = ProcessBuilder(executable, "-T$format").redirectError(ProcessBuilder.Redirect.DISCARD).start()
            val reader = ApplicationManager.getApplication().executeOnPooledThread<String> {
                process.inputStream.readAllBytes().toString(Charsets.UTF_8)
            }
            process.outputStream.use { it.write(dot.toByteArray(Charsets.UTF_8)) }
            val text = reader.get(60, TimeUnit.SECONDS)
            text.takeIf { process.waitFor(60, TimeUnit.SECONDS) && process.exitValue() == 0 }
        }.getOrNull()
    }
}
