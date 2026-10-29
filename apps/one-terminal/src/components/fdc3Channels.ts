// The 8 FDC3 user channels with their display colours. Ids, names and colours
// mirror the Desktop Agent's default `userChannels` (`AgentConfig::default_channels`
// in apps/desktop-agent/src-tauri/src/config.rs) — a join with any other id is
// rejected by the agent with CHANNEL_NOT_FOUND.
// Shared between the tab strip (per-tab dot), the tab context menu (per-tab
// "Set channel" submenu), and the Rust side's `SYSTEM_CHANNEL_IDS` allowlist
// in `layout/commands.rs` — keep ids in sync across all three.

export interface FdcChannel {
  id: string;
  name: string;
  color: string;
}

export const FDC3_CHANNELS: FdcChannel[] = [
  { id: "Red", name: "Red", color: "#E74C3C" },
  { id: "Orange", name: "Orange", color: "#E67E22" },
  { id: "Yellow", name: "Yellow", color: "#F1C40F" },
  { id: "Green", name: "Green", color: "#2ECC71" },
  { id: "Teal", name: "Teal", color: "#1ABC9C" },
  { id: "Blue", name: "Blue", color: "#3498DB" },
  { id: "Purple", name: "Purple", color: "#9B59B6" },
  { id: "Pink", name: "Pink", color: "#EC407A" },
];
