package dev.nimbus.core.ui;

import dev.nimbus.bridge.GameBridge;
import dev.nimbus.bridge.Keys;
import dev.nimbus.bridge.Sounds;
import dev.nimbus.core.Settings;
import dev.nimbus.core.config.Module;
import dev.nimbus.core.config.Option;
import dev.nimbus.core.render.Colors;
import dev.nimbus.core.render.Draw;
import dev.nimbus.core.render.Easing;
import dev.nimbus.core.render.Pixels;
import dev.nimbus.core.render.Ring;
import dev.nimbus.core.render.Shapes;

import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Главное окно клиента.
 *
 * Замысел. Почти все самодельные меню выглядят дёшево по одним и тем же причинам:
 * разные отступы в соседних блоках, текст, вылезающий за край, мгновенные
 * переключения без анимации и десяток кнопок без иерархии. Здесь всё наоборот:
 * один шаг сетки, одна высота строки, три уровня текста по яркости и единственный
 * акцентный цвет, который имеет право светиться.
 *
 * Содержимое не зашито в код окна: окно рисует то, что описано в списке модулей.
 * Поэтому в меню физически не может быть кнопки, которая ничего не делает.
 *
 * Обрезание. У игры внутри есть стопка областей: включение кладёт на неё пересечение
 * с текущей областью, а снятие снимает ровно один слой. Значит вложенность игра
 * считает сама, а от окна требуется единственное - строгая парность. Раньше снятие
 * вложенной области вместо снятия делало третье включение: слои копились от кадра
 * к кадру, пересечение сжималось в точку, и экран уходил в чёрное мелькание.
 * Теперь на каждое включение приходится ровно одно снятие, а остаток снимается в конце кадра.
 *
 * Ввод. Клик ловится как фронт между кадрами, а колесо приходит событием из
 * рантайма: опросить колесо у оконной библиотеки невозможно.
 *
 * Перетаскивание. Положение окна хранится дробным и берётся из дробного положения
 * курсора: округлённые координаты при масштабе интерфейса 3 меняются раз в три
 * настоящих пикселя, и окно едет заметными ступеньками.
 */
public final class ClickGui {

    private static final int WIDTH = 520;
    private static final int HEIGHT = 330;
    private static final int RAIL = 132;
    private static final int HEADER = 42;
    private static final int FOOTER = 26;
    private static final int PAD = 12;
    private static final int CARD_GAP = 6;
    private static final int CARD_HEIGHT = 42;
    private static final int ROW_HEIGHT = 24;
    private static final int COLOR_ROW_HEIGHT = 46;

    /** Высота строки со сбросом настроек модуля. */
    private static final int RESET_ROW_HEIGHT = 26;
    private static final int RADIUS = 9;
    private static final int COMPACT_RAIL = 88;
    private static final int COMPACT_ROW_HEIGHT = 42;
    private static final int COMPACT_COLOR_ROW_HEIGHT = 54;

    /** Сколько точек прокрутки даёт один щелчок колеса. */
    private static final float WHEEL_STEP = 46f;

    private static final Module.Category[] CATEGORIES = Module.Category.values();

    private final Settings settings;
    private final Map<String, Float> anim = new HashMap<>();
    private final Set<String> expanded = new HashSet<>();

    /**
     * Сколько областей обрезания сейчас открыто.
     *
     * Хранить сами прямоугольники не нужно: вложенные области игра пересекает сама.
     * Важно ровно одно - чтобы снятий было столько же, сколько включений.
     */
    private int clipDepth;

    private boolean open;
    private float progress;
    private long lastFrame;
    private boolean framed;

    /** Целевое положение окна (туда тянет курсор). */
    private double windowX = Double.NaN;
    private double windowY = Double.NaN;
    private int windowWidth;
    private int windowHeight;
    private int layoutRail;
    private boolean compact;
    private int lastScreenWidth;
    private int lastScreenHeight;

    /** Показываемое положение окна (догоняет целевое и сглаживает ступеньки). */
    private float shownX;
    private float shownY;

    private boolean dragging;
    private double dragOffsetX;
    private double dragOffsetY;

    private int category;
    private float scroll;
    private float scrollTarget;
    private float railPill;

    private boolean previousDown;
    private boolean scrollDragging;
    private int scrollGrabY;
    private float scrollGrabStart;

    private Option.Slider activeSlider;
    private int activeSliderX;
    private int activeSliderWidth;
    private Option.Color activeHue;
    private Option.Color activeSaturation;
    private int activeColorX;
    private int activeColorWidth;
    private Option.Key capturing;

    /** Текстовая настройка, в которую сейчас идёт ввод с клавиатуры. */
    private Option.Text editingText;

    /**
     * Взведён ли захват клавиши.
     *
     * Становится истиной только после того, как все привязываемые клавиши отпущены.
     */
    private boolean captureArmed;

    /**
     * Когда настройки сохранились в последний раз: слово "Сохранено" ненадолго
     * подсвечивается акцентом. Флаг рядом потому, что нуль - законное значение таймера.
     */
    private long savedAt;
    private boolean everSaved;

    /**
     * Подсказка под курсором.
     *
     * Копится за кадр и рисуется в самом конце. Рисовать её на месте нельзя:
     * её срежет обрезание списка и накроет следующая карточка.
     */
    private String tooltipText;
    private int tooltipX;
    private int tooltipY;

    /** Просьба открыть режим расстановки панелей. Забирается ядром. */
    private boolean layoutRequest;

    public ClickGui(Settings settings) {
        this.settings = settings;
        this.category = Math.max(0, Math.min(CATEGORIES.length - 1, settings.guiCategory()));
    }

    public boolean visible() {
        return open;
    }

    public boolean capturingKey() {
        return capturing != null;
    }

    /** Ловит ли сейчас меню текст: игра обязана замолчать для клавиш. */
    public boolean capturingText() {
        return editingText != null;
    }

    public void cancelKeyCapture() {
        capturing = null;
        captureArmed = false;
    }

    /** Выйти из ввода текста без сохранения набранного: оно и так уже в поле. */
    public void cancelTextCapture() {
        editingText = null;
    }

    public void toggle() {
        if (open) {
            close();
        } else {
            open = true;
        }
    }

    public void close() {
        if (!open) {
            return;
        }
        open = false;
        capturing = null;
        captureArmed = false;
        editingText = null;
        activeSlider = null;
        activeHue = null;
        activeSaturation = null;
        scrollDragging = false;
        dragging = false;
        tooltipText = null;
        settings.saveIfDirty();
    }

    /** Ядро спрашивает один раз: просили ли режим расстановки панелей. */
    public boolean takeLayoutRequest() {
        boolean value = layoutRequest;
        layoutRequest = false;
        return value;
    }

    /** Отметить успешное сохранение: подсветка слова "Сохранено" гаснет за секунду с лишним. */
    private void markSaved(long now) {
        savedAt = now;
        everSaved = true;
    }

