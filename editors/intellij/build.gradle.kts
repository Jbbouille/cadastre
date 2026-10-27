// The IntelliJ plugin of Cadastre: the language server (`cadastre lsp`) through LSP4IJ, and the
// TextMate grammar of the VS Code extension for highlighting before the server answers.

plugins {
    id("org.jetbrains.kotlin.jvm") version "2.2.10"
    id("org.jetbrains.intellij.platform") version "2.18.1"
}

group = "io.github.jbbouille.cadastre"
version = providers.gradleProperty("pluginVersion").get()

kotlin {
    jvmToolchain(21)
}

repositories {
    mavenCentral()
    intellijPlatform {
        defaultRepositories()
    }
}

dependencies {
    intellijPlatform {
        intellijIdeaCommunity(providers.gradleProperty("platformVersion"))
        bundledPlugin("org.jetbrains.plugins.textmate")
        plugin("com.redhat.devtools.lsp4ij", providers.gradleProperty("lsp4ijVersion").get())
    }
}

intellijPlatform {
    pluginConfiguration {
        version = providers.gradleProperty("pluginVersion")
        ideaVersion {
            sinceBuild = providers.gradleProperty("platformSinceBuild")
            untilBuild = provider { null }
        }
    }
}

// The grammar and the editing rules of the VS Code extension, one source for both editors: the
// TextMate bundle is its `package.json` (written here) with these two files, a directory of the
// plugin (`cadastre-intellij/textmate/cadastre`) that TextMate reads in place.
val vscode = layout.projectDirectory.dir("../vscode")
tasks.prepareSandbox {
    val bundle = "${project.name}/textmate/cadastre"
    from(layout.projectDirectory.dir("src/main/textmate/cadastre")) { into(bundle) }
    from(vscode.file("language-configuration.json")) { into(bundle) }
    from(vscode.file("syntaxes/cadastre.tmLanguage.json")) { into("$bundle/syntaxes") }
}
