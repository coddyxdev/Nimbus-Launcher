package dev.nimbus.runtime.transform;

import dev.nimbus.runtime.Log;
import dev.nimbus.runtime.mappings.MappingTable;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassVisitor;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.MethodVisitor;
import org.objectweb.asm.Opcodes;

import java.lang.instrument.ClassFileTransformer;
import java.security.ProtectionDomain;

/**
 * Правка байт-кода игры на лету.
 *
 * Сейчас три точки входа:
 * в классе Minecraft — старт игры и игровой тик,
 * в классе Gui — конец отрисовки игрового интерфейса.
 *
 * Место для отрисовки выбрано именно там, а не в GameRenderer: к этому моменту
 * игра уже настроила плоскую проекцию и передаёт готовый контекст рисования,
 * так что нам не нужно руками возиться с матрицами и состоянием OpenGL.
 */
public final class NimbusTransformer implements ClassFileTransformer {

    private static final String HOOKS = "dev/nimbus/runtime/NimbusHooks";
    private static final String MINECRAFT = "net.minecraft.client.Minecraft";
    private static final String GUI = "net.minecraft.client.gui.Gui";
    private static final String GUI_GRAPHICS = "net.minecraft.client.gui.GuiGraphics";

    private final MappingTable mappings;

    /** Внутренние имена классов игры в запущенной версии. */
    private final String minecraftClass;
    private final String guiClass;

    private final MappingTable.Member runTick;
    private final MappingTable.Member constructor;
    private final MappingTable.Member guiRender;

    private boolean patchedTick;
    private boolean patchedStart;
    private boolean patchedHud;

    public NimbusTransformer(MappingTable mappings) {
        this.mappings = mappings;
        this.minecraftClass = mappings.obfClass(MINECRAFT);
        this.guiClass = mappings.obfClass(GUI);
        this.runTick = mappings.method(MINECRAFT, "runTick", "boolean");
        this.constructor = mappings.method(MINECRAFT, "<init>", "net.minecraft.client.main.GameConfig");
        this.guiRender = mappings.method(GUI, "render", GUI_GRAPHICS, "float");

        Log.debug("класс Minecraft: " + minecraftClass
                + ", runTick: " + describe(runTick)
                + ", <init>: " + describe(constructor));
        Log.debug("класс Gui: " + guiClass + ", render: " + describe(guiRender));

        if (guiRender == null) {
            // Не ошибка: старые и будущие версии могут звать этот метод иначе.
            // Игра запустится, просто без нашего слоя.
            Log.warn("метод отрисовки интерфейса не найден в маппингах");
        }
    }

    private static String describe(MappingTable.Member member) {
        return member == null ? "не найден" : member.name() + member.descriptor();
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
            if (guiRender != null && className.equals(guiClass)) {
                return patchGui(classfileBuffer, loader);
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
                if (matches(constructor, name, descriptor) || "<init>".equals(name)) {
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
                return parent;
            }
        };

        // EXPAND_FRAMES обязателен в паре с COMPUTE_FRAMES.
        reader.accept(visitor, ClassReader.EXPAND_FRAMES);
        Log.debug("класс Gui пропатчен (hud=" + patchedHud + ")");
        return writer.toByteArray();
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
     * Метод обычный, поэтому ячейка 0 — это this, 1 — контекст рисования,
     * 2 — доля тика. Вставляемся в конце, чтобы рисовать поверх игрового
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
        return "tick=" + patchedTick + ", start=" + patchedStart + ", hud=" + patchedHud;
    }

    public MappingTable mappings() {
        return mappings;
    }
}