    public void render(GameBridge game, String version, int fps) {
        long now = System.nanoTime();
        // Нуль - законное значение таймера, поэтому первый кадр отмечен флагом:
        // иначе разница с нулём дала бы шаг времени в целый аптайм системы.
        float delta = framed ? Math.min(0.1f, (now - lastFrame) / 1_000_000_000f) : 0f;
        lastFrame = now;
        framed = true;

        float speed = Math.max(0.25f, settings.speed());
        progress = Easing.approach(progress, open ? 1f : 0f, 0.07f / speed, delta);
        if (!open && progress < 0.01f) {
            // Закрытое окно не рисует ничего вообще и не оставляет за собой обрезание.
            progress = 0f;
            tooltipText = null;
            releaseClips(game);
            previousDown = game.mouseDown(Keys.MOUSE_LEFT);
            game.takeScroll();
            return;
        }

        float ease = Easing.outCubic(progress);
        int screenWidth = game.screenWidth();
        int screenHeight = game.screenHeight();
        int width = Math.min(WIDTH, Math.max(160, screenWidth - 16));
        int height = Math.min(HEIGHT, Math.max(120, screenHeight - 16));
        compact = width < 420 || height < 250;
        layoutRail = compact
                ? Math.min(Math.max(1, width - 1), Math.max(40, Math.min(COMPACT_RAIL, width / 3)))
                : Math.min(RAIL, Math.max(1, width - 1));
        windowWidth = width;
        windowHeight = height;

        if (Double.isNaN(windowX) || screenWidth != lastScreenWidth || screenHeight != lastScreenHeight) {
            float normalizedX = settings.guiWindowX();
            float normalizedY = settings.guiWindowY();
            windowX = 4f + normalizedX * Math.max(0, screenWidth - width - 8);
            windowY = 4f + normalizedY * Math.max(0, screenHeight - height - 8);
            shownX = (float) windowX;
            shownY = (float) windowY;
            lastScreenWidth = screenWidth;
            lastScreenHeight = screenHeight;
        }

        int mouseX = game.mouseX();
        int mouseY = game.mouseY();
        double preciseX = game.mouseXPrecise();
        double preciseY = game.mouseYPrecise();
        boolean down = game.mouseDown(Keys.MOUSE_LEFT);
        boolean click = down && !previousDown;
        boolean release = !down && previousDown;

        // Перетаскивание считается до отрисовки: иначе окно отстаёт от курсора на кадр.
        int shownIntX = Math.round(shownX);
        int shownIntY = Math.round(shownY);
        if (click && inside(mouseX, mouseY, shownIntX, shownIntY, width, HEADER) && mouseX < shownIntX + width - 30) {
            dragging = true;
            dragOffsetX = preciseX - windowX;
            dragOffsetY = preciseY - windowY;
        }
        if (dragging && down) {
            windowX = preciseX - dragOffsetX;
            windowY = preciseY - dragOffsetY;
        }
        windowX = clamp(windowX, 4, Math.max(4, screenWidth - width - 4));
        windowY = clamp(windowY, 4, Math.max(4, screenHeight - height - 4));
        if (dragging && down) {
            float freeWidth = Math.max(1f, screenWidth - width);
            float freeHeight = Math.max(1f, screenHeight - height);
            settings.setGuiWindowPosition((float) ((windowX - 4) / Math.max(1f, freeWidth - 8f)),
                    (float) ((windowY - 4) / Math.max(1f, freeHeight - 8f)));
        }
        // Показываемое положение догоняет целевое: короткий хвост съедает ступеньки
        // целочисленной сетки интерфейса, но не даёт ощущения ватного окна.
        shownX = Easing.approach(shownX, (float) windowX, 0.018f, delta);
        shownY = Easing.approach(shownY, (float) windowY, 0.018f, delta);
        if (Math.abs(shownX - windowX) < 0.35f) {
            shownX = (float) windowX;
        }
        if (Math.abs(shownY - windowY) < 0.35f) {
            shownY = (float) windowY;
        }

        int x = Math.round(shownX);
        int y = Math.round(shownY) + Math.round((1f - ease) * 12f);

        int accent = settings.accent();
        int surface = settings.surfaceIndex();

        // Затемнение игры: без него окно тонет в пёстром мире и читается как мусор.
        float dim = settings.dim() * ease;
        if (dim > 0.002f) {
            game.fill(0, 0, screenWidth, screenHeight, Colors.fade(0xFF05070B, dim));
        }

        // Подсказка собирается заново каждый кадр: иначе она застынет там, где
        // курсор побывал однажды, и будет висеть над пустым местом.
        tooltipText = null;

        drawFrame(game, x, y, width, height, ease, accent, surface);
        // Крестик забирает клик себе. Иначе один и тот же фронт нажатия достанется
        // и тому, что оказалось под закрывшимся окном: категории, карточке, переключателю.
        if (drawHeader(game, x, y, width, version, fps, ease, accent, mouseX, mouseY, click)) {
            click = false;
        }
        drawRail(game, x, y + HEADER, layoutRail, height - HEADER - FOOTER, ease, accent, mouseX, mouseY, click, delta);

        int contentX = x + layoutRail;
        int contentY = y + HEADER;
        int contentWidth = Math.max(1, width - layoutRail);
        int contentHeight = Math.max(1, height - HEADER - FOOTER);
        drawContent(game, contentX, contentY, contentWidth, contentHeight, ease, accent, surface, mouseX, mouseY, down, click, delta);

        drawFooter(game, x, y + height - FOOTER, width, FOOTER, ease, accent, mouseX, mouseY, click);

        // Подсказка рисуется последней: она обязана лежать поверх всего окна.
        drawTooltip(game, screenWidth, screenHeight, ease);

        if (capturing != null) {
            captureKey(game);
        }

        if (release) {
            dragging = false;
            scrollDragging = false;
            activeSlider = null;
            activeHue = null;
            activeSaturation = null;
            if (settings.saveIfDirty()) {
                markSaved(now);
            }
        }

        // Колесо мыши: события накапливает рантайм, пока открыто меню.
        double wheel = game.takeScroll();
        if (wheel != 0.0) {
            scrollTarget -= (float) wheel * WHEEL_STEP;
        }
        // Стрелки остаются запасным способом: на версиях без перехвата колеса они единственные.
        if (game.keyDown(Keys.UP)) {
            scrollTarget -= 420f * delta;
        }
        if (game.keyDown(Keys.DOWN)) {
            scrollTarget += 420f * delta;
        }

        previousDown = down;
        releaseClips(game);
    }

    // ------------------------------------------------------------------ обрезание

    /**
     * Вложенное обрезание через пересечение с текущей областью.
     *
     * Игра умеет только "включить прямоугольник" и "выключить всё", поэтому стопка ведётся здесь.
     */
    private void pushClip(GameBridge game, int x, int y, int width, int height) {
        // Пересекать с внешней областью вручную не надо: игра делает это сама.
        // Счётчик растёт всегда, даже если область вырожденная: иначе парность сорвётся.
        clipDepth++;
        game.scissorOn(x, y, Math.max(0, width), Math.max(0, height));
    }

    private void popClip(GameBridge game) {
        if (clipDepth <= 0) {
            return;
        }
        clipDepth--;
        game.scissorOff();
    }

    /** Страховка на конец кадра: ни одна область не должна утечь в игру. */
    private void releaseClips(GameBridge game) {
        while (clipDepth > 0) {
            clipDepth--;
            game.scissorOff();
        }
    }

