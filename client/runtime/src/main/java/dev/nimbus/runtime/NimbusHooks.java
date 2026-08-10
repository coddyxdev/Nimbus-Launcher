package dev.nimbus.runtime;

/**
 * Точки входа, которые вызывает вставленный в игру байт-код.
 *
 * Методы вызываются из игрового потока десятки раз в секунду:
 * никаких аллокаций и никаких исключений наружу — упавший хук уронит игру.
 */
public final class NimbusHooks {

    /**
     * Через столько тиков (около пяти секунд) в лог уходит единственная
     * строка о том, что игровой цикл действительно наш. Без неё убедиться в
     * этом можно только в отладочном режиме, а он выключен у игроков.
     */
    private static final long CONFIRM_AFTER_TICKS = 100L;

    private static volatile boolean started;
    private static long ticks;
    private static long lastReport = System.nanoTime();

    private NimbusHooks() {
    }

    /** Вызывается один раз при создании клиента. */
    public static void onGameStart() {
        if (started) {
            return;
        }
        started = true;
        try {
            Log.info("игра запущена, клиент внутри");
        } catch (Throwable error) {
            safeReport(error);
        }
    }

    /** Вызывается каждый игровой тик. */
    public static void onTick() {
        try {
            ticks++;
            if (ticks == CONFIRM_AFTER_TICKS) {
                Log.info("игровой цикл подключён, тиков: " + ticks);
            }
            if (!Log.debugEnabled()) {
                return;
            }
            long now = System.nanoTime();
            if (now - lastReport >= 5_000_000_000L) {
                lastReport = now;
                Log.debug("тиков всего: " + ticks);
            }
        } catch (Throwable error) {
            safeReport(error);
        }
    }

    public static long ticks() {
        return ticks;
    }

    private static void safeReport(Throwable error) {
        try {
            Log.error("хук упал", error);
        } catch (Throwable ignored) {
            // Даже логгер не должен ломать игру.
        }
    }
}
