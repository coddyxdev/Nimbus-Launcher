package dev.nimbus.runtime;

import dev.nimbus.bridge.Mappings;
import dev.nimbus.runtime.mappings.MappingTable;

/**
 * Переходник от таблицы имён рантайма к узкому интерфейсу для адаптеров.
 *
 * Адаптеры не должны знать ни про ASM, ни про формат файла маппингов,
 * поэтому таблица проходит к ним через эту обёртку.
 */
public final class MappingsBridge implements Mappings {

    private final MappingTable table;

    public MappingsBridge(MappingTable table) {
        this.table = table;
    }

    @Override
    public String className(String deobfName) {
        String internal = table.obfClass(deobfName);
        return internal == null ? deobfName : internal.replace('/', '.');
    }

    @Override
    public String methodName(String deobfClass, String name, String... argTypes) {
        MappingTable.Member member = table.method(deobfClass, name, argTypes);
        return member == null ? null : member.name();
    }

    @Override
    public String fieldName(String deobfClass, String name) {
        MappingTable.Member member = table.field(deobfClass, name);
        return member == null ? null : member.name();
    }
}
