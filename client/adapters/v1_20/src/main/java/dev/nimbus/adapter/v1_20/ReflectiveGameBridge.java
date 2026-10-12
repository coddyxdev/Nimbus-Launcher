package dev.nimbus.adapter.v1_20;

import dev.nimbus.bridge.ArmorPiece;
import dev.nimbus.bridge.EffectInfo;
import dev.nimbus.bridge.GameBridge;
import dev.nimbus.bridge.Mappings;
import dev.nimbus.bridge.Sounds;
import dev.nimbus.bridge.TargetInfo;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;

/**
 * Адаптер для ветки 1.20.x и выше.
 *
 * Все обращения к игре идут через отражение: у клиента нет ни одной ссылки на
 * классы Minecraft на этапе компиляции, поэтому один и тот же jar работает на
 * любой поддерживаемой версии. Настоящие имена берутся из таблицы маппингов.
 *
 * Исключение - GLFW: это библиотека окон, она не обфусцируется и её имена одинаковы
 * во всех версиях игры.
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
    private static final String POSE_STACK = "com.mojang.blaze3d.vertex.PoseStack";
    private static final String GLFW = "org.lwjgl.glfw.GLFW";
    private static final String ENTITY = "net.minecraft.world.entity.Entity";
    private static final String CLIENT_PACKET_LISTENER = "net.minecraft.client.multiplayer.ClientPacketListener";
    private static final String PLAYER_INFO = "net.minecraft.client.multiplayer.PlayerInfo";
    private static final String SOUND_MANAGER = "net.minecraft.client.sounds.SoundManager";
    private static final String SOUND_INSTANCE = "net.minecraft.client.resources.sounds.SoundInstance";
    private static final String SIMPLE_SOUND = "net.minecraft.client.resources.sounds.SimpleSoundInstance";
    private static final String SOUND_EVENT = "net.minecraft.sounds.SoundEvent";
    private static final String SOUND_EVENTS = "net.minecraft.sounds.SoundEvents";
    private static final String HOLDER = "net.minecraft.core.Holder";
    private static final String PLAYER = "net.minecraft.world.entity.player.Player";
    private static final String LIVING_ENTITY = "net.minecraft.world.entity.LivingEntity";
    private static final String ENTITY_CLASS = "net.minecraft.world.entity.Entity";
    private static final String MOB_EFFECT_INSTANCE = "net.minecraft.world.effect.MobEffectInstance";
    private static final String MOB_EFFECT = "net.minecraft.world.effect.MobEffect";
    private static final String INVENTORY = "net.minecraft.world.entity.player.Inventory";
    private static final String ITEM_STACK = "net.minecraft.world.item.ItemStack";
    private static final String ITEM = "net.minecraft.world.item.Item";
    private static final String SERVER_DATA = "net.minecraft.client.multiplayer.ServerData";
    private static final String OPTIONS = "net.minecraft.client.Options";
    private static final String OPTION_INSTANCE = "net.minecraft.client.OptionInstance";
    private static final String COMPONENT = "net.minecraft.network.chat.Component";
    private static final String I18N = "net.minecraft.client.resources.language.I18n";


    /** Запасная высота строки ванильного шрифта, если поле не нашлось. */
    private static final int DEFAULT_LINE_HEIGHT = 9;

    /** Состояние "клавиша нажата" в GLFW. */
    private static final int GLFW_PRESS = 1;

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
    private Method windowHandle;
    private Field fontField;
    private Field mouseHandlerField;
    private Method grabMouse;
    private Method releaseMouse;
    private Method isMouseGrabbed;
    private Method mouseXpos;
    private Method mouseYpos;
    private Method fill;
    private Method fillGradient;
    private Method drawString;
    private Method enableScissor;
    private Method disableScissor;
    private Method fontWidth;
    private Field lineHeightField;
    private Method pose;
    private Method pushPose;
    private Method popPose;
    private Method poseScale;
    private Method glfwGetKey;
    private Method glfwGetMouseButton;

    // \u0422\u0435\u043b\u0435\u043c\u0435\u0442\u0440\u0438\u044f \u0438\u0433\u0440\u043e\u043a\u0430: \u043d\u0435\u043e\u0431\u044f\u0437\u0430\u0442\u0435\u043b\u044c\u043d\u0430\u044f \u0447\u0430\u0441\u0442\u044c \u043c\u043e\u0441\u0442\u0430.
    private Field playerField;
    private Method entityX;
    private Method entityY;
    private Method entityZ;
    private Method entityYaw;
    private Method entityUuid;
    private Method getConnection;
    private Method getPlayerInfo;
    private Method getLatency;
    private boolean telemetryReady;

    private Method getSoundManager;
    private Method playSoundMethod;
    private Method forUiEvent;
    private Method forUiHolder;
    private Class<?> holderClass;
    private final Field[] soundFields = new Field[Sounds.COUNT];
    private boolean soundReady;

    /**
     * Почему группа звука не собралась.
     *
     * Без этой строки офлайн-проверка сообщала лишь "MISSING", и найти, какое из
     * шести имён исчезло в новой версии, можно было только перебором.
     */
    private String soundFailure;

    // Расширенная часть моста: статус игрока, бой, эффекты, броня.
    // Отдельная группа по образцу телеметрии: не нашлось - модули сами скроются.
    private boolean statusReady;

    /** Почему группа статуса не собралась. Нужно офлайн-проверке. */
    private String statusFailure;
    private Method getGameProfile;
    private Method profileName;
    private Method isAlive;
    private Method getHealth;
    private Method getMaxHealth;
    private Method getAbsorption;
    private Method getArmorValue;
    private Method getAttackStrength;
    private Method getInventory;
    private Field inventoryArmor;
    private Field optionsField;
    private Method optionSet;
    private Field smoothCameraOption;
    private Method currentServer;
    private Field serverIp;
    private Method entityName;
    private Method componentText;
    private Method getEffects;
    private Method effectOf;
    private Method effectAmplifier;
    private Method effectDuration;
    private Method effectDescription;
    private Method effectCategory;
    private Method itemOf;
    private Method itemDescription;
    private Method stackDamage;
    private Method stackMaxDamage;
    private Method stackEmpty;
    private Method i18nGet;
    private Class<?> livingEntityClass;

    /** Снятые за последний кадр эффекты и броня: один снимок на кадр, без пересборок. */
    private List<EffectInfo> effectSnapshot = List.of();
    private List<ArmorPiece> armorSnapshot = List.of();
    private boolean snapshotsTaken;


    private Object graphics;
    private float partialTick;
    private int pushedScales;

    /**
     * Глубина стопки областей обрезания.
     *
     * У игры внутри уже есть такая стопка: включение области кладёт на неё
     * пересечение с текущей, а снятие снимает ровно один слой. Значит на каждое
     * включение обязано приходиться ровно одно снятие. Лишнее включение копится
     * от кадра к кадру, пересечение сжимается в точку и экран гаснет; лишнее
     * снятие опустошает стопку игры и роняет кадр. Счётчик держит равновесие.
     */
    private int scissorDepth;

    /** Масштаб интерфейса, посчитанный один раз за кадр. */
    private double cachedGuiScale;
    private boolean guiScaleCached;

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

    /** Нашлись ли имена телеметрии игрока. Нужно офлайн-проверке. */
    public boolean telemetryReady() {
        return telemetryReady;
    }

    @Override
    public void beginFrame(Object graphics, float partialTick) {
        if (broken || graphics == null) {
            return;
        }
        this.graphics = graphics;
        this.partialTick = partialTick;
        this.pushedScales = 0;
        this.scissorDepth = 0;
        this.guiScaleCached = false;
        this.snapshotsTaken = false;
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
        // Если кто-то забыл закрыть масштаб, стек игры нельзя оставлять кривым:
        // иначе поедет весь остальной интерфейс игры.
        while (pushedScales > 0) {
            popScale();
        }
        // Незакрытая область обрезания - самая дорогая утечка из возможных: игра
        // продолжит рисовать свой интерфейс внутри нашего прямоугольника, и экран
        // останется чёрным. Снимаем всё, что осталось, пока контекст ещё жив.
        while (scissorDepth > 0) {
            scissorOff();
        }
        scissorDepth = 0;
        guiScaleCached = false;
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
        if (guiScaleCached) {
            return cachedGuiScale;
        }
        Object window = window();
        if (window == null) {
            return 1.0;
        }
        try {
            // Масштаб спрашивают десятки раз за кадр: каждое скругление, каждый
            // значок, каждая плашка. Внутри одного кадра он не меняется, а каждый
            // вызов - это цепочка обращений через отражение.
            double value = (double) getGuiScale.invoke(window);
            cachedGuiScale = value;
            guiScaleCached = graphics != null;
            return value;
        } catch (Throwable error) {
            disable("не удалось получить масштаб интерфейса", error);
            return 1.0;
        }
    }

    @Override
    public void pushScale(float scale) {
        if (!drawable() || scale <= 0f) {
            return;
        }
        try {
            Object stack = pose.invoke(graphics);
            pushPose.invoke(stack);
            poseScale.invoke(stack, scale, scale, 1f);
            pushedScales++;
        } catch (Throwable error) {
            disable("не удалось сменить масштаб рисования", error);
        }
    }

    @Override
    public void popScale() {
        if (!drawable() || pushedScales <= 0) {
            return;
        }
        try {
            Object stack = pose.invoke(graphics);
            popPose.invoke(stack);
            pushedScales--;
        } catch (Throwable error) {
            pushedScales = 0;
            disable("не удалось вернуть масштаб рисования", error);
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
    public boolean keyDown(int key) {
        long handle = handle();
        if (handle == 0L) {
            return false;
        }
        try {
            return (int) glfwGetKey.invoke(null, handle, key) == GLFW_PRESS;
        } catch (Throwable error) {
            disable("не удалось опросить клавиатуру", error);
            return false;
        }
    }

    @Override
    public boolean mouseDown(int button) {
        long handle = handle();
        if (handle == 0L) {
            return false;
        }
        try {
            return (int) glfwGetMouseButton.invoke(null, handle, button) == GLFW_PRESS;
        } catch (Throwable error) {
            disable("не удалось опросить мышь", error);
            return false;
        }
    }

    @Override
    public int mouseX() {
        return mousePosition(true);
    }

    @Override
    public int mouseY() {
        return mousePosition(false);
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
        if (!drawable()) {
            return;
        }
        // Пустая область - не повод пропустить вызов. Раньше нулевая ширина или
        // высота тихо возвращала управление, а вызывающий всё равно потом снимал
        // область - и снимал чужую, внешнюю. Именно поэтому раскрытие карточки,
        // у которой высота содержимого начинается с нуля, оставляло игру с чужим
        // прямоугольником обрезания на весь кадр и гасило экран. Пустую область
        // кладём на стопку как есть: рисовать внутри неё нечего, зато снятие
        // снимет ровно её и ничего чужого.
        int right = x + Math.max(0, width);
        int bottom = y + Math.max(0, height);
        try {
            enableScissor.invoke(graphics, x, y, right, bottom);
            scissorDepth++;
        } catch (Throwable error) {
            disable("не удалось ограничить область рисования", error);
        }
    }

    @Override
    public void scissorOff() {
        // Снимаем только то, что сами и положили: лишнее снятие опустошает
        // стопку игры и роняет кадр исключением.
        if (!drawable() || scissorDepth <= 0) {
            return;
        }
        try {
            disableScissor.invoke(graphics);
            scissorDepth--;
        } catch (Throwable error) {
            scissorDepth = 0;
            disable("не удалось снять ограничение рисования", error);
        }
    }

    @Override
    public void printMessage(String text) {
        System.out.println("[Nimbus] " + text);
    }

    @Override
    public boolean inWorld() {
        return player() != null;
    }

    @Override
    public double playerX() {
        return coordinate(entityX);
    }

    @Override
    public double playerY() {
        return coordinate(entityY);
    }

    @Override
    public double playerZ() {
        return coordinate(entityZ);
    }

    @Override
    public float playerYaw() {
        Object player = player();
        if (player == null || entityYaw == null) {
            return 0f;
        }
        try {
            return (float) entityYaw.invoke(player);
        } catch (Throwable error) {
            telemetryReady = false;
            return 0f;
        }
    }

    /**
     * \u0417\u0430\u0434\u0435\u0440\u0436\u043a\u0430 \u0431\u0435\u0440\u0451\u0442\u0441\u044f \u0438\u0437 \u0442\u0430\u0431\u043b\u0438\u0446\u044b \u0438\u0433\u0440\u043e\u043a\u043e\u0432, \u043a\u043e\u0442\u043e\u0440\u0443\u044e \u043f\u0440\u0438\u0441\u044b\u043b\u0430\u0435\u0442 \u0441\u0435\u0440\u0432\u0435\u0440.
     *
     * \u0412 \u043e\u0434\u0438\u043d\u043e\u0447\u043d\u043e\u0439 \u0438\u0433\u0440\u0435 \u0441\u0435\u0440\u0432\u0435\u0440 \u0441\u0432\u043e\u0439 \u0436\u0435, \u043f\u043e\u044d\u0442\u043e\u043c\u0443 \u0442\u0430\u043c \u0447\u0435\u0441\u0442\u043d\u044b\u0439 \u043d\u043e\u043b\u044c.
     */
    @Override
    public int ping() {
        Object player = player();
        if (player == null || entityUuid == null || getConnection == null
                || getPlayerInfo == null || getLatency == null) {
            return -1;
        }
        Object minecraft = minecraft();
        if (minecraft == null) {
            return -1;
        }
        try {
            Object connection = getConnection.invoke(minecraft);
            if (connection == null) {
                return -1;
            }
            Object info = getPlayerInfo.invoke(connection, entityUuid.invoke(player));
            if (info == null) {
                return -1;
            }
            return (int) getLatency.invoke(info);
        } catch (Throwable error) {
            telemetryReady = false;
            return -1;
        }
    }

    private double coordinate(Method getter) {
        Object player = player();
        if (player == null || getter == null) {
            return 0.0;
        }
        try {
            return (double) getter.invoke(player);
        } catch (Throwable error) {
            telemetryReady = false;
            return 0.0;
        }
    }

    /** В главном меню игрока нет, и это норма, а не ошибка. */
    private Object player() {
        if ((!telemetryReady && !statusReady) || broken || !resolved || playerField == null) {
            return null;
        }
        Object minecraft = minecraft();
        if (minecraft == null) {
            return null;
        }
        try {
            return playerField.get(minecraft);
        } catch (Throwable error) {
            telemetryReady = false;
            return null;
        }
    }

    /**
     * \u0418\u043c\u0435\u043d\u0430 \u0434\u043b\u044f \u0442\u0435\u043b\u0435\u043c\u0435\u0442\u0440\u0438\u0438 \u0438\u0449\u0443\u0442\u0441\u044f \u043e\u0442\u0434\u0435\u043b\u044c\u043d\u043e \u043e\u0442 \u043e\u0441\u0442\u0430\u043b\u044c\u043d\u043e\u0433\u043e \u043c\u043e\u0441\u0442\u0430.
     *
     * \u0415\u0441\u043b\u0438 \u0432 \u043a\u0430\u043a\u043e\u0439-\u0442\u043e \u0432\u0435\u0440\u0441\u0438\u0438 \u044d\u0442\u0438 \u0438\u043c\u0435\u043d\u0430 \u0441\u044a\u0435\u0434\u0435\u0442, \u043a\u043e\u043e\u0440\u0434\u0438\u043d\u0430\u0442\u044b \u043f\u0440\u043e\u0441\u0442\u043e \u043f\u0440\u043e\u043f\u0430\u0434\u0443\u0442,
     * \u0430 \u043c\u0435\u043d\u044e \u0438 \u043e\u0442\u0440\u0438\u0441\u043e\u0432\u043a\u0430 \u043e\u0441\u0442\u0430\u043d\u0443\u0442\u0441\u044f \u0440\u0430\u0431\u043e\u0447\u0438\u043c\u0438.
     */
    private void resolveTelemetry(ClassLoader loader, Class<?> minecraftClass) {
        try {
            playerField = field(minecraftClass, MINECRAFT, "player");
            Class<?> entityClass = Class.forName(className(ENTITY), false, loader);
            entityX = method(entityClass, ENTITY, "getX");
            entityY = method(entityClass, ENTITY, "getY");
            entityZ = method(entityClass, ENTITY, "getZ");
            entityYaw = method(entityClass, ENTITY, "getYRot");
            entityUuid = method(entityClass, ENTITY, "getUUID");
            getConnection = method(minecraftClass, MINECRAFT, "getConnection");
            Class<?> listenerClass = Class.forName(className(CLIENT_PACKET_LISTENER), false, loader);
            getPlayerInfo = method(
                    listenerClass,
                    CLIENT_PACKET_LISTENER,
                    "getPlayerInfo",
                    new Class<?>[]{java.util.UUID.class},
                    "java.util.UUID"
            );
            Class<?> infoClass = Class.forName(className(PLAYER_INFO), false, loader);
            getLatency = method(infoClass, PLAYER_INFO, "getLatency");
            telemetryReady = true;
        } catch (Throwable ignored) {
            telemetryReady = false;
        }
    }

    /**
     * \u0417\u0432\u0443\u043a \u0438\u0434\u0451\u0442 \u0447\u0435\u0440\u0435\u0437 \u0448\u0442\u0430\u0442\u043d\u044b\u0439 \u0434\u0432\u0438\u0436\u043e\u043a \u0438\u0433\u0440\u044b, \u043f\u043e\u044d\u0442\u043e\u043c\u0443 \u0435\u0433\u043e \u0433\u0440\u043e\u043c\u043a\u043e\u0441\u0442\u044c \u0438 \u0443\u0441\u0442\u0440\u043e\u0439\u0441\u0442\u0432\u043e
     * \u0432\u044b\u0432\u043e\u0434\u0430 \u043f\u043e\u0434\u0447\u0438\u043d\u044f\u044e\u0442\u0441\u044f \u043d\u0430\u0441\u0442\u0440\u043e\u0439\u043a\u0430\u043c \u0438\u0433\u0440\u043e\u043a\u0430 \u0431\u0435\u0437 \u0435\u0434\u0438\u043d\u043e\u0439 \u0441\u0442\u0440\u043e\u0447\u043a\u0438 \u043a\u043e\u0434\u0430 \u0441 \u043d\u0430\u0448\u0435\u0439 \u0441\u0442\u043e\u0440\u043e\u043d\u044b.
     */
    @Override
    public void playSound(int sound, float pitch) {
        if (!soundReady || broken || !resolved) {
            return;
        }
        if (sound < 0 || sound >= soundFields.length || soundFields[sound] == null) {
            return;
        }
        Object minecraft = minecraft();
        if (minecraft == null) {
            return;
        }
        try {
            Object manager = getSoundManager.invoke(minecraft);
            if (manager == null) {
                return;
            }
            Object event = soundFields[sound].get(null);
            if (event == null) {
                return;
            }
            // \u0412 \u0440\u0430\u0437\u043d\u044b\u0445 \u0432\u0435\u0440\u0441\u0438\u044f\u0445 \u0437\u0432\u0443\u043a \u043b\u0435\u0436\u0438\u0442 \u0442\u043e \u043d\u0430\u043f\u0440\u044f\u043c\u0443\u044e, \u0442\u043e \u0432 \u043e\u0431\u0451\u0440\u0442\u043a\u0435 \u0440\u0435\u0435\u0441\u0442\u0440\u0430.
            boolean wrapped = holderClass != null && holderClass.isInstance(event);
            Method factory = wrapped ? forUiHolder : forUiEvent;
            if (factory == null) {
                soundReady = false;
                return;
            }
            Object instance = factory.invoke(null, event, pitch);
            if (instance == null) {
                return;
            }
            playSoundMethod.invoke(manager, instance);
        } catch (Throwable error) {
            soundReady = false;
        }
    }

    /** \u041d\u0430\u0448\u043b\u0438\u0441\u044c \u043b\u0438 \u0438\u043c\u0435\u043d\u0430 \u0437\u0432\u0443\u043a\u0430. \u041d\u0443\u0436\u043d\u043e \u043e\u0444\u043b\u0430\u0439\u043d-\u043f\u0440\u043e\u0432\u0435\u0440\u043a\u0435. */
    public boolean soundReady() {
        return soundReady;
    }

    /**
     * Значения полей здесь нарочно не читаются.
     *
     * Чтение статического поля запускает инициализацию всего реестра звуков,
     * а офлайн-проверка работает без запущенной игры и упала бы на этом месте.
     * Нам же нужно проверить только наличие имён.
     */
    private void resolveSound(ClassLoader loader, Class<?> minecraftClass) {
        try {
            getSoundManager = method(minecraftClass, MINECRAFT, "getSoundManager");
            Class<?> managerClass = getSoundManager.getReturnType();
            Class<?> instanceClass = Class.forName(className(SOUND_INSTANCE), false, loader);
            playSoundMethod = method(
                    managerClass,
                    SOUND_MANAGER,
                    "play",
                    new Class<?>[]{instanceClass},
                    SOUND_INSTANCE
            );

            Class<?> simpleClass = Class.forName(className(SIMPLE_SOUND), false, loader);
            Class<?> eventClass = Class.forName(className(SOUND_EVENT), false, loader);
            holderClass = Class.forName(className(HOLDER), false, loader);
            forUiEvent = method(
                    simpleClass,
                    SIMPLE_SOUND,
                    "forUI",
                    new Class<?>[]{eventClass, float.class},
                    SOUND_EVENT, "float"
            );
            forUiHolder = method(
                    simpleClass,
                    SIMPLE_SOUND,
                    "forUI",
                    new Class<?>[]{holderClass, float.class},
                    HOLDER, "float"
            );

            Class<?> eventsClass = Class.forName(className(SOUND_EVENTS), false, loader);
            soundFields[Sounds.CLICK] = field(eventsClass, SOUND_EVENTS, "UI_BUTTON_CLICK");
            soundFields[Sounds.TICK] = field(eventsClass, SOUND_EVENTS, "NOTE_BLOCK_HAT");
            soundFields[Sounds.TONE] = field(eventsClass, SOUND_EVENTS, "NOTE_BLOCK_PLING");
            soundFields[Sounds.BIT] = field(eventsClass, SOUND_EVENTS, "NOTE_BLOCK_BIT");
            soundFields[Sounds.HIT] = field(eventsClass, SOUND_EVENTS, "PLAYER_ATTACK_STRONG");
            soundFields[Sounds.HIT_CRIT] = field(eventsClass, SOUND_EVENTS, "PLAYER_ATTACK_CRIT");
            soundReady = true;
            soundFailure = null;
        } catch (Throwable error) {
            soundReady = false;
            soundFailure = String.valueOf(error);
        }
    }

    /**
     * Имена расширенной части моста: статус, бой, эффекты, броня, чат, яркость.
     *
     * Это самая необязательная часть адаптера, поэтому здесь тот же порядок, что и
     * у телеметрии: не нашлось хоть одно имя - группа выключается целиком, а модули,
     * которым не хватило данных, не рисуются. Меню и остальной HUD живут дальше.
     */
    private void resolveStatus(ClassLoader loader, Class<?> minecraftClass) {
        try {
            if (playerField == null) {
                playerField = field(minecraftClass, MINECRAFT, "player");
            }
            Class<?> entityClass = Class.forName(className(ENTITY_CLASS), false, loader);
            isAlive = method(entityClass, ENTITY_CLASS, "isAlive");
            entityName = method(entityClass, ENTITY_CLASS, "getName");
            // Координаты может не быть из-за упавшей телеметрии, а цели они нужны тоже.
            if (entityX == null) {
                entityX = method(entityClass, ENTITY_CLASS, "getX");
                entityY = method(entityClass, ENTITY_CLASS, "getY");
                entityZ = method(entityClass, ENTITY_CLASS, "getZ");
            }
            livingEntityClass = Class.forName(className(LIVING_ENTITY), false, loader);
            getHealth = method(livingEntityClass, LIVING_ENTITY, "getHealth");
            getMaxHealth = method(livingEntityClass, LIVING_ENTITY, "getMaxHealth");
            getAbsorption = method(livingEntityClass, LIVING_ENTITY, "getAbsorptionAmount");
            getEffects = method(livingEntityClass, LIVING_ENTITY, "getActiveEffects");

            Class<?> playerClass = Class.forName(className(PLAYER), false, loader);
            // Загрузчик тот же, что у остальных имён: без него класс ищется среди
            // классов адаптера, и офлайн-проверка не находит библиотеку игры вовсе.
            Class<?> profileClass = Class.forName("com.mojang.authlib.GameProfile", false, loader);
            profileName = profileClass.getMethod("getName");
            getGameProfile = method(playerClass, PLAYER, "getGameProfile");
            // Броня объявлена у живого существа, а не у игрока: искать её среди
            // методов игрока значит не найти вовсе - getDeclaredMethod не видит наследство.
            getArmorValue = method(livingEntityClass, LIVING_ENTITY, "getArmorValue");
            getAttackStrength = method(playerClass, PLAYER, "getAttackStrengthScale", new Class<?>[]{float.class}, "float");
            getInventory = method(playerClass, PLAYER, "getInventory");

            Class<?> inventoryClass = Class.forName(className(INVENTORY), false, loader);
            inventoryArmor = field(inventoryClass, INVENTORY, "armor");

            Class<?> stackClass = Class.forName(className(ITEM_STACK), false, loader);
            Class<?> itemClass = Class.forName(className(ITEM), false, loader);
            stackEmpty = method(stackClass, ITEM_STACK, "isEmpty");
            itemOf = method(stackClass, ITEM_STACK, "getItem");
            stackDamage = method(stackClass, ITEM_STACK, "getDamageValue");
            stackMaxDamage = method(stackClass, ITEM_STACK, "getMaxDamage");
            itemDescription = method(itemClass, ITEM, "getDescriptionId");

            Class<?> effectInstanceClass = Class.forName(className(MOB_EFFECT_INSTANCE), false, loader);
            Class<?> effectClass = Class.forName(className(MOB_EFFECT), false, loader);
            effectOf = method(effectInstanceClass, MOB_EFFECT_INSTANCE, "getEffect");
            effectAmplifier = method(effectInstanceClass, MOB_EFFECT_INSTANCE, "getAmplifier");
            effectDuration = method(effectInstanceClass, MOB_EFFECT_INSTANCE, "getDuration");
            effectDescription = method(effectClass, MOB_EFFECT, "getDescriptionId");
            effectCategory = method(effectClass, MOB_EFFECT, "getCategory");

            Class<?> componentClass = Class.forName(className(COMPONENT), false, loader);
            componentText = method(componentClass, COMPONENT, "getString");

            currentServer = method(minecraftClass, MINECRAFT, "getCurrentServer");
            Class<?> serverClass = Class.forName(className(SERVER_DATA), false, loader);
            serverIp = field(serverClass, SERVER_DATA, "ip");

            Class<?> optionsClass = Class.forName(className(OPTIONS), false, loader);
            Class<?> optionClass = Class.forName(className(OPTION_INSTANCE), false, loader);
            optionsField = field(minecraftClass, MINECRAFT, "options");
            // set у OptionInstance стирается в set(Object): конкретный тип знает
            // только сама настройка. Поиск по boolean.class ничего бы не нашёл и
            // обрушил всю статусную группу, поэтому резолвим по стёртому типу -
            // и необязательно: без него плавная камера просто не тронется.
            optionSet = optionalMethod(optionClass, OPTION_INSTANCE, "set",
                    new Class<?>[]{Object.class}, "java.lang.Object");
            smoothCameraOption = optionalField(optionsClass, OPTIONS, "smoothCamera");

            // Локализация опциональна: нет перевода - покажем хвост ключа.
            i18nGet = optionalStaticMethod(loader, I18N, "get", String.class, "java.lang.String");

            statusReady = true;
            statusFailure = null;
        } catch (Throwable error) {
            statusReady = false;
            statusFailure = String.valueOf(error);
        }
    }

    /** Нашлись ли имена расширенной части моста. Нужно офлайн-проверке. */
    public boolean statusReady() {
        return statusReady;
    }

    /** Причина отказа группы звука или null. Только для лога и офлайн-проверки. */
    public String soundFailure() {
        return soundFailure;
    }

    /** Причина отказа расширенной группы или null. Только для лога и офлайн-проверки. */
    public String statusFailure() {
        return statusFailure;
    }

    // -------------------------------------------------------------- расширенная часть

    /** Игрок, если расширенная часть моста жива; иначе null. */
    private Object statusPlayer() {
        if (!statusReady || broken || !resolved || playerField == null) {
            return null;
        }
        Object minecraft = minecraft();
        if (minecraft == null) {
            return null;
        }
        try {
            return playerField.get(minecraft);
        } catch (Throwable error) {
            statusReady = false;
            return null;
        }
    }

    @Override
    public String playerName() {
        Object player = statusPlayer();
        if (player == null || getGameProfile == null || profileName == null) {
            return "";
        }
        try {
            Object profile = getGameProfile.invoke(player);
            if (profile == null) {
                return "";
            }
            Object name = profileName.invoke(profile);
            return name == null ? "" : (String) name;
        } catch (Throwable error) {
            statusReady = false;
            return "";
        }
    }

    @Override
    public String serverAddress() {
        Object minecraft = statusPlayer() == null ? null : minecraft();
        if (minecraft == null || currentServer == null || serverIp == null) {
            return "";
        }
        try {
            Object server = currentServer.invoke(minecraft);
            if (server == null) {
                // Одиночная игра: адреса нет, и это не ошибка.
                return "";
            }
            Object ip = serverIp.get(server);
            return ip == null ? "" : (String) ip;
        } catch (Throwable error) {
            statusReady = false;
            return "";
        }
    }

    @Override
    public boolean playerAlive() {
        return alive(statusPlayer());
    }

    /** Живость любой сущности игры; null и чужой объект - не жива. */
    private boolean alive(Object entity) {
        if (entity == null || isAlive == null) {
            return false;
        }
        try {
            return (boolean) isAlive.invoke(entity);
        } catch (Throwable error) {
            statusReady = false;
            return false;
        }
    }

    @Override
    public float playerHealth() {
        return number(getHealth, statusPlayer());
    }

    @Override
    public float playerMaxHealth() {
        return number(getMaxHealth, statusPlayer());
    }

    @Override
    public float playerAbsorption() {
        return number(getAbsorption, statusPlayer());
    }

    @Override
    public int playerArmorPoints() {
        if (getArmorValue == null) {
            return 0;
        }
        try {
            Object player = statusPlayer();
            return player == null ? 0 : (int) getArmorValue.invoke(player);
        } catch (Throwable error) {
            statusReady = false;
            return 0;
        }
    }

    @Override
    public int attackStrengthPercent() {
        if (getAttackStrength == null) {
            return 0;
        }
        try {
            Object player = statusPlayer();
            if (player == null) {
                return 0;
            }
            float scale = (float) getAttackStrength.invoke(player, partialTick);
            return Math.max(0, Math.min(100, Math.round(scale * 100f)));
        } catch (Throwable error) {
            statusReady = false;
            return 0;
        }
    }

    /** Общий путь чтения float-свойства сущности. */
    private float number(Method getter, Object entity) {
        if (getter == null || entity == null) {
            return 0f;
        }
        try {
            return (float) getter.invoke(entity);
        } catch (Throwable error) {
            statusReady = false;
            return 0f;
        }
    }

    @Override
    public int effectCount() {
        takeSnapshots();
        return effectSnapshot.size();
    }

    @Override
    public EffectInfo effect(int index) {
        takeSnapshots();
        if (index < 0 || index >= effectSnapshot.size()) {
            return null;
        }
        return effectSnapshot.get(index);
    }

    @Override
    public int armorCount() {
        takeSnapshots();
        return armorSnapshot.size();
    }

    @Override
    public ArmorPiece armor(int index) {
        takeSnapshots();
        if (index < 0 || index >= armorSnapshot.size()) {
            return null;
        }
        return armorSnapshot.get(index);
    }

    /**
     * Снять эффекты и броню одним заходом.
     *
     * Панели спрашивают данные десятки раз за кадр, а итерация по реестру игры
     * через отражение стоит заметно дороже обычного вызова. Снимок делается
     * один раз за кадр и переиспользуется всеми панелями.
     */
    private void takeSnapshots() {
        if (snapshotsTaken || !statusReady || broken || !resolved) {
            return;
        }
        snapshotsTaken = true;
        Object player = statusPlayer();
        if (player == null) {
            effectSnapshot = List.of();
            armorSnapshot = List.of();
            return;
        }
        effectSnapshot = readEffects(player);
        armorSnapshot = readArmor(player);
    }

    private List<EffectInfo> readEffects(Object player) {
        if (getEffects == null) {
            return List.of();
        }
        try {
            Object effects = getEffects.invoke(player);
            if (!(effects instanceof java.util.Collection<?> collection)) {
                return List.of();
            }
            List<EffectInfo> list = new ArrayList<>(collection.size());
            for (Object instance : collection) {
                try {
                    Object effect = effectOf.invoke(instance);
                    if (effect == null) {
                        continue;
                    }
                    int amplifier = (int) effectAmplifier.invoke(instance);
                    int ticks = (int) effectDuration.invoke(instance);
                    int seconds = ticks < 0 ? -1 : Math.max(0, (int) Math.ceil(ticks / 20.0));
                    boolean harmful = effectCategory.invoke(effect).toString().equals("HARMFUL");
                    list.add(new EffectInfo(effectTitle(effectDescription.invoke(effect)), amplifier, seconds, harmful));
                } catch (Throwable ignored) {
                    // Один сломанный эффект не отменяет остальных.
                }
            }
            return list;
        } catch (Throwable error) {
            statusReady = false;
            return List.of();
        }
    }

    private List<ArmorPiece> readArmor(Object player) {
        if (getInventory == null || inventoryArmor == null) {
            return List.of();
        }
        try {
            Object inventory = getInventory.invoke(player);
            if (inventory == null) {
                return List.of();
            }
            Object list = inventoryArmor.get(inventory);
            if (!(list instanceof java.util.List<?> slots)) {
                return List.of();
            }
            List<ArmorPiece> pieces = new ArrayList<>(4);
            for (Object stack : slots) {
                try {
                    if ((boolean) stackEmpty.invoke(stack)) {
                        continue;
                    }
                    Object item = itemOf.invoke(stack);
                    int damage = (int) stackDamage.invoke(stack);
                    int maxDamage = (int) stackMaxDamage.invoke(stack);
                    String name = itemTitle(itemDescription.invoke(item));
                    pieces.add(new ArmorPiece(name, damage, maxDamage));
                } catch (Throwable ignored) {
                    // Один сломанный предмет не отменяет остальных.
                }
            }
            return pieces;
        } catch (Throwable error) {
            statusReady = false;
            return List.of();
        }
    }

    /** Название эффекта: перевод игры, а без него - хвост ключа. */
    private String effectTitle(Object key) {
        if (key == null) {
            return "";
        }
        String text = (String) key;
        if (i18nGet != null) {
            try {
                Object translated = i18nGet.invoke(null, text);
                if (translated != null && !((String) translated).isEmpty()) {
                    return (String) translated;
                }
            } catch (Throwable ignored) {
                // Перевод не обязателен.
            }
        }
        int dot = text.lastIndexOf('.');
        String tail = dot < 0 ? text : text.substring(dot + 1);
        if (!tail.isEmpty()) {
            tail = Character.toUpperCase(tail.charAt(0)) + tail.substring(1);
        }
        return tail;
    }

    /** Название предмета брони: тот же перевод, что у эффектов. */
    private String itemTitle(Object key) {
        return effectTitle(key);
    }

    @Override
    public TargetInfo targetInfo(Object handle) {
        if (handle == null || !statusReady || broken || !resolved) {
            return null;
        }
        try {
            String name = "";
            if (entityName != null && componentText != null) {
                Object component = entityName.invoke(handle);
                if (component != null) {
                    name = (String) componentText.invoke(component);
                }
            }
            float health = 0f;
            float maxHealth = 0f;
            if (livingEntityClass != null && livingEntityClass.isInstance(handle)) {
                health = (float) getHealth.invoke(handle);
                maxHealth = (float) getMaxHealth.invoke(handle);
            }
            return new TargetInfo(name, health, maxHealth, alive(handle),
                    coordinate(entityX, handle), coordinate(entityY, handle), coordinate(entityZ, handle));
        } catch (Throwable error) {
            statusReady = false;
            return null;
        }
    }

    /** Координата произвольной сущности: тот же getter, что и у игрока. */
    private double coordinate(Method getter, Object entity) {
        if (getter == null || entity == null) {
            return 0.0;
        }
        try {
            return (double) getter.invoke(entity);
        } catch (Throwable error) {
            statusReady = false;
            return 0.0;
        }
    }

    @Override
    public void setSmoothCamera(boolean smooth) {
        Object minecraft = statusPlayer() == null ? null : minecraft();
        if (minecraft == null || optionsField == null || smoothCameraOption == null || optionSet == null) {
            return;
        }
        try {
            Object options = optionsField.get(minecraft);
            Object option = smoothCameraOption.get(options);
            if (option != null) {
                // Булева настройка: стёртый set(Object) принимает Boolean.
                optionSet.invoke(option, smooth);
            }
        } catch (Throwable ignored) {
            // Плавная камера - украшение зума, а не сам зум.
        }
    }

    private boolean drawable() {
        return resolved && !broken && graphics != null;
    }

    /** Положение курсора в единицах интерфейса: игра хранит его в пикселях окна. */
    @Override
    public double takeScroll() {
        return dev.nimbus.bridge.ScrollBuffer.take();
    }

    @Override
    public double mouseXPrecise() {
        return mousePrecise(true);
    }

    @Override
    public double mouseYPrecise() {
        return mousePrecise(false);
    }

    /**
     * То же положение курсора, но без округления до единицы интерфейса.
     *
     * При масштабе 3 округлённое значение меняется раз в три настоящих пикселя, и
     * перетаскивание окна идёт рывками. Меню считает своё положение в дробных.
     */
    private double mousePrecise(boolean horizontal) {
        Object mouse = mouseHandler();
        if (mouse == null) {
            return 0.0;
        }
        try {
            double raw = (double) (horizontal ? mouseXpos : mouseYpos).invoke(mouse);
            double scale = guiScale();
            return raw / (scale <= 0 ? 1.0 : scale);
        } catch (Throwable error) {
            disable("не удалось узнать положение курсора", error);
            return 0.0;
        }
    }

    private int mousePosition(boolean horizontal) {
        Object mouse = mouseHandler();
        if (mouse == null) {
            return 0;
        }
        try {
            double raw = (double) (horizontal ? mouseXpos : mouseYpos).invoke(mouse);
            double scale = guiScale();
            return (int) (raw / (scale <= 0 ? 1.0 : scale));
        } catch (Throwable error) {
            disable("не удалось узнать положение курсора", error);
            return 0;
        }
    }

    /** Указатель на окно GLFW: через него идёт опрос клавиатуры и мыши. */
    private long handle() {
        Object window = window();
        if (window == null || glfwGetKey == null) {
            return 0L;
        }
        try {
            return (long) windowHandle.invoke(window);
        } catch (Throwable error) {
            disable("не удалось получить указатель окна", error);
            return 0L;
        }
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
            Class<?> poseStackClass = Class.forName(className(POSE_STACK), false, loader);

            minecraftInstance = method(minecraftClass, MINECRAFT, "getInstance");
            getWindow = method(minecraftClass, MINECRAFT, "getWindow");
            guiScaledWidth = method(windowClass, WINDOW, "getGuiScaledWidth");
            guiScaledHeight = method(windowClass, WINDOW, "getGuiScaledHeight");
            getGuiScale = method(windowClass, WINDOW, "getGuiScale");
            windowHandle = method(windowClass, WINDOW, "getWindow");

            fontField = field(minecraftClass, MINECRAFT, "font");
            mouseHandlerField = field(minecraftClass, MINECRAFT, "mouseHandler");

            Class<?> fontClass = fontField.getType();
            Class<?> mouseClass = mouseHandlerField.getType();

            grabMouse = method(mouseClass, MOUSE_HANDLER, "grabMouse");
            releaseMouse = method(mouseClass, MOUSE_HANDLER, "releaseMouse");
            isMouseGrabbed = method(mouseClass, MOUSE_HANDLER, "isMouseGrabbed");
            mouseXpos = method(mouseClass, MOUSE_HANDLER, "xpos");
            mouseYpos = method(mouseClass, MOUSE_HANDLER, "ypos");

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
            pose = method(guiGraphicsClass, GUI_GRAPHICS, "pose");

            pushPose = method(poseStackClass, POSE_STACK, "pushPose");
            popPose = method(poseStackClass, POSE_STACK, "popPose");
            poseScale = method(
                    poseStackClass,
                    POSE_STACK,
                    "scale",
                    new Class<?>[]{float.class, float.class, float.class},
                    "float", "float", "float"
            );

            // GLFW не обфусцирован: имена берутся как есть.
            Class<?> glfwClass = Class.forName(GLFW, false, loader);
            glfwGetKey = glfwClass.getMethod("glfwGetKey", long.class, int.class);
            glfwGetMouseButton = glfwClass.getMethod("glfwGetMouseButton", long.class, int.class);

            resolveTelemetry(loader, minecraftClass);
            resolveSound(loader, minecraftClass);
            resolveStatus(loader, minecraftClass);
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

    /** Необязательный метод без аргументов: его отсутствие не повод выключать всю группу. */
    private Method optionalMethod(Class<?> owner, String deobfClass, String name) {
        try {
            return method(owner, deobfClass, name);
        } catch (Throwable ignored) {
            return null;
        }
    }

    /** Необязательный метод: его отсутствие не повод выключать всю группу. */
    private Method optionalMethod(
            Class<?> owner,
            String deobfClass,
            String name,
            Class<?>[] parameterTypes,
            String... argTypes
    ) {
        try {
            return method(owner, deobfClass, name, parameterTypes, argTypes);
        } catch (Throwable ignored) {
            return null;
        }
    }

    /** Необязательный статический метод чужого класса, известного по имени как есть. */
    private Method optionalStaticMethod(
            ClassLoader loader,
            String className,
            String name,
            Class<?> parameterType,
            String argType
    ) {
        try {
            Class<?> owner = Class.forName(className, false, loader);
            String real = mappings.methodName(className, name, argType);
            Method found = owner.getMethod(real == null ? name : real, parameterType);
            found.setAccessible(true);
            return found;
        } catch (Throwable ignored) {
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
        pushedScales = 0;
        scissorDepth = 0;
        guiScaleCached = false;
        failure = what + ": " + error;
    }
}