    // ------------------------------------------------------------------ каркас

    private void drawFrame(GameBridge game, int x, int y, int width, int height, float ease, int accent, int surface) {
        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        Shapes.shadow(game, x * s, y * s, width * s, height * s, RADIUS * s, Math.max(3, 5 * s), Colors.fade(0xFF000000, ease * 0.55f));
        Shapes.roundedGradient(
                game,
                x * s,
                y * s,
                width * s,
                height * s,
                RADIUS * s,
                Colors.fade(Theme.baseTop(surface), ease),
                Colors.fade(Theme.base(surface), ease)
        );
        Draw.roundedOutline(game, x * s, y * s, width * s, height * s, RADIUS * s, Math.max(1, s / 2), Colors.fade(Theme.LINE, ease));
        // Левая панель темнее содержимого: глаз сразу видит, где навигация, а где работа.
        game.fill(x * s, (y + HEADER) * s, layoutRail * s, (height - HEADER - FOOTER) * s, Colors.fade(Theme.rail(surface), ease * 0.9f));
        game.fill((x + layoutRail) * s, (y + HEADER) * s, Math.max(1, s / 2), (height - HEADER - FOOTER) * s, Colors.fade(Theme.LINE_SOFT, ease));
        game.fill(x * s, (y + HEADER) * s, width * s, Math.max(1, s / 2), Colors.fade(Theme.LINE_SOFT, ease));
        game.fill(x * s, (y + height - FOOTER) * s, width * s, Math.max(1, s / 2), Colors.fade(Theme.LINE_SOFT, ease));
        // Акцентная нить под заголовком - единственная яркая линия во всём окне.
        Shapes.horizontalGradient(
                game,
                x * s,
                (y + HEADER) * s,
                (width / 2) * s,
                Math.max(1, s / 2),
                Colors.fade(accent, ease * 0.9f),
                Colors.fade(accent, 0f)
        );
        game.popScale();
    }

    /**
     * Шапка окна. Возвращает true, если клик уже израсходован на крестик.
     */
    private boolean drawHeader(
            GameBridge game,
            int x,
            int y,
            int width,
            String version,
            int fps,
            float ease,
            int accent,
            int mouseX,
            int mouseY,
            boolean click
    ) {
        int centerY = y + HEADER / 2;
        int textY = centerY - game.textHeight() / 2;

        // Значок - настоящий логотип лаунчера, а не нарисованная на ходу фигура.
        int logoSize = 18;
        Brand.draw(game, x + 12, centerY - logoSize / 2, logoSize, ease, accent);

        // Заголовок рисуется ровно один раз. Двойная отрисовка со сдвигом в пиксель -
        // самодельная "жирность", от которой буквы слипаются в белое пятно. Воздух между
        // буквами читается как дорогой шрифт гораздо лучше любой толщины.
        int titleX = x + 12 + logoSize + 10;
        int titleColor = Colors.mix(Theme.TEXT_DIM, Theme.TEXT, 0.45f);
        int afterTitle = drawSpaced(game, "NIMBUS", titleX, textY, Colors.fade(titleColor, ease), 1);
        drawSpaced(game, "CLIENT", afterTitle + 7, textY, Colors.fade(Theme.TEXT_MUTED, ease * 0.9f), 1);

        // Правый край: состояние игры и закрытие.
        int closeSize = 16;
        int closeX = x + width - PAD - closeSize;
        boolean overClose = inside(mouseX, mouseY, closeX, centerY - closeSize / 2, closeSize, closeSize);
        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        Draw.roundedRect(
                game,
                closeX * s,
                (centerY - closeSize / 2) * s,
                closeSize * s,
                closeSize * s,
                4 * s,
                Colors.fade(overClose ? 0x30FF5470 : 0x14FFFFFF, ease)
        );
        game.popScale();
        Draw.textCentered(game, "×", closeX + closeSize / 2, textY, Colors.fade(overClose ? Theme.DANGER : Theme.TEXT_DIM, ease), false);
        if (click && overClose) {
            beep(game, Sounds.TONE, 0.85f);
            close();
            return true;
        }

        int chipRight = closeX - 8;
        int available = Math.max(0, closeX - 12 - (titleX + 70));
        String versionText = Shapes.clip(game, version, Math.max(24, available / 3));
        chipRight = chip(game, chipRight, centerY, fps + " fps", ease, Theme.TEXT_DIM);
        int ping = game.ping();
        if (ping >= 0 && chipRight - 6 - game.textWidth(ping + " мс") - 12 > titleX + 70) {
            chipRight = chip(game, chipRight - 6, centerY, ping + " мс", ease, Theme.TEXT_DIM);
        }
        if (chipRight - 6 - game.textWidth(versionText) - 12 > titleX + 70) {
            chip(game, chipRight - 6, centerY, versionText, ease, Colors.withAlpha(accent, 0xFF));
        }
        return false;
    }

    /**
     * Текст с разрядкой между буквами. Возвращает правый край.
     *
     * Шрифт игры один и менять его нельзя, поэтому единственный честный способ
     * сделать из него логотипную надпись - разрядка.
     */
    private int drawSpaced(GameBridge game, String text, int x, int y, int color, int spacing) {
        int cursor = x;
        for (int i = 0; i < text.length(); i++) {
            String letter = text.substring(i, i + 1);
            game.drawText(letter, cursor, y, color, false);
            cursor += game.textWidth(letter) + spacing;
        }
        return cursor - spacing;
    }

    /** Маленькая плашка справа налево. Возвращает левый край. */
    private int chip(GameBridge game, int rightX, int centerY, String text, float ease, int color) {
        int width = game.textWidth(text) + 12;
        int height = game.textHeight() + 6;
        int x = rightX - width;
        int y = centerY - height / 2;
        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        Draw.roundedRect(game, x * s, y * s, width * s, height * s, 4 * s, Colors.fade(0x12FFFFFF, ease));
        game.popScale();
        Draw.textCentered(game, text, x + width / 2, y + 3, Colors.fade(color, ease), false);
        return x;
    }

