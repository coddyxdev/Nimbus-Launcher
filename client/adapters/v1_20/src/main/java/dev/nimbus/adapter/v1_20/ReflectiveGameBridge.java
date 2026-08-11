package dev.nimbus.adapter.v1_20;

import dev.nimbus.bridge.GameBridge;
import dev.nimbus.bridge.Mappings;

import java.lang.reflect.Field;
import java.lang.reflect.Method;

/**
 * Адаптер для ветки 1.20.x и выше.
 *
 * Все обращения к игре идут через отражение: у клиента нет ни одной ссылки на
 * классы Minecraft на этапе компиляции, поэтому один и тот же jar работает на
 * любой поддерживаемой версии. Настоящие имена берутся из таблицы маппингов.
 *
 * Правило безопасности: ни один метод не выпускает исключение наружу. Если
 * что-то не нашлось или упало, мост выключается целиком и игра продолжает идти
 * без нашего интерфейса.
 */
public final class ReflectiveGameBridge implements GameBridge {

    private static final String MINECRAFT = "net.minecraft.client.Minecraft";
    private static final String WINDOW = "com.mojang.blaze3d.platform.Window";
    private static final String MOUSE_HANDLER = "net.minecraft.client.MouseHandler";
    private static final String FONT = "net.minecraft.client.gui.Font";
    private static final String GUI_GRAPHICS = "net.minecraft.client.gui.GuiGraphics";

    /** Запасная высота строки ванильного шрифта, если поле не нашлось. */
    private static final int DEFAULT_LINE_HEIGHT = 9;

    private final String gameVersion;
    private final Mappings mappings;

    private boolean resolved;
    private boolean broken;
    private String failure;

    private Method minecraftInstance;
    private Method getWindow;
    private Method guiScaledWidth;
    private Method guiScaledHeight;
    private Method getGuiScale;
    private Field fontField;
    private Field mouseHandlerField;
    private Method grabMouse;
    private Method releaseMouse;
    private Method isMouseGrabbed;
    private Method fill;
    private Method fillGradient;
    private Method drawString;
    private Method enableScissor;
    private Method disableScissor;
    private Method fontWidth;
    private Field lineHeightField;

    private Object graphics;
    private float partialTick;

    public ReflectiveGameBridge(String gameVersion, Mappings mappings) {
        this.gameVersion = gameVersion;
        this.mappings = mappings == null ? Mappings.IDENTITY : mappings;
    }

    @Override
    public String gameVersion() {
        return gameVersion;
    }

    @Override
    public boolean ready() {
        return resolved && !broken;
    }

    /** Причина отказа, если мост выключился. Нужна только для лога. */
    public String failure() {
        return failure;
    }

    @Override
    public void beginFrame(Object graphics, float partialTick) {
        if (broken || graphics == null) {
            return;
        }
        this.graphics = graphics;
        this.partialTick = partialTick;
        if (!resolved) {
            resolve(graphics.getClass());
        }
    }

    /**
     * Найти все имена заранее, не дожидаясь кадра.
     *
     * Нужно офлайн-проверке: она берёт настоящий jar игры и убеждается, что
     * каждое имя из таблицы маппингов действительно есть в этой версии игры.
     */
    public void prepare(Class<?> guiGraphicsClass) {
        if (!resolved && !broken && guiGraphicsClass != null) {
            resolve(guiGraphicsClass);
        }
    }

    @Override
    public void endFrame() {
        graphics = null;
    }

    @Override
    public float partialTick() {
        return partialTick;
    }

    @Override
    public int screenWidth() {
        Object window = window();
        if (window == null) {
            return 0;
        }
        try {
            return (int) guiScaledWidth.invoke(window);
        } catch (Throwable error) {
            disable("не удалось получить ширину экрана", error);
            return 0;
        }
    }

    @Override
    public int screenHeight() {
        Object window = window();
        if (window == null) {
            return 0;
        }
        try {
            return (int) guiScaledHeight.invoke(window);
        } catch (Throwable error) {
            disable("не удалось получить высоту экрана", error);
            return 0;
        }
    }

    @Override
    public double guiScale() {
        Object window = window();
        if (window == null) {
            return 1.0;
        }
        try {
            return (double) getGuiScale.invoke(window);
        } catch (Throwable error) {
            disable("не удалось получить масштаб интерфейса", error);
            return 1.0;
        }
    }

