package dev.nimbus.runtime;

import java.util.HashMap;
import java.util.Map;

/**
 * Аргументы агента в виде key=value,key=value.
 *
 * Лаунчер передаёт их в строке запуска:
 * -javaagent:nimbus-runtime.jar=version=1.20.1,mappings=C:\...\1.20.1.txt,debug=true
 */
public final class AgentArgs {

    private final Map<String, String> values = new HashMap<>();

    private AgentArgs() {
    }

    public static AgentArgs parse(String raw) {
        AgentArgs args = new AgentArgs();
        if (raw == null || raw.isBlank()) {
            return args;
        }
        for (String pair : raw.split(",")) {
            String trimmed = pair.trim();
            if (trimmed.isEmpty()) {
                continue;
            }
            int eq = trimmed.indexOf('=');
            if (eq <= 0) {
                args.values.put(trimmed, "true");
            } else {
                args.values.put(
                        trimmed.substring(0, eq).trim(),
                        trimmed.substring(eq + 1).trim()
                );
            }
        }
        return args;
    }

    public String get(String key, String fallback) {
        String value = values.get(key);
        if (value == null || value.isBlank()) {
            String property = System.getProperty("nimbus." + key);
            return (property == null || property.isBlank()) ? fallback : property;
        }
        return value;
    }

    public boolean flag(String key) {
        return Boolean.parseBoolean(get(key, "false"));
    }

    @Override
    public String toString() {
        return values.toString();
    }
}