    private void drawRail(
            GameBridge game,
            int x,
            int y,
            int width,
            int height,
            float ease,
            int accent,
            int mouseX,
            int mouseY,
            boolean click,
            float delta
    ) {
        int itemHeight = compact ? 27 : 30;
        int top = y + 10;
        int s = Pixels.scale(game);

        float target = top + category * itemHeight;
        railPill = anim.containsKey("rail") ? Easing.approach(anim.get("rail"), target, 0.06f, delta) : target;
        anim.put("rail", railPill);

        game.pushScale(1f / s);
        Draw.roundedRect(game, (x + 8) * s, Math.round(railPill) * s, (width - 16) * s, (itemHeight - 4) * s, 6 * s, Colors.fade(0x14FFFFFF, ease));
        Draw.roundedRect(game, (x + 8) * s, (Math.round(railPill) + 5) * s, Math.max(1, s * 2), (itemHeight - 14) * s, s, Colors.fade(accent, ease));
        game.popScale();

        for (int i = 0; i < CATEGORIES.length; i++) {
            Module.Category item = CATEGORIES[i];
            int itemY = top + i * itemHeight;
            boolean over = inside(mouseX, mouseY, x + 8, itemY, width - 16, itemHeight - 4);
            float hover = anim("rail." + i, over ? 1f : 0f, 0.05f, delta);
            boolean active = i == category;

            if (click && over && !active) {
                category = i;
                settings.setGuiCategory(category);
                scroll = 0f;
                scrollTarget = 0f;
                beep(game, Sounds.BIT, 1.15f);
            }

            int color = active ? Theme.TEXT : Colors.mix(Theme.TEXT_MUTED, Theme.TEXT_DIM, hover);
            game.drawText(
                    Shapes.clip(game, item.title(), Math.max(24, width - (compact ? 34 : 34))),
                    x + (compact ? 14 : 20),
                    itemY + 5,
                    Colors.fade(color, ease),
                    false
            );

            int activeCount = settings.activeIn(item);
            if (activeCount > 0) {
                Draw.textRight(
                        game,
                        Integer.toString(activeCount),
                        x + width - 14,
                        itemY + 5,
                        Colors.fade(active ? Colors.withAlpha(accent, 0xFF) : Theme.TEXT_MUTED, ease),
                        false
                );
            }
            if (!compact) {
                game.drawText(
                        Shapes.clip(game, item.hint(), Math.max(24, width - 34)),
                        x + 20,
                        itemY + 15,
                        Colors.fade(Theme.TEXT_MUTED, ease * 0.85f),
                        false
                );
            }
        }

        // Низ панели: текущая клавиша меню - её чаще всего и забывают.
        String hint = "Меню: " + Keys.name(settings.menuKey());
        game.drawText(Shapes.clip(game, hint, Math.max(24, width - 24)), x + 12, y + height - 16, Colors.fade(Theme.TEXT_MUTED, ease), false);
    }

    private void drawFooter(
            GameBridge game,
            int x,
            int y,
            int width,
            int height,
            float ease,
            int accent,
            int mouseX,
            int mouseY,
            boolean click
    ) {
        int textY = y + (height - game.textHeight()) / 2;

        String saveText;
        int saveColor;
        if (settings.saveFailed()) {
            saveText = compact ? "Ошибка" : "Ошибка сохранения";
            saveColor = Theme.DANGER;
        } else if (settings.dirty()) {
            saveText = "Не сохранено";
            saveColor = Theme.WARN;
        } else {
            saveText = "Сохранено";
            float fresh = everSaved
                    ? Math.max(0f, 1f - (System.nanoTime() - savedAt) / 1_400_000_000f)
                    : 0f;
            saveColor = Colors.mix(Theme.TEXT_MUTED, Colors.withAlpha(accent, 0xFF), fresh);
        }

        // Кнопки справа занимают только место, которое осталось после статуса.
        String layoutText = compact ? "HUD" : "Расставить панели";
        int buttonWidth = game.textWidth(layoutText) + 16;
        int buttonHeight = 15;
        int right = x + width - PAD;
        int statusWidth = game.textWidth(saveText);
        int buttonY = y + (height - buttonHeight) / 2;
        int buttonX = right - statusWidth - 12 - buttonWidth;
        boolean showLayout = buttonX >= x + PAD;
        boolean overButton = showLayout && inside(mouseX, mouseY, buttonX, buttonY, buttonWidth, buttonHeight);

        // Второе действие скрывается первым, если компактному footer не хватает места.
        boolean anyOpen = anyExpanded();
        String foldText = compact ? "Все" : (anyOpen ? "Свернуть всё" : "Развернуть всё");
        int foldWidth = game.textWidth(foldText) + 16;
        int foldX = buttonX - 6 - foldWidth;
        boolean showFold = showLayout && foldX >= x + PAD;
        boolean overFold = showFold && inside(mouseX, mouseY, foldX, buttonY, foldWidth, buttonHeight);

        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        if (showLayout) {
            Draw.roundedRect(
                    game,
                    buttonX * s,
                    buttonY * s,
                    buttonWidth * s,
                    buttonHeight * s,
                    4 * s,
                    Colors.fade(overButton ? Colors.withAlpha(accent, 0x3A) : 0x12FFFFFF, ease)
            );
        }
        if (showFold) {
            Draw.roundedRect(
                    game,
                    foldX * s,
                    buttonY * s,
                    foldWidth * s,
                    buttonHeight * s,
                    4 * s,
                    Colors.fade(overFold ? Colors.withAlpha(accent, 0x3A) : 0x12FFFFFF, ease)
            );
        }
        game.popScale();
        if (showLayout) {
            Draw.textCentered(
                    game,
                    layoutText,
                    buttonX + buttonWidth / 2,
                    buttonY + 4,
                    Colors.fade(overButton ? Theme.TEXT : Theme.TEXT_DIM, ease),
                    false
            );
        }
        if (showFold) {
            Draw.textCentered(
                    game,
                    foldText,
                    foldX + foldWidth / 2,
                    buttonY + 4,
                    Colors.fade(overFold ? Theme.TEXT : Theme.TEXT_DIM, ease),
                    false
            );
        }
        if (click && overButton) {
            layoutRequest = true;
            beep(game, Sounds.TONE, 1.1f);
        }
        if (click && overFold) {
            toggleAll(!anyOpen);
            beep(game, Sounds.TICK, anyOpen ? 0.95f : 1.3f);
        }

        if (!compact && showFold) {
            game.drawText(
                    Shapes.clip(game, "Колесо листает · ESC закрывает", Math.max(24, foldX - x - PAD - 8)),
                    x + PAD,
                    textY,
                    Colors.fade(Theme.TEXT_MUTED, ease),
                    false
            );
        }
        Draw.textRight(game, saveText, right, textY, Colors.fade(saveColor, ease), false);
    }

    // ------------------------------------------------------------------ содержимое

    private void drawContent(
            GameBridge game,
            int x,
            int y,
            int width,
            int height,
            float ease,
            int accent,
            int surface,
            int mouseX,
            int mouseY,
            boolean down,
            boolean click,
            float delta
    ) {
        List<Module> list = settings.byCategory(CATEGORIES[category]);
        int cardX = x + PAD;
        int cardWidth = Math.max(1, width - PAD * 2 - 6);

        // Первый проход считает высоты: без него нечего ограничивать прокрутку.
        float total = PAD;
        for (int i = 0; i < list.size(); i++) {
            Module module = list.get(i);
            float openAmount = anim(module.key() + ".exp", expanded.contains(module.key()) ? 1f : 0f, 0.07f, delta);
            total += CARD_HEIGHT + optionsHeight(module) * Easing.inOutCubic(openAmount) + CARD_GAP;
        }
        total += PAD - CARD_GAP;

        float maxScroll = Math.max(0f, total - height);
        scrollTarget = Math.max(0f, Math.min(maxScroll, scrollTarget));
        scroll = Easing.approach(scroll, scrollTarget, 0.05f, delta);

        pushClip(game, x, y, width, height);
        int cursorY = Math.round(y + PAD - scroll);
        for (int i = 0; i < list.size(); i++) {
            Module module = list.get(i);
            float openAmount = Easing.inOutCubic(animGet(module.key() + ".exp"));
            int optionsPart = Math.round(optionsHeight(module) * openAmount);
            int cardHeight = CARD_HEIGHT + optionsPart;

            if (cursorY + cardHeight >= y - 4 && cursorY <= y + height + 4) {
                drawCard(game, module, cardX, cursorY, cardWidth, cardHeight, optionsPart, ease, accent, surface, mouseX, mouseY, down, click, delta);
            }
            cursorY += cardHeight + CARD_GAP;
        }
        popClip(game);

        drawScrollbar(game, x + width - 8, y + PAD, height - PAD * 2, height, total, ease, accent, mouseX, mouseY, down, click);
    }

