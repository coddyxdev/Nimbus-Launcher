package dev.nimbus.runtime.mappings;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Разбор официальных маппингов Mojang (формат ProGuard).
 *
 * <pre>
 * net.minecraft.client.Minecraft -&gt; dsm:
 *     net.minecraft.client.Minecraft instance -&gt; F
 *     1234:1240:void runTick(boolean) -&gt; a
 * </pre>
 *
 * Слева читаемое имя, справа обфусцированное — то, что реально лежит в jar.
 */
public final class ProGuardMappings {

    private ProGuardMappings() {
    }

    public static MappingTable parse(Path file, String gameVersion) throws IOException {
        List<String> lines = Files.readAllLines(file, StandardCharsets.UTF_8);
        return parse(lines, gameVersion);
    }

    public static MappingTable parse(List<String> lines, String gameVersion) {
        Map<String, String> classToObf = new HashMap<>();
        Map<String, String> classToDeobf = new HashMap<>();

        // Первый проход: только классы. Они нужны, чтобы собрать дескрипторы.
        for (String raw : lines) {
            if (raw.isEmpty() || raw.charAt(0) == '#' || isMemberLine(raw)) {
                continue;
            }
            String line = raw.trim();
            int arrow = line.indexOf(" -> ");
            if (arrow < 0 || !line.endsWith(":")) {
                continue;
            }
            String deobf = line.substring(0, arrow).trim();
            String obf = line.substring(arrow + 4, line.length() - 1).trim();
            String obfInternal = obf.replace('.', '/');
            classToObf.put(deobf, obfInternal);
            classToDeobf.put(obfInternal, deobf);
        }

        Map<String, MappingTable.Member> methods = new HashMap<>();
        Map<String, MappingTable.Member> fields = new HashMap<>();
        String currentClass = null;

        // Второй проход: члены классов.
        for (String raw : lines) {
            if (raw.isEmpty() || raw.charAt(0) == '#') {
                continue;
            }
            if (!isMemberLine(raw)) {
                String line = raw.trim();
                int arrow = line.indexOf(" -> ");
                currentClass = (arrow >= 0 && line.endsWith(":"))
                        ? line.substring(0, arrow).trim()
                        : null;
                continue;
            }
            if (currentClass == null) {
                continue;
            }

            String line = raw.trim();
            int arrow = line.indexOf(" -> ");
            if (arrow < 0) {
                continue;
            }
            String left = stripLineNumbers(line.substring(0, arrow).trim());
            String obfName = line.substring(arrow + 4).trim();

            int paren = left.indexOf('(');
            if (paren < 0) {
                // Поле: "<тип> <имя>"
                int space = left.indexOf(' ');
                if (space < 0) {
                    continue;
                }
                String type = left.substring(0, space).trim();
                String name = left.substring(space + 1).trim();
                fields.put(
                        MappingTable.fieldKey(currentClass, name),
                        new MappingTable.Member(obfName, descriptorOf(type, classToObf))
                );
                continue;
            }

            // Метод: "<возврат> <имя>(<аргументы>)"
            int space = left.indexOf(' ');
            if (space < 0 || space > paren) {
                continue;
            }
            String returnType = left.substring(0, space).trim();
            String name = left.substring(space + 1, paren).trim();
            String argsRaw = left.substring(paren + 1, left.lastIndexOf(')')).trim();
            String[] argTypes = argsRaw.isEmpty() ? new String[0] : splitArgs(argsRaw);

            StringBuilder descriptor = new StringBuilder("(");
            for (String argType : argTypes) {
                descriptor.append(descriptorOf(argType, classToObf));
            }
            descriptor.append(')').append(descriptorOf(returnType, classToObf));

            methods.put(
                    MappingTable.methodKey(currentClass, name, argTypes),
                    new MappingTable.Member(obfName, descriptor.toString())
            );
        }

        return new MappingTable(gameVersion, classToObf, classToDeobf, methods, fields);
    }

    private static boolean isMemberLine(String raw) {
        char first = raw.charAt(0);
        return first == ' ' || first == '\t';
    }

    /** Убирает префикс вида "1234:1240:". */
    private static String stripLineNumbers(String value) {
        int index = 0;
        while (true) {
            int scan = index;
            while (scan < value.length() && Character.isDigit(value.charAt(scan))) {
                scan++;
            }
            if (scan > index && scan < value.length() && value.charAt(scan) == ':') {
                index = scan + 1;
            } else {
                break;
            }
        }
        return value.substring(index);
    }

    private static String[] splitArgs(String argsRaw) {
        List<String> parts = new ArrayList<>(4);
        for (String part : argsRaw.split(",")) {
            String trimmed = part.trim();
            if (!trimmed.isEmpty()) {
                parts.add(trimmed);
            }
        }
        return parts.toArray(new String[0]);
    }

    /** Читаемый тип (int, java.lang.String, net.minecraft.X[]) -> JVM-дескриптор в обфусцированных именах. */
    static String descriptorOf(String type, Map<String, String> classToObf) {
        int arrayDepth = 0;
        String base = type.trim();
        while (base.endsWith("[]")) {
            arrayDepth++;
            base = base.substring(0, base.length() - 2).trim();
        }

        String descriptor = switch (base) {
            case "void" -> "V";
            case "boolean" -> "Z";
            case "byte" -> "B";
            case "char" -> "C";
            case "short" -> "S";
            case "int" -> "I";
            case "long" -> "J";
            case "float" -> "F";
            case "double" -> "D";
            default -> {
                String obf = classToObf.get(base);
                yield "L" + (obf != null ? obf : base.replace('.', '/')) + ";";
            }
        };

        return "[".repeat(arrayDepth) + descriptor;
    }
}
