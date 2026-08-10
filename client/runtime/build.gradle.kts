plugins {
    java
}

dependencies {
    implementation("org.ow2.asm:asm:9.7.1")
    implementation("org.ow2.asm:asm-tree:9.7.1")
    implementation("org.ow2.asm:asm-commons:9.7.1")
    implementation(project(":core"))
    implementation(project(":bridge"))
    implementation(project(":adapters:v1_20"))
}

tasks.jar {
    archiveBaseName.set("nimbus-runtime")
    duplicatesStrategy = DuplicatesStrategy.EXCLUDE

    manifest {
        attributes(
            "Premain-Class" to "dev.nimbus.runtime.NimbusAgent",
            "Agent-Class" to "dev.nimbus.runtime.NimbusAgent",
            "Can-Retransform-Classes" to "true",
            "Implementation-Title" to "Nimbus Client Runtime",
            "Implementation-Version" to project.version.toString()
        )
    }

    from({
        configurations.runtimeClasspath.get().map { if (it.isDirectory) it else zipTree(it) }
    })
}
