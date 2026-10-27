package io.github.jbbouille.cadastre

import com.intellij.notification.NotificationType
import com.intellij.openapi.actionSystem.ActionUpdateThread
import com.intellij.openapi.actionSystem.AnAction
import com.intellij.openapi.actionSystem.AnActionEvent
import com.intellij.openapi.actionSystem.CommonDataKeys
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.fileChooser.FileChooserFactory
import com.intellij.openapi.fileChooser.FileSaverDescriptor
import com.intellij.openapi.project.DumbAware
import com.intellij.openapi.project.guessProjectDir
import com.intellij.openapi.ui.popup.JBPopupFactory
import com.intellij.openapi.vfs.LocalFileSystem
import com.intellij.ui.SimpleListCellRenderer
import java.nio.file.Files

// The commands of the code lenses (`cadastre.previewView`, `cadastre.buildView`,
// `cadastre.showCheck`), run by LSP4IJ as the actions of the same id, and the commands of the
// Tools > Cadastre menu, as in VS Code.

/** Base of the actions: shown in a project, on the event dispatch thread. */
abstract class CadastreAction : AnAction(), DumbAware {
    override fun getActionUpdateThread() = ActionUpdateThread.BGT

    override fun update(event: AnActionEvent) {
        event.presentation.isEnabledAndVisible = event.project != null
    }
}

/** Preview a view in the editors of the IDE, kept up to date as you type. */
class PreviewViewAction : CadastreAction() {
    override fun actionPerformed(event: AnActionEvent) {
        val project = event.project ?: return
        Cadastre.withView(event) { uri, view -> CadastrePreview.get(project).show(uri, view) }
    }
}

/** `cadastre build --view <view>`: the renderings of a view written to `out/`. */
class BuildViewAction : CadastreAction() {
    override fun actionPerformed(event: AnActionEvent) {
        val project = event.project ?: return
        Cadastre.withView(event) { uri, view ->
            val file = Cadastre.file(uri) ?: return@withView Cadastre.warn(project, "cannot find $uri.")
            CadastreBuild.run(project, Cadastre.root(project, file), view)
        }
    }
}

/** `cadastre build`: every view of the project of the selected file or folder. */
class BuildViewsAction : CadastreAction() {
    override fun actionPerformed(event: AnActionEvent) {
        val project = event.project ?: return
        val file = event.getData(CommonDataKeys.VIRTUAL_FILE)?.takeIf { it.isDirectory || Cadastre.isCadastre(it) }
            ?: Cadastre.currentFile(event)
            ?: project.guessProjectDir()
            ?: return Cadastre.warn(project, "open a .ca, .cav or .cac file, or select a folder.")
        CadastreBuild.run(project, Cadastre.root(project, file), null)
    }
}

/** The result of a check (the title of its code lens), in detail in the Cadastre tool window. */
class ShowCheckAction : CadastreAction() {
    override fun actionPerformed(event: AnActionEvent) {
        val project = event.project ?: return
        val (uri, check) = Cadastre.lensArguments(event) ?: return
        Cadastre.server(project)
            .thenCompose { server -> server?.check(ViewParams(uri, check)) ?: java.util.concurrent.CompletableFuture.completedFuture(null) }
            .whenComplete { report, error ->
                ApplicationManager.getApplication().invokeLater {
                    when {
                        error != null -> Cadastre.warn(project, "the language server did not answer (${error.message}).")
                        report == null -> Cadastre.warn(project, "the language server is not running.")
                        else -> CadastreConsole.get(project).show(CadastreConsole.Tab.CHECKS, report)
                    }
                }
            }
    }
}

/** The whole registry exported as ArchiMate, GraphML or JSON, saved where the user chooses. */
class ExportModelAction : CadastreAction() {
    private class Format(val label: String, val description: String, val format: String, val extension: String)

    private val formats = listOf(
        Format("ArchiMate", "Archi, Sparx EA (Open Exchange Format)", "archimate", "archimate.xml"),
        Format("GraphML", "yEd, Gephi, Cytoscape, NetworkX", "graphml", "graphml"),
        Format("JSON", "the model, for scripts", "json", "json"),
    )

    override fun actionPerformed(event: AnActionEvent) {
        val project = event.project ?: return
        val file = Cadastre.currentFile(event) ?: return Cadastre.warn(project, "open a .ca, .cav or .cac file of the project to export.")
        val uri = Cadastre.uri(file)
        JBPopupFactory.getInstance()
            .createPopupChooserBuilder(formats)
            .setTitle("Export the Whole Registry As")
            .setRenderer(SimpleListCellRenderer.create("") { "${it.label}  —  ${it.description}" })
            .setItemChosenCallback { format ->
                Cadastre.server(project)
                    .thenCompose { server -> server?.export(ExportParams(uri, format.format)) ?: java.util.concurrent.CompletableFuture.completedFuture(null) }
                    .whenComplete { content, error ->
                        ApplicationManager.getApplication().invokeLater {
                            if (error != null || content == null) {
                                Cadastre.notification("fix the errors of the project to export it.", NotificationType.ERROR).notify(project)
                                return@invokeLater
                            }
                            val descriptor = FileSaverDescriptor("Export the Registry", "${format.label} file", format.extension.substringAfterLast('.'))
                            val target = FileChooserFactory.getInstance().createSaveFileDialog(descriptor, project)
                                .save(Cadastre.root(project, file), "model.${format.extension}") ?: return@invokeLater
                            Files.writeString(target.file.toPath(), content)
                            LocalFileSystem.getInstance().refreshNioFiles(listOf(target.file.toPath()))
                        }
                    }
            }
            .createPopup()
            .showCenteredInCurrentWindow(project)
    }
}

/** Restarts `cadastre lsp`, after installing a new version for instance. */
class RestartServerAction : CadastreAction() {
    override fun actionPerformed(event: AnActionEvent) {
        Cadastre.restart(event.project ?: return)
    }
}
