package dev.nimbus.runtime;

import java.io.InputStream;
import java.nio.charset.StandardCharsets;

/**
 * Определение версии игры.
 *
 * Порядок: аргумент лаунчера -> системное свойство -> version.json внутри игрового jar.
 * Лаунчер всегда знает версию точно, остальное — запасной путь для ручного запуска.
 */
public final class GameVersion {

    /** Нижняя граница поддержки. Ниже — другой слой рендера, он сейчас не делается. */
    public static final String MINIMUM_SUPPORTED = "1.20.1";

    private GameVersion() {
    }

    public static String detect(AgentArgs args) {
        String fromArgs = args.get("version", null);
        if (fromArgs != null) {
            return fromArgs;
        }
        String fromJar = readFromVersionJson();
        return fromJar != null ? fromJar : "unknown";
    }

    /** Поддерживается ли версия (>= 1.20.1). */
    public static boolean isSupported(String version) {
        return compare(version, MINIMUM_SUPPORTED) >= 0;
    }

    /**
     * Сравнение версий вида 1.20.1 / 1.21 / 26.2.
     * Нечисловые версии (снапшоты) считаются больше любого релиза: пускаем и логируем.
     */
    public static int compare(String left, String right) {
        int[] a = parts(left);
        int[] b = parts(right);
        if (a == null) {
            return 1;
        }
        if (b == null) {
            return -1;
        }
        for (int i = 0; i < Math.max(a.length, b.length); i++) {
            int x = i < a.length ? a[i] : 0;
            int y = i < b.length ? b[i] : 0;
            if (x != y) {
                return Integer.compare(x, y);
            }
        }
        return 0;
    }

    private static int[] parts(String version) {
        if (version == null || version.isBlank()) {
            return null;
        }
        String[] chunks = version.trim().split("\\.");
        int[] result = new int[chunks.length];
        for (int i = 0; i < chunks.length; i++) {
            try {
                result[i] = Integer.parseInt(chunks[i]);
            } catch (NumberFormatException error) {
                return null;
            }
        }
        return result;
    }

    /** В игровом jar лежит version.json с полем "name" или "id". */
    private static String readFromVersionJson() {
        try (InputStream stream = ClassLoader.getSystemResourceAsStream("version.json")) {
            if (stream == null) {
                return null;
            }
            String json = new String(stream.readAllBytes(), StandardCharsets.UTF_8);
            String name = readJsonString(json, "name");
            return name != null ? name : readJsonString(json, "id");
        } catch (Exception error) {
            Log.debug("version.json не прочитан: " + error);
            return null;
        }
    }

    /** Крошечный разбор одного строкового поля: тянуть сюда JSON-библиотеку в агент нельзя. */
    private static String readJsonString(String json, String key) {
        String needle = "\"" + key + "\"";
        int at = json.indexOf(needle);
        if (at < 0) {
            return null;
        }
        int colon = json.indexOf(':', at + needle.length());
        if (colon < 0) {
            return null;
        }
        int open = json.indexOf('"', colon + 1);
        if (open < 0) {
            return null;
        }
        int close = json.indexOf('"', open + 1);
        if (close < 0) {
            return null;
        }
        return json.substring(open + 1, close);
    }
}
