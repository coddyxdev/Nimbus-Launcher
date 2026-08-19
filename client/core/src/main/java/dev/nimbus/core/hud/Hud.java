package dev.nimbus.core.hud;

import dev.nimbus.bridge.GameBridge;
import dev.nimbus.bridge.Keys;
import dev.nimbus.core.Settings;
import dev.nimbus.core.config.Module;
import dev.nimbus.core.render.Colors;
import dev.nimbus.core.render.Draw;
import dev.nimbus.core.render.Easing;
import dev.nimbus.core.render.Pixels;
import dev.nimbus.core.render.Ring;
import dev.nimbus.core.ui.Brand;
import dev.nimbus.core.ui.Theme;

import java.util.Calendar;

/**
 * Постоянный слой поверх игры: логотип, телеметрия, клавиши, прицел.
 *
 * Всё собрано из одинаковых плашек одной высоты и выровнено по одной сетке.
 * Именно это отличает аккуратный клиент от самодельного: не количество цифр на
 * экране, а то, что они выглядят одним целым.
 *
 * Положение панелей. По умолчанию панели стоят стопкой в выбранном углу, но любую
 * можно выдернуть из стопки и поставить куда угодно в режиме расстановки. Остальные
 * при этом смыкают стопку, а не оставляют дыру.
 *
 * Счётчики кадров и кликов считаются сами: брать их у игры - лишние чужие имена,
 * которые ломаются на каждой новой версии.
 *
 * Геометрия рисуется в физических пикселях, текст - в единицах интерфейса.
 */
public final class Hud {

    private static final int MARGIN = 6;
    private static final int PAD_X = 7;
    private static final int GAP = 3;
    private static final int RADIUS = 5;

    private static final int KIND_CHIP = 0;
    private static final int KIND_BRAND = 1;
    private static final int KIND_GRAPH = 2;
    private static final int KIND_KEYS = 3;

    private static final int SLOTS = 16;
    private static final int HISTORY = 240;

    /** Шаг сетки расстановки в единицах интерфейса. */
    private static final int GRID = 4;

    /** С какого расстояния панель прилипает к направляющей. */
    private static final int SNAP = 5;

    /** Стороны света по часовой стрелке, начиная с юга: так их считает сама игра. */
    private static final String[] FACINGS = {"Ю", "Ю-З", "З", "С-З", "С", "С-В", "В", "Ю-В"};

    private final Settings settings;
    private final HudLayout layout = new HudLayout();
    private final long startedAt = System.nanoTime();

    private final String[] keys = new String[SLOTS];
    private final String[] labels = new String[SLOTS];
    private final String[] values = new String[SLOTS];
    private final int[] colors = new int[SLOTS];
    private final int[] kinds = new int[SLOTS];
    private final int[] boxX = new int[SLOTS];
    private final int[] boxY = new int[SLOTS];
    private final int[] boxW = new int[SLOTS];
    private final int[] boxH = new int[SLOTS];
    private int blocks;

    private int frames;
    private long secondStartedAt = System.nanoTime();
    private int fps;

    private final int[] history = new int[HISTORY];
    private int historyCount;
    private int historyHead;
    private long lastSample;

    /** Кольцевой буфер моментов нажатия: CPS считается по окну в секунду. */
    private final long[][] clicks = new long[2][32];
    private final int[] clickHead = new int[2];
    private final boolean[] mouseWas = new boolean[2];

    private final float[] keyGlow = new float[8];
    private long lastFrame;

    // Режим расстановки
    private boolean editing;
    private float editFade;
    private String dragKey;
    private double dragOffsetX;
    private double dragOffsetY;
    private boolean editPreviousLeft;
    private boolean editPreviousRight;
    private int guideX = Integer.MIN_VALUE;
    private int guideY = Integer.MIN_VALUE;

    public Hud(Settings settings) {
        this.settings = settings;
        layout.load();
    }

    public int fps() {
        return fps;
    }

    /** Открыт ли режим расстановки панелей. */
    public boolean editing() {
        return editing;
    }

    public void startEditing() {
        editing = true;
        dragKey = null;
        editPreviousLeft = true;
        editPreviousRight = true;
    }

    public void stopEditing() {
        if (!editing) {
            return;
        }
        editing = false;
        dragKey = null;
        layout.saveIfDirty();
    }

