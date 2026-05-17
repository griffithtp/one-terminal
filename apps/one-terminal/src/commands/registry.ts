export type CommandGroup = "widgets" | "navigation" | "apps" | "settings" | "widget-instances";

export interface Command {
  id: string;
  label: string;
  keywords: string[];
  /** Human-readable shortcut hint shown in the palette (e.g. "⌘K"). */
  shortcut?: string;
  /** Normalised keybinding string used for lookup (e.g. "CmdOrCtrl+K"). */
  keybinding?: string;
  group: CommandGroup;
  /** For widget-instances commands: the webview label of the target panel. Used
   *  by the palette to drive the highlight ring while the result is focused. */
  widgetLabel?: string;
  action: () => void | Promise<void>;
  /** When false the command is hidden from results. Defaults to true. */
  isAvailable?: () => boolean;
}

// ── Bigram fuzzy search ───────────────────────────────────────────────────────

function bigrams(s: string): Set<string> {
  const result = new Set<string>();
  for (let i = 0; i < s.length - 1; i++) result.add(s.slice(i, i + 2));
  return result;
}

function bigramScore(query: string, haystack: string): number {
  const qBg = bigrams(query);
  const hBg = bigrams(haystack);
  let hits = 0;
  for (const bg of qBg) if (hBg.has(bg)) hits++;
  return hits;
}

// Group display order for empty-query sorting (lower index = shown first).
const GROUP_ORDER: CommandGroup[] = [
  "navigation",
  "widgets",
  "settings",
  "apps",
  "widget-instances",
];

// ── Registry ─────────────────────────────────────────────────────────────────

class CommandRegistry {
  private readonly cmds = new Map<string, Command>();
  private readonly byKeybinding = new Map<string, string>(); // keybinding → id

  register(cmd: Command): void {
    const existing = this.cmds.get(cmd.id);
    if (existing?.keybinding) this.byKeybinding.delete(existing.keybinding);
    this.cmds.set(cmd.id, cmd);
    if (cmd.keybinding) this.byKeybinding.set(cmd.keybinding, cmd.id);
  }

  unregister(id: string): void {
    const cmd = this.cmds.get(id);
    if (!cmd) return;
    if (cmd.keybinding) this.byKeybinding.delete(cmd.keybinding);
    this.cmds.delete(id);
  }

  async execute(id: string): Promise<void> {
    const cmd = this.cmds.get(id);
    if (cmd?.isAvailable?.() !== false) await cmd?.action();
  }

  search(query: string): Command[] {
    const available = Array.from(this.cmds.values()).filter((c) => c.isAvailable?.() !== false);
    const q = query.trim().toLowerCase();
    if (!q) {
      // No query — return up to 10 commands sorted by group priority.
      return available
        .sort((a, b) => GROUP_ORDER.indexOf(a.group) - GROUP_ORDER.indexOf(b.group))
        .slice(0, 10);
    }
    return available
      .map((cmd) => ({
        cmd,
        score: bigramScore(q, `${cmd.label} ${cmd.keywords.join(" ")}`.toLowerCase()),
      }))
      .filter((x) => x.score > 0)
      .sort((a, b) => b.score - a.score)
      .slice(0, 10)
      .map((x) => x.cmd);
  }

  findByKeybinding(kb: string): Command | undefined {
    const id = this.byKeybinding.get(kb);
    return id ? this.cmds.get(id) : undefined;
  }

  getAll(): Command[] {
    return Array.from(this.cmds.values());
  }
}

export const registry = new CommandRegistry();
