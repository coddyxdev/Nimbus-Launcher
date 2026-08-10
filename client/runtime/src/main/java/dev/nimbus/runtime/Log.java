package dev.nimbus.runtime;

/** Минимальный логгер. Агент стартует раньше любых библиотек игры, поэтому — без зависимостей. */
public final class Log {

    private static final String PREFIX = "[Nimbus] ";

    private static boolean debug = Boolean.getBoolean("nimbus.debug");

    private Log() {
    }

    public static void setDebug(boolean value) {
        debug = value;
    }

    public static boolean debugEnabled() {
        return debug;
    }

    public static void info(String message) {
        System.out.println(PREFIX + message);
    }

    public static void debug(String message) {
        if (debug) {
            System.out.println(PREFIX + "debug: " + message);
        }
    }

    public static void warn(String message) {
        System.out.println(PREFIX + "warn: " + message);
    }

    public static void error(String message, Throwable error) {
        System.out.println(PREFIX + "error: " + message);
        if (error != null) {
            error.printStackTrace(System.out);
        }
    }
}
