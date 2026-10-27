package io.github.jbbouille.cadastre

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.notification.NotificationGroupManager
import com.intellij.notification.NotificationType
import com.intellij.openapi.project.Project
import com.intellij.openapi.vfs.VirtualFile
import com.redhat.devtools.lsp4ij.LanguageServerFactory
import com.redhat.devtools.lsp4ij.server.OSProcessStreamConnectionProvider
import com.redhat.devtools.lsp4ij.server.StreamConnectionProvider
import org.eclipse.lsp4j.services.LanguageServer

/** The language server of Cadastre, `cadastre lsp`, for the `.ca`, `.cav` and `.cac` files. */
class CadastreLanguageServerFactory : LanguageServerFactory {
    override fun createConnectionProvider(project: Project): StreamConnectionProvider {
        val executable = CadastreSettings.get().executable() ?: run {
            NotificationGroupManager.getInstance()
                .getNotificationGroup("Cadastre")
                .createNotification(
                    "Cadastre: the cadastre executable was not found",
                    "Install it on the PATH, or give its path in Settings > Tools > Cadastre.",
                    NotificationType.WARNING,
                )
                .notify(project)
            "cadastre"
        }
        val commandLine = GeneralCommandLine(executable, "lsp").withCharset(Charsets.UTF_8)
        project.basePath?.let { commandLine.withWorkDirectory(it) }
        return CadastreConnectionProvider(commandLine)
    }

    /** The requests of Cadastre beside those of LSP: `cadastre/render`, `cadastre/check`… */
    override fun getServerInterface(): Class<out LanguageServer> = CadastreServer::class.java
}

/** `cadastre lsp`, started with the settings of the diagnostics as initialization options. */
private class CadastreConnectionProvider(commandLine: GeneralCommandLine) : OSProcessStreamConnectionProvider(commandLine) {
    override fun getInitializationOptions(rootUri: VirtualFile?): Any = CadastreSettings.get().initializationOptions()
}