    /** Клики в секунду по кнопке мыши. */
    public int cps(int button) {
        long now = System.nanoTime();
        long[] buffer = clicks[button];
        int count = 0;
        for (int i = 0; i < buffer.length; i++) {
            if (buffer[i] != 0L && now - buffer[i] <= 1_000_000_000L) {
                count++;
            }
        }
        return count;
    }

    public void render(GameBridge game, String version, boolean menuOpen) {
        long now = System.nanoTime();
        float delta = lastFrame == 0L ? 0f : Math.min(0.1f, (now - lastFrame) / 1_000_000_000f);
        lastFrame = now;

        countFrame(now);
        trackClicks(game, now, menuOpen);
        sampleHistory(now);

        int accent = settings.accent();
        int surface = settings.surfaceIndex();

        editFade = Easing.approach(editFade, editing ? 1f : 0f, 0.06f, delta);

        build(game, version, accent, delta);
        place(game);

        if (editing) {
            // Ввод считается до отрисовки: иначе панель отстаёт от курсора на кадр.
            handleEditing(game);
            drawEditorBackground(game, accent);
        }

        for (int i = 0; i < blocks; i++) {
            drawBlock(game, i, boxX[i], boxY[i], boxW[i], boxH[i], accent, surface, delta);
        }

        if (editFade > 0.004f) {
            drawEditorForeground(game, accent);
        }

        if (!editing && settings.module("crosshair").on() && game.inWorld() && !menuOpen) {
            drawCrosshair(game, accent);
        }
    }

    // ------------------------------------------------------------------ состав панелей

    private void build(GameBridge game, String version, int accent, float delta) {
        blocks = 0;
        boolean labelled = settings.hudLabels();

        Module watermark = settings.module("watermark");
        if (watermark.on()) {
            add(KIND_BRAND, "watermark", "NIMBUS", watermark.bool("version").get() ? version : "", accent);
        }

        Module fpsModule = settings.module("fps");
        if (fpsModule.on()) {
            int color = fpsModule.bool("colorize").get() ? gradeFps(fps) : Theme.TEXT;
            add(KIND_CHIP, "fps", labelled ? "FPS" : "", labelled ? Integer.toString(fps) : fps + " fps", color);
        }

        Module cpsModule = settings.module("cps");
        if (cpsModule.on()) {
            int mode = cpsModule.choice("mode").index();
            String value;
            if (mode == 0) {
                value = Integer.toString(cps(0));
            } else if (mode == 1) {
                value = Integer.toString(cps(1));
            } else {
                value = cps(0) + " / " + cps(1);
            }
            add(KIND_CHIP, "cps", labelled ? "CPS" : "", labelled ? value : value + " cps", Theme.TEXT);
        }

        Module coords = settings.module("coords");
        if (coords.on()) {
            add(KIND_CHIP, "coords", labelled ? "XYZ" : "", coordinates(game, coords), Theme.TEXT);
        }

        Module clock = settings.module("clock");
        if (clock.on()) {
            add(KIND_CHIP, "clock", "", clock(clock), Theme.TEXT);
        }

        Module session = settings.module("session");
        if (session.on()) {
            add(KIND_CHIP, "session", labelled ? "Сессия" : "", session(), Theme.TEXT);
        }

        Module ping = settings.module("ping");
        if (ping.on()) {
            int value = game.ping();
            int color = ping.bool("colorize").get() ? gradePing(value) : Theme.TEXT;
            String text = value < 0 ? "—" : value + " мс";
            add(KIND_CHIP, "ping", labelled ? "Пинг" : "", text, color);
        }

        if (settings.module("graph").on()) {
            add(KIND_GRAPH, "graph", "", "", accent);
        }

        if (settings.module("keystrokes").on()) {
            add(KIND_KEYS, "keystrokes", "", "", accent);
        }
    }

    private void add(int kind, String key, String label, String value, int color) {
        if (blocks >= SLOTS) {
            return;
        }
        kinds[blocks] = kind;
        keys[blocks] = key;
        labels[blocks] = label;
        values[blocks] = value;
        colors[blocks] = color;
        blocks++;
    }

