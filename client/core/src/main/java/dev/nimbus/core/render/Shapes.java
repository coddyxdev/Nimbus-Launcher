package dev.nimbus.core.render;

import dev.nimbus.bridge.GameBridge;

/**
 * Составные формы поверх примитивов {@link Draw}.
 *
 * Разница между интерфейсом за деньги и интерфейсом "нарисовал прямоугольник"
 * почти целиком в трёх вещах: мягкая тень под окном, вертикальный градиент на
 * панелях и обрезка длинного текста вместо вылезания за край. Всё это здесь.
 */
public final class Shapes {

    private static final int SUB = 4;
    private static final float SOLID = 0.995f;
    private static final float INVISIBLE = 0.004f;

    /** Предел числа слоёв мягкой тени. */
    private static final int MAX_SHADOW_LAYERS = 8;

    /** Тот же запас по радиусу, что и в {@link Draw}: крупный масштаб интерфейса. */
    private static final int CACHE_LIMIT = 96;
    private static final float[][] CACHE = new float[CACHE_LIMIT + 1][];

    private Shapes() {
    }

    /**
     * Прямоугольник со скруглением и вертикальным градиентом.
     *
     * Углы считаются построчно со сглаживанием, середина уходит одним вызовом
     * градиента: рисовать её построчно значит тратить сотни вызовов там, где
     * хватает одного.
     */
    public static void roundedGradient(
            GameBridge game,
            int x,
            int y,
            int width,
            int height,
            int radius,
            int top,
            int bottom
    ) {
        if (width <= 0 || height <= 0) {
            return;
        }
        int r = clampRadius(radius, width, height);
        if (r == 0) {
            game.fillGradient(x, y, width, height, top, bottom);
            return;
        }
        float[] cov = coverage(r);
        float span = Math.max(1f, height - 1f);
        for (int row = 0; row < r; row++) {
            int inset = solidInset(cov, r, row);
            int topY = y + row;
            int bottomY = y + height - 1 - row;
            int topColor = Colors.mix(top, bottom, row / span);
            int bottomColor = Colors.mix(top, bottom, (height - 1 - row) / span);
            int runWidth = width - inset * 2;
            if (runWidth > 0) {
                game.fill(x + inset, topY, runWidth, 1, topColor);
                game.fill(x + inset, bottomY, runWidth, 1, bottomColor);
            }
            for (int col = 0; col < inset; col++) {
                float c = cov[row * r + col];
                if (c <= INVISIBLE) {
                    continue;
                }
                int ct = Colors.fade(topColor, c);
                int cb = Colors.fade(bottomColor, c);
                game.fill(x + col, topY, 1, 1, ct);
                game.fill(x + width - 1 - col, topY, 1, 1, ct);
                game.fill(x + col, bottomY, 1, 1, cb);
                game.fill(x + width - 1 - col, bottomY, 1, 1, cb);
            }
        }
        int middle = height - r * 2;
        if (middle > 0) {
            game.fillGradient(
                    x,
                    y + r,
                    width,
                    middle,
                    Colors.mix(top, bottom, r / span),
                    Colors.mix(top, bottom, (height - 1 - r) / span)
            );
        }
    }

    /**
     * Мягкая тень под панелью.
     *
     * Тень собрана из вложенных рамок, а не из заливок: заливка перекрашивала бы
     * всю площадь под окном столько раз, сколько слоёв, а видно только края.
     */
    public static void shadow(GameBridge game, int x, int y, int width, int height, int radius, int spread, int argb) {
        if (spread <= 0 || Colors.alpha(argb) == 0) {
            return;
        }
        // Каждый слой тени - отдельная рамка, а рамка при крупном масштабе интерфейса
        // стоит сотен вызовов заливки. Больше восьми слоёв глаз всё равно не различает,
        // поэтому слои не множатся, а прореживаются: шаг растёт, толщина растёт вместе
        // с ним, и кольца по-прежнему смыкаются без просветов. Ширина тени не меняется.
        int step = Math.max(1, (spread + MAX_SHADOW_LAYERS - 1) / MAX_SHADOW_LAYERS);
        for (int i = spread; i >= 1; i -= step) {
            float k = 1f - (float) i / (spread + 1f);
            // Прореженные слои компенсируются плотностью, иначе тень станет светлее.
            float alpha = Math.min(1f, k * k * step);
            Draw.roundedOutline(game, x - i, y - i, width + i * 2, height + i * 2, radius + i, step, Colors.fade(argb, alpha));
        }
    }

