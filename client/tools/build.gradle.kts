plugins {
    java
    application
}

dependencies {
    implementation(project(":runtime"))
    implementation("org.ow2.asm:asm:9.7.1")
}

application {
    mainClass.set("dev.nimbus.tools.VerifyPatch")
}
