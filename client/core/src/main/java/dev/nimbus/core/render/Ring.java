package dev.nimbus.core.render;

import dev.nimbus.bridge.GameBridge;

/**
 * Круглые формы: диск, кольцо, сектор кольца, ромб.
 *
 * Игра умеет рисовать только прямоугольники, поэтому круг собирается из
 * горизонтальных полос. Главная цена здесь - количество вызовов отрисовки, а не
 * арифметика, поэтому соседние пиксели с одинаковой прозрачностью склеиваются в один
 * прямоугольник: заливка кольца стоит десятки вызовов, а не тысячи.
 *
 * Сглаживание разделено на два дешёвых шага. По радиусу считается доля пикселя
 * внутри кольца по сетке подвыборок, по углу - через расстояние до граничных
 * лучей. Второе важно: считать угол арктангенсом на каждую подвыборку было бы в
 * десятки раз дороже всего остального кадра.
 *
 * Координаты здесь - те же, в которых рисует мост. Сектора ждут физические
 * пиксели: вызывать их нужно внутри pushScale, иначе дуга будет зернистой.
 *
 * Углы в градусах: 0 смотрит вверх, рост по часовой стрелке.
 */
public final class Ring {

    /** Подвыборок на пиксель по каждой оси. */
    private static final int SUB = 3;

    /** Шаг квантования прозрачности: от него зависит, как длинные выйдут полосы. */
    private static final int LEVELS = 24;

    private static final float INVISIBLE = 1f / 255f;

    private Ring() {
    }

    /** Сплошной круг. */
    public static void disc(GameBridge game, float cx, float cy, float radius, int argb) {
        sector(game, cx, cy, 0f, radius, 0f, 360f, argb);
    }

    /** Кольцо заданной толщины. */
    public static void ring(GameBridge game, float cx, float cy, float radius, float thickness, int argb) {
        sector(game, cx, cy, radius - thickness, radius, 0f, 360f, argb);
    }

    /**
     * Сектор кольца.
     *
     * Раствор больше 180 градусов рисуется двумя половинами: отсечение по двум
     * полуплоскостям работает только на остром угле.
     */
    public static void sector(
            GameBridge game,
            float cx,
            float cy,
            float inner,
            float outer,
            float startDeg,
            float sweepDeg,
            int argb
    ) {
        if (outer <= 0f || sweepDeg <= 0f || Colors.alpha(argb) == 0) {
            return;
        }
        float in = Math.max(0f, Math.min(inner, outer));
        if (sweepDeg < 359.9f && sweepDeg > 179f) {
            float half = sweepDeg / 2f;
            sector(game, cx, cy, in, outer, startDeg, half, argb);
            sector(game, cx, cy, in, outer, startDeg + half, sweepDeg - half, argb);
            return;
        }

        boolean full = sweepDeg >= 359.9f;
        double start = Math.toRadians(startDeg);
        double end = Math.toRadians(startDeg + sweepDeg);
        // Нормали граничных лучей: внутри сектора оба скалярных произведения неотрицательны.
        float n1x = (float) Math.cos(start);
        float n1y = (float) Math.sin(start);
        float n2x = (float) -Math.cos(end);
        float n2y = (float) -Math.sin(end);

        float outer2 = outer * outer;
        float inner2 = in * in;
        int top = (int) Math.floor(cy - outer);
        int bottom = (int) Math.ceil(cy + outer);
        float step = 1f / SUB;
        float half = step / 2f;
        int samples = SUB * SUB;

        for (int y = top; y <= bottom; y++) {
            float dy = y + 0.5f - cy;
            float ady = Math.abs(dy) - 1f;
            float span = outer2 - (ady > 0f ? ady * ady : 0f);
            if (span <= 0f) {
                continue;
            }
            int spanX = (int) Math.ceil(Math.sqrt(span)) + 1;
            int left = (int) Math.floor(cx) - spanX;
            int right = (int) Math.ceil(cx) + spanX;

            int runStart = 0;
            int runLevel = 0;
            for (int x = left; x <= right; x++) {
                float px = x + 0.5f - cx;
                float alpha;
                if (!full) {
                    float a1 = n1x * px + n1y * dy + 0.5f;
                    float a2 = n2x * px + n2y * dy + 0.5f;
                    float angular = Math.min(clamp01(a1), clamp01(a2));
                    alpha = angular <= 0f ? 0f : angular * radial(px, dy, inner2, outer2, step, half, samples);
                } else {
                    alpha = radial(px, dy, inner2, outer2, step, half, samples);
                }

                int level = alpha <= INVISIBLE ? 0 : Math.max(1, Math.round(alpha * LEVELS));
                if (level == runLevel) {
                    continue;
                }
                if (runLevel > 0) {
                    game.fill(runStart, y, x - runStart, 1, Colors.fade(argb, runLevel / (float) LEVELS));
                }
                runStart = x;
                runLevel = level;
            }
            if (runLevel > 0) {
                game.fill(runStart, y, right + 1 - runStart, 1, Colors.fade(argb, runLevel / (float) LEVELS));
            }
        }
    }

