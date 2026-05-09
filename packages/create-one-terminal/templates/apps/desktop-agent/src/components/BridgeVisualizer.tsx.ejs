import { useEffect, useRef, useCallback, useState } from "react";
import type { WindowHandle, PeerInfo } from "../types";

// ── Physics constants ──────────────────────────────────────────────────────────

const REPULSION = 6000; // Coulomb constant
const SPRING_K = 0.04; // Hooke spring stiffness
const SPRING_REST = 130; // natural spring length (px)
const GRAVITY = 0.03; // pull toward center
const DAMPING = 0.82; // velocity damping per frame
const MIN_DIST = 25; // avoid division by zero

// ── Node type ─────────────────────────────────────────────────────────────────

type NodeKind = "hub" | "spoke" | "peer";

interface PhysicsNode {
  id: string;
  label: string;
  sublabel?: string;
  kind: NodeKind;
  x: number;
  y: number;
  vx: number;
  vy: number;
  pinned: boolean;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function color(kind: NodeKind): string {
  if (kind === "hub") return "#388bfd";
  if (kind === "peer") return "#f0a040";
  return "#3fb950";
}

function radius(kind: NodeKind): number {
  if (kind === "hub") return 26;
  if (kind === "peer") return 18;
  return 14;
}

// ── Component ─────────────────────────────────────────────────────────────────

interface Props {
  connections: WindowHandle[];
  peers: PeerInfo[];
}

export function BridgeVisualizer({ connections, peers }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const nodesRef = useRef<PhysicsNode[]>([]);
  const frameRef = useRef<number>(0);
  const dragRef = useRef<{ id: string; ox: number; oy: number } | null>(null);
  const [renderKey, setRenderKey] = useState(0);

  // Sync physics nodes when data changes — preserve positions for existing nodes.
  useEffect(() => {
    const svg = svgRef.current;
    const W = svg?.clientWidth ?? 600;
    const H = svg?.clientHeight ?? 400;
    const cx = W / 2;
    const cy = H / 2;

    const prev = new Map(nodesRef.current.map((n) => [n.id, n]));

    const next: PhysicsNode[] = [];

    // Hub node (always present, pinned at center)
    const hub = prev.get("__hub__") ?? {
      id: "__hub__",
      label: "CDA",
      sublabel: "hub",
      kind: "hub" as NodeKind,
      x: cx,
      y: cy,
      vx: 0,
      vy: 0,
      pinned: true,
    };
    hub.pinned = true;
    hub.x = cx;
    hub.y = cy;
    next.push(hub);

    // Local spoke nodes
    connections.forEach((c, i) => {
      const existing = prev.get(c.instanceId);
      const angle = (i / Math.max(connections.length, 1)) * 2 * Math.PI;
      next.push(
        existing ?? {
          id: c.instanceId,
          label: c.displayName ?? c.appId,
          sublabel: c.currentChannel,
          kind: "spoke",
          x: cx + Math.cos(angle) * 120,
          y: cy + Math.sin(angle) * 120,
          vx: 0,
          vy: 0,
          pinned: false,
        }
      );
      if (existing) {
        existing.label = c.displayName ?? c.appId;
        existing.sublabel = c.currentChannel;
      }
    });

    // External peer nodes
    peers.forEach((p, i) => {
      const existing = prev.get(p.desktopAgentId);
      const angle = (i / Math.max(peers.length, 1)) * 2 * Math.PI + Math.PI / 6;
      next.push(
        existing ?? {
          id: p.desktopAgentId,
          label: p.desktopAgentId,
          sublabel: p.url,
          kind: "peer",
          x: cx + Math.cos(angle) * 190,
          y: cy + Math.sin(angle) * 190,
          vx: 0,
          vy: 0,
          pinned: false,
        }
      );
      if (existing) {
        existing.label = p.desktopAgentId;
        existing.sublabel = p.url;
      }
    });

    nodesRef.current = next;
  }, [connections, peers]);

  // Animation loop — Verlet integration with Coulomb + spring + gravity forces.
  useEffect(() => {
    function tick() {
      const nodes = nodesRef.current;
      const svg = svgRef.current;
      if (!svg) {
        frameRef.current = requestAnimationFrame(tick);
        return;
      }
      const W = svg.clientWidth || 600;
      const H = svg.clientHeight || 400;
      const cx = W / 2;
      const cy = H / 2;

      for (let i = 0; i < nodes.length; i++) {
        const n = nodes[i];
        if (n.pinned) continue;

        let fx = 0,
          fy = 0;

        // Coulomb repulsion from all other nodes
        for (let j = 0; j < nodes.length; j++) {
          if (i === j) continue;
          const m = nodes[j];
          const dx = n.x - m.x;
          const dy = n.y - m.y;
          const d2 = dx * dx + dy * dy;
          const d = Math.max(Math.sqrt(d2), MIN_DIST);
          fx += (REPULSION / d2) * (dx / d);
          fy += (REPULSION / d2) * (dy / d);
        }

        // Hooke spring toward hub (all nodes are "connected" to hub)
        const hub = nodes[0];
        const sx = hub.x - n.x;
        const sy = hub.y - n.y;
        const sd = Math.max(Math.sqrt(sx * sx + sy * sy), MIN_DIST);
        const stretch = sd - SPRING_REST;
        fx += SPRING_K * stretch * (sx / sd);
        fy += SPRING_K * stretch * (sy / sd);

        // Gravity toward canvas center
        fx += GRAVITY * (cx - n.x);
        fy += GRAVITY * (cy - n.y);

        // Integrate
        n.vx = (n.vx + fx) * DAMPING;
        n.vy = (n.vy + fy) * DAMPING;

        // Drag override
        if (dragRef.current?.id === n.id) {
          n.vx = 0;
          n.vy = 0;
        } else {
          n.x += n.vx;
          n.y += n.vy;
        }

        // Clamp inside SVG with padding
        const pad = radius(n.kind) + 4;
        n.x = Math.max(pad, Math.min(W - pad, n.x));
        n.y = Math.max(pad, Math.min(H - pad, n.y));
      }

      // Throttle React re-render to every other frame
      setRenderKey((k) => k + 1);
      frameRef.current = requestAnimationFrame(tick);
    }

    frameRef.current = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frameRef.current);
  }, []);

