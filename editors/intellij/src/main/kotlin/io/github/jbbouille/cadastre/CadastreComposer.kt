package io.github.jbbouille.cadastre

import com.intellij.execution.configurations.GeneralCommandLine
import com.intellij.ide.BrowserUtil
import com.intellij.openapi.Disposable
import com.intellij.openapi.application.ApplicationManager
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import com.intellij.openapi.fileEditor.impl.HTMLEditorProvider
import com.intellij.openapi.progress.ProgressIndicator
import com.intellij.openapi.progress.Task
import com.intellij.openapi.project.Project
import com.intellij.ui.jcef.JBCefApp
import java.net.HttpURLConnection
import java.net.URI
import java.net.URLEncoder
import java.nio.file.Path
import java.util.concurrent.ConcurrentHashMap
import kotlin.concurrent.thread

/**
 * The composer of views in an editor of the IDE: `cadastre serve --edit` started on the project,
 * on a port of its own, and its page of the composer shown by the browser of the IDE (JCEF), or
 * by that of the system. The same page as on the site of the project: a view written by taking
 * words among those proposed, saved into its `.cav` file.
 */
@Service(Service.Level.PROJECT)
class CadastreComposer(private val project: Project) : Disposable {
    private class Server(val process: Process, val url: String)

    /** The server of each project root with a composer shown; stopped with the project. */
    private val servers = ConcurrentHashMap<Path, Server>()

    /** Shows the composer on a view of the project at `root`, or on a new one. */
    fun show(root: Path, view: String?) {
        val executable = CadastreSettings.get().executable()
            ?: return Cadastre.warn(project, "the cadastre executable was not found: install it on the PATH, or give its path in Settings > Tools > Cadastre.")
        object : Task.Backgroundable(project, "Cadastre: starting the composer", true) {
            override fun run(indicator: ProgressIndicator) {
                val url = runCatching { serverUrl(executable, root) }.getOrElse { error ->
                    ApplicationManager.getApplication().invokeLater {
                        Cadastre.warn(project, error.message ?: "cannot start cadastre serve.")
                    }
                    return
                }
                val what = view?.let { "view=" + URLEncoder.encode(it, Charsets.UTF_8) } ?: "new"
                val page = "${url}playground/compose.html#$what"
                ApplicationManager.getApplication().invokeLater {
                    if (JBCefApp.isSupported()) {
                        HTMLEditorProvider.openEditor(project, view?.let { "Compose: $it" } ?: "Compose a View", HTMLEditorProvider.Request.url(page))
                    } else {
                        BrowserUtil.browse(page)
                    }
                }
            }
        }.queue()
    }

    /** Where the server of a root listens (`http://127.0.0.1:53211/`), started if needed. */
    @Synchronized
    private fun serverUrl(executable: String, root: Path): String {
        servers[root]?.takeIf { it.process.isAlive }?.let { return it.url }
        // Any free port: the server says which on a line of its output.
        val process = GeneralCommandLine(executable, "serve", root.toString(), "--edit", "--port", "0")
            .withCharset(Charsets.UTF_8)
            .withRedirectErrorStream(true)
            .createProcess()
        val reader = process.inputStream.bufferedReader()
        val said = StringBuilder()
        var url: String? = null
        while (url == null) {
            val line = reader.readLine() ?: break
            url = Regex("http://[^\\s/]+/").find(line)?.value
            if (url == null) said.appendLine(line)
        }
        if (url == null) {
            process.destroy()
            throw IllegalStateException(said.toString().trim().ifEmpty { "cadastre serve stopped." })
        }
        // What it says later is read and dropped: its pipe must not fill.
        thread(isDaemon = true, name = "cadastre serve output") { runCatching { reader.forEachLine { } } }
        val found = runCatching {
            val connection = URI("${url}playground/compose.html").toURL().openConnection() as HttpURLConnection
            connection.connectTimeout = 5000
            connection.responseCode == 200
        }.getOrDefault(false)
        if (!found) {
            process.destroy()
            throw IllegalStateException("the composer was not found: it comes with the releases of Cadastre, in the playground folder beside the executable (or set CADASTRE_PLAYGROUND).")
        }
        servers[root] = Server(process, url)
        return url
    }

    override fun dispose() {
        servers.values.forEach { it.process.destroy() }
        servers.clear()
    }

    companion object {
        fun get(project: Project): CadastreComposer = project.service()
    }
}
