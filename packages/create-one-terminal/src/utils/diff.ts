export function unifiedDiff(label: string, original: string, updated: string): string {
  const a = original.split("\n");
  const b = updated.split("\n");
  const lines: string[] = [`--- ${label} (yours)`, `+++ ${label} (framework)`];
  const maxLen = Math.max(a.length, b.length);
  for (let i = 0; i < maxLen; i++) {
    if (a[i] !== b[i]) {
      if (a[i] !== undefined) lines.push(`- ${a[i]}`);
      if (b[i] !== undefined) lines.push(`+ ${b[i]}`);
    } else {
      lines.push(`  ${a[i]}`);
    }
  }
  return lines.join("\n");
}

export function conflictMarkers(label: string, yours: string, framework: string): string {
  return `<<<<<<< yours\n${yours}\n=======\n${framework}\n>>>>>>> framework ${label}`;
}
