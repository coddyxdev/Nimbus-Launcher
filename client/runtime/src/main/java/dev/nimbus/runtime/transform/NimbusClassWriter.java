package dev.nimbus.runtime.transform;

import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassWriter;

/**
 * ClassWriter, умеющий пересчитывать карты кадров для классов игры.
 *
 * При пересчёте ASM иногда нужно знать общего предка двух типов. Базовая
 * реализация ищет его через загрузчик самого ASM, а там классов игры нет —
 * вместо патча получился бы ClassNotFoundException прямо во время запуска.
 *
 * Поэтому ищем в загрузчике того класса, который правим, а если не вышло —
 * честно возвращаем Object. Это всегда верный ответ (хоть и не самый точный),
 * и он гораздо лучше падения игры.
 */
final class NimbusClassWriter extends ClassWriter {

    private final ClassLoader loader;

    NimbusClassWriter(ClassReader reader, int flags, ClassLoader loader) {
        super(reader, flags);
        this.loader = loader != null ? loader : NimbusClassWriter.class.getClassLoader();
    }

    @Override
    protected String getCommonSuperClass(String type1, String type2) {
        if (type1.equals(type2)) {
            return type1;
        }
        try {
            Class<?> first = Class.forName(type1.replace('/', '.'), false, loader);
            Class<?> second = Class.forName(type2.replace('/', '.'), false, loader);

            if (first.isAssignableFrom(second)) {
                return type1;
            }
            if (second.isAssignableFrom(first)) {
                return type2;
            }
            if (first.isInterface() || second.isInterface()) {
                return "java/lang/Object";
            }

            Class<?> parent = first;
            do {
                parent = parent.getSuperclass();
                if (parent == null) {
                    return "java/lang/Object";
                }
            } while (!parent.isAssignableFrom(second));

            return parent.getName().replace('.', '/');
        } catch (Throwable error) {
            return "java/lang/Object";
        }
    }
}
