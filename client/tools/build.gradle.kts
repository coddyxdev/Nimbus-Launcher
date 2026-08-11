plugins {
    java
    application
}

// Инструменты разработчика. В игру не попадают и в jar рантайма не входят.
dependencies {
    implementation(project(":runtime"))
    implementation(project(":bridge"))
    implementation(project(":core"))
    implementation(project(":adapters:v1_20"))
    implementation("org.ow2.asm:asm:9.7.1")
}

application {
    mainClass.set("dev.nimbus.tools.VerifyPatch")
}

// Проверка имён, которые мост ищет через отражение:
// gradlew :tools:verifyBridge --args="<client.jar> <mappings.txt> <версия> [каталог библиотек]"
tasks.register<JavaExec>("verifyBridge") {
    group = "verification"
    description = "Проверяет имена моста версии на настоящем jar игры"
    mainClass.set("dev.nimbus.tools.VerifyBridge")
    classpath = sourceSets["main"].runtimeClasspath
}
