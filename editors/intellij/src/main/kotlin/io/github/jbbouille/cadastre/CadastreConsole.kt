package io.github.jbbouille.cadastre

import com.intellij.execution.filters.Filter
import com.intellij.execution.filters.OpenFileHyperlinkInfo
import com.intellij.execution.filters.TextConsoleBuilderFactory
import com.intellij.execution.ui.ConsoleView
import com.intellij.execution.ui.ConsoleViewContentType
import com.intellij.openapi.Disposable
import com.intellij.openapi.components.Service
import com.intellij.openapi.components.service
import com.intellij.openapi.project.DumbAware
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.Disposer
import com.intellij.openapi.vfs.LocalFileSystem
import com.intellij.openapi.wm.ToolWindow
import com.intellij.openapi.wm.ToolWindowFactory
import com.intellij.openapi.wm.ToolWindowManager
import com.intellij.ui.content.ContentFactory
import java.nio.file.Path

/** The Cadastre tool window: the output of the builds, the results of the checks. */
@Service(Service.Level.PROJECT)
class CadastreConsole(private val project: Project) : Disposable {
    enum class Tab(val title: String) { BUILD("Build"), CHECKS("Checks") }

    private val consoles = mutableMapOf<Tab, ConsoleView>()

    fun console(tab: Tab): ConsoleView = consoles.getOrPut(tab) {
        TextConsoleBuilderFactory.getInstance().createBuilder(project)
            .filters(PlacesFilter(project))
            .console
            .also { Disposer.register(this, it) }
    }

    /** Replaces the text of a tab, and shows it. */
    fun show(tab: Tab, text: String, type: ConsoleViewContentType = ConsoleViewContentType.NORMAL_OUTPUT) {
        val console = console(tab)
        console.clear()
        console.print(text, type)
        reveal(tab)
    }

    fun reveal(tab: Tab) {
        val window = ToolWindowManager.getInstance(project).getToolWindow(ID) ?: return
        window.show {
            window.contentManager.findContent(tab.title)?.let { window.contentManager.setSelectedContent(it) }
        }
    }

    override fun dispose() {}

    companion object {
        const val ID = "Cadastre"

        fun get(project: Project): CadastreConsole = project.service()
    }
}

class CadastreToolWindowFactory : ToolWindowFactory, DumbAware {
    override fun createToolWindowContent(project: Project, toolWindow: ToolWindow) {
        val console = CadastreConsole.get(project)
        for (tab in CadastreConsole.Tab.entries) {
            val content = ContentFactory.getInstance().createContent(console.console(tab).component, tab.title, false)
            content.isCloseable = false
            toolWindow.contentManager.addContent(content)
        }
    }
}

/**
 * Links in the output: the places of a check report (`C:\…\apps.ca:12:3`) and the files written
 * by a build (`wrote out/a.svg`).
 */
private class PlacesFilter(private val project: Project) : Filter {
    override fun applyFilter(line: String, entireLength: Int): Filter.Result? {
        val start = entireLength - line.length
        val items = PLACE.findAll(line).mapNotNull { match ->
            val file = LocalFileSystem.getInstance().findFileByNioFile(Path.of(match.groupValues[1])) ?: return@mapNotNull null
            val info = OpenFileHyperlinkInfo(project, file, match.groupValues[2].toInt() - 1, match.groupValues[3].toInt() - 1)
            Filter.ResultItem(start + match.range.first, start + match.range.last + 1, info)
        }.toMutableList()
        WROTE.find(line)?.let { match ->
            val group = match.groups[1]!!
            val path = Path.of(group.value.trim()).let { if (it.isAbsolute) it else project.basePath?.let { base -> Path.of(base).resolve(it) } ?: it }
            LocalFileSystem.getInstance().findFileByNioFile(path)?.let { file ->
                items += Filter.ResultItem(start + group.range.first, start + group.range.last + 1, OpenFileHyperlinkInfo(project, file, 0))
            }
        }
        return if (items.isEmpty()) null else Filter.Result(items)
    }

    private companion object {
        // A path of a Cadastre file, absolute (`C:\` or `/`, after a space), then its line and column.
        val PLACE = Regex("""((?:[A-Za-z]:[\\/]|(?<=^|\s)/)[^:\r\n]*?\.ca[vc]?):(\d+):(\d+)""")
        val WROTE = Regex("""^wrote (.+?)\s*$""")
    }
}