    private int optionsHeight(Module module) {
        List<Option> options = module.options();
        if (options.isEmpty()) {
            return 0;
        }
        // Шесть точек воздуха сверху и строка сброса снизу.
        int height = 6 + RESET_ROW_HEIGHT;
        for (int i = 0; i < options.size(); i++) {
            height += rowHeight(options.get(i));
        }
        return height;
    }

    private int rowHeight(Option option) {
        if (option instanceof Option.Color) {
            return compact ? COMPACT_COLOR_ROW_HEIGHT : COLOR_ROW_HEIGHT;
        }
        return compact ? COMPACT_ROW_HEIGHT : ROW_HEIGHT;
    }

    private void drawCard(
            GameBridge game,
            Module module,
            int x,
            int y,
            int width,
            int height,
            int optionsPart,
            float ease,
            int accent,
            int surface,
            int mouseX,
            int mouseY,
            boolean down,
            boolean click,
            float delta
    ) {
        boolean overHeader = inside(mouseX, mouseY, x, y, width, CARD_HEIGHT);
        float hover = anim(module.key() + ".hover", overHeader ? 1f : 0f, 0.05f, delta);
        float on = anim(module.key() + ".on", module.on() ? 1f : 0f, 0.05f, delta);

        int background = Colors.mix(Theme.card(surface), Theme.cardHover(surface), hover);
        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        Draw.roundedRect(game, x * s, y * s, width * s, height * s, 7 * s, Colors.fade(background, ease));
        Draw.roundedOutline(
                game,
                x * s,
                y * s,
                width * s,
                height * s,
                7 * s,
                Math.max(1, s / 2),
                Colors.fade(Colors.mix(Theme.LINE_SOFT, Colors.withAlpha(accent, 0x66), on * 0.8f), ease)
        );
        if (module.toggleable()) {
            // Акцентная риска включённого модуля: состояние читается боковым зрением.
            Draw.roundedRect(game, x * s, (y + 10) * s, Math.max(1, s * 2), Math.round((CARD_HEIGHT - 20) * on) * s, s, Colors.fade(accent, ease * on));
        }
        game.popScale();

        int titleY = y + (compact ? 8 : 9);
        int textColor = module.on() ? Theme.TEXT : Theme.TEXT_DIM;
        game.drawText(Shapes.clip(game, module.title(), width - 90), x + 14, titleY, Colors.fade(textColor, ease), false);
        if (!compact) {
            game.drawText(
                    Shapes.clip(game, module.description(), Math.max(32, width - 90)),
                    x + 14,
                    titleY + 12,
                    Colors.fade(Theme.TEXT_MUTED, ease),
                    false
            );
        }

        int controlRight = x + width - 12;
        if (module.toggleable()) {
            int switchWidth = 30;
            int switchHeight = 15;
            int switchX = controlRight - switchWidth;
            int switchY = y + (CARD_HEIGHT - switchHeight) / 2;
            boolean overSwitch = inside(mouseX, mouseY, switchX - 3, switchY - 3, switchWidth + 6, switchHeight + 6);
            drawSwitch(game, switchX, switchY, switchWidth, switchHeight, on, ease, accent);
            if (click && overSwitch) {
                module.toggle();
                beep(game, Sounds.CLICK, module.on() ? 1.2f : 0.9f);
                // Раньше здесь стоял выход из метода, и в кадре с переключением
                // раскрытые настройки модуля просто не рисовались - список моргал.
                // Клик просто гасится, чтобы не уйти ещё и в раскрытие карточки.
                click = false;
            }
            controlRight = switchX - 10;
        }

        if (!module.options().isEmpty()) {
            float openAmount = animGet(module.key() + ".exp");
            drawChevron(game, controlRight - 8, y + CARD_HEIGHT / 2, openAmount, Colors.fade(Theme.TEXT_MUTED, ease));
            if (click && overHeader && mouseX < controlRight + 4) {
                if (expanded.contains(module.key())) {
                    expanded.remove(module.key());
                } else {
                    expanded.add(module.key());
                }
                beep(game, Sounds.TICK, 1.4f);
            }
        }

        if (optionsPart <= 2) {
            return;
        }

        pushClip(game, x, y + CARD_HEIGHT, width, optionsPart);
        int rowY = y + CARD_HEIGHT + 2;
        List<Option> options = module.options();
        for (int i = 0; i < options.size(); i++) {
            Option option = options.get(i);
            int rowHeight = rowHeight(option);
            drawOption(game, module, option, x + 10, rowY, width - 20, rowHeight, ease, accent, mouseX, mouseY, down, click, delta);
            rowY += rowHeight;
        }
        drawResetRow(game, module, x + 10, rowY, width - 20, ease, mouseX, mouseY, click, delta);
        popClip(game);
    }

