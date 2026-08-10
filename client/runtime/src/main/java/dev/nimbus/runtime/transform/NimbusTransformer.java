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
 * Первый этап: два хука в классе Minecraft — старт игры и игровой тик.
 * Всё остальное строится уже на них.
 */
public final class NimbusTransformer implements ClassFileTransformer {

    private static final String HOOKS = "dev/nimbus/runtime/NimbusHooks";
    private static final String MINECRAFT = "net.minecraft.client.Minecraft";

    private final MappingTable mappings;

    /** Внутреннее имя класса Minecraft в запущенной версии. */
    private final String minecraftClass;
    private final MappingTable.Member runTick;
    private final MappingTable.Member constructor;

    private boolean patchedTick;
    private boolean patchedStart;

    public NimbusTransformer(MappingTable mappings) {
        this.mappings = mappings;
        this.minecraftClass = mappings.obfClass(MINECRAFT);
        this.runTick = mappings.method(MINECRAFT, "runTick", "boolean");
        this.constructor = mappings.method(MINECRAFT, "<init>", "net.minecraft.client.main.GameConfig");

        Log.debug("класс Minecraft: " + minecraftClass
                + ", runTick: " + describe(runTick)
                + ", <init>: " + describe(constructor));
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

    @Override
    public byte[] transform(
            ClassLoader loader,
            String className,
            Class<?> classBeingRedefined,
            ProtectionDomain protectionDomain,
            byte[] classfileBuffer
    ) {
        if (className == null || !className.equals(minecraftClass)) {
            return null;
        }
        try {
            return patchMinecraft(classfileBuffer);
        } catch (Throwable error) {
            // Сломанный трансформер не должен мешать игре запуститься.
            Log.error("не удалось править " + className + ", игра запустится без клиента", error);
            return null;
        }
    }

    private byte[] patchMinecraft(byte[] original) {
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
}
