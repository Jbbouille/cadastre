package io.github.jbbouille.cadastre

import com.intellij.ide.plugins.PluginManagerCore
import com.intellij.notification.NotificationAction
import com.intellij.notification.NotificationType
import com.intellij.openapi.Disposable
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.application.PathManager
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import com.intellij.openapi.editor.EditorFactory
import com.intellij.openapi.editor.event.DocumentEvent
import com.intellij.openapi.editor.event.DocumentListener
import com.intellij.openapi.extensions.PluginId
import com.intellij.openapi.fileEditor.FileDocumentManager
import com.intellij.openapi.fileEditor.FileEditorManager
import com.intellij.openapi.options.ShowSettingsUtil
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.LocalFileSystem
import com.intellij.util.Alarm
import java.nio.file.Files
import java.nio.file.Path
import java.util.Base64
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit

/**
 * Live preview of a view in the editors of the IDE: the language server renders it from the open
 * documents, Graphviz lays out its diagrams, and each output is written as `cadastre build`
 * names it, in a directory of the IDE; the main one is opened (the SVG in the image editor, the
 * Mermaid diagrams with the Mermaid plugin, the tables in the HTML or Markdown editor) and
 * rewritten as the documents change, as long as one of them stays open.
 */
@Service(Service.Level.PROJECT)
class CadastrePreview(private val project: Project) : Disposable {
    private class Preview(val uri: String, val view: String) {
        @Volatile
        var files: List<Path> = emptyList()
    }

    private val previews = ConcurrentHashMap<String, Preview>()
    private val alarm = Alarm(Alarm.ThreadToUse.POOLED_THREAD, this)
    private val directory: Path = Path.of(PathManager.getSystemPath(), "cadastre-preview", project.locationHash)

    init {
        EditorFactory.getInstance().eventMulticaster.addDocumentListener(
            object : DocumentListener {
                override fun documentChanged(event: DocumentEvent) {
                    if (previews.isEmpty()) return
                    if (Cadastre.isCadastre(FileDocumentManager.getInstance().getFile(event.document))) schedule()
                }
            },
            this,
        )
    }

    /** Renders a view and opens its main output. */
    fun show(uri: String, view: String) {
        val preview = previews.getOrPut("$uri#$view") { Preview(uri, view) }
        ApplicationManager.getApplication().executeOnPooledThread { render(preview, explicit = true) }
    }

    /** Renders the open previews again, shortly after the last edit. */
    private fun schedule() {
        alarm.cancelAllRequests()
        alarm.addRequest(::refresh, 400)
    }

    private fun refresh() {
        val editors = FileEditorManager.getInstance(project)
        for ((key, preview) in previews) {
            val open = preview.files.any { path -> LocalFileSystem.getInstance().findFileByNioFile(path)?.let(editors::isFileOpen) == true }
            if (open) render(preview, explicit = false) else previews.remove(key)
        }
    }

    /**
     * Renders a preview: on the first rendering, opens its main output and reports its messages;
     * afterwards, rewrites its files silently, the last ones staying while the project has errors.
     */
    private fun render(preview: Preview, explicit: Boolean) {
        val server = runCatching { Cadastre.server(project).get(30, TimeUnit.SECONDS) }.getOrNull()
            ?: return later { if (explicit) Cadastre.warn(project, "the language server is not running.") }
        val rendering = runCatching { server.render(ViewParams(preview.uri, preview.view)).get(60, TimeUnit.SECONDS) }
            .getOrElse { error -> return later { if (explicit) Cadastre.warn(project, "cannot render ${preview.view} (${error.message}).") } }
        if (!rendering.ok) {
            if (explicit) later { message(preview.view, rendering.messages, NotificationType.ERROR) }
            return
        }
        val notes = rendering.messages.toMutableList()
        Files.createDirectories(directory)
        val files = rendering.outputs.mapNotNull { output -> write(server, preview, output, notes) }
        preview.files = files
        LocalFileSystem.getInstance().refreshNioFiles(files)
        if (!explicit) return
        later {
            val main = files.minByOrNull { path -> MAIN.indexOfFirst { path.fileName.toString().endsWith(".$it") }.let { if (it < 0) MAIN.size else it } }
                ?: return@later message(preview.view, notes + "this view renders nothing", NotificationType.WARNING)
            LocalFileSystem.getInstance().refreshAndFindFileByNioFile(main)?.let { FileEditorManager.getInstance(project).openFile(it, true) }
            if (main.fileName.toString().endsWith(".mmd") && !mermaidPlugin()) {
                Cadastre.notification(
                    "the Mermaid plugin draws ${main.fileName}; without it, its source is shown.",
                    NotificationType.INFORMATION,
                ).addAction(NotificationAction.createSimpleExpiring("Install the Mermaid plugin…") {
                    ShowSettingsUtil.getInstance().showSettingsDialog(project, "Plugins")
                }).notify(project)
            }
            val others = files.filter { it != main }
            if (notes.isNotEmpty() || others.isNotEmpty()) {
                val notification = Cadastre.notification(
                    (listOf("preview of ${preview.view}") + notes).joinToString("\n"),
                    if (notes.isEmpty()) NotificationType.INFORMATION else NotificationType.WARNING,
                )
                for (other in others) {
                    notification.addAction(NotificationAction.createSimple("Open ${other.fileName}") {
                        LocalFileSystem.getInstance().refreshAndFindFileByNioFile(other)?.let { FileEditorManager.getInstance(project).openFile(it, true) }
                    })
                }
                notification.notify(project)
            }
        }
    }

