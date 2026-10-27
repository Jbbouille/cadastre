package io.github.jbbouille.cadastre

import com.intellij.notification.Notification
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.application.ReadAction
import com.intellij.openapi.fileEditor.FileEditorManager
import com.intellij.openapi.project.Project
import com.intellij.openapi.roots.ProjectFileIndex
import com.intellij.openapi.ui.popup.JBPopupFactory
import com.intellij.openapi.vfs.LocalFileSystem
import com.intellij.openapi.vfs.VirtualFile
import com.redhat.devtools.lsp4ij.LSPIJUtils
import com.redhat.devtools.lsp4ij.LanguageServerManager
import com.redhat.devtools.lsp4ij.commands.CommandExecutor
import org.eclipse.lsp4j.jsonrpc.services.JsonRequest
import org.eclipse.lsp4j.services.LanguageServer
import java.net.URI
import java.nio.file.Path
import java.util.concurrent.CompletableFuture

/** The requests of the Cadastre language server beside those of LSP. */
interface CadastreServer : LanguageServer {
    /** The view names of the project of a file. */
    @JsonRequest("cadastre/views")
    fun views(params: ViewParams): CompletableFuture<List<String>>

    /** The result of a check, as text: its expectation, what it selects, what fails it. */
    @JsonRequest("cadastre/check")
    fun check(params: ViewParams): CompletableFuture<String>

    /** A view rendered from the open documents; its `svg` outputs are DOT, to lay out. */
    @JsonRequest("cadastre/render")
    fun render(params: ViewParams): CompletableFuture<Rendering>

    /** An export of the whole registry; `null` when the project has errors. */
    @JsonRequest("cadastre/export")
    fun export(params: ExportParams): CompletableFuture<String?>

    /** A draw.io file laid out by Graphviz: `layout`, the `-Tjson` output of the DOT of the view. */
    @JsonRequest("cadastre/drawio")
    fun drawio(params: DrawioParams): CompletableFuture<String?>

    /** An SVG laid out by Graphviz, with the icons of the kinds. */
    @JsonRequest("cadastre/icons")
    fun icons(params: IconsParams): CompletableFuture<String>
}

data class ViewParams(val uri: String, val view: String? = null)

data class ExportParams(val uri: String, val format: String)

data class DrawioParams(val uri: String, val view: String, val layout: String)

data class IconsParams(val svg: String)

class Rendering {
    var view: String = ""
    var outputs: List<Output> = emptyList()
    var messages: List<String> = emptyList()
    var ok: Boolean = false

    /** An output: `content` in its format; a `drawio` one with the DOT of the view, an `xlsx` one in base64. */
    class Output {
        var format: String = ""
        var content: String = ""
        var dot: String? = null
    }
}

/** What the actions share: the language server, the files and views they act on, notifications. */
object Cadastre {
    const val SERVER_ID = "cadastre"

    /** The language server of a project, started if needed; `null` when it cannot run. */
    fun server(project: Project): CompletableFuture<CadastreServer?> =
        LanguageServerManager.getInstance(project).getLanguageServer(SERVER_ID)
            .thenCompose { item -> item?.initializedServer ?: CompletableFuture.completedFuture(null) }
            .thenApply { it as? CadastreServer }

    fun restart(project: Project) {
        LanguageServerManager.getInstance(project)
            .start(SERVER_ID, LanguageServerManager.StartOptions().setForceRestart(true))
    }

    fun isCadastre(file: VirtualFile?): Boolean = file != null && !file.isDirectory && file.extension in setOf("ca", "cav", "cac")

    fun uri(file: VirtualFile): String = LSPIJUtils.toUriAsString(file)

    fun file(uri: String): VirtualFile? =
        runCatching { LocalFileSystem.getInstance().findFileByNioFile(Path.of(URI(uri))) }.getOrNull()

    /** The project of a file, as the language server sees it: its content root, or its directory. */
    fun root(project: Project, file: VirtualFile): Path {
        val root = ReadAction.compute<VirtualFile?, RuntimeException> {
            ProjectFileIndex.getInstance(project).getContentRootForFile(file)
        } ?: if (file.isDirectory) file else file.parent
        return root.toNioPath()
    }

    /** The arguments of the code lens that ran an action (`[uri, name]`), if any. */
    fun lensArguments(event: AnActionEvent): Pair<String, String>? {
        val command = event.getData(CommandExecutor.LSP_COMMAND) ?: return null
        val uri = runCatching { command.getArgumentAt(0, String::class.java) }.getOrNull() ?: return null
        val name = runCatching { command.getArgumentAt(1, String::class.java) }.getOrNull() ?: return null
        return uri to name
    }

    /** A Cadastre file of the current project: the selected one, otherwise the one being edited. */
    fun currentFile(event: AnActionEvent): VirtualFile? {
        val project = event.project ?: return null
        event.getData(CommonDataKeys.VIRTUAL_FILE)?.takeIf { isCadastre(it) }?.let { return it }
        return FileEditorManager.getInstance(project).selectedFiles.firstOrNull { isCadastre(it) }
    }

    /**
     * The view to act on: given by a code lens, or picked among the views of the project of the
     * current file; `then` runs on the event dispatch thread.
     */
    fun withView(event: AnActionEvent, then: (uri: String, view: String) -> Unit) {
        val project = event.project ?: return
        lensArguments(event)?.let { (uri, view) -> return then(uri, view) }
        val file = currentFile(event) ?: return warn(project, "open a .ca, .cav or .cac file of the project.")
        val uri = uri(file)
        server(project).thenCompose { server -> server?.views(ViewParams(uri)) ?: CompletableFuture.completedFuture(null) }
            .whenComplete { views, error ->
                ApplicationManager.getApplication().invokeLater {
                    when {
                        error != null -> warn(project, "the language server did not answer (${error.message}).")
                        views == null -> warn(project, "the language server is not running.")
                        views.isEmpty() -> warn(project, "this project has no view (declare one in a .cav file).")
                        views.size == 1 -> then(uri, views[0])
                        else -> JBPopupFactory.getInstance()
                            .createPopupChooserBuilder(views)
                            .setTitle("View")
                            .setItemChosenCallback { then(uri, it) }
                            .createPopup()
                            .showCenteredInCurrentWindow(project)
                    }
                }
            }
    }

    fun notification(content: String, type: NotificationType): Notification =
        NotificationGroupManager.getInstance().getNotificationGroup("Cadastre").createNotification("Cadastre", content, type)

    fun warn(project: Project, message: String) = notification(message, NotificationType.WARNING).notify(project)
}