    /**
     * Расстановка панелей.
     *
     * Сначала все не тронутые вручную панели выстраиваются стопкой в углу, потом
     * перенесённые садятся на свои места. Порядок важен: иначе перенесённая панель
     * оставляла бы за собой пустое место в стопке.
     */
    private void place(GameBridge game) {
        int chipHeight = game.textHeight() + 8;
        int corner = settings.hudCorner();
        boolean right = corner == 1 || corner == 3;
        boolean bottom = corner == 2 || corner == 3;
        boolean row = settings.hudRow();

        int cursorX = right ? game.screenWidth() - MARGIN : MARGIN;
        int cursorY = bottom ? game.screenHeight() - MARGIN : MARGIN;

        for (int i = 0; i < blocks; i++) {
            int width = blockWidth(game, i);
            int height = blockHeight(game, i, chipHeight);
            boxW[i] = width;
            boxH[i] = height;

            float[] custom = layout.get(keys[i]);
            if (custom != null) {
                boxX[i] = Math.round(custom[0] * Math.max(0, game.screenWidth() - width));
                boxY[i] = Math.round(custom[1] * Math.max(0, game.screenHeight() - height));
                continue;
            }

            if (kinds[i] == KIND_KEYS) {
                // У клавиш есть свои ползунки положения - они и есть положение по умолчанию.
                Module module = settings.module("keystrokes");
                boxX[i] = Math.round((game.screenWidth() - width) * module.slider("x").get() / 100f);
                boxY[i] = Math.round((game.screenHeight() - height) * module.slider("y").get() / 100f);
                continue;
            }

            boxX[i] = right ? cursorX - width : cursorX;
            boxY[i] = bottom ? cursorY - height : cursorY;
            if (row) {
                cursorX += right ? -(width + GAP) : width + GAP;
            } else {
                cursorY += bottom ? -(height + GAP) : height + GAP;
            }
        }
    }

    private int blockWidth(GameBridge game, int index) {
        if (kinds[index] == KIND_GRAPH) {
            return settings.module("graph").slider("width").asInt();
        }
        if (kinds[index] == KIND_KEYS) {
            return 18 * 3 + 2 * 2;
        }
        if (kinds[index] == KIND_BRAND) {
            return brandWidth(game, values[index]);
        }
        int width = PAD_X * 2 + game.textWidth(values[index]);
        if (!labels[index].isEmpty()) {
            width += game.textWidth(labels[index]) + 5;
        }
        if (settings.hudAccentBar()) {
            width += 4;
        }
        return width;
    }

    private int blockHeight(GameBridge game, int index, int chipHeight) {
        if (kinds[index] == KIND_GRAPH) {
            return settings.module("graph").slider("height").asInt();
        }
        if (kinds[index] == KIND_KEYS) {
            Module module = settings.module("keystrokes");
            int size = 18;
            int gap = 2;
            return size * 2 + gap
                    + (module.bool("mouse").get() ? size + gap : 0)
                    + (module.bool("space").get() ? 9 + gap : 0);
        }
        return chipHeight;
    }

    private int brandWidth(GameBridge game, String version) {
        int style = settings.module("watermark").choice("style").index();
        if (style == 2) {
            return game.textHeight() + 14;
        }
        int width = game.textWidth("NIMBUS") + 5 + PAD_X * 2 + 14;
        if (!version.isEmpty()) {
            width += game.textWidth(version) + 6;
        }
        if (style == 1) {
            width -= PAD_X * 2;
        }
        return width;
    }

    private void drawBlock(GameBridge game, int index, int x, int y, int width, int height, int accent, int surface, float delta) {
        if (kinds[index] == KIND_GRAPH) {
            drawGraph(game, x, y, width, height, accent, surface);
            return;
        }
        if (kinds[index] == KIND_KEYS) {
            drawKeystrokes(game, x, y, width, accent, surface, delta);
            return;
        }
        if (kinds[index] == KIND_BRAND) {
            drawBrand(game, x, y, width, height, values[index], accent, surface);
            return;
        }

        float opacity = settings.hudOpacity();
        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        Draw.roundedRect(game, x * s, y * s, width * s, height * s, RADIUS * s, Colors.fade(Theme.chip(surface), opacity));
        Draw.roundedOutline(
                game,
                x * s,
                y * s,
                width * s,
                height * s,
                RADIUS * s,
                Math.max(1, s / 2),
                Colors.fade(Theme.LINE, opacity)
        );
        if (settings.hudAccentBar()) {
            Draw.roundedRect(game, (x + 4) * s, (y + 4) * s, Math.max(1, s), (height - 8) * s, Math.max(1, s / 2), accent);
        }
        game.popScale();

        int textX = x + PAD_X + (settings.hudAccentBar() ? 4 : 0);
        int textY = y + (height - game.textHeight()) / 2 + 1;
        if (!labels[index].isEmpty()) {
            game.drawText(labels[index], textX, textY, Theme.TEXT_DIM, true);
            textX += game.textWidth(labels[index]) + 5;
        }
        game.drawText(values[index], textX, textY, colors[index], true);
    }