    private void drawOption(
            GameBridge game,
            Module module,
            Option option,
            int x,
            int y,
            int width,
            int height,
            float ease,
            int accent,
            int mouseX,
            int mouseY,
            boolean down,
            boolean click,
            float delta
    ) {
        String id = module.key() + "." + option.key();
        int rowBase = compact ? COMPACT_ROW_HEIGHT : ROW_HEIGHT;
        int textY = y + (rowBase - game.textHeight()) / 2;
        // У цветной строки значение стоит в правом верхнем углу, и длинное название
        // раньше налезало прямо на него: подпись обрезается с оглядкой на значение.
        int labelWidth = option instanceof Option.Color ? width - 72 : width - 150;

        // Подсветка строки под курсором: без неё непонятно, к чему относится подсказка.
        boolean overRow = inside(mouseX, mouseY, x, y, width, height);
        float rowHover = anim(id + ".row", overRow ? 1f : 0f, 0.05f, delta);
        if (rowHover > 0.01f) {
            int rowScale = Pixels.scale(game);
            game.pushScale(1f / rowScale);
            Draw.roundedRect(
                    game,
                    x * rowScale,
                    y * rowScale,
                    width * rowScale,
                    height * rowScale,
                    4 * rowScale,
                    Colors.fade(0x0AFFFFFF, ease * rowHover)
            );
            game.popScale();
        }
        // Пояснения к настройкам были написаны в коде, но нигде не показывались.
        if (overRow && !option.hint().isEmpty()) {
            tooltipText = option.hint();
            tooltipX = mouseX;
            tooltipY = mouseY;
        }

        game.drawText(Shapes.clip(game, option.title(), Math.max(40, labelWidth)), x + 6, textY, Colors.fade(Theme.TEXT_DIM, ease), false);

        int right = x + width - 6;

        if (option instanceof Option.Bool) {
            Option.Bool value = (Option.Bool) option;
            int switchWidth = 24;
            int switchHeight = 12;
            int switchX = right - switchWidth;
            int switchY = y + (rowBase - switchHeight) / 2;
            float on = anim(id + ".on", value.get() ? 1f : 0f, 0.05f, delta);
            drawSwitch(game, switchX, switchY, switchWidth, switchHeight, on, ease, accent);
            if (click && inside(mouseX, mouseY, switchX - 4, y, switchWidth + 8, rowBase)) {
                value.toggle();
                beep(game, Sounds.BIT, value.get() ? 1.25f : 0.95f);
            }
            return;
        }

        if (option instanceof Option.Slider) {
            Option.Slider value = (Option.Slider) option;
            int valueWidth = 42;
            int trackWidth = Math.max(28, Math.min(96, width - 92));
            int trackX = right - valueWidth - trackWidth;
            int trackY = y + rowBase / 2;
            Draw.textRight(game, value.display(), right, textY, Colors.fade(Theme.TEXT, ease), false);

            int s = Pixels.scale(game);
            float fraction = value.fraction();
            game.pushScale(1f / s);
            Draw.roundedRect(game, trackX * s, (trackY - 1) * s, trackWidth * s, Math.max(1, 2 * s), s, Colors.fade(0x24FFFFFF, ease));
            Draw.roundedRect(game, trackX * s, (trackY - 1) * s, Math.round(trackWidth * fraction) * s, Math.max(1, 2 * s), s, Colors.fade(accent, ease));
            Ring.disc(game, (trackX + trackWidth * fraction) * s, trackY * s, 3.5f * s, Colors.fade(Theme.TEXT, ease));
            game.popScale();

            if (click && inside(mouseX, mouseY, trackX - 6, y, trackWidth + 12, rowBase)) {
                activeSlider = value;
            }
            if (activeSlider == value) {
                // Дорожка запоминается каждый кадр, а не только в момент нажатия:
                // список под курсором может уехать колесом прямо во время перетаскивания,
                // и тогда ползунок считал бы себя по старым координатам.
                activeSliderX = trackX;
                activeSliderWidth = trackWidth;
            }
            if (down && activeSlider == value) {
                value.setFraction((mouseX - activeSliderX) / (float) activeSliderWidth);
            }
            return;
        }

        if (option instanceof Option.Choice) {
            Option.Choice value = (Option.Choice) option;
            // Зона нажатия строится вокруг самого знака. Раньше правая зона вылезала
            // на четыре точки за край строки и при этом не совпадала с нарисованной
            // стрелкой: визуально попадаешь, а нажатие уходит мимо.
            int arrow = 10;
            int choiceWidth = Math.max(48, Math.min(96, width - 54));
            int leftArrowX = right - choiceWidth;
            int rightArrowX = right - arrow;
            boolean overLeft = inside(mouseX, mouseY, leftArrowX - 4, y, arrow + 8, rowBase);
            boolean overRight = inside(mouseX, mouseY, rightArrowX - 4, y, arrow + 4, rowBase);
            game.drawText("‹", leftArrowX, textY, Colors.fade(overLeft ? Theme.TEXT : Theme.TEXT_MUTED, ease), false);
            game.drawText("›", rightArrowX, textY, Colors.fade(overRight ? Theme.TEXT : Theme.TEXT_MUTED, ease), false);
            Draw.textCentered(
                    game,
                    Shapes.clip(game, value.display(), 74),
                    right - choiceWidth / 2,
                    textY,
                    Colors.fade(Theme.TEXT, ease),
                    false
            );
            if (click && overLeft) {
                value.previous();
                beep(game, Sounds.TICK, 1.1f);
            } else if (click && overRight) {
                value.next();
                beep(game, Sounds.TICK, 1.35f);
            }
            return;
        }

        if (option instanceof Option.Key) {
            Option.Key value = (Option.Key) option;
            boolean listening = capturing == value;
            String text = listening ? "нажми клавишу" : value.display();
            int chipWidth = Math.max(46, game.textWidth(text) + 14);
            int chipHeight = 15;
            int chipX = right - chipWidth;
            int chipY = y + (rowBase - chipHeight) / 2;
            boolean over = inside(mouseX, mouseY, chipX, chipY, chipWidth, chipHeight);
            int s = Pixels.scale(game);
            game.pushScale(1f / s);
            Draw.roundedRect(game, chipX * s, chipY * s, chipWidth * s, chipHeight * s, 4 * s, Colors.fade(listening ? Colors.withAlpha(accent, 0x3A) : 0x14FFFFFF, ease));
            Draw.roundedOutline(
                    game,
                    chipX * s,
                    chipY * s,
                    chipWidth * s,
                    chipHeight * s,
                    4 * s,
                    Math.max(1, s / 2),
                    Colors.fade(listening ? Colors.withAlpha(accent, 0xFF) : (over ? Theme.LINE : Theme.LINE_SOFT), ease)
            );
            game.popScale();
            Draw.textCentered(game, text, chipX + chipWidth / 2, chipY + 4, Colors.fade(listening ? Theme.TEXT : Theme.TEXT_DIM, ease), false);
            if (click && over) {
                capturing = listening ? null : value;
                captureArmed = false;
                editingText = null;
                beep(game, Sounds.BIT, 1.3f);
            }
            return;
        }

        if (option instanceof Option.Text) {
            drawTextOption(game, (Option.Text) option, right, y, rowBase, width, ease, accent, mouseX, mouseY, click);
            return;
        }

        if (option instanceof Option.Color) {
            Option.Color value = (Option.Color) option;
            int stripX = x + 6;
            int stripWidth = Math.max(18, width - 12 - 30);
            int hueY = y + 16;
            int satY = y + 30;
            int s = Pixels.scale(game);

            Draw.textRight(game, value.display(), x + width - 6, y + 2, Colors.fade(Theme.TEXT_DIM, ease), false);

            game.pushScale(1f / s);
            Shapes.hueStrip(game, stripX * s, hueY * s, stripWidth * s, 8 * s, value.saturation(), ease);
            Shapes.horizontalGradient(
                    game,
                    stripX * s,
                    satY * s,
                    stripWidth * s,
                    8 * s,
                    Colors.fade(Colors.hsv(value.hue(), 0f, 1f, 255), ease),
                    Colors.fade(Colors.hsv(value.hue(), 1f, 1f, 255), ease)
            );
            float huePosition = stripX + stripWidth * (value.hue() / 360f);
            float satPosition = stripX + stripWidth * value.saturation();
            Draw.roundedRect(game, Math.round(huePosition - 1) * s, (hueY - 2) * s, Math.max(1, 2 * s), 12 * s, s, Colors.fade(Theme.TEXT, ease));
            Draw.roundedRect(game, Math.round(satPosition - 1) * s, (satY - 2) * s, Math.max(1, 2 * s), 12 * s, s, Colors.fade(Theme.TEXT, ease));
            // Образец текущего цвета справа от полос.
            Draw.roundedRect(game, (stripX + stripWidth + 8) * s, hueY * s, 20 * s, 22 * s, 5 * s, Colors.fade(value.argb(), ease));
            game.popScale();

            if (click && inside(mouseX, mouseY, stripX, hueY - 3, stripWidth, 14)) {
                activeHue = value;
            } else if (click && inside(mouseX, mouseY, stripX, satY - 3, stripWidth, 14)) {
                activeSaturation = value;
            }
            if (activeHue == value || activeSaturation == value) {
                // Та же причина, что и у ползунка: полосы могут уехать под курсором.
                activeColorX = stripX;
                activeColorWidth = stripWidth;
            }
            if (down && activeHue == value) {
                value.setHue((mouseX - activeColorX) / (float) activeColorWidth * 360f);
            }
            if (down && activeSaturation == value) {
                value.setSaturation((mouseX - activeColorX) / (float) activeColorWidth);
            }
        }
    }

