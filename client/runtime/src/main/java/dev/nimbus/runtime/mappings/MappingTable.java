package dev.nimbus.runtime.mappings;

import java.util.Collections;
import java.util.HashMap;
import java.util.Map;

/**
 * Таблица имён для одной версии игры.
 *
 * Код клиента всегда оперирует читаемыми именами (net.minecraft.client.Minecraft),
 * а таблица переводит их в реальные обфусцированные имена запущенной версии.
 */
public final class MappingTable {

    /** Имя и дескриптор члена класса в обфусцированном виде. */
    public record Member(String name, String descriptor) {
    }

    private final String gameVersion;
    private final Map<String, String> classToObf;
    private final Map<String, String> classToDeobf;
    private final Map<String, Member> methods;
    private final Map<String, Member> fields;

    MappingTable(
            String gameVersion,
            Map<String, String> classToObf,
            Map<String, String> classToDeobf,
            Map<String, Member> methods,
            Map<String, Member> fields
    ) {
        this.gameVersion = gameVersion;
        this.classToObf = Collections.unmodifiableMap(new HashMap<>(classToObf));
        this.classToDeobf = Collections.unmodifiableMap(new HashMap<>(classToDeobf));
        this.methods = Collections.unmodifiableMap(new HashMap<>(methods));
        this.fields = Collections.unmodifiableMap(new HashMap<>(fields));
    }

    /** Пустая таблица: имена возвращаются как есть (для деобфусцированных сборок). */
    public static MappingTable identity(String gameVersion) {
        return new MappingTable(gameVersion, Map.of(), Map.of(), Map.of(), Map.of());
    }

    public String gameVersion() {
        return gameVersion;
    }

    public boolean isEmpty() {
        return classToObf.isEmpty();
    }

    public int classCount() {
        return classToObf.size();
    }

    public int methodCount() {
        return methods.size();
    }

    /** Читаемое имя класса -> внутреннее обфусцированное (a/b/c). */
    public String obfClass(String deobfName) {
        String obf = classToObf.get(deobfName);
        return obf != null ? obf : deobfName.replace('.', '/');
    }

    /** Внутреннее обфусцированное имя -> читаемое, или null. */
    public String deobfClass(String internalObfName) {
        return classToDeobf.get(internalObfName);
    }

    /** Есть ли такой класс в таблице. */
    public boolean knowsClass(String deobfName) {
        return classToObf.containsKey(deobfName);
    }

    /**
     * Метод по читаемой сигнатуре.
     *
     * @param deobfClass например net.minecraft.client.Minecraft
     * @param name       например runTick
     * @param argTypes   читаемые типы аргументов, например "boolean"
     */
    public Member method(String deobfClass, String name, String... argTypes) {
        return methods.get(methodKey(deobfClass, name, argTypes));
    }

    public Member field(String deobfClass, String name) {
        return fields.get(deobfClass + "#" + name);
    }

    static String methodKey(String deobfClass, String name, String... argTypes) {
        return deobfClass + "#" + name + "(" + String.join(",", argTypes) + ")";
    }

    static String fieldKey(String deobfClass, String name) {
        return deobfClass + "#" + name;
    }
}
