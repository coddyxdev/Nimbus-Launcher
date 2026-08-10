plugins {
    java
}

// Адаптер для ветки 1.20.x. Компилируется без игровых классов:
// доступ к игре идёт через маппинги и MethodHandle, а не через компил-тайм зависимость.
dependencies {
    implementation(project(":bridge"))
}