    /**
     * Строка текстовой настройки: превью слева от кнопки, ввод - по кнопке.
     *
     * Набирается только латиница, цифры и знаки препинания: их достаточно для
     * команд и сообщений любого сервера, а от шрифта игры кириллица в числах
     * и позициях всё равно смотрелась бы чужеродно.
     */
    private void drawTextOption(
            GameBridge game,
            Option.Text value,
            int right,
            int y,
            int rowBase,
            int width,
            float ease,
            int accent,
            int mouseX,
            int mouseY,
            boolean click
    ) {
        boolean editing = editingText == value;
        int chipHeight = 15;
        int chipY = y + (rowBase - chipHeight) / 2;
        String chipText = editing ? "готово" : "изменить";
        int chipWidth = Math.max(40, game.textWidth(chipText) + 12);
        int chipX = right - chipWidth;
        boolean over = inside(mouseX, mouseY, chipX, chipY, chipWidth, chipHeight);

        // Превью текущего текста: обрезается под место, которое осталось от кнопки.
        String shown = editing ? value.get() + "_" : value.display();
        int previewWidth = Math.max(20, chipX - 10 - (right - width) - 10);
        game.drawText(
                Shapes.clip(game, shown, previewWidth),
                right - width + 10 + Math.max(0, previewWidth - game.textWidth(shown)) / 2,
                y + (rowBase - game.textHeight()) / 2,
                Colors.fade(editing ? Theme.TEXT : Theme.TEXT_DIM, ease),
                false
        );

        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        Draw.roundedRect(game, chipX * s, chipY * s, chipWidth * s, chipHeight * s, 4 * s,
                Colors.fade(editing ? Colors.withAlpha(accent, 0x3A) : 0x14FFFFFF, ease));
        Draw.roundedOutline(game, chipX * s, chipY * s, chipWidth * s, chipHeight * s, 4 * s, Math.max(1, s / 2),
                Colors.fade(editing ? Colors.withAlpha(accent, 0xFF) : (over ? Theme.LINE : Theme.LINE_SOFT), ease));
        game.popScale();
        Draw.textCentered(game, chipText, chipX + chipWidth / 2, chipY + 4,
                Colors.fade(editing ? Theme.TEXT : Theme.TEXT_DIM, ease), false);

        if (over && !editing) {
            tooltipText = "Латиница, цифры и знаки. Команды начинаются с косой черты";
            tooltipX = mouseX;
            tooltipY = mouseY;
        }
        if (click && over) {
            if (editing) {
                editingText = null;
                if (settings.saveIfDirty()) {
                    markSaved(System.nanoTime());
                }
            } else {
                capturing = null;
                captureArmed = false;
                editingText = value;
            }
            beep(game, Sounds.BIT, 1.25f);
        }
    }

    /**
     * Клавиша во время ввода текста.
     *
     * Приходит из хука клавиатуры и всегда съедается: пока поле открыто, игре
     * не должно доставаться ничего - даже те клавиши, что мы не понимаем.
     * Возвращает true всегда, кроме случая, когда поле закрыто.
     */
    public boolean textKey(int key, int action) {
        if (editingText == null) {
            return false;
        }
        if (action != 1 && action != 2) {
            return true;
        }
        if (key == Keys.ESCAPE) {
            editingText = null;
            return true;
        }
        if (key == 257) { // Enter: принять и выйти
            editingText = null;
            settings.saveIfDirty();
            return true;
        }
        if (key == 259) { // Backspace: стереть последнюю букву, повторы разрешены
            String current = editingText.get();
            if (!current.isEmpty()) {
                editingText.set(current.substring(0, current.length() - 1));
            }
            return true;
        }
        // Печатные знаки ASCII: латиница, цифры, знаки препинания, косая черта.
        if (key >= 32 && key <= 126) {
            char letter = (char) key;
            if (letter >= 'A' && letter <= 'Z') {
                letter = (char) (letter + 32);
            }
            editingText.set(editingText.get() + letter);
        }
        return true;
    }

    private void drawScrollbar(
            GameBridge game,
            int x,
            int y,
            int height,
            int viewHeight,
            float total,
            float ease,
            int accent,
            int mouseX,
            int mouseY,
            boolean down,
            boolean click
    ) {
        if (total <= viewHeight) {
            return;
        }
        int s = Pixels.scale(game);
        int knobHeight = Math.max(18, Math.round(height * viewHeight / total));
        float maxScroll = Math.max(1f, total - viewHeight);
        int knobY = y + Math.round((height - knobHeight) * (scroll / maxScroll));

        game.pushScale(1f / s);
        Draw.roundedRect(game, x * s, y * s, Math.max(1, 3 * s), height * s, s, Colors.fade(0x14FFFFFF, ease));
        Draw.roundedRect(game, x * s, knobY * s, Math.max(1, 3 * s), knobHeight * s, s, Colors.fade(Colors.withAlpha(accent, 0xCC), ease));
        game.popScale();

        if (click && inside(mouseX, mouseY, x - 4, y, 12, height)) {
            scrollDragging = true;
            scrollGrabY = mouseY;
            scrollGrabStart = scrollTarget;
        }
        if (scrollDragging && down) {
            float perPixel = maxScroll / Math.max(1, height - knobHeight);
            scrollTarget = scrollGrabStart + (mouseY - scrollGrabY) * perPixel;
        }
    }

    // ------------------------------------------------------------------ мелкие части

    private void drawSwitch(GameBridge game, int x, int y, int width, int height, float on, float ease, int accent) {
        int s = Pixels.scale(game);
        int track = Colors.mix(0x1FFFFFFF, Colors.withAlpha(accent, 0xFF), on);
        game.pushScale(1f / s);
        Draw.roundedRect(game, x * s, y * s, width * s, height * s, (height / 2) * s, Colors.fade(track, ease));
        if (on < 0.98f) {
            Draw.roundedOutline(game, x * s, y * s, width * s, height * s, (height / 2) * s, Math.max(1, s / 2), Colors.fade(Theme.LINE, ease * (1f - on)));
        }
        float knobRadius = height / 2f - 2f;
        float knobX = x + 2f + knobRadius + (width - 4f - knobRadius * 2f) * on;
        Ring.disc(game, knobX * s, (y + height / 2f) * s, knobRadius * s, Colors.fade(Colors.mix(Theme.TEXT_DIM, 0xFFFFFFFF, on), ease));
        game.popScale();
    }