    /**
     * Логотип.
     *
     * Значок - настоящая иконка лаунчера. Три стиля не прихоть: на стриме нужна
     * крупная подпись, на турнире - самая незаметная из возможных.
     */
    private void drawBrand(GameBridge game, int x, int y, int width, int height, String version, int accent, int surface) {
        int style = settings.module("watermark").choice("style").index();
        float opacity = settings.hudOpacity();
        int s = Pixels.scale(game);

        if (style != 1) {
            game.pushScale(1f / s);
            Draw.roundedRect(game, x * s, y * s, width * s, height * s, RADIUS * s, Colors.fade(Theme.chip(surface), opacity));
            Draw.roundedOutline(
                    game,
                    x * s,
                    y * s,
                    width * s,
                    height * s,
                    RADIUS * s,
                    Math.max(1, s / 2),
                    Colors.fade(Colors.withAlpha(accent, 0x40), opacity)
            );
            game.popScale();
        }

        int centerY = y + height / 2;
        int textY = y + (height - game.textHeight()) / 2 + 1;

        if (style == 2) {
            int mark = Math.min(height - 4, 14);
            Brand.draw(game, x + (width - mark) / 2, centerY - mark / 2, mark, opacity, accent);
            return;
        }

        int markSize = Math.min(height - 4, 12);
        int textX = x + (style == 1 ? 0 : PAD_X);
        Brand.draw(game, textX, centerY - markSize / 2, markSize, opacity, accent);
        textX += markSize + 5;

        // Одна отрисовка вместо двух со сдвигом: самодельная жирность слепляет буквы
        // в белое пятно. Читаемость даёт тень и воздух между буквами, а не толщина.
        int titleColor = Colors.fade(Colors.mix(Theme.TEXT_DIM, Theme.TEXT, 0.5f), opacity);
        String title = "NIMBUS";
        for (int i = 0; i < title.length(); i++) {
            String letter = title.substring(i, i + 1);
            game.drawText(letter, textX, textY, titleColor, true);
            textX += game.textWidth(letter) + 1;
        }
        textX += 5;
        if (!version.isEmpty()) {
            game.drawText(version, textX, textY, Colors.fade(Colors.withAlpha(accent, 0xFF), opacity), true);
        }
    }

    // ------------------------------------------------------------------ режим расстановки