    /** Ромб с центром в точке и полудиагональю half. */
    public static void diamond(GameBridge game, float cx, float cy, float halfSize, int argb) {
        if (halfSize <= 0f || Colors.alpha(argb) == 0) {
            return;
        }
        int top = (int) Math.floor(cy - halfSize);
        int bottom = (int) Math.ceil(cy + halfSize);
        for (int y = top; y <= bottom; y++) {
            float dy = Math.abs(y + 0.5f - cy);
            float width = halfSize - dy;
            if (width <= 0f) {
                continue;
            }
            int whole = (int) Math.floor(width);
            float edge = width - whole;
            if (whole > 0) {
                game.fill((int) (cx - whole), y, whole * 2, 1, argb);
            }
            if (edge > INVISIBLE) {
                int color = Colors.fade(argb, edge);
                game.fill((int) (cx - whole) - 1, y, 1, 1, color);
                game.fill((int) (cx + whole), y, 1, 1, color);
            }
        }
    }

    /** Контур ромба: разность двух заливок здесь не годится - рисуем полосы по краям. */
    public static void diamondOutline(GameBridge game, float cx, float cy, float halfSize, float thickness, int argb) {
        if (halfSize <= 0f || thickness <= 0f || Colors.alpha(argb) == 0) {
            return;
        }
        float insideHalf = Math.max(0f, halfSize - thickness * 1.4142f);
        int top = (int) Math.floor(cy - halfSize);
        int bottom = (int) Math.ceil(cy + halfSize);
        for (int y = top; y <= bottom; y++) {
            float dy = Math.abs(y + 0.5f - cy);
            float width = halfSize - dy;
            if (width <= 0f) {
                continue;
            }
            float insideWidth = insideHalf - dy;
            int outerWhole = (int) Math.ceil(width);
            int innerWhole = insideWidth > 0f ? (int) Math.floor(insideWidth) : 0;
            int band = outerWhole - innerWhole;
            if (band <= 0) {
                continue;
            }
            if (innerWhole <= 0) {
                game.fill((int) (cx - outerWhole), y, outerWhole * 2, 1, argb);
            } else {
                game.fill((int) (cx - outerWhole), y, band, 1, argb);
                game.fill((int) (cx + innerWhole), y, band, 1, argb);
            }
        }
    }

    private static float radial(float px, float py, float inner2, float outer2, float step, float half, int samples) {
        float d2 = px * px + py * py;
        // Глубоко внутри и глубоко снаружи подвыборки не нужны - это почти всё кольцо.
        float d = (float) Math.sqrt(d2);
        float outer = (float) Math.sqrt(outer2);
        float inner = (float) Math.sqrt(inner2);
        if (d > outer + 1f) {
            return 0f;
        }
        if (d < inner - 1f) {
            return 0f;
        }
        if (d < outer - 1f && d > inner + 1f) {
            return 1f;
        }
        int hits = 0;
        for (int sy = 0; sy < SUB; sy++) {
            float sampleY = py - 0.5f + half + sy * step;
            float sy2 = sampleY * sampleY;
            for (int sx = 0; sx < SUB; sx++) {
                float sampleX = px - 0.5f + half + sx * step;
                float dist2 = sampleX * sampleX + sy2;
                if (dist2 <= outer2 && dist2 >= inner2) {
                    hits++;
                }
            }
        }
        return (float) hits / samples;
    }

    private static float clamp01(float value) {
        if (value < 0f) {
            return 0f;
        }
        return value > 1f ? 1f : value;
    }
}
