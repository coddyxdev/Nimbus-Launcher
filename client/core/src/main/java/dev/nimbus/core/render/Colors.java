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

    /**
     * Осветление или затемнение цвета без смены оттенка.
     *
     * Множитель больше единицы делает цвет светлее, меньше - темнее. Нужен
     * градиентам: акцент сверху всегда чуть ярче, чем снизу, иначе плоско.
     */
    public static int shade(int color, float factor) {
        return argb(
                alpha(color),
                Math.round(red(color) * factor),
                Math.round(green(color) * factor),
                Math.round(blue(color) * factor)
        );
    }

    /**
     * Цвет из тона, насыщенности и яркости.
     *
     * Палитра клиента задаётся одним ползунком тона, а не шестью готовыми
     * цветами: так у каждого игрока получается свой оттенок, а не выбор из чужого
     * списка.
     *
     * @param hue        тон в градусах, 0..360
     * @param saturation насыщенность, 0..1
     * @param value      яркость, 0..1
     * @param alpha      прозрачность, 0..255
     */
    public static int hsv(float hue, float saturation, float value, int alpha) {
        float h = ((hue % 360f) + 360f) % 360f / 60f;
        float s = clamp01(saturation);
        float v = clamp01(value);
        int sector = (int) Math.floor(h);
        float f = h - sector;
        float p = v * (1f - s);
        float q = v * (1f - s * f);
        float t = v * (1f - s * (1f - f));
        float r;
        float g;
        float b;
        switch (sector % 6) {
            case 0:
                r = v;
                g = t;
                b = p;
                break;
            case 1:
                r = q;
                g = v;
                b = p;
                break;
            case 2:
                r = p;
                g = v;
                b = t;
                break;
            case 3:
                r = p;
                g = q;
                b = v;
                break;
            case 4:
                r = t;
                g = p;
                b = v;
                break;
            default:
                r = v;
                g = p;
                b = q;
                break;
        }
        return argb(alpha, Math.round(r * 255f), Math.round(g * 255f), Math.round(b * 255f));
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