    /** Writes an output as `cadastre build` names it; `null` when it cannot be written. */
    private fun write(server: CadastreServer, preview: Preview, output: Rendering.Output, notes: MutableList<String>): Path? {
        val view = preview.view
        val (extension, bytes) = when (output.format) {
            "svg" -> when (val svg = if (drawn(output.content)) output.content else Graphviz.render(output.content, "svg")) {
                null -> {
                    notes += "Graphviz (dot) is not installed: the DOT source is shown instead of the SVG."
                    "dot" to output.content
                }
                // `layout: elk`: already drawn by the compiler, with its icons.
                else -> "svg" to if (drawn(output.content)) svg else (runCatching { server.icons(IconsParams(svg)).get(30, TimeUnit.SECONDS) }.getOrNull() ?: svg)
            }
            "drawio" -> {
                val layout = output.dot?.let { Graphviz.render(it, "json") }
                val laidOut = layout?.let { runCatching { server.drawio(DrawioParams(preview.uri, view, it)).get(30, TimeUnit.SECONDS) }.getOrNull() }
                "drawio" to (laidOut ?: output.content)
            }
            "xlsx" -> return runCatching { directory.resolve("$view.xlsx").also { Files.write(it, Base64.getDecoder().decode(output.content)) } }.getOrNull()
            else -> (EXTENSIONS[output.format] ?: "dot") to output.content
        }
        val path = directory.resolve("$view.$extension")
        val content = (bytes as String).toByteArray(Charsets.UTF_8)
        // Unchanged files are not rewritten: their editors keep their scroll and zoom.
        if (!Files.exists(path) || !Files.readAllBytes(path).contentEquals(content)) Files.write(path, content)
        return path
    }

    /** An `svg` output already drawn by the compiler (`layout: elk`), rather than DOT to lay out. */
    private fun drawn(content: String): Boolean = content.trimStart().startsWith("<svg")

    private fun message(view: String, messages: List<String>, type: NotificationType) =
        Cadastre.notification((listOf("preview of $view") + messages).joinToString("\n"), type).notify(project)

    private fun later(action: () -> Unit) = ApplicationManager.getApplication().invokeLater(action, project.disposed)

    private fun mermaidPlugin(): Boolean =
        PluginManagerCore.getPlugin(PluginId.getId("com.intellij.mermaid"))?.isEnabled == true

    override fun dispose() {}

    companion object {
        fun get(project: Project): CadastrePreview = project.service()

        /** The extensions of the outputs, as `cadastre build` writes them. */
        private val EXTENSIONS = mapOf(
            "text" to "txt",
            "mermaid" to "mmd",
            "sequence" to "sequence.mmd",
            "traversal" to "traversal.svg",
            "ascii" to "ascii.txt",
            "csv" to "csv",
            "tsv" to "tsv",
            "markdown" to "md",
            "html" to "html",
            "dot" to "dot",
        )

        /** The output opened, by order of preference. */
        private val MAIN = listOf("svg", "traversal.svg", "sequence.mmd", "mmd", "html", "md", "drawio", "dot", "ascii.txt", "txt")
    }
}
