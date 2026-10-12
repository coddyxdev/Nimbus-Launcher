package dev.nimbus.core.hud;

import dev.nimbus.bridge.GameBridge;

import java.util.Calendar;

/**
 * Собирает локальные и bridge-метрики для одного кадра HUD.
 *
 * Единственный источник телеметрии клиента: и HUD, и меню читают одно и то же
 * значение, поэтому цифры на экране не расходятся между собой.
 *
 * Время считается только разностями {@link System#nanoTime()}. Само значение
 * может быть отрицательным - это допустимо по контракту метода, поэтому нулевые
 * отметки времени не используются как признак "ещё не измеряли": для этого есть
 * отдельные флаги.
 */
public final class HudTelemetry {

    private static final int FRAME_HISTORY = 240;
    private static final long SLOW_SAMPLE_NANOS = 250_000_000L;
    private static final long CLOCK_SAMPLE_NANOS = 1_000_000_000L;
    private static final long FPS_WINDOW_NANOS = 500_000_000L;

    private final long startedAt;
    private final long[] frameTimes = new long[FRAME_HISTORY];
    private final long[] sortedFrameTimes = new long[FRAME_HISTORY];
    private int frameCount;
    private int frameHead;

    private long lastFrame;
    private boolean frameMeasured;

    /**
     * Кадры считаются окном, а не обратной величиной последнего кадра.
     *
     * Мгновенное значение прыгает на сотни единиц от одного случайного кадра:
     * такой счётчик невозможно читать и по нему нельзя судить о плавности.
     */
    private int windowFrames;
    private long windowStartedAt;
    private int fps;

    private int onePercentLow;
    private float frameTimeMs;

    private long lastSlowSample;
    private boolean slowSampled;
    private long lastClockSample;
    private boolean clockSampled;

    private int cachedPing = -1;
    private long usedMemory;
    private long maxMemory;
    private int hour;
    private int minute;
    private int second;
    private final HudSnapshot snapshot = new HudSnapshot();

    public HudTelemetry() {
        this(System.nanoTime());
    }

    /** Явное начало отсчёта: с ним у офлайн-проверки один и тот же ход времени. */
    public HudTelemetry(long startNanos) {
        this.startedAt = startNanos;
        this.windowStartedAt = startNanos;
    }

    public HudSnapshot sample(GameBridge game, int leftCps, int rightCps) {
        return sample(game, leftCps, rightCps, System.nanoTime());
    }

    /**
     * Снимок с явным моментом времени.
     *
     * Время параметром, а не из таймера: проверить один процент худших кадров
     * настоящими кадрами нельзя - для этого пришлось бы просаживать сборку на
     * секунды и надеяться, что машина просядет ровно так, как задумано.
     */
    public HudSnapshot sample(GameBridge game, int leftCps, int rightCps, long now) {
        updateFrame(now);
        updateSlowMetrics(game, now);
        updateClock(now);

        boolean inWorld = game.inWorld();
        double x = 0d;
        double y = 0d;
        double z = 0d;
        float yaw = 0f;
        if (inWorld) {
            x = game.playerX();
            y = game.playerY();
            z = game.playerZ();
            yaw = game.playerYaw();
        }

        long sessionSeconds = Math.max(0L, (now - startedAt) / 1_000_000_000L);
        snapshot.update(
                fps,
                onePercentLow,
                frameTimeMs,
                leftCps,
                rightCps,
                inWorld,
                x,
                y,
                z,
                yaw,
                cachedPing,
                sessionSeconds,
                usedMemory,
                maxMemory,
                hour,
                minute,
                second
        );
        return snapshot;
    }

    private void updateFrame(long now) {
        if (frameMeasured) {
            long elapsed = Math.max(0L, now - lastFrame);
            frameTimeMs = Math.min(1_000f, elapsed / 1_000_000f);
            frameTimes[frameHead] = elapsed;
            frameHead = (frameHead + 1) % FRAME_HISTORY;
            if (frameCount < FRAME_HISTORY) {
                frameCount++;
            }
        }
        lastFrame = now;
        frameMeasured = true;

        windowFrames++;
        long window = now - windowStartedAt;
        if (window >= FPS_WINDOW_NANOS) {
            fps = clampFps(Math.round(windowFrames * 1_000_000_000d / window));
            windowFrames = 0;
            windowStartedAt = now;
        } else if (fps == 0 && frameTimeMs > 0f) {
            // Первое полсекунды показываем приблизительную оценку, иначе HUD стартует с нуля.
            fps = clampFps(Math.round(1000f / frameTimeMs));
        }
    }

    private void updateSlowMetrics(GameBridge game, long now) {
        if (slowSampled && now - lastSlowSample < SLOW_SAMPLE_NANOS) {
            return;
        }
        lastSlowSample = now;
        slowSampled = true;
        onePercentLow = calculateOnePercentLow();
        cachedPing = game.ping();
        Runtime runtime = Runtime.getRuntime();
        usedMemory = Math.max(0L, runtime.totalMemory() - runtime.freeMemory());
        maxMemory = Math.max(0L, runtime.maxMemory());
    }

    private void updateClock(long now) {
        if (clockSampled && now - lastClockSample < CLOCK_SAMPLE_NANOS) {
            return;
        }
        lastClockSample = now;
        clockSampled = true;
        Calendar calendar = Calendar.getInstance();
        hour = calendar.get(Calendar.HOUR_OF_DAY);
        minute = calendar.get(Calendar.MINUTE);
        second = calendar.get(Calendar.SECOND);
    }

    private int calculateOnePercentLow() {
        if (frameCount < 10) {
            return fps;
        }
        // Порядок кадров не важен: берём самые медленные, а не последние.
        System.arraycopy(frameTimes, 0, sortedFrameTimes, 0, frameCount);
        java.util.Arrays.sort(sortedFrameTimes, 0, frameCount);
        int slowestCount = Math.max(1, (int) Math.ceil(frameCount * 0.01));
        long total = 0L;
        for (int i = frameCount - slowestCount; i < frameCount; i++) {
            total += sortedFrameTimes[i];
        }
        long averageNanos = total / slowestCount;
        if (averageNanos <= 0L) {
            return fps;
        }
        return clampFps(Math.round(1_000_000_000d / averageNanos));
    }

    private static int clampFps(long value) {
        if (value < 1L) {
            return 1;
        }
        return value > 10_000L ? 10_000 : (int) value;
    }
}
