// The 8 standard FDC3 2.2 system channels with their display colours.
// Shared between the tab strip (per-tab dot), the tab context menu (per-tab
// "Set channel" submenu), and the Rust side's `SYSTEM_CHANNEL_IDS` allowlist
// in `layout/commands.rs` — keep ids in sync across all three.

export interface FdcChannel {
  id: string;
  name: string;
  color: string;
}

export const FDC3_CHANNELS: FdcChannel[] = [
  { id: "fdc3.channel.1", name: "Channel 1", color: "#e11d48" },
  { id: "fdc3.channel.2", name: "Channel 2", color: "#ea580c" },
  { id: "fdc3.channel.3", name: "Channel 3", color: "#ca8a04" },
  { id: "fdc3.channel.4", name: "Channel 4", color: "#16a34a" },
  { id: "fdc3.channel.5", name: "Channel 5", color: "#0891b2" },
  { id: "fdc3.channel.6", name: "Channel 6", color: "#2563eb" },
  { id: "fdc3.channel.7", name: "Channel 7", color: "#7c3aed" },
  { id: "fdc3.channel.8", name: "Channel 8", color: "#db2777" },
];
