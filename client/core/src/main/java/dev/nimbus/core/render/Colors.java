package dev.nimbus.core.render;

/**
 * Работа с цветом в формате ARGB (0xAARRGGBB).
 *
 * От игры ничего не требуется, поэтому всё считается здесь и одинаково работает
 * на всех версиях.
 */
public final class Colors {

    private Colors() {
    }

    public static int argb(int alpha, int red, int green, int blue) {
        return (clamp(alpha) << 24) | (clamp(red) << 16) | (clamp(green) << 8) | clamp(blue);
    }

    public static int alpha(int color) {
        return (color >>> 24) & 0xFF;
    }

    public static int red(int color) {
        return (color >>> 16) & 0xFF;
    }

    public static int green(int color) {
        return (color >>> 8) & 0xFF;
    }

    public static int blue(int color) {
        return color & 0xFF;
    }

    /** Тот же цвет с другой прозрачностью (0..255). */
    public static int withAlpha(int color, int alpha) {
        return (clamp(alpha) << 24) | (color & 0x00FFFFFF);
    }

    /** Тот же цвет, прозрачность умножена на множитель 0..1. Нужно анимациям появления. */
    public static int fade(int color, float factor) {
        return withAlpha(color, Math.round(alpha(color) * clamp01(factor)));
    }

    /** Плавный переход между двумя цветами: t = 0 даёт первый, t = 1 второй. */
    public static int mix(int from, int to, float t) {
        float k = clamp01(t);
        return argb(
                Math.round(alpha(from) + (alpha(to) - alpha(from)) * k),
                Math.round(red(from) + (red(to) - red(from)) * k),
                Math.round(green(from) + (green(to) - green(from)) * k),
                Math.round(blue(from) + (blue(to) - blue(from)) * k)
        );
    }

    private static int clamp(int value) {
        return value < 0 ? 0 : Math.min(value, 255);
    }

    private static float clamp01(float value) {
        if (value < 0f) {
            return 0f;
        }
        return value > 1f ? 1f : value;
    }
}
