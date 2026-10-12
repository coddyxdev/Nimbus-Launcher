package dev.nimbus.runtime.transform;

import dev.nimbus.runtime.Log;
import dev.nimbus.runtime.mappings.MappingTable;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassVisitor;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.Label;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;

import java.lang.instrument.ClassFileTransformer;
import java.security.ProtectionDomain;

/**
 * Правка байт-кода игры на лету.
 *
 * Точки входа:
 * в классе Minecraft - старт игры и игровой тик,
 * в классе Gui - конец отрисовки интерфейса и отрисовка ванильного прицела,
 * в классе MouseHandler - начало обработки нажатия кнопки и прокрутки колеса,
 * в классе MultiPlayerGameMode - каждая атака игрока,
 * в классе GameRenderer - каждый готовый угол обзора,
 * в классе KeyboardHandler - нажатие клавиш во время ввода текста в наших настройках.
 *
 * Место для отрисовки выбрано именно там, а не в GameRenderer: к этому моменту
 * игра уже настроила плоскую проекцию и передаёт готовый контекст рисования,
 * так что нам не нужно руками возиться с матрицами и состоянием OpenGL.
 *
 * Перехват мыши стоит в самом низком месте - в обработчике события окна. Выше его
 * игра успевает захватить курсор обратно, а это швыряет курсор в центр экрана
 * прямо во время работы с нашим меню.
 *
 * Колесо перехватывается там же и по той же причине: иначе прокрутка списка
 * в меню одновременно листает быстрый слот игрока.
 *
 * Боевой хук стоит в начале attack: к этому моменту цель ещё не проверена игрой
 * на живость, и это честно - удар был, даже если мимо. Хук угла обзора перехватывает
 * готовое значение перед возвратом из метода угла обзора: игра успела всё посчитать,
 * а мы лишь масштабируем результат.
 */
public final class NimbusTransformer implements ClassFileTransformer {

    private static final String HOOKS = "dev/nimbus/runtime/NimbusHooks";
    private static final String MINECRAFT = "net.minecraft.client.Minecraft";
    private static final String GUI = "net.minecraft.client.gui.Gui";
    private static final String GUI_GRAPHICS = "net.minecraft.client.gui.GuiGraphics";
    private static final String MOUSE_HANDLER = "net.minecraft.client.MouseHandler";
    private static final String GAME_MODE = "net.minecraft.client.multiplayer.MultiPlayerGameMode";
    private static final String PLAYER = "net.minecraft.world.entity.player.Player";
    private static final String GAME_RENDERER = "net.minecraft.client.renderer.GameRenderer";
    private static final String CAMERA = "net.minecraft.client.Camera";
    private static final String KEYBOARD = "net.minecraft.client.KeyboardHandler";

    private final MappingTable mappings;

    /** Внутренние имена классов игры в запущенной версии. */
    private final String minecraftClass;
    private final String guiClass;
    private final String mouseHandlerClass;
    private final String gameModeClass;
    private final String gameRendererClass;
    private final String keyboardClass;

    private final MappingTable.Member runTick;
    private final MappingTable.Member constructor;
    private final MappingTable.Member guiRender;
    private final MappingTable.Member guiCrosshair;
    private final MappingTable.Member mouseOnPress;
    private final MappingTable.Member mouseOnScroll;
    private final MappingTable.Member modeAttack;
    private final MappingTable.Member rendererFov;
    private final MappingTable.Member keyboardKey;

    private boolean patchedTick;
    private boolean patchedStart;
    private boolean patchedHud;
    private boolean patchedMouse;
    private boolean patchedScroll;
    private boolean patchedCrosshair;
    private boolean patchedAttack;
    private boolean patchedFov;
    private boolean patchedKey;

