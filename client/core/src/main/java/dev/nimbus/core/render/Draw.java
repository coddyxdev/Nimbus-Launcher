package dev.nimbus.core.render;

import dev.nimbus.bridge.GameBridge;

/**
 * Примитивы рисования, сложенные из того, что умеет сама игра.
 *
 * Игра умеет рисовать только прямоугольники и текст. Скруглённые углы, рамки
 * и мягкие тени собираются из них здесь, чтобы ни одна версия игры не требовала
 * своего кода рисования.
 *
 * Скруглённый прямоугольник рисуется горизонтальными полосами: для каждой строки
 * считается отступ по уравнению окружности, а соседние строки с одинаковым
 * отступом склеиваются в один вызов. На панель выходит порядка десятка
 * прямоугольников вместо сотни отдельных строк.
 */
public final class Draw {

    private Draw() {
    }

    /** Прямоугольник со скруглёнными углами. */
    public static void roundedRect(GameBridge game, int x, int y, int width, int height, int radius, int argb) {
        if (width <= 0 || height <= 0 || Colors.alpha(argb) == 0) {
            return;
        }
        int r = clampRadius(radius, width, height);
        if (r == 0) {
            game.fill(x, y, width, height, argb);
            return;
        }

        int runStart = 0;
        int runInset = inset(r, 0, height);
        for (int row = 1; row <= height; row++) {
            int current = row < height ? inset(r, row, height) : Integer.MIN_VALUE;
            if (current == runInset) {
                continue;
            }
            game.fill(x + runInset, y + runStart, width - runInset * 2, row - runStart, argb);
            runStart = row;
            runInset = current;
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
        int t = Math.min(thickness, Math.min(width, height) / 2);
        for (int row = 0; row < height; row++) {
            int outer = inset(r, row, height);
            int innerRow = row - t;
            int innerHeight = height - t * 2;
            boolean insideVertically = innerRow >= 0 && innerRow < innerHeight;
            if (!insideVertically) {
                game.fill(x + outer, y + row, width - outer * 2, 1, argb);
                continue;
            }
            int innerRadius = Math.max(0, r - t);
            int inner = t + inset(innerRadius, innerRow, innerHeight);
            game.fill(x + outer, y + row, inner - outer, 1, argb);
            game.fill(x + width - inner, y + row, inner - outer, 1, argb);
        }
    }

    /**
     * Мягкая тень под панелью.
     *
     * Настоящего размытия без шейдера не сделать, поэтому тень собирается из
     * нескольких вложенных рамок с падающей прозрачностью. Глаз читает это как
     * мягкое свечение, а стоит оно несколько прямоугольников.
     */
    public static void shadow(GameBridge game, int x, int y, int width, int height, int radius, int spread, int argb) {
        if (spread <= 0 || Colors.alpha(argb) == 0) {
            return;
        }
        for (int step = spread; step >= 1; step--) {
            float strength = 1f - (float) step / (spread + 1);
            int color = Colors.fade(argb, strength * strength);
            roundedRect(
                    game,
                    x - step,
                    y - step + 1,
                    width + step * 2,
                    height + step * 2,
                    radius + step,
                    color
            );
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
        int limit = Math.min(width, height) / 2;
        if (radius < 0) {
            return 0;
        }
        return Math.min(radius, limit);
    }

    /**
     * Отступ строки от края прямоугольника из-за скругления.
     *
     * Строка берётся серединой (row + 0.5), центр угловой окружности - в точке r,
     * поэтому скругление выглядит симметрично сверху и снизу.
     */
    private static int inset(int radius, int row, int height) {
        if (radius <= 0) {
            return 0;
        }
        double distance;
        if (row < radius) {
            distance = radius - (row + 0.5);
        } else if (row >= height - radius) {
            distance = (row + 0.5) - (height - radius);
        } else {
            return 0;
        }
        double half = Math.sqrt(Math.max(0.0, radius * (double) radius - distance * distance));
        int value = (int) Math.round(radius - half);
        return Math.max(0, Math.min(value, radius));
    }
}
