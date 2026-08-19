package dev.nimbus.core.render;

import dev.nimbus.bridge.GameBridge;

/**
 * Примитивы рисования, сложенные из того, что умеет сама игра.
 *
 * Игра умеет рисовать только прямоугольники и текст, поэтому скруглённые углы
 * собираются здесь. Код общий для всех версий игры.
 *
 * Сглаживание. Простое округление отступа до целых пикселей даёт заметную лесенку:
 * угол выглядит рваным и кривым. Здесь для каждого углового пикселя считается доля
 * площади, попавшая внутрь окружности (сетка подвыборок {@link #SUB}x{@link #SUB}),
 * и краевой пиксель рисуется с пропорциональной прозрачностью. Это настоящее
 * сглаживание через альфа-смешивание, а не имитация.
 *
 * Стоимость. Таблица покрытия считается один раз на радиус и кэшируется. Сплошная
 * середина рисуется одним прямоугольником, строки с одинаковым отступом склеиваются,
 * отдельно идут только полупрозрачные пиксели по дуге.
 */
public final class Draw {

    /** Подвыборок на пиксель по каждой оси. */
    private static final int SUB = 4;

    /** С какого покрытия пиксель считается сплошным. */
    private static final float SOLID = 0.995f;

    /** Ниже этого покрытия пиксель не рисуется вовсе. */
    private static final float INVISIBLE = 0.004f;

    /**
     * До какого радиуса таблицы покрытия кэшируются.
     *
     * Предел в 32 был меньше реально запрашиваемых радиусов: тень окна при
     * масштабе интерфейса 3 доходит до 42, а при 6 - до 84. Всё, что не влезало
     * в кэш, пересчитывалось заново на каждый слой каждого кадра - десятки тысяч
     * лишних операций в кадре на ровном месте. Граница взята с запасом на самый
     * крупный масштаб. Таблицы создаются по требованию, пустые места ничего не стоят.
     */
    private static final int CACHE_LIMIT = 96;
    private static final float[][] CACHE = new float[CACHE_LIMIT + 1][];

    private Draw() {
    }

    /** Прямоугольник со скруглёнными углами и сглаженной дугой. */
    public static void roundedRect(GameBridge game, int x, int y, int width, int height, int radius, int argb) {
        if (width <= 0 || height <= 0 || Colors.alpha(argb) == 0) {
            return;
        }
        int r = clampRadius(radius, width, height);
        if (r == 0) {
            game.fill(x, y, width, height, argb);
            return;
        }

        float[] cov = coverage(r);

        int middle = height - r * 2;
        if (middle > 0) {
            game.fill(x, y + r, width, middle, argb);
        }

        // Сплошная часть угловых строк: соседние строки с равным отступом склеиваются.
        int runStart = 0;
        int runInset = solidInset(cov, r, 0);
        for (int row = 1; row <= r; row++) {
            int current = row < r ? solidInset(cov, r, row) : Integer.MIN_VALUE;
            if (current == runInset) {
                continue;
            }
            int runWidth = width - runInset * 2;
            int runHeight = row - runStart;
            if (runWidth > 0) {
                game.fill(x + runInset, y + runStart, runWidth, runHeight, argb);
                game.fill(x + runInset, y + height - row, runWidth, runHeight, argb);
            }
            runStart = row;
            runInset = current;
        }

        // Полупрозрачные пиксели по дуге, сразу во всех четырёх углах.
        for (int row = 0; row < r; row++) {
            int solid = solidInset(cov, r, row);
            int topY = y + row;
            int bottomY = y + height - 1 - row;
            for (int col = 0; col < solid; col++) {
                float c = cov[row * r + col];
                if (c <= INVISIBLE) {
                    continue;
                }
                int color = Colors.fade(argb, c);
                int leftX = x + col;
                int rightX = x + width - 1 - col;
                game.fill(leftX, topY, 1, 1, color);
                game.fill(rightX, topY, 1, 1, color);
                game.fill(leftX, bottomY, 1, 1, color);
                game.fill(rightX, bottomY, 1, 1, color);
            }
        }
    }