    public NimbusTransformer(MappingTable mappings) {
        this.mappings = mappings;
        this.minecraftClass = mappings.obfClass(MINECRAFT);
        this.guiClass = mappings.obfClass(GUI);
        this.mouseHandlerClass = mappings.obfClass(MOUSE_HANDLER);
        this.gameModeClass = mappings.obfClass(GAME_MODE);
        this.gameRendererClass = mappings.obfClass(GAME_RENDERER);
        this.keyboardClass = mappings.obfClass(KEYBOARD);
        this.runTick = mappings.method(MINECRAFT, "runTick", "boolean");
        this.constructor = mappings.method(MINECRAFT, "<init>", "net.minecraft.client.main.GameConfig");
        this.guiRender = mappings.method(GUI, "render", GUI_GRAPHICS, "float");
        this.guiCrosshair = mappings.method(GUI, "renderCrosshair", GUI_GRAPHICS);
        this.mouseOnPress = mappings.method(MOUSE_HANDLER, "onPress", "long", "int", "int", "int");
        this.mouseOnScroll = mappings.method(MOUSE_HANDLER, "onScroll", "long", "double", "double");
        this.modeAttack = mappings.method(GAME_MODE, "attack", PLAYER, "net.minecraft.world.entity.Entity");
        this.rendererFov = fovMethod(mappings);
        this.keyboardKey = mappings.method(KEYBOARD, "keyPress", "long", "int", "int", "int", "int");

        Log.debug("класс Minecraft: " + minecraftClass
                + ", runTick: " + describe(runTick)
                + ", <init>: " + describe(constructor));
        Log.debug("класс Gui: " + guiClass
                + ", render: " + describe(guiRender)
                + ", renderCrosshair: " + describe(guiCrosshair));
        Log.debug("класс MouseHandler: " + mouseHandlerClass
                + ", onPress: " + describe(mouseOnPress)
                + ", onScroll: " + describe(mouseOnScroll));
        Log.debug("класс MultiPlayerGameMode: " + gameModeClass
                + ", attack: " + describe(modeAttack));
        Log.debug("класс GameRenderer: " + gameRendererClass
                + ", угол обзора: " + describe(rendererFov));
        Log.debug("класс KeyboardHandler: " + keyboardClass
                + ", keyPress: " + describe(keyboardKey));

        if (guiRender == null) {
            // Не ошибка: старые и будущие версии могут звать этот метод иначе.
            // Игра запустится, просто без нашего слоя.
            Log.warn("метод отрисовки интерфейса не найден в маппингах");
        }
        if (mouseOnPress == null) {
            // Без перехвата меню останется рабочим, но клики будут уходить и в игру.
            Log.warn("обработчик мыши не найден в маппингах, клики останутся у игры");
        }
        if (mouseOnScroll == null) {
            // Без этого меню не увидит колеса, а игра будет листать слоты под ним.
            Log.warn("обработчик колеса не найден в маппингах, прокрутка останется у игры");
        }
        if (guiCrosshair == null) {
            // Свой прицел будет рисоваться поверх ванильного, а не вместо него.
            Log.warn("отрисовка ванильного прицела не найдена в маппингах");
        }
        if (modeAttack == null) {
            // Без боевого хука не будет панели цели и звука попадания.
            Log.warn("боевой метод не найден в маппингах, панели цели не будет");
        }
        if (rendererFov == null) {
            // Без хука угла обзора зум останется недоступным.
            Log.warn("метод угла обзора не найден в маппингах, зум не будет работать");
        }
        if (keyboardKey == null) {
            // Без перехвата клавиатуры ввод текста в настройках не выйдет: игра
            // получит каждую букву и откроет свой чат или инвентарь.
            Log.warn("обработчик клавиатуры не найден в маппингах, ввод текста отключён");
        }
    }

    private static String describe(MappingTable.Member member) {
        return member == null ? "не найден" : member.name() + member.descriptor();
    }

