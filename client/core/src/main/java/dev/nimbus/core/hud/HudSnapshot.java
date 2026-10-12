package dev.nimbus.core.hud;

/**
 * Снимок данных одного кадра HUD.
 *
 * Значения собираются один раз за кадр, чтобы панели не дёргали bridge каждая по
 * отдельности. Строки и локальные метрики обновляются внутри HudTelemetry реже,
 * чем игровые координаты.
 */
public final class HudSnapshot {

    private int fps;
    private int onePercentLow;
    private float frameTimeMs;
    private int leftCps;
    private int rightCps;
    private boolean inWorld;
    private double playerX;
    private double playerY;
    private double playerZ;
    private float playerYaw;
    private int ping;
    private long sessionSeconds;
    private long usedMemory;
    private long maxMemory;
    private int hour;
    private int minute;
    private int second;

    HudSnapshot() {
    }

    void update(
            int fps,
            int onePercentLow,
            float frameTimeMs,
            int leftCps,
            int rightCps,
            boolean inWorld,
            double playerX,
            double playerY,
            double playerZ,
            float playerYaw,
            int ping,
            long sessionSeconds,
            long usedMemory,
            long maxMemory,
            int hour,
            int minute,
            int second
    ) {
        this.fps = Math.max(0, fps);
        this.onePercentLow = Math.max(0, onePercentLow);
        this.frameTimeMs = Math.max(0f, frameTimeMs);
        this.leftCps = Math.max(0, leftCps);
        this.rightCps = Math.max(0, rightCps);
        this.inWorld = inWorld;
        this.playerX = playerX;
        this.playerY = playerY;
        this.playerZ = playerZ;
        this.playerYaw = playerYaw;
        this.ping = ping;
        this.sessionSeconds = Math.max(0L, sessionSeconds);
        this.usedMemory = Math.max(0L, usedMemory);
        this.maxMemory = Math.max(0L, maxMemory);
        this.hour = hour;
        this.minute = minute;
        this.second = second;
    }

    public int fps() {
        return fps;
    }

    public int onePercentLow() {
        return onePercentLow;
    }

    public float frameTimeMs() {
        return frameTimeMs;
    }

    public int leftCps() {
        return leftCps;
    }

    public int rightCps() {
        return rightCps;
    }

    public boolean inWorld() {
        return inWorld;
    }

    public double playerX() {
        return playerX;
    }

    public double playerY() {
        return playerY;
    }

    public double playerZ() {
        return playerZ;
    }

    public float playerYaw() {
        return playerYaw;
    }

    public int ping() {
        return ping;
    }

    public long sessionSeconds() {
        return sessionSeconds;
    }

    public long usedMemory() {
        return usedMemory;
    }

    public long maxMemory() {
        return maxMemory;
    }

    public int hour() {
        return hour;
    }

    public int minute() {
        return minute;
    }

    public int second() {
        return second;
    }
}
