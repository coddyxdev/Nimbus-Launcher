plugins {
    java
}

allprojects {
    group = "dev.nimbus"
    version = "0.1.0"
}

subprojects {
    apply(plugin = "java")

    repositories {
        mavenCentral()
    }

    tasks.withType<JavaCompile>().configureEach {
        options.encoding = "UTF-8"
        options.release.set(17)
    }
}
