package io.github.jbbouille.cadastre

import com.intellij.openapi.components.BaseState
import com.intellij.openapi.components.SimplePersistentStateComponent
import com.intellij.openapi.components.State
import com.intellij.openapi.components.Storage
import com.intellij.openapi.components.service
import com.intellij.openapi.options.BoundConfigurable
import com.intellij.openapi.project.ProjectManager
import com.intellij.openapi.ui.DialogPanel
import com.intellij.ui.dsl.builder.bindItem
import com.intellij.ui.dsl.builder.bindSelected
import com.intellij.ui.dsl.builder.bindText
import com.intellij.ui.dsl.builder.columns
import com.intellij.ui.dsl.builder.panel
import java.io.File

/** The settings of the plugin: where the `cadastre` executable is, how incomplete elements are reported. */
@State(name = "Cadastre", storages = [Storage("cadastre.xml")])
class CadastreSettings : SimplePersistentStateComponent<CadastreSettings.Options>(Options()) {
    class Options : BaseState() {
        /** The path of `cadastre`; empty: the `CADASTRE_SERVER` environment variable, then the `PATH`. */
        var serverPath by string("")

        /** Severity of the incomplete elements: `hint`, `information`, `warning` or `off`. */
        var incomplete by string("hint")

        /** Incomplete elements are errors, like `cadastre check --strict`. */
        var strict by property(false)
    }

    /**
     * The executable to run: the configured one, then the `CADASTRE_SERVER` environment variable,
     * then the one found on the `PATH`.
     */
    fun executable(): String? {
        state.serverPath?.takeIf { it.isNotBlank() }?.let { return it }
        System.getenv("CADASTRE_SERVER")?.takeIf { it.isNotBlank() }?.let { return it }
        return onPath("cadastre")
    }

    /** The options of the language server, as VS Code sends its `cadastre` settings. */
    fun initializationOptions(): Map<String, Any> =
        mapOf("diagnostics" to mapOf("incomplete" to (state.incomplete ?: "hint"), "strict" to state.strict))

    companion object {
        fun get(): CadastreSettings = service()

        /** An executable on the `PATH`: `cadastre`, `dot`. */
        fun onPath(name: String): String? {
            val names = if (System.getProperty("os.name").startsWith("Windows")) listOf("$name.exe", name) else listOf(name)
            return System.getenv("PATH").orEmpty().split(File.pathSeparator)
                .flatMap { directory -> names.map { File(directory, it) } }
                .firstOrNull { it.isFile && it.canExecute() }
                ?.path
        }
    }
}

/** Settings > Tools > Cadastre; applying them restarts the language server. */
class CadastreConfigurable : BoundConfigurable("Cadastre") {
    override fun createPanel(): DialogPanel = panel {
        val options = CadastreSettings.get().state
        row("Path of cadastre:") {
            textField()
                .bindText({ options.serverPath.orEmpty() }, { options.serverPath = it })
                .columns(40)
                .comment("Empty: the <code>CADASTRE_SERVER</code> environment variable, then <code>cadastre</code> on the PATH. The language server is <code>cadastre lsp</code>.")
        }
        row("Incomplete elements:") {
            comboBox(listOf("hint", "information", "warning", "off"))
                .bindItem({ options.incomplete ?: "hint" }, { options.incomplete = it ?: "hint" })
                .comment("How what is missing for a complete model, without being wrong, is reported.")
        }
        row {
            checkBox("Report incomplete elements as errors, like cadastre check --strict")
                .bindSelected({ options.strict }, { options.strict = it })
        }
    }

    override fun apply() {
        super.apply()
        ProjectManager.getInstance().openProjects.forEach { Cadastre.restart(it) }
    }
}