    @Override
    public boolean cursorGrabbed() {
        Object mouse = mouseHandler();
        if (mouse == null) {
            return true;
        }
        try {
            return (boolean) isMouseGrabbed.invoke(mouse);
        } catch (Throwable error) {
            disable("не удалось узнать состояние курсора", error);
            return true;
        }
    }

    @Override
    public void setCursorGrabbed(boolean grabbed) {
        Object mouse = mouseHandler();
        if (mouse == null) {
            return;
        }
        try {
            if (grabbed) {
                grabMouse.invoke(mouse);
            } else {
                releaseMouse.invoke(mouse);
            }
        } catch (Throwable error) {
            disable("не удалось переключить курсор", error);
        }
    }

    @Override
    public void fill(int x, int y, int width, int height, int argb) {
        if (!drawable() || width <= 0 || height <= 0) {
            return;
        }
        try {
            fill.invoke(graphics, x, y, x + width, y + height, argb);
        } catch (Throwable error) {
            disable("не удалось залить прямоугольник", error);
        }
    }

    @Override
    public void fillGradient(int x, int y, int width, int height, int topArgb, int bottomArgb) {
        if (!drawable() || width <= 0 || height <= 0) {
            return;
        }
        try {
            fillGradient.invoke(graphics, x, y, x + width, y + height, topArgb, bottomArgb);
        } catch (Throwable error) {
            disable("не удалось нарисовать градиент", error);
        }
    }

    @Override
    public int drawText(String text, int x, int y, int argb, boolean shadow) {
        if (!drawable() || text == null || text.isEmpty()) {
            return x;
        }
        Object font = font();
        if (font == null) {
            return x;
        }
        try {
            return (int) drawString.invoke(graphics, font, text, x, y, argb, shadow);
        } catch (Throwable error) {
            disable("не удалось нарисовать текст", error);
            return x;
        }
    }

    @Override
    public int textWidth(String text) {
        if (broken || !resolved || text == null || text.isEmpty()) {
            return 0;
        }
        Object font = font();
        if (font == null) {
            return 0;
        }
        try {
            return (int) fontWidth.invoke(font, text);
        } catch (Throwable error) {
            disable("не удалось измерить текст", error);
            return 0;
        }
    }

    @Override
    public int textHeight() {
        if (lineHeightField == null) {
            return DEFAULT_LINE_HEIGHT;
        }
        Object font = font();
        if (font == null) {
            return DEFAULT_LINE_HEIGHT;
        }
        try {
            int value = lineHeightField.getInt(font);
            return value > 0 ? value : DEFAULT_LINE_HEIGHT;
        } catch (Throwable ignored) {
            // Поле есть, но прочитать не вышло - больше не пробуем.
            lineHeightField = null;
            return DEFAULT_LINE_HEIGHT;
        }
    }

    @Override
    public void scissorOn(int x, int y, int width, int height) {
        if (!drawable() || width <= 0 || height <= 0) {
            return;
        }
        try {
            enableScissor.invoke(graphics, x, y, x + width, y + height);
        } catch (Throwable error) {
            disable("не удалось ограничить область рисования", error);
        }
    }

    @Override
    public void scissorOff() {
        if (!drawable()) {
            return;
        }
        try {
            disableScissor.invoke(graphics);
        } catch (Throwable error) {
            disable("не удалось снять ограничение рисования", error);
        }
    }

    @Override
    public void printMessage(String text) {
        System.out.println("[Nimbus] " + text);
    }

    private boolean drawable() {
        return resolved && !broken && graphics != null;
    }

    private Object minecraft() {
        if (broken || !resolved) {
            return null;
        }
        try {
            return minecraftInstance.invoke(null);
        } catch (Throwable error) {
            disable("не удалось получить объект игры", error);
            return null;
        }
    }

    private Object window() {
        Object minecraft = minecraft();
        if (minecraft == null) {
            return null;
        }
        try {
            return getWindow.invoke(minecraft);
        } catch (Throwable error) {
            disable("не удалось получить окно игры", error);
            return null;
        }
    }

    private Object mouseHandler() {
        Object minecraft = minecraft();
        if (minecraft == null) {
            return null;
        }
        try {
            return mouseHandlerField.get(minecraft);
        } catch (Throwable error) {
            disable("не удалось получить обработчик мыши", error);
            return null;
        }
    }

    private Object font() {
        Object minecraft = minecraft();
        if (minecraft == null) {
            return null;
        }
        try {
            return fontField.get(minecraft);
        } catch (Throwable error) {
            disable("не удалось получить шрифт игры", error);
            return null;
        }
    }

