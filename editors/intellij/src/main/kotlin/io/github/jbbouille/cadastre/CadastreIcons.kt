package io.github.jbbouille.cadastre

import com.intellij.ide.FileIconProvider
import com.intellij.openapi.project.Project
import com.intellij.openapi.util.IconLoader
import com.intellij.openapi.vfs.VirtualFile
import javax.swing.Icon

/** The icons of the plugin: a cadastral plan, its parcels, one of them highlighted. */
object CadastreIcons {
    @JvmField
    val FILE: Icon = IconLoader.getIcon("/icons/cadastre.svg", CadastreIcons::class.java)
}

/** The icon of the `.ca`, `.cav` and `.cac` files, whose file type is that of the TextMate bundles. */
class CadastreFileIconProvider : FileIconProvider {
    override fun getIcon(file: VirtualFile, flags: Int, project: Project?): Icon? =
        if (Cadastre.isCadastre(file)) CadastreIcons.FILE else null
}
