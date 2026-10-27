package io.github.jbbouille.cadastre

import com.intellij.ide.plugins.PluginManagerCore
import com.intellij.openapi.extensions.PluginId
import org.jetbrains.plugins.textmate.api.TextMateBundleProvider

/**
 * The TextMate grammar of the VS Code extension, shipped in the plugin: highlighting from the
 * first keystroke, refined by the semantic tokens of the language server. The bundle is a
 * directory of the plugin, read in place: a copy elsewhere could be read half-written while the
 * IDE starts.
 */
class CadastreBundleProvider : TextMateBundleProvider {
    override fun getBundles(): List<TextMateBundleProvider.PluginBundle> {
        val plugin = PluginManagerCore.getPlugin(PluginId.getId("io.github.jbbouille.cadastre")) ?: return emptyList()
        return listOf(TextMateBundleProvider.PluginBundle("Cadastre", plugin.pluginPath.resolve("textmate/cadastre")))
    }
}