    private void handleEditing(GameBridge game) {
        boolean left = game.mouseDown(Keys.MOUSE_LEFT);
        boolean right = game.mouseDown(Keys.MOUSE_RIGHT);
        boolean leftClick = left && !editPreviousLeft;
        boolean rightClick = right && !editPreviousRight;
        editPreviousLeft = left;
        editPreviousRight = right;

        int mouseX = game.mouseX();
        int mouseY = game.mouseY();
        double preciseX = game.mouseXPrecise();
        double preciseY = game.mouseYPrecise();

        int hovered = blockAt(mouseX, mouseY);

        if (rightClick && hovered >= 0) {
            // Правая кнопка возвращает панель в стопку.
            layout.reset(keys[hovered]);
            dragKey = null;
            return;
        }

        if (leftClick && hovered >= 0) {
            dragKey = keys[hovered];
            dragOffsetX = preciseX - boxX[hovered];
            dragOffsetY = preciseY - boxY[hovered];
        }
        if (!left) {
            if (dragKey != null) {
                dragKey = null;
                layout.saveIfDirty();
            }
            guideX = Integer.MIN_VALUE;
            guideY = Integer.MIN_VALUE;
            return;
        }
        if (dragKey == null) {
            return;
        }

        int index = indexOf(dragKey);
        if (index < 0) {
            dragKey = null;
            return;
        }

        int width = boxW[index];
        int height = boxH[index];
        int screenWidth = game.screenWidth();
        int screenHeight = game.screenHeight();

        int x = (int) Math.round(preciseX - dragOffsetX);
        int y = (int) Math.round(preciseY - dragOffsetY);

        // Сначала сетка, потом направляющие: прилипание должно перебивать шаг сетки,
        // иначе панель никогда не встанет ровно по центру нечётного экрана.
        x = Math.round(x / (float) GRID) * GRID;
        y = Math.round(y / (float) GRID) * GRID;

        guideX = Integer.MIN_VALUE;
        guideY = Integer.MIN_VALUE;

        int centerX = (screenWidth - width) / 2;
        int centerY = (screenHeight - height) / 2;
        if (Math.abs(x - centerX) <= SNAP) {
            x = centerX;
            guideX = screenWidth / 2;
        } else if (Math.abs(x - MARGIN) <= SNAP) {
            x = MARGIN;
        } else if (Math.abs(x - (screenWidth - MARGIN - width)) <= SNAP) {
            x = screenWidth - MARGIN - width;
        }
        if (Math.abs(y - centerY) <= SNAP) {
            y = centerY;
            guideY = screenHeight / 2;
        } else if (Math.abs(y - MARGIN) <= SNAP) {
            y = MARGIN;
        } else if (Math.abs(y - (screenHeight - MARGIN - height)) <= SNAP) {
            y = screenHeight - MARGIN - height;
        }

        // Выравнивание по соседним панелям: ровные колонки сами собой не получаются.
        for (int i = 0; i < blocks; i++) {
            if (i == index) {
                continue;
            }
            if (Math.abs(x - boxX[i]) <= SNAP) {
                x = boxX[i];
                guideX = x;
            } else if (Math.abs(x + width - (boxX[i] + boxW[i])) <= SNAP) {
                x = boxX[i] + boxW[i] - width;
                guideX = x + width;
            }
            if (Math.abs(y - boxY[i]) <= SNAP) {
                y = boxY[i];
                guideY = y;
            } else if (Math.abs(y + height - (boxY[i] + boxH[i])) <= SNAP) {
                y = boxY[i] + boxH[i] - height;
                guideY = y + height;
            }
        }

        x = Math.max(0, Math.min(Math.max(0, screenWidth - width), x));
        y = Math.max(0, Math.min(Math.max(0, screenHeight - height), y));

        boxX[index] = x;
        boxY[index] = y;
        layout.set(
                dragKey,
                x / (float) Math.max(1, screenWidth - width),
                y / (float) Math.max(1, screenHeight - height)
        );
    }

    private int blockAt(int x, int y) {
        for (int i = blocks - 1; i >= 0; i--) {
            if (x >= boxX[i] && x < boxX[i] + boxW[i] && y >= boxY[i] && y < boxY[i] + boxH[i]) {
                return i;
            }
        }
        return -1;
    }

    private int indexOf(String key) {
        for (int i = 0; i < blocks; i++) {
            if (keys[i].equals(key)) {
                return i;
            }
        }
        return -1;
    }

    /** Затемнение и сетка под панелями. */
    private void drawEditorBackground(GameBridge game, int accent) {
        int screenWidth = game.screenWidth();
        int screenHeight = game.screenHeight();
        game.fill(0, 0, screenWidth, screenHeight, Colors.fade(0xFF05070B, 0.55f * editFade));

        int s = Pixels.scale(game);
        int step = GRID * 4;
        int line = Colors.fade(0xFFFFFFFF, 0.05f * editFade);
        game.pushScale(1f / s);
        for (int x = 0; x <= screenWidth; x += step) {
            game.fill(x * s, 0, Math.max(1, s / 2), screenHeight * s, line);
        }
        for (int y = 0; y <= screenHeight; y += step) {
            game.fill(0, y * s, screenWidth * s, Math.max(1, s / 2), line);
        }
        // Оси экрана видны всегда: по ним ловится центр.
        game.fill((screenWidth / 2) * s, 0, Math.max(1, s / 2), screenHeight * s, Colors.fade(accent, 0.18f * editFade));
        game.fill(0, (screenHeight / 2) * s, screenWidth * s, Math.max(1, s / 2), Colors.fade(accent, 0.18f * editFade));
        game.popScale();
    }

