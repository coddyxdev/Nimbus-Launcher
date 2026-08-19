package dev.nimbus.core.ui;

import dev.nimbus.bridge.GameBridge;
import dev.nimbus.core.render.Colors;
import dev.nimbus.core.render.Pixels;
import dev.nimbus.core.render.Ring;

/**
 * Рисование настоящего логотипа лаунчера.
 *
 * Загрузить картинку как текстуру было бы проще, но текстуры живут в классах игры,
 * а их имена меняются от версии к версии: пришлось бы тянуть в мост целый новый
 * слой ресурсов ради одной иконки. Логотип вместо этого запечён в код как сжатые
 * отрезки одинаковых точек ({@link Logo}) и рисуется теми же прямоугольниками,
 * что и весь остальной интерфейс. Работает на любой версии игры без единого
 * дополнительного патча.
 *
 * Рисуем по физическим пикселям экрана: иначе при масштабе интерфейса 3 значок 16x16
 * превращается в шестнадцать больших квадратов и выглядит как лестница.
 */
public final class Brand {

    private static int[] pixels;
    private static boolean parsed;
    private static boolean usable;

    private Brand() {
    }

    /**
     * Разобран ли логотип и есть ли в нём видимые точки.
     *
     * Нужно, чтобы интерфейс мог честно откатиться к геометрическому значку, а не
     * показывать пустое место, если данные когда-нибудь перегенерируют неверно.
     */
    public static boolean available() {
        ensure();
        return usable;
    }

    /**
     * Нарисовать логотип квадратом size x size в единицах интерфейса.
     *
     * Вызывать снаружи pushScale: масштаб он выставляет себе сам.
     */
    public static void draw(GameBridge game, int x, int y, int size, float alpha, int fallbackColor) {
        if (alpha <= 0.004f || size <= 0) {
            return;
        }
        ensure();
        int s = Pixels.scale(game);
        if (!usable) {
            // Запасной значок, чтобы шапка никогда не оставалась пустой.
            game.pushScale(1f / s);
            Ring.diamond(game, (x + size / 2f) * s, (y + size / 2f) * s, size * 0.32f * s, Colors.fade(fallbackColor, alpha));
            game.popScale();
            return;
        }

        int side = Logo.SIZE;
        int target = Math.max(1, size * s);
        int baseX = x * s;
        int baseY = y * s;

        game.pushScale(1f / s);
        for (int row = 0; row < side; row++) {
            int y0 = baseY + row * target / side;
            int y1 = baseY + (row + 1) * target / side;
            if (y1 <= y0) {
                continue;
            }
            int column = 0;
            while (column < side) {
                int color = pixels[row * side + column];
                int end = column + 1;
                while (end < side && pixels[row * side + end] == color) {
                    end++;
                }
                if ((color >>> 24) != 0) {
                    int x0 = baseX + column * target / side;
                    int x1 = baseX + end * target / side;
                    if (x1 > x0) {
                        game.fill(x0, y0, x1 - x0, y1 - y0, Colors.fade(color, alpha));
                    }
                }
                column = end;
            }
        }
        game.popScale();
    }

    /** Разбор сжатых данных один раз за запуск. */
    private static synchronized void ensure() {
        if (parsed) {
            return;
        }
        parsed = true;
        try {
            int side = Logo.SIZE;
            int[] result = new int[side * side];
            String[] rows = Logo.DATA.split(";");
            boolean anyVisible = false;
            for (int row = 0; row < rows.length && row < side; row++) {
                String line = rows[row].trim();
                if (line.isEmpty()) {
                    continue;
                }
                int cursor = 0;
                String[] runs = line.split(",");
                for (int i = 0; i < runs.length && cursor < side; i++) {
                    String run = runs[i];
                    int colon = run.indexOf(':');
                    if (colon <= 0) {
                        continue;
                    }
                    int length = Integer.parseInt(run.substring(0, colon).trim());
                    int argb = (int) Long.parseLong(run.substring(colon + 1).trim(), 16);
                    if ((argb >>> 24) != 0) {
                        anyVisible = true;
                    }
                    for (int step = 0; step < length && cursor < side; step++) {
                        result[row * side + cursor] = argb;
                        cursor++;
                    }
                }
            }
            pixels = result;
            usable = anyVisible;
        } catch (Throwable error) {
            // Логотип - украшение, а не причина уронить кадр.
            pixels = null;
            usable = false;
        }
    }
}