    /**
     * Метод угла обзора: у Mojang он сменил и имя, и тип результата.
     *
     * До 1.18 это float getFOVModifier(Camera, float, boolean), с 1.18 -
     * double getFov(...) с теми же аргументами. Искали только старое имя, поэтому
     * на боевой версии хук молча не вставал и зум не работал вовсе. Имя ищется
     * по списку, а тип результата берётся из найденного дескриптора.
     */
    private static MappingTable.Member fovMethod(MappingTable mappings) {
        MappingTable.Member modern = mappings.method(GAME_RENDERER, "getFov", CAMERA, "float", "boolean");
        if (modern != null) {
            return modern;
        }
        return mappings.method(GAME_RENDERER, "getFOVModifier", CAMERA, "float", "boolean");
    }

    /** Тип результата метода по его дескриптору: то, что стоит после закрывающей скобки. */
    private static char returnKind(String descriptor) {
        int close = descriptor.lastIndexOf(')');
        return close < 0 || close + 1 >= descriptor.length() ? 'V' : descriptor.charAt(close + 1);
    }

    public boolean patchedTick() {
        return patchedTick;
    }

    public boolean patchedStart() {
        return patchedStart;
    }

    public boolean patchedHud() {
        return patchedHud;
    }

    public boolean patchedMouse() {
        return patchedMouse;
    }

    public boolean patchedScroll() {
        return patchedScroll;
    }

    public boolean patchedCrosshair() {
        return patchedCrosshair;
    }

    public boolean patchedAttack() {
        return patchedAttack;
    }

    public boolean patchedFov() {
        return patchedFov;
    }

    public boolean patchedKey() {
        return patchedKey;
    }

    @Override
    public byte[] transform(
            ClassLoader loader,
            String className,
            Class<?> classBeingRedefined,
            ProtectionDomain protectionDomain,
            byte[] classfileBuffer
    ) {
        if (className == null) {
            return null;
        }
        try {
            if (className.equals(minecraftClass)) {
                return patchMinecraft(classfileBuffer);
            }
            if ((guiRender != null || guiCrosshair != null) && className.equals(guiClass)) {
                return patchGui(classfileBuffer, loader);
            }
            if ((mouseOnPress != null || mouseOnScroll != null) && className.equals(mouseHandlerClass)) {
                return patchMouseHandler(classfileBuffer, loader);
            }
            if (modeAttack != null && className.equals(gameModeClass)) {
                return patchGameMode(classfileBuffer, loader);
            }
            if (rendererFov != null && className.equals(gameRendererClass)) {
                return patchGameRenderer(classfileBuffer);
            }
            if (keyboardKey != null && className.equals(keyboardClass)) {
                return patchKeyboardHandler(classfileBuffer, loader);
            }
            return null;
        } catch (Throwable error) {
            // Сломанный трансформер не должен мешать игре запуститься.
            Log.error("не удалось править " + className + ", игра запустится без клиента", error);
            return null;
        }
    }

    private byte[] patchMinecraft(byte[] original) {
        ClassReader reader = new ClassReader(original);
        // Здесь вставки без аргументов и без работы со стеком,
        // поэтому родные карты кадров остаются верными и пересчёт не нужен.
        ClassWriter writer = new ClassWriter(reader, 0);

        ClassVisitor visitor = new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(
                    int access,
                    String name,
                    String descriptor,
                    String signature,
                    String[] exceptions
            ) {
                MethodVisitor parent = super.visitMethod(access, name, descriptor, signature, exceptions);

                if (matches(runTick, name, descriptor)) {
                    patchedTick = true;
                    return new HeadCallInjector(parent, "onTick");
                }
                // Запасной путь только при неизвестной подписи конструктора: иначе
                // вставка ушла бы в каждый конструктор класса, а сводка врала бы
                // об успехе даже тогда, когда имя в маппингах не нашлось.
                boolean anyConstructor = constructor == null && "<init>".equals(name);
                if (matches(constructor, name, descriptor) || anyConstructor) {
                    patchedStart = true;
                    return new ReturnCallInjector(parent, "onGameStart");
                }
                return parent;
            }
        };