    /** Рамки панелей, направляющие и подсказка. */
    private void drawEditorForeground(GameBridge game, int accent) {
        int s = Pixels.scale(game);
        int screenWidth = game.screenWidth();
        int screenHeight = game.screenHeight();
        int mouseX = game.mouseX();
        int mouseY = game.mouseY();
        int hovered = blockAt(mouseX, mouseY);

        game.pushScale(1f / s);
        for (int i = 0; i < blocks; i++) {
            boolean active = dragKey != null && dragKey.equals(keys[i]);
            boolean over = i == hovered;
            int color = Colors.fade(active ? Colors.withAlpha(accent, 0xFF) : (over ? Theme.TEXT_DIM : Theme.LINE), editFade);
            Draw.roundedOutline(
                    game,
                    (boxX[i] - 2) * s,
                    (boxY[i] - 2) * s,
                    (boxW[i] + 4) * s,
                    (boxH[i] + 4) * s,
                    4 * s,
                    Math.max(1, s / 2),
                    color
            );
        }
        if (guideX != Integer.MIN_VALUE) {
            game.fill(guideX * s, 0, Math.max(1, s / 2), screenHeight * s, Colors.fade(accent, 0.8f * editFade));
        }
        if (guideY != Integer.MIN_VALUE) {
            game.fill(0, guideY * s, screenWidth * s, Math.max(1, s / 2), Colors.fade(accent, 0.8f * editFade));
        }
        game.popScale();

        String hint = "Перетащите панели · ПКМ возвращает в стопку · ESC завершает";
        int width = game.textWidth(hint) + 20;
        int height = game.textHeight() + 10;
        int x = (screenWidth - width) / 2;
        int y = 14;
        game.pushScale(1f / s);
        Draw.roundedRect(game, x * s, y * s, width * s, height * s, RADIUS * s, Colors.fade(0xFF0B0E14, 0.9f * editFade));
        Draw.roundedOutline(game, x * s, y * s, width * s, height * s, RADIUS * s, Math.max(1, s / 2), Colors.fade(Colors.withAlpha(accent, 0x80), editFade));
        game.popScale();
        Draw.textCentered(game, hint, screenWidth / 2, y + 5, Colors.fade(Theme.TEXT_DIM, editFade), false);
    }

    // ------------------------------------------------------------------ график

    private void sampleHistory(long now) {
        if (now - lastSample < 200_000_000L) {
            return;
        }
        lastSample = now;
        history[historyHead] = fps;
        historyHead = (historyHead + 1) % HISTORY;
        if (historyCount < HISTORY) {
            historyCount++;
        }
    }

    private void drawGraph(GameBridge game, int x, int y, int width, int height, int accent, int surface) {
        float opacity = settings.hudOpacity();
        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        Draw.roundedRect(game, x * s, y * s, width * s, height * s, RADIUS * s, Colors.fade(Theme.chip(surface), opacity));
        Draw.roundedOutline(
                game,
                x * s,
                y * s,
                width * s,
                height * s,
                RADIUS * s,
                Math.max(1, s / 2),
                Colors.fade(Theme.LINE, opacity)
        );
        game.popScale();

        int plotX = x + 4;
        int plotY = y + 4;
        int plotWidth = width - 8;
        int plotHeight = height - 8;
        if (plotWidth <= 2 || plotHeight <= 2 || historyCount == 0) {
            return;
        }

        int peak = 60;
        int shown = Math.min(historyCount, plotWidth);
        for (int i = 0; i < shown; i++) {
            peak = Math.max(peak, sample(shown, i));
        }

        boolean fill = settings.module("graph").bool("fill").get();
        int previous = -1;
        for (int i = 0; i < shown; i++) {
            int value = sample(shown, i);
            int columnHeight = Math.max(1, Math.round(value / (float) peak * plotHeight));
            int columnX = plotX + plotWidth - shown + i;
            int columnY = plotY + plotHeight - columnHeight;
            if (fill) {
                game.fillGradient(
                        columnX,
                        columnY,
                        1,
                        columnHeight,
                        Colors.withAlpha(accent, 0x8C),
                        Colors.withAlpha(accent, 0x14)
                );
            }
            game.fill(columnX, columnY, 1, 1, accent);
            if (previous >= 0 && Math.abs(previous - columnY) > 1) {
                // Соединительная стойка: без неё линия рассыпается на точки на резких просадках.
                int top = Math.min(previous, columnY);
                game.fill(columnX, top, 1, Math.abs(previous - columnY), Colors.withAlpha(accent, 0xB0));
            }
            previous = columnY;
        }

        Draw.textRight(game, Integer.toString(peak), x + width - 4, y + 2, Theme.TEXT_MUTED, false);
    }

