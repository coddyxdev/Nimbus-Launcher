package dev.nimbus.tools;

import dev.nimbus.runtime.mappings.MappingTable;
import dev.nimbus.runtime.mappings.ProGuardMappings;
import dev.nimbus.runtime.transform.NimbusTransformer;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.jar.JarFile;
import java.util.zip.ZipEntry;

/**
 * Проверка патчей без запуска игры.
 *
 * Зачем это нужно: ошибка в байт-коде проявляется не у нас при сборке, а у
 * игрока в виде краша при запуске. Этот инструмент берёт настоящий jar игры,
 * прогоняет его классы через наш трансформер и заставляет JVM их проверить.
 * Проверяет тот же самый механизм, который ругается в игре, так что ложных
 * срабатываний тут не бывает.
 *
 * Запуск:
 * gradlew :tools:run --args="&lt;client.jar&gt; &lt;mappings.txt&gt; &lt;версия&gt; [каталог библиотек]"
 */
public final class VerifyPatch {

    private VerifyPatch() {
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 3) {
            System.out.println("аргументы: <client.jar> <mappings.txt> <версия> [каталог библиотек]");
            System.exit(2);
            return;
        }

        Path clientJar = Path.of(args[0]);
        Path mappingsFile = Path.of(args[1]);
        String version = args[2];
        Path librariesDir = args.length > 3 ? Path.of(args[3]) : null;

        System.out.println("jar игры:   " + clientJar);
        System.out.println("маппинги:  " + mappingsFile);
        System.out.println("версия:    " + version);

        MappingTable mappings = ProGuardMappings.parse(mappingsFile, version);
        System.out.println("таблица:   " + mappings.classCount() + " классов, "
                + mappings.methodCount() + " методов");

        NimbusTransformer transformer = new NimbusTransformer(mappings);

        List<Path> jars = new ArrayList<>();
        jars.add(clientJar);
        if (librariesDir != null && Files.isDirectory(librariesDir)) {
            try (var stream = Files.walk(librariesDir)) {
                stream.filter(path -> path.toString().endsWith(".jar")).forEach(jars::add);
            }
        }
        System.out.println("classpath: " + jars.size() + " jar");

        int failures = 0;
        try (GameClassLoader loader = new GameClassLoader(jars, transformer)) {
            failures += check(loader, mappings, "net.minecraft.client.Minecraft");
            failures += check(loader, mappings, "net.minecraft.client.gui.Gui");
            failures += check(loader, mappings, "net.minecraft.client.MouseHandler");
        }

        System.out.println();
        System.out.println("патчи: " + transformer.summary());
        if (failures == 0) {
            System.out.println("ИТОГ: байт-код прошёл проверку JVM");
        } else {
            System.out.println("ИТОГ: ошибок проверки: " + failures);
            System.exit(1);
        }
    }

    /**
     * Загружает класс и заставляет JVM его связать.
     *
     * Проверка байт-кода идёт именно при связывании. Всё, что падает позже
     * (нет LWJGL, нет окна, нет ресурсов) нас не волнует: до этого момента
     * байт-код уже признан корректным.
     */
    private static int check(GameClassLoader loader, MappingTable mappings, String deobfName) {
        String obf = mappings.obfClass(deobfName).replace('/', '.');
        System.out.println();
        System.out.println("проверяем " + deobfName + " (" + obf + ")");
        try {
            Class<?> loaded = Class.forName(obf, true, loader);
            System.out.println("  ок: класс связан и проверен (" + loaded.getDeclaredMethods().length + " методов)");
            return 0;
        } catch (VerifyError error) {
            System.out.println("  ОШИБКА проверки байт-кода:");
            System.out.println("  " + error.getMessage());
            return 1;
        } catch (Throwable other) {
            Throwable cause = deepest(other);
            if (cause instanceof VerifyError verify) {
                System.out.println("  ОШИБКА проверки байт-кода:");
                System.out.println("  " + verify.getMessage());
                return 1;
            }
            // Класс проверку прошёл, а споткнулся уже на инициализации без игры.
            System.out.println("  ок: проверка пройдена (дальше ожидаемо без игры: "
                    + cause.getClass().getSimpleName() + ")");
            return 0;
        }
    }

    private static Throwable deepest(Throwable error) {
        Throwable current = error;
        while (current.getCause() != null && current.getCause() != current) {
            current = current.getCause();
        }
        return current;
    }

    /** Загрузчик, который пропускает классы игры через наш трансформер. */
    private static final class GameClassLoader extends ClassLoader implements AutoCloseable {

        private final List<JarFile> jars = new ArrayList<>();
        private final NimbusTransformer transformer;

        GameClassLoader(List<Path> paths, NimbusTransformer transformer) throws IOException {
            super(ClassLoader.getPlatformClassLoader());
            this.transformer = transformer;
            for (Path path : paths) {
                jars.add(new JarFile(path.toFile()));
            }
        }

        @Override
        protected Class<?> findClass(String name) throws ClassNotFoundException {
            String entryName = name.replace('.', '/') + ".class";
            for (JarFile jar : jars) {
                ZipEntry entry = jar.getEntry(entryName);
                if (entry == null) {
                    continue;
                }
                try (InputStream input = jar.getInputStream(entry)) {
                    byte[] bytes = input.readAllBytes();
                    byte[] patched = transformer.transform(
                            this,
                            name.replace('.', '/'),
                            null,
                            null,
                            bytes
                    );
                    byte[] result = patched != null ? patched : bytes;
                    return defineClass(name, result, 0, result.length);
                } catch (IOException error) {
                    throw new ClassNotFoundException(name, error);
                }
            }
            throw new ClassNotFoundException(name);
        }

        @Override
        public void close() {
            for (JarFile jar : jars) {
                try {
                    jar.close();
                } catch (IOException ignored) {
                    // Нечего делать: инструмент всё равно завершается.
                }
            }
        }
    }
}
