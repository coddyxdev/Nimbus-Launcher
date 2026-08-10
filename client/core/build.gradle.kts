plugins {
    java
}

// core ничего не знает о Minecraft и не имеет зависимостей от игры.
dependencies {
    implementation(project(":bridge"))
}
