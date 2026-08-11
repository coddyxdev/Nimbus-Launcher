package dev.nimbus.core.render;

import dev.nimbus.bridge.GameBridge;

/**
 * Небольшая плашка в углу экрана.
 *
 * Первое видимое что-либо от клиента: проверяет сразу всю цепочку - вставку в
 * отрисовку, мост версии, примитивы и шрифт. Позже станет обычным модулем,
 * который можно выключить или перетащить.
 */
public final class Watermark {

    private static final String TITLE = "Nimbus";

    private static final int PADDING_X = 7;
    private static final int PADDING_Y = 5;
    private static final int MARGIN = 6;
    private static final int RADIUS = 6;
    private static final int ACCENT_WIDTH = 2;

    private static final int BACKGROUND = 0xCC101318;
    private static final int BACKGROUND_TOP = 0x14FFFFFF;
    private static final int OUTLINE = 0x33FFFFFF;
    private static final int SHADOW = 0x66000000;
    private static final int ACCENT = 0xFF5B8CFF;
    private static final int TEXT = 0xFFF2F5FA;
    private static final int TEXT_DIM = 0xFF8C93A1;

    /** Сколько секунд длится появление. */
    private static final float FADE_SECONDS = 0.45f;

    private long shownAt;

    public void render(GameBridge game, String subtitle) {
        long now = System.nanoTime();
        if (shownAt == 0L) {
            shownAt = now;
        }
        float progress = Easing.outCubic(Math.min(1f, (now - shownAt) / (FADE_SECONDS * 1_000_000_000f)));
        if (progress <= 0f) {
            return;
        }

        int textHeight = game.textHeight();
        int titleWidth = game.textWidth(TITLE);
        int subtitleWidth = subtitle == null || subtitle.isEmpty() ? 0 : game.textWidth(subtitle) + 5;

        int width = PADDING_X * 2 + ACCENT_WIDTH + 6 + titleWidth + subtitleWidth;
        int height = PADDING_Y * 2 + textHeight;

        // Появление: плашка выезжает сверху и проявляется.
        int x = MARGIN;
        int y = MARGIN - Math.round((1f - progress) * 6f);

        Draw.shadow(game, x, y, width, height, RADIUS, 4, Colors.fade(SHADOW, progress));
        Draw.roundedRect(game, x, y, width, height, RADIUS, Colors.fade(BACKGROUND, progress));
        Draw.roundedRect(game, x, y, width, height / 2, RADIUS, Colors.fade(BACKGROUND_TOP, progress));
        Draw.roundedOutline(game, x, y, width, height, RADIUS, 1, Colors.fade(OUTLINE, progress));

        int accentX = x + PADDING_X;
        int accentY = y + PADDING_Y;
        Draw.roundedRect(game, accentX, accentY, ACCENT_WIDTH, textHeight, 1, Colors.fade(ACCENT, progress));

        int textX = accentX + ACCENT_WIDTH + 6;
        int textY = y + PADDING_Y;
        game.drawText(TITLE, textX, textY, Colors.fade(TEXT, progress), false);
        if (subtitleWidth > 0) {
            game.drawText(subtitle, textX + titleWidth + 5, textY, Colors.fade(TEXT_DIM, progress), false);
        }
    }
}