    private int sample(int shown, int index) {
        int position = (historyHead - shown + index + HISTORY * 2) % HISTORY;
        return history[position];
    }

    // ------------------------------------------------------------------ клавиши

    private void drawKeystrokes(GameBridge game, int x, int y, int blockWidth, int accent, int surface, float delta) {
        Module module = settings.module("keystrokes");
        boolean mouse = module.bool("mouse").get();
        boolean showCps = module.bool("cps").get();
        boolean space = module.bool("space").get();

        int size = 18;
        int gap = 2;

        key(game, 0, x + size + gap, y, size, size, "W", game.keyDown(Keys.W), accent, surface, delta);
        int row = y + size + gap;
        key(game, 1, x, row, size, size, "A", game.keyDown(Keys.A), accent, surface, delta);
        key(game, 2, x + size + gap, row, size, size, "S", game.keyDown(Keys.S), accent, surface, delta);
        key(game, 3, x + (size + gap) * 2, row, size, size, "D", game.keyDown(Keys.D), accent, surface, delta);

        int next = row + size + gap;
        if (mouse) {
            int half = (blockWidth - gap) / 2;
            boolean left = game.mouseDown(Keys.MOUSE_LEFT);
            boolean right = game.mouseDown(Keys.MOUSE_RIGHT);
            key(game, 4, x, next, half, size, showCps ? Integer.toString(cps(0)) : "ЛКМ", left, accent, surface, delta);
            key(game, 5, x + half + gap, next, blockWidth - half - gap, size, showCps ? Integer.toString(cps(1)) : "ПКМ", right, accent, surface, delta);
            next += size + gap;
        }
        if (space) {
            key(game, 6, x, next, blockWidth, 9, "", game.keyDown(Keys.SPACE), accent, surface, delta);
        }
    }

    private void key(
            GameBridge game,
            int slot,
            int x,
            int y,
            int width,
            int height,
            String text,
            boolean pressed,
            int accent,
            int surface,
            float delta
    ) {
        keyGlow[slot] = Easing.approach(keyGlow[slot], pressed ? 1f : 0f, 0.045f, delta);
        float glow = keyGlow[slot];
        float opacity = settings.hudOpacity();

        int background = Colors.mix(Colors.fade(Theme.chip(surface), opacity), Colors.withAlpha(accent, 0xE6), glow);
        int border = Colors.mix(Colors.fade(Theme.LINE, opacity), Colors.withAlpha(accent, 0xFF), glow);

        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        Draw.roundedRect(game, x * s, y * s, width * s, height * s, 4 * s, background);
        Draw.roundedOutline(game, x * s, y * s, width * s, height * s, 4 * s, Math.max(1, s / 2), border);
        game.popScale();

        if (!text.isEmpty()) {
            int color = Colors.mix(Theme.TEXT_DIM, 0xFF0B0E14, glow);
            Draw.textCentered(game, text, x + width / 2, y + (height - game.textHeight()) / 2 + 1, color, false);
        }
    }

    // ------------------------------------------------------------------ прицел

    /**
     * Свой прицел.
     *
     * Центр считается в физических пикселях, а не в единицах интерфейса. При масштабе 3
     * одна единица - это три настоящих пикселя, и округление центра уводит прицел
     * в сторону от точки, куда реально смотрит игрок. Толщина подгоняется по чётности
     * экрана: иначе лишний пиксель всегда достаётся одной из сторон.
     */
    private void drawCrosshair(GameBridge game, int accent) {
        Module module = settings.module("crosshair");
        int s = Pixels.scale(game);
        int length = Math.max(1, module.slider("length").asInt()) * s;
        int gap = module.slider("gap").asInt() * s;
        boolean outline = module.bool("outline").get();
        int color = module.bool("accent").get() ? Colors.withAlpha(accent, 0xFF) : 0xFFFFFFFF;

        int screenWidth = game.screenWidth() * s;
        int screenHeight = game.screenHeight() * s;

        int thickX = Math.max(1, module.slider("thickness").asInt() * s);
        int thickY = thickX;
        if (((screenWidth - thickX) & 1) != 0) {
            thickX++;
        }
        if (((screenHeight - thickY) & 1) != 0) {
            thickY++;
        }

        int left = (screenWidth - thickX) / 2;
        int top = (screenHeight - thickY) / 2;

        game.pushScale(1f / s);
        arm(game, left - gap - length, top, length, thickY, color, outline, s);
        arm(game, left + thickX + gap, top, length, thickY, color, outline, s);
        arm(game, left, top - gap - length, thickX, length, color, outline, s);
        arm(game, left, top + thickY + gap, thickX, length, color, outline, s);
        if (module.bool("dot").get()) {
            arm(game, left, top, thickX, thickY, color, outline, s);
        }
        game.popScale();
    }