  // Drag handlers
  const onMouseDown = useCallback((e: React.MouseEvent, id: string) => {
    e.preventDefault();
    const svg = svgRef.current!;
    const rect = svg.getBoundingClientRect();
    dragRef.current = { id, ox: e.clientX - rect.left, oy: e.clientY - rect.top };
  }, []);

  useEffect(() => {
    function onMove(e: MouseEvent) {
      const d = dragRef.current;
      if (!d) return;
      const svg = svgRef.current!;
      const rect = svg.getBoundingClientRect();
      const nx = e.clientX - rect.left;
      const ny = e.clientY - rect.top;
      const node = nodesRef.current.find((n) => n.id === d.id);
      if (node) {
        node.x = nx;
        node.y = ny;
      }
    }
    function onUp() {
      dragRef.current = null;
    }
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

  const nodes = nodesRef.current;
  void renderKey; // consumed to trigger re-render

  return (
    <section className="panel bv">
      <h2 className="panel__title">
        Bridge Visualizer
        <span className="panel__badge">
          {peers.length} peer{peers.length !== 1 ? "s" : ""}
        </span>
        <span className="panel__badge">
          {connections.length} spoke{connections.length !== 1 ? "s" : ""}
        </span>
      </h2>
      <svg
        ref={svgRef}
        className="bv__svg"
        style={{ width: "100%", height: 340, display: "block", userSelect: "none" }}
      >
        {/* Edges — hub to every spoke/peer */}
        {nodes.slice(1).map((n) => {
          const hub = nodes[0];
          if (!hub) return null;
          return (
            <line
              key={`edge-${n.id}`}
              x1={hub.x}
              y1={hub.y}
              x2={n.x}
              y2={n.y}
              stroke={n.kind === "peer" ? "#f0a04055" : "#388bfd33"}
              strokeWidth={n.kind === "peer" ? 2 : 1.5}
              strokeDasharray={n.kind === "peer" ? "6 4" : undefined}
            />
          );
        })}

        {/* Nodes */}
        {nodes.map((n) => {
          const r = radius(n.kind);
          const c = color(n.kind);
          const isHub = n.kind === "hub";
          return (
            <g
              key={n.id}
              transform={`translate(${n.x},${n.y})`}
              style={{ cursor: n.pinned ? "default" : "grab" }}
              onMouseDown={n.pinned ? undefined : (e) => onMouseDown(e, n.id)}
            >
              {/* Outer glow ring for hub */}
              {isHub && <circle r={r + 8} fill="none" stroke={c} strokeWidth={1} opacity={0.2} />}
              <circle r={r} fill={`${c}22`} stroke={c} strokeWidth={isHub ? 2.5 : 1.5} />
              <text
                textAnchor="middle"
                dominantBaseline="middle"
                fontSize={isHub ? 11 : 9}
                fontWeight={isHub ? 700 : 500}
                fill={c}
                style={{ pointerEvents: "none" }}
              >
                {n.label.length > 10 ? n.label.slice(0, 9) + "…" : n.label}
              </text>
              {n.sublabel && (
                <text
                  y={r + 10}
                  textAnchor="middle"
                  fontSize={8}
                  fill="#8b949e"
                  style={{ pointerEvents: "none" }}
                >
                  {n.sublabel.length > 14 ? n.sublabel.slice(0, 13) + "…" : n.sublabel}
                </text>
              )}
            </g>
          );
        })}
      </svg>

      {/* Legend */}
      <div className="bv__legend">
        <span>
          <span className="bv__dot" style={{ background: "#388bfd" }} />
          Hub (CDA)
        </span>
        <span>
          <span className="bv__dot" style={{ background: "#3fb950" }} />
          Spoke app
        </span>
        <span>
          <span className="bv__dot" style={{ background: "#f0a040" }} />
          Peer bridge
        </span>
      </div>
    </section>
  );
}