        reader.accept(visitor, 0);
        Log.debug("класс Minecraft пропатчен (tick=" + patchedTick + ", start=" + patchedStart + ")");
        return writer.toByteArray();
    }

    private byte[] patchGui(byte[] original, ClassLoader loader) {
        ClassReader reader = new ClassReader(original);

        // Здесь мы кладём на стек аргументы метода, а значит меняем его глубину
        // и требования к живости локальных переменных. Старые карты кадров становятся
        // неверными, и проверяющий механизм JVM отказывается грузить класс. Поэтому
        // карты кадров пересчитываются целиком.
        ClassWriter writer = new NimbusClassWriter(reader, ClassWriter.COMPUTE_FRAMES, loader);

        ClassVisitor visitor = new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(
                    int access,
                    String name,
                    String descriptor,
                    String signature,
                    String[] exceptions
            ) {
                MethodVisitor parent = super.visitMethod(access, name, descriptor, signature, exceptions);

                if (matches(guiRender, name, descriptor)) {
                    patchedHud = true;
                    return new HudCallInjector(parent);
                }
                if (matches(guiCrosshair, name, descriptor)) {
                    patchedCrosshair = true;
                    return new GuardInjector(parent, "blockCrosshair");
                }
                return parent;
            }
        };

        // EXPAND_FRAMES обязателен в паре с COMPUTE_FRAMES.
        reader.accept(visitor, ClassReader.EXPAND_FRAMES);
        Log.debug("класс Gui пропатчен (hud=" + patchedHud + ", crosshair=" + patchedCrosshair + ")");
        return writer.toByteArray();
    }

    /**
     * Ранний выход из обработчиков мыши, пока открыто наше меню.
     *
     * Вставка содержит ветвление, значит в методе появляется новая точка схода
     * управления, для которой нужна своя карта кадра. Ручной расчёт таких карт -
     * прямая дорога к отказу загрузчика классов, поэтому карты считаются заново.
     */
    private byte[] patchMouseHandler(byte[] original, ClassLoader loader) {
        ClassReader reader = new ClassReader(original);
        ClassWriter writer = new NimbusClassWriter(reader, ClassWriter.COMPUTE_FRAMES, loader);

        ClassVisitor visitor = new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(
                    int access,
                    String name,
                    String descriptor,
                    String signature,
                    String[] exceptions
            ) {
                MethodVisitor parent = super.visitMethod(access, name, descriptor, signature, exceptions);

                if (matches(mouseOnPress, name, descriptor)) {
                    patchedMouse = true;
                    return new GuardInjector(parent, "blockMouse");
                }
                if (matches(mouseOnScroll, name, descriptor)) {
                    patchedScroll = true;
                    return new ScrollInjector(parent);
                }
                return parent;
            }
        };

        reader.accept(visitor, ClassReader.EXPAND_FRAMES);
        if (patchedMouse) {
            dev.nimbus.runtime.NimbusHooks.markMousePatched();
        }
        Log.debug("класс MouseHandler пропатчен (mouse=" + patchedMouse + ", scroll=" + patchedScroll + ")");
        return writer.toByteArray();
    }

    /**
     * Боевой хук: сообщаем о каждой атаке игрока.
     *
     * Вставка стоит в начале attack(Player, Entity) и отдаёт хуку вторую клетку -
     * сущность-цель (ноль - this, один - игрок, два - цель). Глубина стека меняется,
     * поэтому карты кадров пересчитываются: то же правило, что и у слоя отрисовки.
     */
    private byte[] patchGameMode(byte[] original, ClassLoader loader) {
        ClassReader reader = new ClassReader(original);
        ClassWriter writer = new NimbusClassWriter(reader, ClassWriter.COMPUTE_FRAMES, loader);

        ClassVisitor visitor = new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(
                    int access,
                    String name,
                    String descriptor,
                    String signature,
                    String[] exceptions
            ) {
                MethodVisitor parent = super.visitMethod(access, name, descriptor, signature, exceptions);
                if (matches(modeAttack, name, descriptor)) {
                    patchedAttack = true;
                    return new EntityHeadCallInjector(parent, "onAttack", 2);
                }
                return parent;
            }
        };

        reader.accept(visitor, ClassReader.EXPAND_FRAMES);
        Log.debug("класс MultiPlayerGameMode пропатчен (attack=" + patchedAttack + ")");
        return writer.toByteArray();
    }

    /**
     * Хук угла обзора: каждый готовый FOV проходит через наш модификатор.
     *
     * Значение снимается со стека прямо перед каждым возвратом и хук возвращает
     * его на то же место. Тип результата у метода свой в каждой версии игры,
     * поэтому опкод возврата и подпись хука берутся из дескриптора: подмена типа
     * здесь означает отказ загрузчика классов уже в игре.
     *
     * Глубина стека не меняется - сколько сняли, столько и положили, - поэтому
     * карты кадров остаются верными и пересчёт не нужен.
     */
    private byte[] patchGameRenderer(byte[] original) {
        char kind = returnKind(rendererFov.descriptor());
        if (kind != 'D' && kind != 'F') {
            // Незнакомый тип угла обзора: молча портить возврат нельзя.
            Log.warn("угол обзора возвращает неизвестный тип " + kind + ", зум отключён");
            return null;
        }

        ClassReader reader = new ClassReader(original);
        ClassWriter writer = new ClassWriter(reader, 0);

        ClassVisitor visitor = new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(
                    int access,
                    String name,
                    String descriptor,
                    String signature,
                    String[] exceptions
            ) {
                MethodVisitor parent = super.visitMethod(access, name, descriptor, signature, exceptions);
                if (matches(rendererFov, name, descriptor)) {
                    patchedFov = true;
                    return new FovReturnInjector(parent, "onFov", kind);
                }
                return parent;
            }
        };

        reader.accept(visitor, 0);
        Log.debug("класс GameRenderer пропатчен (fov=" + patchedFov + ")");
        return writer.toByteArray();
    }

    /**
     * Хук клавиатуры: во время ввода текста в наших настройках игра не видит клавиш.
     *
     * Ранний выход создаёт новую точку схода управления, а у метода есть свой
     * тип результата, поэтому пустить управление мимо можно только корректным
     * значением этого типа - иначе проверяющий механизм JVM отвергнет класс.
     */
    private byte[] patchKeyboardHandler(byte[] original, ClassLoader loader) {
        ClassReader reader = new ClassReader(original);
        ClassWriter writer = new NimbusClassWriter(reader, ClassWriter.COMPUTE_FRAMES, loader);
        int[] returnCode = {Opcodes.RETURN};

        ClassVisitor visitor = new ClassVisitor(Opcodes.ASM9, writer) {
            @Override
            public MethodVisitor visitMethod(
                    int access,
                    String name,
                    String descriptor,
                    String signature,
                    String[] exceptions
            ) {
                MethodVisitor parent = super.visitMethod(access, name, descriptor, signature, exceptions);
                if (matches(keyboardKey, name, descriptor)) {
                    returnCode[0] = defaultReturn(descriptor);
                    patchedKey = true;
                    // Клетки у обычного метода: 0 - this, 1-2 - long-указатель окна,
                    // 3 - клавиша, 4 - сканкод, 5 - действие, 6 - модификаторы.
                    return new GuardReturnInjector(parent, "onKeyPress", returnCode[0], 3, 5);
                }
                return parent;
            }
        };

        reader.accept(visitor, ClassReader.EXPAND_FRAMES);
        Log.debug("класс KeyboardHandler пропатчен (key=" + patchedKey + ")");
        return writer.toByteArray();
    }

    /** Инструкция возврата значения по умолчанию для типа результата метода. */
    private static int defaultReturn(String descriptor) {
        String result = descriptor.substring(descriptor.indexOf(')') + 1);
        if (result.isEmpty()) {
            return Opcodes.RETURN;
        }
        return switch (result.charAt(0)) {
            case 'J' -> Opcodes.LCONST_0;
            case 'F' -> Opcodes.FCONST_0;
            case 'D' -> Opcodes.DCONST_0;
            case 'V' -> Opcodes.RETURN;
            default -> Opcodes.ICONST_0;
        };
    }

    private static boolean matches(MappingTable.Member member, String name, String descriptor) {
        return member != null && member.name().equals(name) && member.descriptor().equals(descriptor);
    }

    /** Вставляет вызов хука в начало метода. */
    private static final class HeadCallInjector extends MethodVisitor {

        private final String hook;

        HeadCallInjector(MethodVisitor parent, String hook) {
            super(Opcodes.ASM9, parent);
            this.hook = hook;
        }

        @Override
        public void visitCode() {
            super.visitCode();
            super.visitMethodInsn(Opcodes.INVOKESTATIC, HOOKS, hook, "()V", false);
        }
    }

    /**
     * Вставляет вызов хука в начало метода с одним объектным аргументом.
     *
     * Аргумент лежит в клетке метода по номеру slot: у обычного метода ноль -
     * this, дальше аргументы по порядку. Объект кладётся на стек как есть и в хук
     * приходит как Object: разбирать его будет ядро через мост версии.
     */
    private static final class EntityHeadCallInjector extends MethodVisitor {

        private final String hook;
        private final int slot;

        EntityHeadCallInjector(MethodVisitor parent, String hook, int slot) {
            super(Opcodes.ASM9, parent);
            this.hook = hook;
            this.slot = slot;
        }

        @Override
        public void visitCode() {
            super.visitCode();
            super.visitVarInsn(Opcodes.ALOAD, slot);
            super.visitMethodInsn(Opcodes.INVOKESTATIC, HOOKS, hook, "(Ljava/lang/Object;)V", false);
        }
    }

    /**
     * Проводит каждое возвращаемое значение угла обзора через статический хук.
     *
     * Опкод возврата и подпись хука зависят от типа результата метода: (D)D для
     * double и (F)F для float. Ловим только свой опкод: ARETURN и IRETURN в том же
     * методе - это чужие пути, и трогать их значит подменить не то значение.
     */
    private static final class FovReturnInjector extends MethodVisitor {

        private final String hook;
        private final int returnOpcode;
        private final String hookDescriptor;

        FovReturnInjector(MethodVisitor parent, String hook, char kind) {
            super(Opcodes.ASM9, parent);
            this.hook = hook;
            this.returnOpcode = kind == 'D' ? Opcodes.DRETURN : Opcodes.FRETURN;
            this.hookDescriptor = kind == 'D' ? "(D)D" : "(F)F";
        }

        @Override
        public void visitInsn(int opcode) {
            if (opcode == returnOpcode) {
                super.visitMethodInsn(Opcodes.INVOKESTATIC, HOOKS, hook, hookDescriptor, false);
            }
            super.visitInsn(opcode);
        }
    }

    /**
     * Ранний выход из метода, если хук сказал "событие наше".
     *
     * Отличается от GuardInjector типом результата: здесь метод что-то возвращает,
     * поэтому вместо пустого RETURN на стек кладётся значение по умолчанию этого типа.
     * Клетки аргументов передаются хуку парами: у длинного указателя окна каждая
     * клетка - своя, поэтому клавиша и действие сидят не в клетках один и два.
     */
    private static final class GuardReturnInjector extends MethodVisitor {

        private final String hook;
        private final int returnCode;
        private final int firstSlot;
        private final int secondSlot;

        GuardReturnInjector(MethodVisitor parent, String hook, int returnCode, int firstSlot, int secondSlot) {
            super(Opcodes.ASM9, parent);
            this.hook = hook;
            this.returnCode = returnCode;
            this.firstSlot = firstSlot;
            this.secondSlot = secondSlot;
        }

        @Override
        public void visitCode() {
            super.visitCode();
            Label proceed = new Label();
            super.visitVarInsn(Opcodes.ILOAD, firstSlot);
            super.visitVarInsn(Opcodes.ILOAD, secondSlot);
            super.visitMethodInsn(Opcodes.INVOKESTATIC, HOOKS, hook, "(II)Z", false);
            super.visitJumpInsn(Opcodes.IFEQ, proceed);
            super.visitInsn(returnCode);
            super.visitLabel(proceed);
        }
    }

    /**
     * Вставляет в начало метода условие "если хук сказал да - выходим сразу".
     *
     * Годится только для методов, ничего не возвращающих.
     */
    private static final class GuardInjector extends MethodVisitor {

        private final String hook;

        GuardInjector(MethodVisitor parent, String hook) {
            super(Opcodes.ASM9, parent);
            this.hook = hook;
        }

        @Override
        public void visitCode() {
            super.visitCode();
            Label proceed = new Label();
            super.visitMethodInsn(Opcodes.INVOKESTATIC, HOOKS, hook, "()Z", false);
            super.visitJumpInsn(Opcodes.IFEQ, proceed);
            super.visitInsn(Opcodes.RETURN);
            super.visitLabel(proceed);
        }
    }

    /**
     * Забирает прокрутку колеса себе, пока открыто меню.
     *
     * Подпись обработчика: onScroll(long window, double xOffset, double yOffset). Метод
     * обычный, поэтому ячейка 0 - this, 1 и 2 - указатель окна, 3 и 4 - горизонталь,
     * 5 и 6 - вертикаль. Нужна именно вертикаль: ею листают списки.
     */
    private static final class ScrollInjector extends MethodVisitor {

        ScrollInjector(MethodVisitor parent) {
            super(Opcodes.ASM9, parent);
        }

        @Override
        public void visitCode() {
            super.visitCode();
            Label proceed = new Label();
            super.visitVarInsn(Opcodes.DLOAD, 5);
            super.visitMethodInsn(Opcodes.INVOKESTATIC, HOOKS, "onScroll", "(D)Z", false);
            super.visitJumpInsn(Opcodes.IFEQ, proceed);
            super.visitInsn(Opcodes.RETURN);
            super.visitLabel(proceed);
        }
    }

    /** Вставляет вызов хука перед каждым выходом из метода. */
    private static final class ReturnCallInjector extends MethodVisitor {

        private final String hook;

        ReturnCallInjector(MethodVisitor parent, String hook) {
            super(Opcodes.ASM9, parent);
            this.hook = hook;
        }

        @Override
        public void visitInsn(int opcode) {
            if (opcode >= Opcodes.IRETURN && opcode <= Opcodes.RETURN) {
                super.visitMethodInsn(Opcodes.INVOKESTATIC, HOOKS, hook, "()V", false);
            }
            super.visitInsn(opcode);
        }
    }

    /**
     * Вставляет вызов отрисовки перед выходом из render(GuiGraphics, float),
     * передавая в хук оба аргумента метода.
     *
     * Метод обычный, поэтому ячейка 0 - это this, 1 - контекст рисования,
     * 2 - доля тика. Вставляемся в конце, чтобы рисовать поверх игрового
     * интерфейса, а не под ним.
     */
    private static final class HudCallInjector extends MethodVisitor {

        HudCallInjector(MethodVisitor parent) {
            super(Opcodes.ASM9, parent);
        }

        @Override
        public void visitInsn(int opcode) {
            if (opcode == Opcodes.RETURN) {
                super.visitVarInsn(Opcodes.ALOAD, 1);
                super.visitVarInsn(Opcodes.FLOAD, 2);
                super.visitMethodInsn(
                        Opcodes.INVOKESTATIC,
                        HOOKS,
                        "onRenderHud",
                        "(Ljava/lang/Object;F)V",
                        false
                );
            }
            super.visitInsn(opcode);
        }
    }

    /** Короткая сводка для лога при старте. */
    public String summary() {
        return "tick=" + patchedTick
                + ", start=" + patchedStart
                + ", hud=" + patchedHud
                + ", mouse=" + patchedMouse
                + ", scroll=" + patchedScroll
                + ", crosshair=" + patchedCrosshair
                + ", attack=" + patchedAttack
                + ", fov=" + patchedFov
                + ", key=" + patchedKey;
    }

    public MappingTable mappings() {
        return mappings;
    }
}
