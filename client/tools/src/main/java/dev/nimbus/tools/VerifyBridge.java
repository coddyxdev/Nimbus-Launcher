package dev.nimbus.tools;

import dev.nimbus.adapter.v1_20.ReflectiveGameBridge;
import dev.nimbus.runtime.MappingsBridge;
import dev.nimbus.runtime.mappings.MappingTable;
import dev.nimbus.runtime.mappings.ProGuardMappings;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.jar.JarFile;
import java.util.zip.ZipEntry;

/**
 * Проверка имён моста версии без запуска игры.
 *
 * Патч байт-кода проверяет VerifyPatch, но остаётся второй источник крашей:
 * мост ищет методы и поля игры через отражение, и опечатка в сигнатуре всплывёт
 * только в игре. Этот инструмент берёт настоящий jar игры и просит мост найти
 * всё, что ему нужно, ещё до того, как игру запустит человек.
 *
 * Запуск:
 * gradlew :tools:verifyBridge --args="&lt;client.jar&gt; &lt;mappings.txt&gt; &lt;версия&gt; [каталог библиотек]"
 */
public final class VerifyBridge {

    private static final String GUI_GRAPHICS = "net.minecraft.client.gui.GuiGraphics";

    private VerifyBridge() {
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

        MappingTable mappings = ProGuardMappings.parse(mappingsFile, version);
        System.out.println("таблица:   " + mappings.classCount() + " классов, "
                + mappings.methodCount() + " методов");

        List<Path> jars = new ArrayList<>();
        jars.add(clientJar);
        if (librariesDir != null && Files.isDirectory(librariesDir)) {
            try (var stream = Files.walk(librariesDir)) {
                stream.filter(path -> path.toString().endsWith(".jar")).forEach(jars::add);
            }
        }
        System.out.println("classpath: " + jars.size() + " jar");

        String graphicsName = mappings.obfClass(GUI_GRAPHICS).replace('/', '.');
        System.out.println("контекст:  " + GUI_GRAPHICS + " (" + graphicsName + ")");

        ReflectiveGameBridge bridge = new ReflectiveGameBridge(version, new MappingsBridge(mappings));
        try (JarClassLoader loader = new JarClassLoader(jars)) {
            Class<?> graphicsClass = Class.forName(graphicsName, false, loader);
            bridge.prepare(graphicsClass);
        }

        System.out.println();
        if (bridge.ready()) {
            System.out.println("ИТОГ: все имена моста найдены в версии " + version);
            return;
        }
        System.out.println("ИТОГ: мост отключён");
        System.out.println("причина: " + bridge.failure());
        System.exit(1);
    }

    /** Простой загрузчик классов из jar-файлов игры. Патчи здесь не нужны. */
    private static final class JarClassLoader extends ClassLoader implements AutoCloseable {

        private final List<JarFile> jars = new ArrayList<>();

        JarClassLoader(List<Path> paths) throws IOException {
            super(ClassLoader.getPlatformClassLoader());
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
                    return defineClass(name, bytes, 0, bytes.length);
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