    /** Полоса всех оттенков: выбор акцентного цвета. */
    public static void hueStrip(GameBridge game, int x, int y, int width, int height, float saturation, float alpha) {
        if (width <= 0 || height <= 0) {
            return;
        }
        // Соседние столбцы после округления цвета до байтов часто совпадают. Склейка
        // одинаковых столбцов в один прямоугольник убирает лишние вызовы заливки:
        // широкая полоса раньше стоила ровно по вызову на каждый пиксель ширины.
        int runStart = 0;
        int runColor = Colors.fade(Colors.hsv(0f, saturation, 1f, 255), alpha);
        for (int i = 1; i < width; i++) {
            int color = Colors.fade(Colors.hsv(i * 360f / width, saturation, 1f, 255), alpha);
            if (color == runColor) {
                continue;
            }
            game.fill(x + runStart, y, i - runStart, height, runColor);
            runStart = i;
            runColor = color;
        }
        game.fill(x + runStart, y, width - runStart, height, runColor);
    }

    /** Горизонтальный градиент: игра умеет только вертикальный. */
    public static void horizontalGradient(GameBridge game, int x, int y, int width, int height, int left, int right) {
        if (width <= 0 || height <= 0) {
            return;
        }
        float span = Math.max(1f, width - 1f);
        // Та же склейка одинаковых столбцов, что и в полосе оттенков.
        int runStart = 0;
        int runColor = Colors.mix(left, right, 0f);
        for (int i = 1; i < width; i++) {
            int color = Colors.mix(left, right, i / span);
            if (color == runColor) {
                continue;
            }
            game.fill(x + runStart, y, i - runStart, height, runColor);
            runStart = i;
            runColor = color;
        }
        game.fill(x + runStart, y, width - runStart, height, runColor);
    }

    /** Текст, обрезанный по ширине с многоточием. */
    public static String clip(GameBridge game, String text, int maxWidth) {
        if (maxWidth <= 0) {
            return "";
        }
        if (game.textWidth(text) <= maxWidth) {
            return text;
        }
        String tail = "...";
        int limit = maxWidth - game.textWidth(tail);
        if (limit <= 0) {
            return "";
        }
        int width = 0;
        StringBuilder result = new StringBuilder(text.length());
        for (int i = 0; i < text.length(); i++) {
            char c = text.charAt(i);
            int step = game.textWidth(String.valueOf(c));
            if (width + step > limit) {
                break;
            }
            width += step;
            result.append(c);
        }
        return result.toString() + tail;
    }

    private static int clampRadius(int radius, int width, int height) {
        if (radius <= 0) {
            return 0;
        }
        return Math.min(radius, Math.min(width, height) / 2);
    }

    private static int solidInset(float[] cov, int radius, int row) {
        for (int col = 0; col < radius; col++) {
            if (cov[row * radius + col] >= SOLID) {
                return col;
            }
        }
        return radius;
    }

    private static float[] coverage(int radius) {
        if (radius <= CACHE_LIMIT && CACHE[radius] != null) {
            return CACHE[radius];
        }
        float[] table = new float[radius * radius];
        double limit = radius * (double) radius;
        double step = 1.0 / SUB;
        double half = step / 2.0;
        for (int row = 0; row < radius; row++) {
            for (int col = 0; col < radius; col++) {
                int hits = 0;
                for (int sy = 0; sy < SUB; sy++) {
                    double py = row + half + sy * step;
                    double dy = radius - py;
                    double dy2 = dy * dy;
                    for (int sx = 0; sx < SUB; sx++) {
                        double px = col + half + sx * step;
                        double dx = radius - px;
                        if (dx * dx + dy2 <= limit) {
                            hits++;
                        }
                    }
                }
                table[row * radius + col] = (float) hits / (SUB * SUB);
            }
        }
        if (radius <= CACHE_LIMIT) {
            CACHE[radius] = table;
        }
        return table;
    }
}