    /**
     * Уголок раскрытия: две короткие полосы, поворачивающиеся через высоту ступенек.
     *
     * Свёрнутая карточка показывает знак вниз ("раскрой меня"), раскрытая - вверх
     * ("сверни меня"). Раньше знак стоял ровно наоборот и врал про состояние карточки.
     * Середина считается дробной, иначе знак получается несимметричным.
     */
    private void drawChevron(GameBridge game, int centerX, int centerY, float open, int color) {
        int steps = 3;
        float middle = (steps - 1) / 2f;
        for (int i = 0; i < steps; i++) {
            int dy = Math.round((i - middle) * (1f - open * 2f));
            game.fill(centerX - steps + i, centerY + dy, 1, 1, color);
            game.fill(centerX + steps - 1 - i, centerY + dy, 1, 1, color);
        }
    }

    /**
     * Ожидание новой клавиши для привязки.
     *
     * Захват сначала взводится: пока хотя бы одна привязываемая клавиша зажата,
     * назначать нечего. Иначе привязка мгновенно съедала клавишу, которую игрок
     * держал в момент входа в режим - а держал он ровно ту, которой открыл меню.
     */
    private void captureKey(GameBridge game) {
        if (game.keyDown(Keys.ESCAPE)) {
            capturing = null;
            captureArmed = false;
            return;
        }

        int pressed = -1;
        for (int i = 0; i < Keys.BINDABLE.length; i++) {
            int code = Keys.BINDABLE[i];
            if (game.keyDown(code)) {
                pressed = code;
                break;
            }
        }

        if (!captureArmed) {
            captureArmed = pressed < 0;
            return;
        }
        if (pressed < 0) {
            return;
        }

        capturing.set(pressed);
        capturing = null;
        captureArmed = false;
        beep(game, Sounds.CLICK, 1.25f);
        if (settings.saveIfDirty()) {
            markSaved(System.nanoTime());
        }
    }

    /**
     * Строка сброса настроек модуля.
     *
     * Настройки легко довести до состояния, из которого руками уже не выбраться.
     * Кнопка возвращает модулю ровно те значения, с которыми он объявлен в коде,
     * включая сам переключатель.
     */
    private void drawResetRow(
            GameBridge game,
            Module module,
            int x,
            int y,
            int width,
            float ease,
            int mouseX,
            int mouseY,
            boolean click,
            float delta
    ) {
        String text = "Сбросить настройки";
        int buttonWidth = game.textWidth(text) + 18;
        int buttonHeight = 15;
        int buttonX = x + width - 6 - buttonWidth;
        int buttonY = y + (RESET_ROW_HEIGHT - buttonHeight) / 2;
        boolean over = inside(mouseX, mouseY, buttonX, buttonY, buttonWidth, buttonHeight);
        float hover = anim(module.key() + ".reset", over ? 1f : 0f, 0.05f, delta);

        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        game.fill(x * s, y * s, width * s, Math.max(1, s / 2), Colors.fade(Theme.LINE_SOFT, ease));
        Draw.roundedRect(
                game,
                buttonX * s,
                buttonY * s,
                buttonWidth * s,
                buttonHeight * s,
                4 * s,
                Colors.fade(Colors.mix(0x10FFFFFF, 0x30FF5470, hover), ease)
        );
        game.popScale();
        Draw.textCentered(
                game,
                text,
                buttonX + buttonWidth / 2,
                buttonY + 4,
                Colors.fade(Colors.mix(Theme.TEXT_MUTED, Theme.DANGER, hover), ease),
                false
        );

        if (over) {
            tooltipText = "Вернуть модулю значения по умолчанию";
            tooltipX = mouseX;
            tooltipY = mouseY;
        }
        if (click && over) {
            module.resetAll();
            if (settings.saveIfDirty()) {
                markSaved(System.nanoTime());
            }
            beep(game, Sounds.TONE, 0.8f);
        }
    }

    /** Есть ли в текущем разделе хоть одна раскрытая карточка. */
    private boolean anyExpanded() {
        List<Module> list = settings.byCategory(CATEGORIES[category]);
        for (int i = 0; i < list.size(); i++) {
            if (expanded.contains(list.get(i).key())) {
                return true;
            }
        }
        return false;
    }

    /** Раскрыть или свернуть все карточки текущего раздела. */
    private void toggleAll(boolean expand) {
        List<Module> list = settings.byCategory(CATEGORIES[category]);
        for (int i = 0; i < list.size(); i++) {
            Module module = list.get(i);
            // У модуля без настроек раскрывать нечего, и уголка у него тоже нет.
            if (module.options().isEmpty()) {
                continue;
            }
            if (expand) {
                expanded.add(module.key());
            } else {
                expanded.remove(module.key());
            }
        }
    }

    /**
     * Подсказка под курсором.
     *
     * Рисуется в самом конце кадра и вне всякого обрезания, иначе её срежет край
     * списка. Плашка сама отходит от края экрана, чтобы текст не обрывался.
     */
    private void drawTooltip(GameBridge game, int screenWidth, int screenHeight, float ease) {
        if (tooltipText == null || tooltipText.isEmpty() || ease < 0.6f) {
            return;
        }
        int padding = 6;
        int boxWidth = game.textWidth(tooltipText) + padding * 2;
        int boxHeight = game.textHeight() + padding * 2 - 1;
        int boxX = tooltipX + 10;
        int boxY = tooltipY + 12;
        if (boxX + boxWidth > screenWidth - 4) {
            boxX = Math.max(4, tooltipX - 10 - boxWidth);
        }
        if (boxY + boxHeight > screenHeight - 4) {
            boxY = Math.max(4, tooltipY - 6 - boxHeight);
        }

        int s = Pixels.scale(game);
        game.pushScale(1f / s);
        Shapes.shadow(game, boxX * s, boxY * s, boxWidth * s, boxHeight * s, 4 * s, Math.max(2, 2 * s), Colors.fade(0xFF000000, ease * 0.5f));
        Draw.roundedRect(game, boxX * s, boxY * s, boxWidth * s, boxHeight * s, 4 * s, Colors.fade(0xF2121722, ease));
        Draw.roundedOutline(game, boxX * s, boxY * s, boxWidth * s, boxHeight * s, 4 * s, Math.max(1, s / 2), Colors.fade(Theme.LINE, ease));
        game.popScale();
        game.drawText(tooltipText, boxX + padding, boxY + padding, Colors.fade(Theme.TEXT_DIM, ease), false);
    }

    private void beep(GameBridge game, int sound, float pitch) {
        if (settings.sound()) {
            game.playSound(sound, pitch);
        }
    }

    private float anim(String key, float target, float halfLife, float delta) {
        Float current = anim.get(key);
        float value = current == null ? target : Easing.approach(current.floatValue(), target, halfLife, delta);
        anim.put(key, value);
        return value;
    }

    private float animGet(String key) {
        Float current = anim.get(key);
        return current == null ? 0f : current.floatValue();
    }

    private static boolean inside(int px, int py, int x, int y, int width, int height) {
        return px >= x && px < x + width && py >= y && py < y + height;
    }

    private static double clamp(double value, double min, double max) {
        if (value < min) {
            return min;
        }
        return value > max ? max : value;
    }
}