    private static void arm(GameBridge game, int x, int y, int width, int height, int color, boolean outline, int scale) {
        if (width <= 0 || height <= 0) {
            return;
        }
        if (outline) {
            int edge = Math.max(1, scale / 2);
            game.fill(x - edge, y - edge, width + edge * 2, height + edge * 2, 0x9E000000);
        }
        game.fill(x, y, width, height, color);
    }

    // ------------------------------------------------------------------ счётчики

    private void countFrame(long now) {
        frames++;
        long elapsed = now - secondStartedAt;
        if (elapsed >= 1_000_000_000L) {
            fps = (int) Math.round(frames * 1_000_000_000.0 / elapsed);
            frames = 0;
            secondStartedAt = now;
        }
    }

    private void trackClicks(GameBridge game, long now, boolean menuOpen) {
        for (int button = 0; button < 2; button++) {
            boolean down = game.mouseDown(button);
            if (down && !mouseWas[button] && !menuOpen) {
                clicks[button][clickHead[button]] = now;
                clickHead[button] = (clickHead[button] + 1) % clicks[button].length;
            }
            mouseWas[button] = down;
        }
    }

    private static int gradeFps(int value) {
        if (value >= 120) {
            return Theme.GOOD;
        }
        if (value >= 45) {
            return Theme.TEXT;
        }
        return value >= 25 ? Theme.WARN : Theme.DANGER;
    }

    private static int gradePing(int value) {
        if (value < 0) {
            return Theme.TEXT_MUTED;
        }
        if (value <= 60) {
            return Theme.GOOD;
        }
        return value <= 140 ? Theme.WARN : Theme.DANGER;
    }

    private static String coordinates(GameBridge game, Module module) {
        if (!game.inWorld()) {
            return "вне мира";
        }
        double scale = module.bool("nether").get() ? 0.125 : 1.0;
        int x = (int) Math.floor(game.playerX() * scale);
        int y = (int) Math.floor(game.playerY());
        int z = (int) Math.floor(game.playerZ() * scale);
        String text = x + " " + y + " " + z;
        if (module.bool("direction").get()) {
            text = text + "  " + facing(game.playerYaw());
        }
        return text;
    }

    /**
     * Поворот камеры бывает любым числом, в том числе отрицательным и больше круга,
     * поэтому его сначала приводим к одному обороту.
     */
    private static String facing(float yaw) {
        float normalized = ((yaw % 360f) + 360f) % 360f;
        int index = (int) Math.floor((normalized + 22.5f) / 45f) % FACINGS.length;
        return FACINGS[index];
    }

    private static String clock(Module module) {
        Calendar now = Calendar.getInstance();
        boolean half = module.choice("format").index() == 1;
        int hour = now.get(Calendar.HOUR_OF_DAY);
        String suffix = "";
        if (half) {
            suffix = hour >= 12 ? " PM" : " AM";
            hour = hour % 12;
            if (hour == 0) {
                hour = 12;
            }
        }
        String text = two(hour) + ":" + two(now.get(Calendar.MINUTE));
        if (module.bool("seconds").get()) {
            text = text + ":" + two(now.get(Calendar.SECOND));
        }
        return text + suffix;
    }

    private String session() {
        long seconds = (System.nanoTime() - startedAt) / 1_000_000_000L;
        long hours = seconds / 3600L;
        long minutes = (seconds % 3600L) / 60L;
        if (hours > 0) {
            return hours + " ч " + minutes + " мин";
        }
        return minutes + " мин " + (seconds % 60L) + " с";
    }

    private static String two(int value) {
        return value < 10 ? "0" + value : Integer.toString(value);
    }
}