    /** Рамка со скруглёнными углами толщиной thickness. */
    public static void roundedOutline(
            GameBridge game,
            int x,
            int y,
            int width,
            int height,
            int radius,
            int thickness,
            int argb
    ) {
        if (width <= 0 || height <= 0 || thickness <= 0 || Colors.alpha(argb) == 0) {
            return;
        }
        int r = clampRadius(radius, width, height);
        int t = Math.max(1, Math.min(thickness, Math.min(width, height) / 2));
        if (r == 0) {
            game.fill(x, y, width, t, argb);
            game.fill(x, y + height - t, width, t, argb);
            game.fill(x, y + t, t, height - t * 2, argb);
            game.fill(x + width - t, y + t, t, height - t * 2, argb);
            return;
        }

        int straightWidth = width - r * 2;
        int straightHeight = height - r * 2;
        if (straightWidth > 0) {
            game.fill(x + r, y, straightWidth, t, argb);
            game.fill(x + r, y + height - t, straightWidth, t, argb);
        }
        if (straightHeight > 0) {
            game.fill(x, y + r, t, straightHeight, argb);
            game.fill(x + width - t, y + r, t, straightHeight, argb);
        }

        // Угол рамки - разность покрытий внешней и внутренней дуг.
        int innerRadius = Math.max(0, r - t);
        float[] outer = coverage(r);
        float[] inner = innerRadius > 0 ? coverage(innerRadius) : null;
        for (int row = 0; row < r; row++) {
            int topY = y + row;
            int bottomY = y + height - 1 - row;
            for (int col = 0; col < r; col++) {
                float a = outer[row * r + col] - innerCoverage(inner, innerRadius, row - t, col - t);
                if (a <= INVISIBLE) {
                    continue;
                }
                int color = Colors.fade(argb, Math.min(1f, a));
                int leftX = x + col;
                int rightX = x + width - 1 - col;
                game.fill(leftX, topY, 1, 1, color);
                game.fill(rightX, topY, 1, 1, color);
                game.fill(leftX, bottomY, 1, 1, color);
                game.fill(rightX, bottomY, 1, 1, color);
            }
        }
    }

    /** Горизонтальная линия толщиной в одну единицу. */
    public static void separator(GameBridge game, int x, int y, int width, int argb) {
        game.fill(x, y, width, 1, argb);
    }

    /** Текст, выровненный по центру относительно точки centerX. */
    public static void textCentered(GameBridge game, String text, int centerX, int y, int argb, boolean shadow) {
        game.drawText(text, centerX - game.textWidth(text) / 2, y, argb, shadow);
    }

    /** Текст, прижатый правым краем к точке rightX. */
    public static void textRight(GameBridge game, String text, int rightX, int y, int argb, boolean shadow) {
        game.drawText(text, rightX - game.textWidth(text), y, argb, shadow);
    }

    private static int clampRadius(int radius, int width, int height) {
        if (radius <= 0) {
            return 0;
        }
        return Math.min(radius, Math.min(width, height) / 2);
    }

    /** Первый столбец строки, который уже целиком внутри фигуры. */
    private static int solidInset(float[] cov, int radius, int row) {
        for (int col = 0; col < radius; col++) {
            if (cov[row * radius + col] >= SOLID) {
                return col;
            }
        }
        return radius;
    }

    /** Покрытие внутренней дуги в точке, смещённой на толщину рамки. */
    private static float innerCoverage(float[] inner, int innerRadius, int row, int col) {
        if (row < 0 || col < 0) {
            return 0f;
        }
        if (inner == null || row >= innerRadius || col >= innerRadius) {
            return 1f;
        }
        return inner[row * innerRadius + col];
    }

    /**
     * Доля каждого пикселя угла, попавшая внутрь окружности.
     *
     * Центр дуги - точка (radius, radius) в координатах левого верхнего угла.
     * Остальные три угла зеркальны, поэтому таблица одна на все.
     */
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
