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

    /** Единственная production-версия до появления отдельного адаптера. */
    public static final String SUPPORTED_VERSION = "1.20.1";
    /** Сохраняется для совместимости с диагностикой старого runtime. */
    public static final String MINIMUM_SUPPORTED = SUPPORTED_VERSION;

    public enum Support {
        SUPPORTED,
        KNOWN_UNSUPPORTED,
        UNKNOWN
    }

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

    /** Classifies versions without allowing a newer version to use old mappings. */
    public static Support classify(String version) {
        if (!isReleaseVersion(version)) {
            return Support.UNKNOWN;
        }
        return SUPPORTED_VERSION.equals(version.trim())
                ? Support.SUPPORTED
                : Support.KNOWN_UNSUPPORTED;
    }

    public static boolean isSupported(String version) {
        return classify(version) == Support.SUPPORTED;
    }

    /**
     * Compares numeric release versions. Invalid values sort after valid values
     * only for diagnostics; callers deciding support must use {@link #classify}.
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

    private static boolean isReleaseVersion(String version) {
        return parts(version) != null;
    }

    private static int[] parts(String version) {
        if (version == null || version.isBlank()) {
            return null;
        }
        String normalized = version.trim();
        if (!normalized.matches("\\d+(?:\\.\\d+){1,2}")) {
            return null;
        }
        String[] chunks = normalized.split("\\.");

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
