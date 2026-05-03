export function getManagedPaths(config: Record<string, unknown>): string[] {
  return (config["_managed"] as string[] | undefined) ?? [];
}

export function setManagedPaths(config: Record<string, unknown>, paths: string[]): void {
  config["_managed"] = paths;
}

export function addManagedPath(config: Record<string, unknown>, path: string): void {
  const current = getManagedPaths(config);
  if (!current.includes(path)) {
    setManagedPaths(config, [...current, path]);
  }
}

export function getNestedValue(
  obj: Record<string, unknown>,
  dotPath: string,
): unknown {
  return dotPath.split(".").reduce<unknown>((cur, key) => {
    if (cur && typeof cur === "object") return (cur as Record<string, unknown>)[key];
    return undefined;
  }, obj);
}

export function setNestedValue(
  obj: Record<string, unknown>,
  dotPath: string,
  value: unknown,
): void {
  const keys = dotPath.split(".");
  let cur: Record<string, unknown> = obj;
  for (let i = 0; i < keys.length - 1; i++) {
    if (typeof cur[keys[i]] !== "object" || cur[keys[i]] === null) {
      cur[keys[i]] = {};
    }
    cur = cur[keys[i]] as Record<string, unknown>;
  }
  cur[keys[keys.length - 1]] = value;
}
