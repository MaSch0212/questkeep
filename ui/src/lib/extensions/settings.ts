import { createSignal, type Accessor } from "solid-js";
import { coerceValue, type DeclaredSetting } from "./declaredSettings";

/**
 * One plugin's settings: what is stored, the declared defaults, and who listens. The extension
 * (`api.settings`) and the settings page both change values through it, so listeners hear both.
 */
export interface SettingsController {
  readonly declared: readonly DeclaredSetting[];
  /** The stored values (without defaults); a signal, so the settings page follows changes. */
  stored: Accessor<Readonly<Record<string, unknown>>>;
  /** The stored value (if it fits a declared type), else the declared default, else `fallback`. */
  get(key: string, fallback?: unknown): unknown;
  /** Declared defaults overlaid with the stored values. */
  all(): Record<string, unknown>;
  /** Whether a value is stored for the key (else its default shows). */
  isSet(key: string): boolean;
  /** Stores a value and saves; `undefined` removes the key. Rejects when saving fails. */
  set(key: string, value: unknown): Promise<void>;
  onChange(listener: (key: string, value: unknown) => void): () => void;
}

export function createSettingsController(options: {
  declared: readonly DeclaredSetting[];
  initial: Record<string, unknown> | null | undefined;
  /** Saves the whole object. */
  save: (settings: Record<string, unknown>) => Promise<unknown> | unknown;
  /** A listener threw. */
  onListenerError?: (error: unknown) => void;
}): SettingsController {
  const declared = new Map(options.declared.map((setting) => [setting.key, setting]));
  const initial =
    typeof options.initial === "object" && options.initial !== null ? options.initial : {};
  const [stored, setStored] = createSignal<Readonly<Record<string, unknown>>>({ ...initial });
  const listeners = new Set<(key: string, value: unknown) => void>();
  // One save at a time, in order, so an older object never overwrites a newer one.
  let saving: Promise<unknown> = Promise.resolve();

  function get(key: string, fallback?: unknown): unknown {
    const setting = declared.get(key);
    const values = stored();
    if (Object.hasOwn(values, key)) {
      const value = setting ? coerceValue(setting, values[key]) : values[key];
      if (value !== undefined) return value;
    }
    return setting ? setting.default : fallback;
  }

  function set(key: string, value: unknown): Promise<void> {
    const { [key]: _old, ...rest } = stored();
    const next = value === undefined ? rest : { ...rest, [key]: value };
    setStored(next);
    const current = get(key);
    // A copy: a listener may add or remove listeners.
    for (const listener of Array.from(listeners)) {
      try {
        listener(key, current);
      } catch (error) {
        options.onListenerError?.(error);
      }
    }
    const done = saving.then(() => options.save(next));
    saving = done.catch(() => undefined);
    return done.then(() => undefined);
  }

  return {
    declared: options.declared,
    stored,
    get,
    all: () => {
      const values: Record<string, unknown> = { ...stored() };
      for (const key of declared.keys()) values[key] = get(key);
      return values;
    },
    isSet: (key) => Object.hasOwn(stored(), key),
    set,
    onChange: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