    /**
     * Одноразовый поиск всех нужных имён.
     *
     * Классы ищутся тем же загрузчиком, что загрузил контекст рисования: у игры
     * свой загрузчик, и системный про её классы ничего не знает.
     */
    private void resolve(Class<?> guiGraphicsClass) {
        try {
            ClassLoader loader = guiGraphicsClass.getClassLoader();

            Class<?> minecraftClass = Class.forName(className(MINECRAFT), false, loader);
            Class<?> windowClass = Class.forName(className(WINDOW), false, loader);

            minecraftInstance = method(minecraftClass, MINECRAFT, "getInstance");
            getWindow = method(minecraftClass, MINECRAFT, "getWindow");
            guiScaledWidth = method(windowClass, WINDOW, "getGuiScaledWidth");
            guiScaledHeight = method(windowClass, WINDOW, "getGuiScaledHeight");
            getGuiScale = method(windowClass, WINDOW, "getGuiScale");

            fontField = field(minecraftClass, MINECRAFT, "font");
            mouseHandlerField = field(minecraftClass, MINECRAFT, "mouseHandler");

            Class<?> fontClass = fontField.getType();
            Class<?> mouseClass = mouseHandlerField.getType();

            grabMouse = method(mouseClass, MOUSE_HANDLER, "grabMouse");
            releaseMouse = method(mouseClass, MOUSE_HANDLER, "releaseMouse");
            isMouseGrabbed = method(mouseClass, MOUSE_HANDLER, "isMouseGrabbed");

            fontWidth = method(fontClass, FONT, "width", new Class<?>[]{String.class}, "java.lang.String");
            lineHeightField = optionalField(fontClass, FONT, "lineHeight");

            fill = method(
                    guiGraphicsClass,
                    GUI_GRAPHICS,
                    "fill",
                    new Class<?>[]{int.class, int.class, int.class, int.class, int.class},
                    "int", "int", "int", "int", "int"
            );
            fillGradient = method(
                    guiGraphicsClass,
                    GUI_GRAPHICS,
                    "fillGradient",
                    new Class<?>[]{int.class, int.class, int.class, int.class, int.class, int.class},
                    "int", "int", "int", "int", "int", "int"
            );
            drawString = method(
                    guiGraphicsClass,
                    GUI_GRAPHICS,
                    "drawString",
                    new Class<?>[]{fontClass, String.class, int.class, int.class, int.class, boolean.class},
                    FONT, "java.lang.String", "int", "int", "int", "boolean"
            );
            enableScissor = method(
                    guiGraphicsClass,
                    GUI_GRAPHICS,
                    "enableScissor",
                    new Class<?>[]{int.class, int.class, int.class, int.class},
                    "int", "int", "int", "int"
            );
            disableScissor = method(guiGraphicsClass, GUI_GRAPHICS, "disableScissor");

            resolved = true;
        } catch (Throwable error) {
            disable("не удалось связать имена игры", error);
        }
    }

    /** Необязательное поле: его отсутствие не повод выключать весь мост. */
    private Field optionalField(Class<?> owner, String deobfClass, String name) {
        try {
            return field(owner, deobfClass, name);
        } catch (Throwable ignored) {
            // Высота строки ванильного шрифта не менялась с самых старых версий,
            // так что запасное значение честнее, чем отказ от текста целиком.
            return null;
        }
    }

    private String className(String deobfName) {
        String name = mappings.className(deobfName);
        return name == null ? deobfName : name;
    }

    private Method method(Class<?> owner, String deobfClass, String name) throws NoSuchMethodException {
        return method(owner, deobfClass, name, new Class<?>[0]);
    }

    private Method method(
            Class<?> owner,
            String deobfClass,
            String name,
            Class<?>[] parameterTypes,
            String... argTypes
    ) throws NoSuchMethodException {
        String real = mappings.methodName(deobfClass, name, argTypes);
        Method method = owner.getDeclaredMethod(real == null ? name : real, parameterTypes);
        method.setAccessible(true);
        return method;
    }

    private Field field(Class<?> owner, String deobfClass, String name) throws NoSuchFieldException {
        String real = mappings.fieldName(deobfClass, name);
        Field field = owner.getDeclaredField(real == null ? name : real);
        field.setAccessible(true);
        return field;
    }

    private void disable(String what, Throwable error) {
        broken = true;
        graphics = null;
        failure = what + ": " + error;
    }
}
