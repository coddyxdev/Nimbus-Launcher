plugins {
    java
}

// Адаптер для Minecraft 1.20.1. Компилируется без игровых классов:
// доступ к игре идёт через маппинги и MethodHandle, а не через компил-тайм зависимость.
dependencies {
    implementation(project(":bridge"))
}
