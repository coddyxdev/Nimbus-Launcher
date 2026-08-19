package dev.nimbus.core.ui;

/**
 * Единый язык оформления: цвета, скругления, отступы.
 *
 * Главная причина, по которой самодельные клиенты выглядят дёшево: в каждом
 * месте свой оттенок серого, своё скругление и свой отступ. Здесь они заданы один
 * раз и общие для меню, HUD и колеса.
 */
public final class Theme {

    public static final int TEXT = 0xFFEFF3F9;
    public static final int TEXT_DIM = 0xFF98A2B4;
    public static final int TEXT_MUTED = 0xFF667085;
    public static final int LINE = 0x1FFFFFFF;
    public static final int LINE_SOFT = 0x12FFFFFF;
    public static final int SHADOW = 0xFF000000;
    public static final int DANGER = 0xFFFF5470;
    public static final int GOOD = 0xFF3DDC97;
    public static final int WARN = 0xFFFFC24B;

    public static final int RADIUS = 9;
    public static final int CARD_RADIUS = 6;
    public static final int CHIP_RADIUS = 5;

    public static final String[] SURFACE_NAMES = {"Полночь", "Графит", "Уголь"};

    /** base, верх градиента, боковая панель, карточка, карточка под курсором, чип HUD. */
    private static final int[][] SURFACES = {
            {0xFA0A0E16, 0xFA131A2A, 0xFF0B1019, 0x12FFFFFF, 0x20FFFFFF, 0xE60A0E16},
            {0xFA101216, 0xFA1B1F27, 0xFF131519, 0x14FFFFFF, 0x22FFFFFF, 0xE6101216},
            {0xFA08080A, 0xFA141417, 0xFF0A0A0C, 0x12FFFFFF, 0x20FFFFFF, 0xE608080A}
    };

    private Theme() {
    }

    private static int[] palette(int surface) {
        int index = surface < 0 ? 0 : surface % SURFACES.length;
        return SURFACES[index];
    }

    public static int base(int surface) {
        return palette(surface)[0];
    }

    public static int baseTop(int surface) {
        return palette(surface)[1];
    }

    public static int rail(int surface) {
        return palette(surface)[2];
    }

    public static int card(int surface) {
        return palette(surface)[3];
    }

    public static int cardHover(int surface) {
        return palette(surface)[4];
    }

    public static int chip(int surface) {
        return palette(surface)[5];
    }
}
