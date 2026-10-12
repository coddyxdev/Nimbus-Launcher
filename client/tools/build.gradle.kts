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

tasks.register<JavaExec>("verifyVersionPolicy") {
    group = "verification"
    description = "Проверяет строгую политику поддерживаемых версий Minecraft"
    mainClass.set("dev.nimbus.tools.VerifyVersionPolicy")
    classpath = sourceSets["main"].runtimeClasspath
}

tasks.register<JavaExec>("verifySettingsPersistence") {
    group = "verification"
    description = "Проверяет сохранение настроек и состояния ClickGui"
    mainClass.set("dev.nimbus.tools.VerifySettingsPersistence")
    classpath = sourceSets["main"].runtimeClasspath
}

tasks.register<JavaExec>("verifyCoreModel") {
    group = "verification"
    description = "Проверяет раскладку HUD, уведомления, телеметрию и границы отрисовки"
    mainClass.set("dev.nimbus.tools.VerifyCoreModel")
    classpath = sourceSets["main"].runtimeClasspath
}

tasks.named("check") {
    dependsOn("verifyVersionPolicy", "verifySettingsPersistence", "verifyCoreModel")
}
