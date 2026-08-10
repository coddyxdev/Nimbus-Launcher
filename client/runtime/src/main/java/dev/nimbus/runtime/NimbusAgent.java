package dev.nimbus.runtime;

import dev.nimbus.runtime.mappings.MappingTable;
import dev.nimbus.runtime.mappings.ProGuardMappings;
import dev.nimbus.runtime.transform.NimbusTransformer;

import java.io.File;
import java.lang.instrument.Instrumentation;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.jar.JarFile;

/**
 * Точка входа клиента.
 *
 * Лаунчер добавляет в строку запуска:
 * -javaagent:nimbus-runtime.jar=version=1.20.1,mappings=&lt;путь&gt;
 *
 * Агент стартует до main-класса игры и успевает править её классы до загрузки.
 */
public final class NimbusAgent {

    private static final String VERSION = "0.1.0";

    private static Instrumentation instrumentation;
    private static MappingTable mappings = MappingTable.identity("unknown");
    private static String gameVersion = "unknown";

    private NimbusAgent() {
    }

    public static void premain(String rawArgs, Instrumentation inst) {
        long startedAt = System.nanoTime();
        instrumentation = inst;

        try {
            AgentArgs args = AgentArgs.parse(rawArgs);
            if (args.flag("debug")) {
                Log.setDebug(true);
            }
            Log.info("Nimbus Client " + VERSION + " запускается");
            Log.debug("аргументы агента: " + args);

            makeHooksVisible(inst);

            gameVersion = GameVersion.detect(args);
            Log.info("версия игры: " + gameVersion);

            if (!GameVersion.isSupported(gameVersion)) {
                Log.warn("версия ниже " + GameVersion.MINIMUM_SUPPORTED
                        + ", клиент отключён — игра запустится в ванильном виде");
                return;
            }

            mappings = loadMappings(args, gameVersion);
            if (mappings.isEmpty()) {
                Log.warn("таблица имён пуста: ожидается деобфусцированная сборка");
            } else {
                Log.info("маппинги: " + mappings.classCount() + " классов, "
                        + mappings.methodCount() + " методов");
            }

            inst.addTransformer(new NimbusTransformer(mappings), false);

            long tookMs = (System.nanoTime() - startedAt) / 1_000_000L;
            Log.info("агент готов за " + tookMs + " мс");
        } catch (Throwable error) {
            // Любая наша ошибка не должна мешать человеку играть.
            Log.error("агент не запустился, игра продолжит без клиента", error);
        }
    }

    /** Запуск агента в уже работающую JVM (пригодится для отладки). */
    public static void agentmain(String rawArgs, Instrumentation inst) {
        premain(rawArgs, inst);
    }

    public static Instrumentation instrumentation() {
        return instrumentation;
    }

    public static MappingTable mappings() {
        return mappings;
    }

    public static String gameVersion() {
        return gameVersion;
    }

    /**
     * Классы игры грузит системный загрузчик. Чтобы вставленный вызов хука разрешился,
     * наш jar должен быть виден тому же загрузчику.
     */
    private static void makeHooksVisible(Instrumentation inst) {
        try {
            Path self = Path.of(NimbusAgent.class
                    .getProtectionDomain()
                    .getCodeSource()
                    .getLocation()
                    .toURI());
            File file = self.toFile();
            if (file.isFile()) {
                inst.appendToSystemClassLoaderSearch(new JarFile(file));
                Log.debug("jar агента добавлен в systemClassLoader: " + file);
            } else {
                Log.debug("агент запущен из каталога классов: " + file);
            }
        } catch (Throwable error) {
            Log.warn("не удалось добавить jar агента в classpath: " + error);
        }
    }

    private static MappingTable loadMappings(AgentArgs args, String version) {
        String path = args.get("mappings", null);
        if (path == null) {
            Log.warn("путь к маппингам не передан");
            return MappingTable.identity(version);
        }
        Path file = Path.of(path);
        if (!Files.isRegularFile(file)) {
            Log.warn("файл маппингов не найден: " + file);
            return MappingTable.identity(version);
        }
        try {
            long startedAt = System.nanoTime();
            MappingTable table = ProGuardMappings.parse(file, version);
            Log.debug("маппинги разобраны за "
                    + (System.nanoTime() - startedAt) / 1_000_000L + " мс");
            return table;
        } catch (Throwable error) {
            Log.error("маппинги не разобраны: " + file, error);
            return MappingTable.identity(version);
        }
    }
}
